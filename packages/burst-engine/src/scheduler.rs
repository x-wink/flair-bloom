use qzh_profile::key_id::KeyId;
use qzh_profile::profile::{BurstRule, MIN_EFFECTIVE_INTERVAL_MS};
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
    /// 每批 dispatch 的「单事件平均注入耗时」（ns）。这是下游背压唯一可观测的代理：
    /// LL hook 链 / RIT / 前台输入队列拥塞时 SendInput/驱动注入会变慢，此值随之上升。
    dispatch_cost_ns: Mutex<Vec<u128>>,
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

    fn record_dispatch_cost(&self, per_event: Duration) {
        self.dispatch_cost_ns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(per_event.as_nanos());
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
        let dispatch_cost = self
            .dispatch_cost_ns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        SchedulerStatsSnapshot {
            delay_ns: delays,
            dispatch_cost_ns: dispatch_cost,
            sent_events: self.sent_events.load(Ordering::Relaxed),
            failed_events: self.failed_events.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct SchedulerStatsSnapshot {
    pub delay_ns: Vec<u128>,
    pub dispatch_cost_ns: Vec<u128>,
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

/// 引擎依赖的调度器命令接口。抽成 trait 是为了在测试里注入「命令录制替身」，
/// 对「合成物理按键 → 引擎 → 调度命令（含 generation）」整条管线做确定性断言——
/// 真实 [`SchedulerHandle`] 跑在真线程 + waitable timer 上，无法直接确定性测命令时序。
pub(crate) trait Scheduler: Send + Sync {
    fn start_rule(&self, rule: Arc<BurstRule>, generation: u64);
    fn stop_rule(&self, rule_id: String, generation: u64);
    fn stop_all_async(&self, generation: u64);
    fn stop_all_blocking(&self, generation: u64) -> bool;
    fn shutdown_blocking(&self, generation: u64) -> bool;
    fn hp_degraded(&self) -> bool;
}

impl Scheduler for SchedulerHandle {
    fn start_rule(&self, rule: Arc<BurstRule>, generation: u64) {
        SchedulerHandle::start_rule(self, rule, generation);
    }
    fn stop_rule(&self, rule_id: String, generation: u64) {
        SchedulerHandle::stop_rule(self, rule_id, generation);
    }
    fn stop_all_async(&self, generation: u64) {
        SchedulerHandle::stop_all_async(self, generation);
    }
    fn stop_all_blocking(&self, generation: u64) -> bool {
        SchedulerHandle::stop_all_blocking(self, generation)
    }
    fn shutdown_blocking(&self, generation: u64) -> bool {
        SchedulerHandle::shutdown_blocking(self, generation)
    }
    fn hp_degraded(&self) -> bool {
        SchedulerHandle::hp_degraded(self)
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
        min_interval_ms: MIN_EFFECTIVE_INTERVAL_MS,
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
    /// 注入周期「基础」下限（生产取 [`MIN_EFFECTIVE_INTERVAL_MS`]，测试构造器设 1 不限速）。
    /// 每条规则的有效下限 = 此值 × 当前活跃规则数（总并发限速，详见 `process_due`）：管线可持续
    /// 的「总」注入速率有上限，多条规则同时连发按规则数等分，避免叠加冲破天花板导致「收不住」。
    /// 只拉长拍间间隔、不影响 `hold` 点按时长。
    min_interval_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcquireOutcome {
    Acquired,
    Shared,
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
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.rules.values().map(|r| r.next_at).min()
    }

    fn process_due(&mut self, now: Instant) {
        // 总并发限速：管线可持续的「总」注入速率有上限——单条规则没问题，多条同时连发会叠加
        // 冲破天花板（实测 2 toggle + 1 hold 即收不住）。故有效下限按活跃规则数等分：每条规则
        // 的下限 = 基础下限(min_interval_ms) × 活跃规则数，使总 tap 速率 ≈ 1000/基础下限、与规则
        // 数无关；单规则时退化为基础下限。effective interval = max(请求 interval, floor)，只拉长
        // 拍间间隔、不改 hold 点按手感，绝不快过规则请求，且 >=1ms 保证 next_at 单调推进。
        let current_generation = self.current_generation;
        let active_count = self
            .rules
            .values()
            .filter(|r| r.generation == current_generation)
            .count()
            .max(1) as u32;
        let floor_ms = self.min_interval_ms.saturating_mul(active_count);
        let stats = self.stats.as_deref();
        // 与 rules.iter_mut() 拆分借用，原地处理到期规则，避免每 tick 的 id 克隆与 remove/insert。
        let target_holds = &mut self.target_holds;

        let mut any_due = false;
        let mut events = Vec::new();
        let mut expired: Vec<String> = Vec::new();

        for (id, rule) in self.rules.iter_mut() {
            if rule.next_at > now {
                continue;
            }
            any_due = true;

            // 过期 generation 规则只做惰性释放（快速启停 / 切代后常见）。
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

            // 调度迟到采样，供 stress 诊断（不再驱动任何降频）。
            if let Some(stats) = stats {
                stats.record_delay(now.saturating_duration_since(rule.next_at));
            }

            // hold（按下时长）按规则请求的 interval 计算，硬下限不拉长按下、保持点按特性；
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

        // 测量本批注入 wall-clock（归一到单事件）供 stress 诊断；仅在采样时测量，不驱动降频。
        let event_count = events.len();
        let t0 = stats.map(|_| Instant::now());
        self.dispatch_events(events);
        if let (Some(stats), Some(t0)) = (stats, t0) {
            if event_count > 0 {
                let cost = Instant::now().saturating_duration_since(t0);
                stats.record_dispatch_cost(cost / event_count as u32);
            }
        }
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
mod sim_tests;

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
            min_interval_ms: 1,
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
