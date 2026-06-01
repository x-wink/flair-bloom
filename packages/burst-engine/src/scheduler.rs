use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use qzh_profile::key_id::KeyId;
#[cfg(windows)]
use tracing::{error, info};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0},
    System::Threading::{
        CreateEventW, CreateWaitableTimerExW, SetEvent, SetWaitableTimer, WaitForMultipleObjects,
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, SYNCHRONIZATION_SYNCHRONIZE,
        TIMER_MODIFY_STATE,
    },
};

use crate::safety::{emit_key_events, plan_key_down, plan_key_up, release_simulated_keys};
use crate::{
    metrics::EngineMetrics, revive, ActiveRules, KeyEvent, Metrics, PanelToggleCb, PhysicalKeys,
    SimulatedKeys, MAX_BURST_INTERVAL_MS, MIN_BURST_INTERVAL_MS,
};

#[derive(Clone)]
pub(crate) struct ScheduledRuleConfig {
    pub(crate) id: String,
    pub(crate) target_key: KeyId,
    pub(crate) interval_ms: u32,
    pub(crate) allow_while_physical_down: bool,
    pub(crate) stop_generation: u64,
}

pub(crate) enum SchedulerCommand {
    Start(ScheduledRuleConfig),
    Stop(String, Instant),
    StopAll {
        sent_at: Instant,
        ack: Option<Sender<()>>,
    },
    Shutdown,
}
#[derive(Clone)]
pub(crate) struct SchedulerWake {
    #[cfg(windows)]
    command_event: Option<Arc<WinHandle>>,
}

impl SchedulerWake {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(windows)]
            command_event: WinHandle::create_auto_reset_event().map(Arc::new),
        }
    }

    fn notify(&self) {
        #[cfg(windows)]
        if let Some(event) = &self.command_event {
            if !event.set() {
                error!("唤醒连发 scheduler 命令事件失败，命令可能延迟到下一次 timer 唤醒");
            }
        }
    }
}

pub(crate) struct SchedulerCommandSender {
    tx: Sender<SchedulerCommand>,
    wake: SchedulerWake,
}

impl SchedulerCommandSender {
    pub(crate) fn new(tx: Sender<SchedulerCommand>, wake: SchedulerWake) -> Self {
        Self { tx, wake }
    }

    pub(crate) fn send(
        &self,
        cmd: SchedulerCommand,
    ) -> Result<(), mpsc::SendError<SchedulerCommand>> {
        self.tx.send(cmd)?;
        self.wake.notify();
        Ok(())
    }
}

pub(crate) enum SchedulerWaitOutcome {
    Command(SchedulerCommand),
    Timeout,
    Disconnected,
}

pub(crate) struct SchedulerWaiter {
    #[cfg(windows)]
    high_precision: Option<HighPrecisionWaiter>,
    #[cfg(windows)]
    on_degraded: PanelToggleCb,
    #[cfg(windows)]
    hp_degraded: Arc<AtomicBool>,
}

