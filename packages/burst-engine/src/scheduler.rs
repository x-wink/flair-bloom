use qzh_profile::key_id::KeyId;
use qzh_profile::profile::BurstRule;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::warn;
use win_input::{key_events, DispatchResult, InputEvent};

const ACK_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulePhase {
    DownPhase,
    UpPhase,
}

#[derive(Debug)]
struct ScheduledRule {
    rule: Arc<BurstRule>,
    generation: u64,
    allow_while_physical_down: bool,
    phase: RulePhase,
    next_at: Instant,
    is_down: bool,
}

enum SchedulerCommand {
    Start {
        rule: Arc<BurstRule>,
        generation: u64,
    },
    Stop {
        rule_id: String,
        generation: u64,
        ack: Option<Sender<()>>,
    },
    StopAll {
        generation: u64,
        ack: Option<Sender<()>>,
    },
    Shutdown {
        generation: u64,
        ack: Sender<()>,
    },
}

pub struct SchedulerHandle {
    tx: Sender<SchedulerCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
    hp_degraded: Arc<AtomicBool>,
    command_waker: platform_wait::CommandWaker,
}

pub(crate) type PhysicalKeys = Arc<Mutex<HashSet<KeyId>>>;
pub(crate) type SimulatedKeys = Arc<Mutex<HashMap<KeyId, usize>>>;

pub(crate) trait EventDispatcher: Send + Sync {
    fn dispatch(&self, events: &[InputEvent]) -> Vec<DispatchResult>;
}

struct WinInputDispatcher;

impl EventDispatcher for WinInputDispatcher {
    fn dispatch(&self, events: &[InputEvent]) -> Vec<DispatchResult> {
        key_events(events)
    }
}

#[derive(Default)]
pub(crate) struct SchedulerStats {
    delay_ns: Mutex<Vec<u128>>,
    sent_events: AtomicU64,
    failed_events: AtomicU64,
    batches: AtomicU64,
}

impl SchedulerStats {
    fn record_delay(&self, delay: Duration) {
        self.delay_ns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(delay.as_nanos());
    }

