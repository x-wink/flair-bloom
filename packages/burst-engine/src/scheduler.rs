use qzh_profile::key_id::KeyId;
use qzh_profile::profile::BurstRule;
use std::collections::{hash_map::Entry, HashMap, HashSet};
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
    pub fn start(simulated_keys: SimulatedKeys) -> Self {
        Self::start_with_dispatcher(Arc::new(WinInputDispatcher), None, simulated_keys)
    }

    pub(crate) fn start_with_dispatcher(
        dispatcher: Arc<dyn EventDispatcher>,
        stats: Option<Arc<SchedulerStats>>,
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
    simulated_keys: SimulatedKeys,
) {
    let mut worker = SchedulerWorker {
        rules: HashMap::new(),
        target_holds: HashMap::new(),
        current_generation: 0,
        dispatcher,
        stats,
        simulated_keys,
        throttle: Throttle::new(hp_degraded.clone()),
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
    /// 目标键 -> 当前持有它的规则 ID 集合。集合非空即该键处于注入按下态，
    /// 多条规则共享同一目标键时只发一次 down，最后一个 owner 释放时才发 up。
    target_holds: HashMap<KeyId, HashSet<String>>,
    current_generation: u64,
    dispatcher: Arc<dyn EventDispatcher>,
    stats: Option<Arc<SchedulerStats>>,
    simulated_keys: SimulatedKeys,
    throttle: Throttle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcquireOutcome {
    Acquired,
    Shared,
}

/// 智能降频下限范围：1ms 表示不降频，最高放宽到 50ms。
const THROTTLE_FLOOR_MIN_MS: u32 = 1;
const THROTTLE_FLOOR_MAX_MS: u32 = 50;
/// 无高精度定时器（hp_degraded）时亚 ~15ms 周期无法稳定达成，硬钳到此可持续下限（信号 C）。
const HP_DEGRADED_FLOOR_MS: u32 = 8;
/// 承压时每批升高的步长，健康时每批衰减的步长。升快降慢形成迟滞，避免临界点抖动。
const THROTTLE_RAISE_STEP_MS: u32 = 2;
const THROTTLE_DECAY_STEP_MS: u32 = 1;
/// 调度迟到超过此阈值视作线程跟不上节奏（信号 B）。
const THROTTLE_LATE_THRESHOLD: Duration = Duration::from_millis(4);

/// 智能降频：引擎自我调节，用让步注入效率（优先级③）守住物理输入优先 + 连发稳定
/// （优先级①②）——CPU 因温控/电量降频同理。`dynamic_floor_ms` 是当前允许的最快注入
/// 周期下限，仅会放慢、绝不快过规则请求的 interval；健康时缓慢衰减、停止时复位回 1ms。
struct Throttle {
    dynamic_floor_ms: u32,
    hp_degraded: Arc<AtomicBool>,
}

impl Throttle {
    fn new(hp_degraded: Arc<AtomicBool>) -> Self {
        Self {
            dynamic_floor_ms: THROTTLE_FLOOR_MIN_MS,
            hp_degraded,
        }
    }

    /// 当前有效注入周期下限：自适应下限与 hp_degraded 硬底（信号 C）取较大者。
    fn floor_ms(&self) -> u32 {
        let degraded_floor = if self.hp_degraded.load(Ordering::Relaxed) {
            HP_DEGRADED_FLOOR_MS
        } else {
            THROTTLE_FLOOR_MIN_MS
        };
        self.dynamic_floor_ms.max(degraded_floor)
    }

    /// 按本轮是否承压（信号 B 调度迟到）调整下限：承压升高、健康衰减。
    fn observe(&mut self, stressed: bool) {
        self.dynamic_floor_ms = if stressed {
            (self.dynamic_floor_ms + THROTTLE_RAISE_STEP_MS).min(THROTTLE_FLOOR_MAX_MS)
        } else {
            self.dynamic_floor_ms
                .saturating_sub(THROTTLE_DECAY_STEP_MS)
                .max(THROTTLE_FLOOR_MIN_MS)
        };
    }

    /// 复位到不降频。停止 / 切代 / 退出等会话边界调用，避免一次瞬时承压把下限
    /// 顶高后跨会话残留、把下一轮连发长期钳慢。
    fn reset(&mut self) {
        self.dynamic_floor_ms = THROTTLE_FLOOR_MIN_MS;
    }
}

impl SchedulerWorker {
    fn handle_command(&mut self, command: SchedulerCommand) -> bool {
        match command {
            SchedulerCommand::Start { rule, generation } => {
                if generation == self.current_generation {
                    self.rules.entry(rule.id.clone()).or_insert(ScheduledRule {
                        rule,
                        generation,
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
            let event = Self::release_owner(&mut self.target_holds, rule.rule.target_key, rule_id);
            self.dispatch_events(event.into_iter().collect());
        }
    }

    fn cleanup_all(&mut self) {
        self.rules.clear();
        let events = self.release_all_target_holds();
        self.target_holds.clear();
        self.dispatch_events(events);
        // 会话结束：复位降频，下一轮连发从不降频开始，避免上次的瞬时承压残留拖慢。
        self.throttle.reset();
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.rules.values().map(|r| r.next_at).min()
    }

    fn process_due(&mut self, now: Instant) {
        // 智能降频：effective interval = max(请求 interval, floor)，只拉长「下一拍的间隔」，
        // 绝不快过规则请求；floor>=1ms 保证 next_at 单调推进。
        let floor_ms = self.throttle.floor_ms();
        let current_generation = self.current_generation;
        let stats = self.stats.as_deref();
        // 与 rules.iter_mut() 拆分借用，原地处理到期规则，避免每 tick 的 id 克隆与 remove/insert。
        let target_holds = &mut self.target_holds;

        let mut any_due = false;
        let mut max_late = Duration::ZERO;
        let mut events = Vec::new();
        let mut expired: Vec<String> = Vec::new();

        for (id, rule) in self.rules.iter_mut() {
            if rule.next_at > now {
                continue;
            }
            any_due = true;

            // 过期 generation 规则只做惰性释放，不参与降频度量：其 next_at 早已过期，
            // 计入会把信号 B 误顶满（快速启停 / 切代后常见）。
            if rule.generation != current_generation {
                if rule.is_down {
                    if let Some(event) = Self::release_owner(target_holds, rule.rule.target_key, id)
                    {
                        events.push(event);
                    }
                }
                expired.push(id.clone());
                continue;
            }

            // 信号 B：仅统计当前 generation、真正参与注入的规则的调度迟到。
            let late = now.saturating_duration_since(rule.next_at);
            max_late = max_late.max(late);
            if let Some(stats) = stats {
                stats.record_delay(late);
            }

            // hold（按下时长）按规则请求的 interval 计算，降频不拉长按下、保持点按特性；
            // 仅把拍与拍之间的「间隔」放慢。
            let interval = Duration::from_millis(rule.rule.interval_ms.max(floor_ms) as u64);
            let hold = hold_duration(rule.rule.interval_ms);
            match rule.phase {
                RulePhase::DownPhase if hold.is_zero() => {
                    match Self::acquire_owner(target_holds, rule.rule.target_key, id) {
                        AcquireOutcome::Acquired => {
                            events.push(InputEvent::down(rule.rule.target_key));
                            if let Some(event) =
                                Self::release_owner(target_holds, rule.rule.target_key, id)
                            {
                                events.push(event);
                            }
                        }
                        AcquireOutcome::Shared => {
                            let _ = Self::release_owner(target_holds, rule.rule.target_key, id);
                        }
                    }
                    rule.is_down = false;
                    rule.phase = RulePhase::DownPhase;
                    rule.next_at = now + interval;
                }
                RulePhase::DownPhase => {
                    if let AcquireOutcome::Acquired =
                        Self::acquire_owner(target_holds, rule.rule.target_key, id)
                    {
                        events.push(InputEvent::down(rule.rule.target_key));
                    }
                    rule.is_down = true;
                    rule.phase = RulePhase::UpPhase;
                    rule.next_at = now + hold;
                }
                RulePhase::UpPhase => {
                    if let Some(event) = Self::release_owner(target_holds, rule.rule.target_key, id)
                    {
                        events.push(event);
                    }
                    rule.is_down = false;
                    rule.phase = RulePhase::DownPhase;
                    let rest = interval.saturating_sub(hold).max(Duration::from_millis(1));
                    rule.next_at = now + rest;
                }
            }
        }

        for id in expired {
            self.rules.remove(&id);
        }
        if !any_due {
            return;
        }

        self.dispatch_events(events);
        // 信号 B/C：调度持续迟到（线程跟不上节奏）则升高下限，否则衰减恢复。
        // 不用注入失败率作信号——失败几乎都是单键/单后端问题，不该全局降速。
        self.throttle.observe(max_late > THROTTLE_LATE_THRESHOLD);
    }

    /// 申请目标键的注入所有权。引擎不为业务取舍兜底：不判断该键是否被用户物理按住
    /// （把某键设为连发 target 即声明它会被不断按下弹起，是业务层取舍）。同一目标键被多条
    /// 规则共享时只发一次 down，避免相互打断。单次 entry 查找区分首占用与共享。
    /// 取 `&mut target_holds` 而非 `&mut self`，使 `process_due` 能与 `rules.iter_mut()`
    /// 拆分借用、原地处理规则，省掉每 tick 的 id 克隆与 remove/insert。
    fn acquire_owner(
        target_holds: &mut HashMap<KeyId, HashSet<String>>,
        key: KeyId,
        owner: &str,
    ) -> AcquireOutcome {
        match target_holds.entry(key) {
            Entry::Occupied(mut e) => {
                e.get_mut().insert(owner.to_string());
                AcquireOutcome::Shared
            }
            Entry::Vacant(e) => {
                e.insert(HashSet::new()).insert(owner.to_string());
                AcquireOutcome::Acquired
            }
        }
    }

    fn release_owner(
        target_holds: &mut HashMap<KeyId, HashSet<String>>,
        key: KeyId,
        owner: &str,
    ) -> Option<InputEvent> {
        let owners = target_holds.get_mut(&key)?;
        owners.remove(owner);
        if !owners.is_empty() {
            None
        } else {
            target_holds.remove(&key);
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

    fn test_worker() -> (SchedulerWorker, SimulatedKeys) {
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));
        let worker = SchedulerWorker {
            rules: HashMap::new(),
            target_holds: HashMap::new(),
            current_generation: 0,
            dispatcher: Arc::new(TestDispatcher),
            stats: None,
            simulated_keys: simulated_keys.clone(),
            throttle: Throttle::new(Arc::new(AtomicBool::new(false))),
        };
        (worker, simulated_keys)
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
    fn throttle_raises_floor_under_stress_and_decays_when_healthy() {
        let mut t = Throttle::new(Arc::new(AtomicBool::new(false)));
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MIN_MS);

        // 持续承压：下限单调升高并封顶。
        for _ in 0..100 {
            t.observe(true);
        }
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MAX_MS);

        // 恢复健康：缓慢衰减回不降频。
        for _ in 0..(THROTTLE_FLOOR_MAX_MS as usize) {
            t.observe(false);
        }
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MIN_MS);
    }

    #[test]
    fn throttle_raise_is_faster_than_decay() {
        let mut t = Throttle::new(Arc::new(AtomicBool::new(false)));
        t.observe(true); // +RAISE
        let raised = t.floor_ms();
        t.observe(false); // -DECAY
                          // 升快降慢：一升一降后仍高于初始，形成迟滞。
        assert!(t.floor_ms() > THROTTLE_FLOOR_MIN_MS);
        assert!(raised - t.floor_ms() == THROTTLE_DECAY_STEP_MS);
    }

    #[test]
    fn throttle_hp_degraded_enforces_min_floor() {
        let t = Throttle::new(Arc::new(AtomicBool::new(true)));
        // 无高精度定时器时即便未承压也钳到可持续下限（信号 C）。
        assert!(t.floor_ms() >= HP_DEGRADED_FLOOR_MS);
    }

    #[test]
    fn cleanup_all_resets_throttle_so_next_session_starts_unthrottled() {
        let (mut worker, _) = test_worker();
        for _ in 0..100 {
            worker.throttle.observe(true);
        }
        assert!(worker.throttle.floor_ms() > THROTTLE_FLOOR_MIN_MS);

        // 停止 / 切代 / 退出都经 cleanup_all：复位降频，避免瞬时承压跨会话残留。
        worker.cleanup_all();
        assert_eq!(worker.throttle.floor_ms(), THROTTLE_FLOOR_MIN_MS);
    }

    #[test]
    fn old_generation_start_is_discarded_after_stop_all() {
        let (mut worker, _) = test_worker();
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
        let (mut worker, _) = test_worker();
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
        let (mut worker, _) = test_worker();
        let key = KeyId::Keyboard(0x45);
        let th = &mut worker.target_holds;

        assert_eq!(
            SchedulerWorker::acquire_owner(th, key, "a"),
            AcquireOutcome::Acquired
        );
        assert_eq!(
            SchedulerWorker::acquire_owner(th, key, "b"),
            AcquireOutcome::Shared
        );
        assert_eq!(SchedulerWorker::release_owner(th, key, "a"), None);
        assert_eq!(
            SchedulerWorker::release_owner(th, key, "b"),
            Some(InputEvent::up(key))
        );
    }

    #[test]
    fn acquire_owner_no_longer_gates_on_physical_state() {
        // 引擎不为业务取舍兜底：target 键即便被物理按住也照常注入（由业务层声明风险）。
        let (mut worker, _) = test_worker();
        let key = KeyId::Keyboard(0x45);

        assert_eq!(
            SchedulerWorker::acquire_owner(&mut worker.target_holds, key, "different-trigger"),
            AcquireOutcome::Acquired
        );
        assert!(worker.target_holds.contains_key(&key));
    }

    #[test]
    fn key_up_releases_simulated_down() {
        let (worker, simulated_keys) = test_worker();
        let key = KeyId::Keyboard(0x45);
        simulated_keys.lock().unwrap().insert(key, 1);

        worker.dispatch_events(vec![InputEvent::up(key)]);

        assert!(!simulated_keys.lock().unwrap().contains_key(&key));
    }
}