impl SchedulerWaiter {
    pub(crate) fn new(
        wake: SchedulerWake,
        on_degraded: PanelToggleCb,
        hp_degraded: Arc<AtomicBool>,
    ) -> Self {
        #[cfg(windows)]
        {
            let high_precision = wake
                .command_event
                .clone()
                .and_then(HighPrecisionWaiter::new);
            if high_precision.is_some() {
                info!("连发 scheduler 启用 Windows 高精度 waitable timer");
            } else {
                hp_degraded.store(true, Ordering::SeqCst);
                info!("Windows 高精度 waitable timer 不可用，scheduler 降级标准等待路径");
            }
            Self {
                high_precision,
                on_degraded,
                hp_degraded,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = wake;
            let _ = (on_degraded, hp_degraded);
            Self {}
        }
    }

    pub(crate) fn wait(
        &mut self,
        rx: &Receiver<SchedulerCommand>,
        timeout: Option<Duration>,
    ) -> SchedulerWaitOutcome {
        #[cfg(windows)]
        if let Some(waiter) = &self.high_precision {
            match waiter.wait(rx, timeout) {
                Ok(outcome) => return outcome,
                Err(reason) => {
                    error!("Windows 高精度 scheduler 等待失败，降级标准等待路径: {reason}");
                    self.high_precision = None;
                    self.hp_degraded.store(true, Ordering::SeqCst);
                    if let Some(f) = revive(self.on_degraded.lock()).as_ref() {
                        f();
                    }
                }
            }
        }

        wait_standard(rx, timeout)
    }
}

pub(crate) fn wait_standard(
    rx: &Receiver<SchedulerCommand>,
    timeout: Option<Duration>,
) -> SchedulerWaitOutcome {
    match timeout {
        Some(timeout) => match rx.recv_timeout(timeout) {
            Ok(cmd) => SchedulerWaitOutcome::Command(cmd),
            Err(RecvTimeoutError::Timeout) => SchedulerWaitOutcome::Timeout,
            Err(RecvTimeoutError::Disconnected) => SchedulerWaitOutcome::Disconnected,
        },
        None => match rx.recv() {
            Ok(cmd) => SchedulerWaitOutcome::Command(cmd),
            Err(_) => SchedulerWaitOutcome::Disconnected,
        },
    }
}

#[cfg(windows)]
struct HighPrecisionWaiter {
    command_event: Arc<WinHandle>,
    timer: WinHandle,
}

#[cfg(windows)]
impl HighPrecisionWaiter {
    fn new(command_event: Arc<WinHandle>) -> Option<Self> {
        let timer = WinHandle::create_high_resolution_timer()?;
        Some(Self {
            command_event,
            timer,
        })
    }

    fn wait(
        &self,
        rx: &Receiver<SchedulerCommand>,
        timeout: Option<Duration>,
    ) -> Result<SchedulerWaitOutcome, &'static str> {
        let handles = [self.command_event.raw(), self.timer.raw()];
        let handle_count = if let Some(timeout) = timeout {
            if timeout.is_zero() {
                return Ok(SchedulerWaitOutcome::Timeout);
            }
            let due_time = duration_to_relative_100ns(timeout);
            // SAFETY: timer 是 CreateWaitableTimerExW 返回的有效句柄；due_time 指针在调用期间有效；
            // completion routine 为空，lparam 为空，设置一次性相对时间。
            let ok = unsafe {
                SetWaitableTimer(self.timer.raw(), &due_time, 0, None, std::ptr::null(), 0)
            };
            if ok == 0 {
                return Err("SetWaitableTimer");
            }
            handles.len() as u32
        } else {
            1
        };

        // SAFETY: handles 前 handle_count 个元素均为有效同步对象句柄；不等待全部对象；
        // INFINITE 只阻塞 scheduler 线程，命令 event 会唤醒它。
        let wait = unsafe { WaitForMultipleObjects(handle_count, handles.as_ptr(), 0, INFINITE) };
        match wait {
            WAIT_OBJECT_0 => match rx.try_recv() {
                Ok(cmd) => Ok(SchedulerWaitOutcome::Command(cmd)),
                Err(mpsc::TryRecvError::Empty) => Ok(SchedulerWaitOutcome::Timeout),
                Err(mpsc::TryRecvError::Disconnected) => Ok(SchedulerWaitOutcome::Disconnected),
            },
            value if value == WAIT_OBJECT_0 + 1 => Ok(SchedulerWaitOutcome::Timeout),
            WAIT_FAILED => Err("WaitForMultipleObjects"),
            _ => Err("WaitForMultipleObjects: unexpected result"),
        }
    }
}

#[cfg(windows)]
fn duration_to_relative_100ns(timeout: Duration) -> i64 {
    let ticks = timeout
        .as_nanos()
        .saturating_add(99)
        .saturating_div(100)
        .clamp(1, i64::MAX as u128) as i64;
    -ticks
}