    fn record_results(&self, results: &[DispatchResult]) {
        self.batches.fetch_add(1, Ordering::Relaxed);
        let sent = results.iter().filter(|r| r.was_sent()).count() as u64;
        let failed = results
            .iter()
            .filter(|r| matches!(r, DispatchResult::Failed))
            .count() as u64;
        self.sent_events.fetch_add(sent, Ordering::Relaxed);
        self.failed_events.fetch_add(failed, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> SchedulerStatsSnapshot {
        let delays = self
            .delay_ns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        SchedulerStatsSnapshot {
            delay_ns: delays,
            sent_events: self.sent_events.load(Ordering::Relaxed),
            failed_events: self.failed_events.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct SchedulerStatsSnapshot {
    pub delay_ns: Vec<u128>,
    pub sent_events: u64,
    pub failed_events: u64,
    pub batches: u64,
}

impl SchedulerHandle {
    pub fn start(physical_keys: PhysicalKeys, simulated_keys: SimulatedKeys) -> Self {
        Self::start_with_dispatcher(
            Arc::new(WinInputDispatcher),
            None,
            physical_keys,
            simulated_keys,
        )
    }

    pub(crate) fn start_with_dispatcher(
        dispatcher: Arc<dyn EventDispatcher>,
        stats: Option<Arc<SchedulerStats>>,
        physical_keys: PhysicalKeys,
        simulated_keys: SimulatedKeys,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let hp_degraded = Arc::new(AtomicBool::new(false));
        let worker_degraded = hp_degraded.clone();
        let command_waker = platform_wait::CommandWaker::new(&hp_degraded);
        let worker_waker = command_waker.clone();
        let join = thread::spawn(move || {
            worker_loop(
                rx,
                worker_degraded,
                worker_waker,
                dispatcher,
                stats,
                physical_keys,
                simulated_keys,
            )
        });
        Self {
            tx,
            join: Mutex::new(Some(join)),
            hp_degraded,
            command_waker,
        }
    }

    pub fn start_rule(&self, rule: Arc<BurstRule>, generation: u64) {
        self.send(SchedulerCommand::Start { rule, generation });
    }

    pub fn stop_rule(&self, rule_id: String, generation: u64) {
        self.send(SchedulerCommand::Stop {
            rule_id,
            generation,
            ack: None,
        });
    }

    pub fn stop_all_async(&self, generation: u64) {
        self.send(SchedulerCommand::StopAll {
            generation,
            ack: None,
        });
    }

    pub fn stop_all_blocking(&self, generation: u64) -> bool {
        let (tx, rx) = mpsc::channel();
        if !self.send(SchedulerCommand::StopAll {
            generation,
            ack: Some(tx),
        }) {
            return false;
        }
        rx.recv_timeout(ACK_TIMEOUT).is_ok()
    }

    pub fn shutdown_blocking(&self, generation: u64) -> bool {
        let (tx, rx) = mpsc::channel();
        if !self.send(SchedulerCommand::Shutdown {
            generation,
            ack: tx,
        }) {
            return false;
        }
        let acked = rx.recv_timeout(ACK_TIMEOUT).is_ok();
        if let Some(join) = self.join.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = join.join();
        }
        acked
    }

    pub fn hp_degraded(&self) -> bool {
        self.hp_degraded.load(Ordering::SeqCst)
    }

    fn send(&self, command: SchedulerCommand) -> bool {
        if self.tx.send(command).is_err() {
            return false;
        }
        self.command_waker.wake();
        true
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_blocking(u64::MAX);
    }
}

fn worker_loop(
    rx: Receiver<SchedulerCommand>,
    hp_degraded: Arc<AtomicBool>,
    command_waker: platform_wait::CommandWaker,
    dispatcher: Arc<dyn EventDispatcher>,
    stats: Option<Arc<SchedulerStats>>,
    physical_keys: PhysicalKeys,
    simulated_keys: SimulatedKeys,
) {
    let mut worker = SchedulerWorker {
        rules: HashMap::new(),
        target_holds: HashMap::new(),
        current_generation: 0,
        dispatcher,
        stats,
        physical_keys,
        simulated_keys,
    };
    let mut wait = platform_wait::WaitContext::new(command_waker, hp_degraded);

    loop {
        let deadline = worker.next_deadline();
        let command = match wait.recv(&rx, deadline) {
            platform_wait::WaitOutcome::Command(command) => Some(command),
            platform_wait::WaitOutcome::Timeout => None,
            platform_wait::WaitOutcome::Disconnected => {
                worker.cleanup_all();
                break;
            }
        };

        if let Some(command) = command {
            if worker.handle_command(command) {
                break;
            }
            while let Ok(command) = rx.try_recv() {
                if worker.handle_command(command) {
                    return;
                }
            }
        }

        worker.process_due(Instant::now());
    }
}

struct SchedulerWorker {
    rules: HashMap<String, ScheduledRule>,
    target_holds: HashMap<KeyId, TargetHold>,
    current_generation: u64,
    dispatcher: Arc<dyn EventDispatcher>,
    stats: Option<Arc<SchedulerStats>>,
    physical_keys: PhysicalKeys,
    simulated_keys: SimulatedKeys,
}

#[derive(Default)]
struct TargetHold {
    owners: HashMap<String, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcquireOutcome {
    Acquired,
    Shared,
    BlockedByPhysical,
}

impl SchedulerWorker {
    fn handle_command(&mut self, command: SchedulerCommand) -> bool {
        match command {
            SchedulerCommand::Start { rule, generation } => {
                if generation == self.current_generation {
                    let allow_while_physical_down = rule.mode
                        == qzh_profile::profile::BurstMode::Hold
                        && rule.trigger_key == rule.target_key;
                    self.rules.entry(rule.id.clone()).or_insert(ScheduledRule {
                        rule,
                        generation,
                        allow_while_physical_down,
                        phase: RulePhase::DownPhase,
                        next_at: Instant::now(),
                        is_down: false,
                    });
                }
                false
            }
            SchedulerCommand::Stop {
                rule_id,
                generation,
                ack,
            } => {
                if generation == self.current_generation {
                    self.stop_rule(&rule_id);
                }
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
                false
            }
            SchedulerCommand::StopAll { generation, ack } => {
                if generation >= self.current_generation {
                    self.current_generation = generation;
                    self.cleanup_all();
                }
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
                false
            }
            SchedulerCommand::Shutdown { generation, ack } => {
                self.current_generation = self.current_generation.max(generation);
                self.cleanup_all();
                let _ = ack.send(());
                true
            }
        }
    }

    fn stop_rule(&mut self, rule_id: &str) {
        let Some(rule) = self.rules.remove(rule_id) else {
            return;
        };
        if rule.is_down {
            let event = self.release_owner(rule.rule.target_key, rule_id);
            self.dispatch_events(event.into_iter().collect());
        }
    }

    fn cleanup_all(&mut self) {
        self.rules.clear();
        let events = self.release_all_target_holds();
        self.target_holds.clear();
        self.dispatch_events(events);
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.rules.values().map(|r| r.next_at).min()
    }

    fn process_due(&mut self, now: Instant) {
        let due_ids = self
            .rules
            .iter()
            .filter_map(|(id, rule)| (rule.next_at <= now).then_some(id.clone()))
            .collect::<Vec<_>>();
        if due_ids.is_empty() {
            return;
        }

        let mut events = Vec::new();
        for id in due_ids {
            let Some(mut rule) = self.rules.remove(&id) else {
                continue;
            };
            if let Some(stats) = &self.stats {
                stats.record_delay(now.saturating_duration_since(rule.next_at));
            }
            if rule.generation != self.current_generation {
                if rule.is_down {
                    if let Some(event) = self.release_owner(rule.rule.target_key, &id) {
                        events.push(event);
                    }
                }
                continue;
            }

            let interval = Duration::from_millis(rule.rule.interval_ms as u64);
            let hold = hold_duration(rule.rule.interval_ms);
            match rule.phase {
                RulePhase::DownPhase if hold.is_zero() => {
                    match self.acquire_owner(
                        rule.rule.target_key,
                        &id,
                        rule.allow_while_physical_down,
                    ) {
                        AcquireOutcome::Acquired => {
                            events.push(InputEvent::down(rule.rule.target_key));
                            if let Some(event) = self.release_owner(rule.rule.target_key, &id) {
                                events.push(event);
                            }
                        }
                        AcquireOutcome::Shared => {
                            let _ = self.release_owner(rule.rule.target_key, &id);
                        }
                        AcquireOutcome::BlockedByPhysical => {}
                    }
                    rule.is_down = false;
                    rule.phase = RulePhase::DownPhase;
                    rule.next_at = now + interval.max(Duration::from_millis(1));
                }
                RulePhase::DownPhase => {
                    match self.acquire_owner(
                        rule.rule.target_key,
                        &id,
                        rule.allow_while_physical_down,
                    ) {
                        AcquireOutcome::Acquired => {
                            events.push(InputEvent::down(rule.rule.target_key));
                            rule.is_down = true;
                            rule.phase = RulePhase::UpPhase;
                            rule.next_at = now + hold;
                        }
                        AcquireOutcome::Shared => {
                            rule.is_down = true;
                            rule.phase = RulePhase::UpPhase;
                            rule.next_at = now + hold;
                        }
                        AcquireOutcome::BlockedByPhysical => {
                            rule.is_down = false;
                            rule.phase = RulePhase::DownPhase;
                            rule.next_at = now + interval.max(Duration::from_millis(1));
                        }
                    }
                }
                RulePhase::UpPhase => {
                    if let Some(event) = self.release_owner(rule.rule.target_key, &id) {
                        events.push(event);
                    }
                    rule.is_down = false;
                    rule.phase = RulePhase::DownPhase;
                    let rest = interval.saturating_sub(hold).max(Duration::from_millis(1));
                    rule.next_at = now + rest;
                }
            }
            self.rules.insert(id, rule);
        }

        self.dispatch_events(events);
    }

    fn acquire_owner(
        &mut self,
        key: KeyId,
        owner: &str,
        allow_while_physical_down: bool,
    ) -> AcquireOutcome {
        if let Some(hold) = self.target_holds.get_mut(&key) {
            hold.owners
                .insert(owner.to_string(), allow_while_physical_down);
            return AcquireOutcome::Shared;
        }

        if !allow_while_physical_down && self.is_physically_blocked(key) {
            return AcquireOutcome::BlockedByPhysical;
        }

        let owners = self.target_holds.entry(key).or_default();
        owners
            .owners
            .insert(owner.to_string(), allow_while_physical_down);
        AcquireOutcome::Acquired
    }

    /// 目标键是否真的被物理按住。`physical_keys` 由 hook 维护，但 DD 等后端的注入回灌
    /// 仅靠启发式队列过滤（非 SIM_MARKER 精确过滤），偶尔漏过会把自身注入误记为物理按下，
    /// 且该集合从不整体清空 → 目标键被永久误判为"按住"而停止注入。命中拦截前用
    /// `GetAsyncKeyState` 复核真实物理状态，发现陈旧泄漏就地清除并放行。
    fn is_physically_blocked(&self, key: KeyId) -> bool {
        let mut pressed = self.physical_keys.lock().unwrap_or_else(|e| e.into_inner());
        if !pressed.contains(&key) {
            return false;
        }
        if key_physically_down(key) {
            return true;
        }
        pressed.remove(&key);
        warn!("清除陈旧物理按键记录（疑似注入回灌泄漏）: key={key:?}");
        false
    }

    fn release_owner(&mut self, key: KeyId, owner: &str) -> Option<InputEvent> {
        let hold = self.target_holds.get_mut(&key)?;
        hold.owners.remove(owner);
        if !hold.owners.is_empty() {
            None
        } else {
            self.target_holds.remove(&key);
            Some(InputEvent::up(key))
        }
    }

    fn release_all_target_holds(&mut self) -> Vec<InputEvent> {
        self.target_holds
            .drain()
            .map(|(key, _)| InputEvent::up(key))
            .collect()
    }

    fn dispatch_events(&self, events: Vec<InputEvent>) {
        if events.is_empty() {
            return;
        }
        let results = self.dispatcher.dispatch(&events);
        self.record_simulated_ledger(&events, &results);
        if let Some(stats) = &self.stats {
            stats.record_results(&results);
        }
        for (event, result) in events.iter().zip(results) {
            if result == DispatchResult::Failed {
                warn!("输入注入失败: key={:?} is_up={}", event.key, event.is_up);
            }
        }
    }

    fn record_simulated_ledger(&self, events: &[InputEvent], results: &[DispatchResult]) {
        let mut simulated = self
            .simulated_keys
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (event, result) in events.iter().zip(results.iter().copied()) {
            if !result.was_sent() {
                continue;
            }
            if event.is_up {
                decrement_simulated_key(&mut simulated, event.key);
            } else {
                *simulated.entry(event.key).or_default() += 1;
            }
        }
    }
}

pub(crate) fn release_simulated_keys(simulated_keys: &SimulatedKeys) {
    let events = simulated_keys
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain()
        .map(|(key, _)| InputEvent::up(key))
        .collect::<Vec<_>>();
    if !events.is_empty() {
        let _ = key_events(&events);
    }
}

/// 查询某个键当前是否处于物理按下状态。高位 `0x8000` 表示按下。
/// 非 Windows 平台无法查询，保守地信任 `physical_keys` 记录（返回 `true`），
/// 使既有单元测试与跨平台行为保持不变。
#[cfg(windows)]
fn key_physically_down(key: KeyId) -> bool {
    use qzh_profile::key_id::MouseButton;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let vk: i32 = match key {
        KeyId::Keyboard(vk) => vk as i32,
        KeyId::Mouse(MouseButton::Left) => 0x01, // VK_LBUTTON
        KeyId::Mouse(MouseButton::Right) => 0x02, // VK_RBUTTON
        KeyId::Mouse(MouseButton::Middle) => 0x04, // VK_MBUTTON
        KeyId::Mouse(MouseButton::X1) => 0x05,   // VK_XBUTTON1
        KeyId::Mouse(MouseButton::X2) => 0x06,   // VK_XBUTTON2
        // 滚轮是瞬发事件，不存在"按住"状态，一律视为未按下（陈旧记录即清除）。
        KeyId::Mouse(MouseButton::WheelUp | MouseButton::WheelDown) => return false,
    };
    // SAFETY: GetAsyncKeyState 对任意 vKey 取值安全，无副作用
    let state = unsafe { GetAsyncKeyState(vk) };
    (state as u16 & 0x8000) != 0
}

#[cfg(not(windows))]
fn key_physically_down(_key: KeyId) -> bool {
    true
}

fn decrement_simulated_key(simulated: &mut HashMap<KeyId, usize>, key: KeyId) {
    let Some(count) = simulated.get_mut(&key) else {
        return;
    };
    if *count <= 1 {
        simulated.remove(&key);
    } else {
        *count -= 1;
    }
}

fn hold_duration(interval_ms: u32) -> Duration {
    if interval_ms <= 1 {
        return Duration::ZERO;
    }
    let interval = interval_ms as u64;
    let hold = (interval / 3).clamp(1, 30).min(interval - 1);
    Duration::from_millis(hold)
}

#[cfg(not(windows))]
mod platform_wait {
    use super::{Duration, Instant, Receiver, SchedulerCommand};
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::RecvTimeoutError;
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct CommandWaker;

    impl CommandWaker {
        pub fn new(_degraded: &Arc<AtomicBool>) -> Self {
            Self
        }

        pub fn wake(&self) {}
    }

    pub enum WaitOutcome {
        Command(SchedulerCommand),
        Timeout,
        Disconnected,
    }

    pub struct WaitContext;

    impl WaitContext {
        pub fn new(_command_waker: CommandWaker, _degraded: Arc<AtomicBool>) -> Self {
            Self
        }

        pub fn recv(
            &mut self,
            rx: &Receiver<SchedulerCommand>,
            deadline: Option<Instant>,
        ) -> WaitOutcome {
            match deadline {
                Some(next_at) => {
                    let now = Instant::now();
                    if next_at <= now {
                        return WaitOutcome::Timeout;
                    }
                    recv_timeout(rx, next_at.duration_since(now))
                }
                None => match rx.recv() {
                    Ok(command) => WaitOutcome::Command(command),
                    Err(_) => WaitOutcome::Disconnected,
                },
            }
        }
    }

    fn recv_timeout(rx: &Receiver<SchedulerCommand>, timeout: Duration) -> WaitOutcome {
        match rx.recv_timeout(timeout) {
            Ok(command) => WaitOutcome::Command(command),
            Err(RecvTimeoutError::Timeout) => WaitOutcome::Timeout,
            Err(RecvTimeoutError::Disconnected) => WaitOutcome::Disconnected,
        }
    }
}

#[cfg(windows)]
mod platform_wait {
    use super::{Duration, Instant, Receiver, SchedulerCommand};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
    use std::sync::Arc;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateWaitableTimerExW, SetEvent, SetWaitableTimer, WaitForMultipleObjects,
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
    };

    struct WinHandle(HANDLE);

    unsafe impl Send for WinHandle {}
    unsafe impl Sync for WinHandle {}

    impl Drop for WinHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    #[derive(Clone)]
    pub struct CommandWaker {
        event: Option<Arc<WinHandle>>,
    }

    impl CommandWaker {
        pub fn new(degraded: &Arc<AtomicBool>) -> Self {
            let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
            if event.is_null() {
                degraded.store(true, Ordering::SeqCst);
                Self { event: None }
            } else {
                Self {
                    event: Some(Arc::new(WinHandle(event))),
                }
            }
        }

        pub fn wake(&self) {
            if let Some(event) = &self.event {
                unsafe { SetEvent(event.0) };
            }
        }
    }

    pub enum WaitOutcome {
        Command(SchedulerCommand),
        Timeout,
        Disconnected,
    }

    pub struct WaitContext {
        command_waker: CommandWaker,
        timer: Option<WinHandle>,
        degraded: Arc<AtomicBool>,
    }

    impl WaitContext {
        pub fn new(command_waker: CommandWaker, degraded: Arc<AtomicBool>) -> Self {
            let timer = unsafe {
                CreateWaitableTimerExW(
                    std::ptr::null(),
                    std::ptr::null(),
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS,
                )
            };
            let timer = if timer.is_null() {
                degraded.store(true, Ordering::SeqCst);
                None
            } else {
                Some(WinHandle(timer))
            };
            Self {
                command_waker,
                timer,
                degraded,
            }
        }

        pub fn recv(
            &mut self,
            rx: &Receiver<SchedulerCommand>,
            deadline: Option<Instant>,
        ) -> WaitOutcome {
            match rx.try_recv() {
                Ok(command) => return WaitOutcome::Command(command),
                Err(TryRecvError::Disconnected) => return WaitOutcome::Disconnected,
                Err(TryRecvError::Empty) => {}
            }

            let Some(event) = self.command_waker.event.as_ref() else {
                return fallback_recv(rx, deadline);
            };
            let Some(timer) = self.timer.as_ref() else {
                return fallback_recv(rx, deadline);
            };

            let handles = match deadline {
                Some(next_at) => {
                    let now = Instant::now();
                    if next_at <= now {
                        return WaitOutcome::Timeout;
                    }
                    let due_time = relative_due_time(next_at.duration_since(now));
                    let ok = unsafe {
                        SetWaitableTimer(timer.0, &due_time, 0, None, std::ptr::null(), 0)
                    };
                    if ok == 0 {
                        self.degraded.store(true, Ordering::SeqCst);
                        return fallback_recv(rx, deadline);
                    }
                    [event.0, timer.0]
                }
                None => [event.0, std::ptr::null_mut()],
            };

            let (count, ptr) = if deadline.is_some() {
                (2, handles.as_ptr())
            } else {
                (1, handles.as_ptr())
            };
            let wait = unsafe { WaitForMultipleObjects(count, ptr, 0, INFINITE) };
            match wait {
                WAIT_OBJECT_0 => match rx.try_recv() {
                    Ok(command) => WaitOutcome::Command(command),
                    Err(TryRecvError::Disconnected) => WaitOutcome::Disconnected,
                    Err(TryRecvError::Empty) => WaitOutcome::Timeout,
                },
                x if x == WAIT_OBJECT_0 + 1 => WaitOutcome::Timeout,
                WAIT_TIMEOUT => WaitOutcome::Timeout,
                _ => {
                    self.degraded.store(true, Ordering::SeqCst);
                    fallback_recv(rx, deadline)
                }
            }
        }
    }

    fn fallback_recv(rx: &Receiver<SchedulerCommand>, deadline: Option<Instant>) -> WaitOutcome {
        match deadline {
            Some(next_at) => {
                let now = Instant::now();
                if next_at <= now {
                    return WaitOutcome::Timeout;
                }
                match rx.recv_timeout(next_at.duration_since(now)) {
                    Ok(command) => WaitOutcome::Command(command),
                    Err(RecvTimeoutError::Timeout) => WaitOutcome::Timeout,
                    Err(RecvTimeoutError::Disconnected) => WaitOutcome::Disconnected,
                }
            }
            None => match rx.recv() {
                Ok(command) => WaitOutcome::Command(command),
                Err(_) => WaitOutcome::Disconnected,
            },
        }
    }

    fn relative_due_time(duration: Duration) -> i64 {
        let ticks = (duration.as_nanos() / 100).clamp(1, i64::MAX as u128) as i64;
        -ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qzh_profile::profile::BurstMode;

    struct TestDispatcher;

    impl EventDispatcher for TestDispatcher {
        fn dispatch(&self, events: &[InputEvent]) -> Vec<DispatchResult> {
            vec![DispatchResult::Sent; events.len()]
        }
    }

    fn test_worker() -> (SchedulerWorker, PhysicalKeys, SimulatedKeys) {
        let physical_keys = Arc::new(Mutex::new(HashSet::new()));
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));
        let worker = SchedulerWorker {
            rules: HashMap::new(),
            target_holds: HashMap::new(),
            current_generation: 0,
            dispatcher: Arc::new(TestDispatcher),
            stats: None,
            physical_keys: physical_keys.clone(),
            simulated_keys: simulated_keys.clone(),
        };
        (worker, physical_keys, simulated_keys)
    }

    fn rule(id: &str, target_key: KeyId) -> Arc<BurstRule> {
        Arc::new(BurstRule {
            id: id.to_string(),
            enabled: true,
            trigger_key: KeyId::Keyboard(0x51),
            target_key,
            mode: BurstMode::Toggle,
            stop_key: None,
            interval_ms: 10,
            group: None,
        })
    }

    #[test]
    fn hold_duration_for_one_ms_uses_tap_mode() {
        assert_eq!(hold_duration(1), Duration::ZERO);
    }

    #[test]
    fn old_generation_start_is_discarded_after_stop_all() {
        let (mut worker, _, _) = test_worker();
        worker.handle_command(SchedulerCommand::StopAll {
            generation: 1,
            ack: None,
        });
        worker.handle_command(SchedulerCommand::Start {
            rule: rule("old", KeyId::Keyboard(0x45)),
            generation: 0,
        });

        assert!(worker.rules.is_empty());
    }

    #[test]
    fn stale_stop_all_does_not_cleanup_newer_generation_rules() {
        let (mut worker, _, _) = test_worker();
        worker.current_generation = 2;
        worker.handle_command(SchedulerCommand::Start {
            rule: rule("new", KeyId::Keyboard(0x45)),
            generation: 2,
        });
        let (ack_tx, ack_rx) = mpsc::channel();

        worker.handle_command(SchedulerCommand::StopAll {
            generation: 1,
            ack: Some(ack_tx),
        });

        assert!(ack_rx.try_recv().is_ok());
        assert!(worker.rules.contains_key("new"));
        assert_eq!(worker.current_generation, 2);
    }

    #[test]
    fn same_target_owners_share_single_down() {
        let (mut worker, _, _) = test_worker();
        let key = KeyId::Keyboard(0x45);

        assert_eq!(
            worker.acquire_owner(key, "a", false),
            AcquireOutcome::Acquired
        );
        assert_eq!(
            worker.acquire_owner(key, "b", false),
            AcquireOutcome::Shared
        );
        assert_eq!(worker.release_owner(key, "a"), None);
        assert_eq!(worker.release_owner(key, "b"), Some(InputEvent::up(key)));
    }

    #[test]
    fn physical_target_down_blocks_different_trigger_injection() {
        let (mut worker, physical_keys, _) = test_worker();
        let key = KeyId::Keyboard(0x45);
        physical_keys.lock().unwrap().insert(key);

        assert_eq!(
            worker.acquire_owner(key, "different-trigger", false),
            AcquireOutcome::BlockedByPhysical
        );
        assert!(worker.target_holds.is_empty());
    }

    #[test]
    fn hold_target_equals_trigger_can_inject_while_physical_down() {
        let (mut worker, physical_keys, _) = test_worker();
        let key = KeyId::Keyboard(0x45);
        physical_keys.lock().unwrap().insert(key);

        assert_eq!(
            worker.acquire_owner(key, "same-trigger", true),
            AcquireOutcome::Acquired
        );
        assert!(worker.target_holds.contains_key(&key));
    }

    #[test]
    fn key_up_releases_simulated_down_even_when_physical_key_is_down() {
        let (worker, physical_keys, simulated_keys) = test_worker();
        let key = KeyId::Keyboard(0x45);
        physical_keys.lock().unwrap().insert(key);
        simulated_keys.lock().unwrap().insert(key, 1);

        worker.dispatch_events(vec![InputEvent::up(key)]);

        assert!(!simulated_keys.lock().unwrap().contains_key(&key));
    }
}