#[cfg(windows)]
struct WinHandle {
    raw: HANDLE,
}

#[cfg(windows)]
unsafe impl Send for WinHandle {}

#[cfg(windows)]
unsafe impl Sync for WinHandle {}

#[cfg(windows)]
impl WinHandle {
    fn create_auto_reset_event() -> Option<Self> {
        // SAFETY: 安全属性和名称为空；auto-reset、初始未触发的匿名事件。
        let raw = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if raw.is_null() {
            info!("无法创建 scheduler 命令唤醒事件，高精度 timer 将不启用");
            return None;
        }
        Some(Self { raw })
    }

    fn create_high_resolution_timer() -> Option<Self> {
        // SAFETY: 安全属性和名称为空；创建匿名高精度 waitable timer。
        let raw = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
            )
        };
        if raw.is_null() {
            info!("Windows 高精度 waitable timer 不可用，scheduler 降级标准等待路径");
            return None;
        }
        Some(Self { raw })
    }

    fn raw(&self) -> HANDLE {
        self.raw
    }

    fn set(&self) -> bool {
        // SAFETY: raw 是 CreateEventW 返回的事件句柄；SetEvent 可跨线程唤醒等待者。
        unsafe { SetEvent(self.raw) != 0 }
    }
}

#[cfg(windows)]
impl Drop for WinHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: raw 由本对象拥有，Drop 只执行一次。
            unsafe { CloseHandle(self.raw) };
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PulsePhase {
    Down,
    Up,
}

pub(crate) struct ScheduledRule {
    config: ScheduledRuleConfig,
    hold_ms: u64,
    rest_ms: u64,
    phase: PulsePhase,
    next_at: Instant,
    is_down: bool,
}

#[derive(Default)]
pub(crate) struct TargetHold {
    pub(crate) owners: HashMap<String, bool>,
}

pub(crate) struct SchedulerContext<'a> {
    pub(crate) stop_all_generation: &'a AtomicU64,
    pub(crate) physical_keys: &'a PhysicalKeys,
    pub(crate) simulated_keys: &'a SimulatedKeys,
    pub(crate) active_rules: &'a ActiveRules,
    pub(crate) metrics: &'a EngineMetrics,
}
impl ScheduledRule {
    pub(crate) fn new(config: ScheduledRuleConfig, now: Instant) -> Self {
        debug_assert!((MIN_BURST_INTERVAL_MS..=MAX_BURST_INTERVAL_MS).contains(&config.interval_ms));
        let interval_ms = config.interval_ms as u64;
        let hold_ms = (interval_ms / 3)
            .clamp(5, 30)
            .min(interval_ms.saturating_sub(1));
        Self {
            config,
            hold_ms,
            rest_ms: interval_ms - hold_ms,
            phase: PulsePhase::Down,
            next_at: now,
            is_down: false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_scheduler(
    rx: Receiver<SchedulerCommand>,
    wake: SchedulerWake,
    hp_degraded: Arc<AtomicBool>,
    on_degraded: PanelToggleCb,
    stop_all_generation: Arc<AtomicU64>,
    physical_keys: PhysicalKeys,
    simulated_keys: SimulatedKeys,
    active_rules: ActiveRules,
    metrics: Metrics,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_scheduler(
                rx,
                wake,
                hp_degraded,
                on_degraded,
                &stop_all_generation,
                &physical_keys,
                &simulated_keys,
                &active_rules,
                &metrics,
            );
        }));
        if result.is_err() {
            revive(active_rules.lock()).clear();
            metrics.set_active_rules(0);
            release_simulated_keys(&physical_keys, &simulated_keys);
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn run_scheduler(
    rx: Receiver<SchedulerCommand>,
    wake: SchedulerWake,
    hp_degraded: Arc<AtomicBool>,
    on_degraded: PanelToggleCb,
    stop_all_generation: &AtomicU64,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    active_rules: &ActiveRules,
    metrics: &EngineMetrics,
) {
    let mut rules = HashMap::<String, ScheduledRule>::new();
    let mut target_holds = HashMap::<KeyId, TargetHold>::new();
    let mut waiter = SchedulerWaiter::new(wake, on_degraded, hp_degraded);
    let context = SchedulerContext {
        stop_all_generation,
        physical_keys,
        simulated_keys,
        active_rules,
        metrics,
    };
    loop {
        while let Ok(cmd) = rx.try_recv() {
            if handle_scheduler_command(cmd, &mut rules, &mut target_holds, &context) {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    context.physical_keys,
                    context.simulated_keys,
                    context.active_rules,
                    true,
                );
                return;
            }
        }

        let mut events = Vec::new();
        step_due_rules(
            &mut rules,
            &mut target_holds,
            context.physical_keys,
            context.simulated_keys,
            context.metrics,
            &mut events,
        );
        emit_key_events(&events);
        context.metrics.add_injected_events(events.len());

        let timeout = next_scheduler_timeout(&rules);
        let command = match waiter.wait(&rx, timeout) {
            SchedulerWaitOutcome::Command(cmd) => Some(cmd),
            SchedulerWaitOutcome::Timeout => None,
            SchedulerWaitOutcome::Disconnected => {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    context.physical_keys,
                    context.simulated_keys,
                    context.active_rules,
                    true,
                );
                return;
            }
        };

        if let Some(cmd) = command {
            if handle_scheduler_command(cmd, &mut rules, &mut target_holds, &context) {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    context.physical_keys,
                    context.simulated_keys,
                    context.active_rules,
                    true,
                );
                return;
            }
        }
    }
}

pub(crate) fn handle_scheduler_command(
    cmd: SchedulerCommand,
    rules: &mut HashMap<String, ScheduledRule>,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    context: &SchedulerContext<'_>,
) -> bool {
    match cmd {
        SchedulerCommand::Start(config) => {
            if config.stop_generation != context.stop_all_generation.load(Ordering::SeqCst) {
                return false;
            }
            rules.insert(
                config.id.clone(),
                ScheduledRule::new(config, Instant::now()),
            );
            false
        }
        SchedulerCommand::Stop(rule_id, sent_at) => {
            context.metrics.record_stop_response(sent_at.elapsed());
            if let Some(rule) = rules.remove(&rule_id) {
                stop_scheduled_rule(
                    rule,
                    target_holds,
                    context.physical_keys,
                    context.simulated_keys,
                );
            }
            false
        }
        SchedulerCommand::StopAll { sent_at, ack } => {
            context.metrics.record_stop_response(sent_at.elapsed());
            cleanup_scheduler_rules(
                rules,
                target_holds,
                context.physical_keys,
                context.simulated_keys,
                context.active_rules,
                false,
            );
            if let Some(ack) = ack {
                let _ = ack.send(());
            }
            false
        }
        SchedulerCommand::Shutdown => true,
    }
}

fn stop_scheduled_rule(
    rule: ScheduledRule,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
) {
    if rule.is_down {
        let mut events = Vec::new();
        release_target_owner(
            &rule.config.id,
            rule.config.target_key,
            target_holds,
            physical_keys,
            simulated_keys,
            rule.config.allow_while_physical_down,
            &mut events,
        );
        emit_key_events(&events);
    }
}

fn cleanup_scheduler_rules(
    rules: &mut HashMap<String, ScheduledRule>,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    active_rules: &ActiveRules,
    clear_active_rules: bool,
) {
    let mut events = Vec::new();
    for (_, rule) in rules.drain() {
        if rule.is_down {
            release_target_owner(
                &rule.config.id,
                rule.config.target_key,
                target_holds,
                physical_keys,
                simulated_keys,
                rule.config.allow_while_physical_down,
                &mut events,
            );
        }
    }
    release_all_target_holds(target_holds, physical_keys, simulated_keys, &mut events);
    emit_key_events(&events);
    if clear_active_rules {
        revive(active_rules.lock()).clear();
    }
    release_simulated_keys(physical_keys, simulated_keys);
}

fn acquire_target_owner(
    rule_id: &str,
    target_key: KeyId,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
    events: &mut Vec<KeyEvent>,
) -> bool {
    if let Some(hold) = target_holds.get_mut(&target_key) {
        hold.owners
            .insert(rule_id.to_string(), allow_while_physical_down);
        return true;
    }

    if !plan_key_down(
        target_key,
        physical_keys,
        simulated_keys,
        allow_while_physical_down,
        events,
    ) {
        return false;
    }

    let mut hold = TargetHold::default();
    hold.owners
        .insert(rule_id.to_string(), allow_while_physical_down);
    target_holds.insert(target_key, hold);
    true
}

fn release_target_owner(
    rule_id: &str,
    target_key: KeyId,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
    events: &mut Vec<KeyEvent>,
) {
    let release_allows_physical_down = {
        let Some(hold) = target_holds.get_mut(&target_key) else {
            return;
        };
        let release_allows_physical_down = hold
            .owners
            .remove(rule_id)
            .unwrap_or(allow_while_physical_down);
        if !hold.owners.is_empty() {
            return;
        }
        release_allows_physical_down
    };
    target_holds.remove(&target_key);
    if let Some(event) = plan_key_up(
        target_key,
        physical_keys,
        simulated_keys,
        release_allows_physical_down,
    ) {
        events.push(event);
    }
}

fn release_all_target_holds(
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    events: &mut Vec<KeyEvent>,
) {
    let holds: Vec<_> = target_holds
        .drain()
        .map(|(target_key, hold)| {
            let allow = hold.owners.into_values().any(|v| v);
            (target_key, allow)
        })
        .collect();
    for (target_key, allow_while_physical_down) in holds {
        if let Some(event) = plan_key_up(
            target_key,
            physical_keys,
            simulated_keys,
            allow_while_physical_down,
        ) {
            events.push(event);
        }
    }
}

pub(crate) fn step_due_rules(
    rules: &mut HashMap<String, ScheduledRule>,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    metrics: &EngineMetrics,
    events: &mut Vec<KeyEvent>,
) {
    let now = Instant::now();
    let due_ids: Vec<_> = rules
        .iter()
        .filter(|(_, rule)| rule.next_at <= now)
        .map(|(id, _)| id.clone())
        .collect();

    for id in due_ids {
        let Some(rule) = rules.get_mut(&id) else {
            continue;
        };
        metrics.add_scheduler_step();
        metrics.record_delay(now.saturating_duration_since(rule.next_at));
        match rule.phase {
            PulsePhase::Down => {
                if acquire_target_owner(
                    &rule.config.id,
                    rule.config.target_key,
                    target_holds,
                    physical_keys,
                    simulated_keys,
                    rule.config.allow_while_physical_down,
                    events,
                ) {
                    rule.is_down = true;
                    rule.phase = PulsePhase::Up;
                    rule.next_at = now + Duration::from_millis(rule.hold_ms);
                } else {
                    metrics.add_skipped_pulse();
                    rule.next_at = now + Duration::from_millis(rule.config.interval_ms as u64);
                }
            }
            PulsePhase::Up => {
                if rule.is_down {
                    release_target_owner(
                        &rule.config.id,
                        rule.config.target_key,
                        target_holds,
                        physical_keys,
                        simulated_keys,
                        rule.config.allow_while_physical_down,
                        events,
                    );
                    rule.is_down = false;
                }
                rule.phase = PulsePhase::Down;
                rule.next_at = now + Duration::from_millis(rule.rest_ms);
            }
        }
    }
}

fn next_scheduler_timeout(rules: &HashMap<String, ScheduledRule>) -> Option<Duration> {
    let now = Instant::now();
    rules
        .values()
        .map(|rule| rule.next_at.saturating_duration_since(now))
        .min()
}
