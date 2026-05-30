use qzh_profile::key_id::KeyId;
#[cfg(any(test, windows))]
use qzh_profile::key_id::MouseButton;
use qzh_profile::profile::{BurstMode, BurstRule, Hotkeys};
#[cfg(windows)]
use std::sync::{atomic::AtomicU32, RwLock, Weak};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::AtomicU8,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
use tracing::error;
use tracing::info;
use win_input::key_events;
#[cfg(windows)]
use win_input::{clear_pending_injections, try_consume_injection, SIM_MARKER};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LPARAM, WAIT_FAILED, WAIT_OBJECT_0, WPARAM},
    System::Threading::{
        CreateEventW, CreateWaitableTimerExW, GetCurrentThreadId, SetEvent, SetWaitableTimer,
        WaitForMultipleObjects, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE,
        SYNCHRONIZATION_SYNCHRONIZE, TIMER_MODIFY_STATE,
    },
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
        WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
    },
};

/// 重装键盘 hook 的自定义线程消息：面板获得焦点时，由主线程投递给 hook 线程，
/// 触发 unhook + rehook 使我们的 hook 重新排到 Chromium hook 之后安装，即优先被调用。
#[cfg(windows)]
const WM_REHOOK_KEYBOARD: u32 = WM_USER + 1;

/// hook 线程 ID，用于跨线程投递 WM_REHOOK_KEYBOARD 消息；0 表示线程尚未启动。
#[cfg(windows)]
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// hook 回调通过静态 Weak 引用访问引擎，避免 Arc 延长生命周期；RwLock 支持重复注册
#[cfg(windows)]
static ENGINE_HOOK: RwLock<Option<Weak<BurstEngine>>> = RwLock::new(None);

/// 向 hook 线程投递重装键盘 hook 的信号。
/// 在面板窗口获得焦点后调用，使我们的 hook 重新排到 Chromium 安装的 hook 之后（即优先执行）。
#[cfg(windows)]
pub fn rehook_keyboard() {
    let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        // SAFETY: tid 来自 hook 线程自身写入的有效线程 ID；消息参数均为 0，合法。
        unsafe { PostThreadMessageW(tid, WM_REHOOK_KEYBOARD, 0, 0) };
    }
}
#[cfg(not(windows))]
pub fn rehook_keyboard() {}

type GlobalChangedCb = Arc<Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>>>;
type PanelToggleCb = Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>;
type PhysicalKeys = Arc<Mutex<HashSet<KeyId>>>;
type SimulatedKeys = Arc<Mutex<HashMap<KeyId, usize>>>;
type ActiveRules = Arc<Mutex<HashSet<String>>>;
type KeyEvent = (KeyId, bool);
type Metrics = Arc<EngineMetrics>;
const DELAY_SAMPLE_LIMIT: usize = 4096;

#[derive(Clone)]
struct ScheduledRuleConfig {
    id: String,
    target_key: KeyId,
    interval_ms: u32,
    allow_while_physical_down: bool,
}

enum SchedulerCommand {
    Start(ScheduledRuleConfig),
    Stop(String, Instant),
    StopAll(Instant),
    SetWaitMode(SchedulerWaitMode),
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchedulerWaitMode {
    #[default]
    Standard,
    HighPrecision,
}

impl SchedulerWaitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::HighPrecision => "high_precision",
        }
    }
}

impl std::str::FromStr for SchedulerWaitMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "standard" => Ok(Self::Standard),
            "high_precision" => Ok(Self::HighPrecision),
            _ => Err(()),
        }
    }
}

fn wait_mode_to_u8(mode: SchedulerWaitMode) -> u8 {
    match mode {
        SchedulerWaitMode::Standard => 0,
        SchedulerWaitMode::HighPrecision => 1,
    }
}

fn wait_mode_from_u8(value: u8) -> SchedulerWaitMode {
    match value {
        1 => SchedulerWaitMode::HighPrecision,
        _ => SchedulerWaitMode::Standard,
    }
}

#[derive(Clone)]
struct SchedulerWake {
    #[cfg(windows)]
    command_event: Option<Arc<WinHandle>>,
}

impl SchedulerWake {
    fn new() -> Self {
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

struct SchedulerCommandSender {
    tx: Sender<SchedulerCommand>,
    wake: SchedulerWake,
}

impl SchedulerCommandSender {
    fn new(tx: Sender<SchedulerCommand>, wake: SchedulerWake) -> Self {
        Self { tx, wake }
    }

    fn send(&self, cmd: SchedulerCommand) -> Result<(), mpsc::SendError<SchedulerCommand>> {
        self.tx.send(cmd)?;
        self.wake.notify();
        Ok(())
    }
}

enum SchedulerWaitOutcome {
    Command(SchedulerCommand),
    Timeout,
    Disconnected,
}

struct SchedulerWaiter {
    mode: SchedulerWaitMode,
    #[cfg(windows)]
    high_precision: Option<HighPrecisionWaiter>,
}

impl SchedulerWaiter {
    fn new(wake: SchedulerWake, mode: SchedulerWaitMode) -> Self {
        #[cfg(windows)]
        {
            let high_precision = match mode {
                SchedulerWaitMode::Standard => None,
                SchedulerWaitMode::HighPrecision => Self::create_high_precision(&wake),
            };
            Self {
                mode,
                high_precision,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = wake;
            Self { mode }
        }
    }

    fn set_mode(&mut self, wake: &SchedulerWake, mode: SchedulerWaitMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        #[cfg(windows)]
        {
            self.high_precision = match mode {
                SchedulerWaitMode::Standard => {
                    info!("连发 scheduler 已切换为标准等待");
                    None
                }
                SchedulerWaitMode::HighPrecision => Self::create_high_precision(wake),
            };
        }
        #[cfg(not(windows))]
        {
            let _ = wake;
        }
    }

    fn wait(
        &mut self,
        rx: &Receiver<SchedulerCommand>,
        timeout: Option<Duration>,
    ) -> SchedulerWaitOutcome {
        #[cfg(windows)]
        if self.mode == SchedulerWaitMode::HighPrecision {
            let Some(waiter) = &self.high_precision else {
                return wait_standard(rx, timeout);
            };
            match waiter.wait(rx, timeout) {
                Ok(outcome) => return outcome,
                Err(reason) => {
                    error!("Windows 高精度 scheduler 等待失败，降级标准等待路径: {reason}");
                    self.high_precision = None;
                }
            }
        }

        wait_standard(rx, timeout)
    }

    #[cfg(windows)]
    fn create_high_precision(wake: &SchedulerWake) -> Option<HighPrecisionWaiter> {
        let high_precision = wake
            .command_event
            .clone()
            .and_then(HighPrecisionWaiter::new);
        if high_precision.is_some() {
            info!("连发 scheduler 已切换为 Windows 高精度 waitable timer");
        } else {
            info!("Windows 高精度 waitable timer 不可用，scheduler 保持标准等待路径");
        }
        high_precision
    }
}

fn wait_standard(
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

struct ScheduledRule {
    config: ScheduledRuleConfig,
    hold_ms: u64,
    rest_ms: u64,
    phase: PulsePhase,
    next_at: Instant,
    is_down: bool,
}

#[derive(Default)]
struct TargetHold {
    owners: HashMap<String, bool>,
}

struct EngineMetrics {
    started_at: Instant,
    active_rules: AtomicUsize,
    injected_events: AtomicU64,
    scheduler_steps: AtomicU64,
    skipped_pulses: AtomicU64,
    stop_commands: AtomicU64,
    delay_samples_us: Mutex<VecDeque<u64>>,
    hook_samples_us: Mutex<VecDeque<u64>>,
    stop_response_samples_us: Mutex<VecDeque<u64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineMetricsSnapshot {
    pub active_rules: usize,
    pub injected_events: u64,
    pub injection_rate_per_sec: f64,
    pub scheduler_steps: u64,
    pub skipped_pulses: u64,
    pub stop_commands: u64,
    pub delay_sample_count: usize,
    pub delay_p50_us: u64,
    pub delay_p95_us: u64,
    pub delay_p99_us: u64,
    pub delay_max_us: u64,
    pub hook_sample_count: usize,
    pub hook_p50_us: u64,
    pub hook_p95_us: u64,
    pub hook_p99_us: u64,
    pub hook_max_us: u64,
    pub stop_response_sample_count: usize,
    pub stop_response_p50_us: u64,
    pub stop_response_p95_us: u64,
    pub stop_response_p99_us: u64,
    pub stop_response_max_us: u64,
}

impl EngineMetrics {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            active_rules: AtomicUsize::new(0),
            injected_events: AtomicU64::new(0),
            scheduler_steps: AtomicU64::new(0),
            skipped_pulses: AtomicU64::new(0),
            stop_commands: AtomicU64::new(0),
            delay_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
            hook_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
            stop_response_samples_us: Mutex::new(VecDeque::with_capacity(DELAY_SAMPLE_LIMIT)),
        }
    }

    fn set_active_rules(&self, count: usize) {
        self.active_rules.store(count, Ordering::Relaxed);
    }

    fn add_injected_events(&self, count: usize) {
        self.injected_events
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    fn add_scheduler_step(&self) {
        self.scheduler_steps.fetch_add(1, Ordering::Relaxed);
    }

    fn add_skipped_pulse(&self) {
        self.skipped_pulses.fetch_add(1, Ordering::Relaxed);
    }

    fn add_stop_command(&self) {
        self.stop_commands.fetch_add(1, Ordering::Relaxed);
    }

    fn record_delay(&self, delay: Duration) {
        push_duration_sample(&self.delay_samples_us, delay);
    }

    #[cfg(windows)]
    fn record_hook_callback(&self, delay: Duration) {
        push_duration_sample(&self.hook_samples_us, delay);
    }

    fn record_stop_response(&self, delay: Duration) {
        push_duration_sample(&self.stop_response_samples_us, delay);
    }

    fn snapshot(&self) -> EngineMetricsSnapshot {
        let delay_samples = sorted_samples(&self.delay_samples_us);
        let hook_samples = sorted_samples(&self.hook_samples_us);
        let stop_response_samples = sorted_samples(&self.stop_response_samples_us);
        let injected_events = self.injected_events.load(Ordering::Relaxed);
        let elapsed = self.started_at.elapsed().as_secs_f64().max(0.001);
        EngineMetricsSnapshot {
            active_rules: self.active_rules.load(Ordering::Relaxed),
            injected_events,
            injection_rate_per_sec: injected_events as f64 / elapsed,
            scheduler_steps: self.scheduler_steps.load(Ordering::Relaxed),
            skipped_pulses: self.skipped_pulses.load(Ordering::Relaxed),
            stop_commands: self.stop_commands.load(Ordering::Relaxed),
            delay_sample_count: delay_samples.len(),
            delay_p50_us: percentile(&delay_samples, 50),
            delay_p95_us: percentile(&delay_samples, 95),
            delay_p99_us: percentile(&delay_samples, 99),
            delay_max_us: delay_samples.last().copied().unwrap_or(0),
            hook_sample_count: hook_samples.len(),
            hook_p50_us: percentile(&hook_samples, 50),
            hook_p95_us: percentile(&hook_samples, 95),
            hook_p99_us: percentile(&hook_samples, 99),
            hook_max_us: hook_samples.last().copied().unwrap_or(0),
            stop_response_sample_count: stop_response_samples.len(),
            stop_response_p50_us: percentile(&stop_response_samples, 50),
            stop_response_p95_us: percentile(&stop_response_samples, 95),
            stop_response_p99_us: percentile(&stop_response_samples, 99),
            stop_response_max_us: stop_response_samples.last().copied().unwrap_or(0),
        }
    }
}

fn push_duration_sample(samples: &Mutex<VecDeque<u64>>, duration: Duration) {
    let delay_us = duration.as_micros().min(u128::from(u64::MAX)) as u64;
    let mut samples = revive(samples.lock());
    if samples.len() >= DELAY_SAMPLE_LIMIT {
        samples.pop_front();
    }
    samples.push_back(delay_us);
}

fn sorted_samples(samples: &Mutex<VecDeque<u64>>) -> Vec<u64> {
    let mut samples: Vec<_> = revive(samples.lock()).iter().copied().collect();
    samples.sort_unstable();
    samples
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let last = samples.len() - 1;
    let idx = (last * percentile).div_ceil(100);
    samples[idx.min(last)]
}

#[derive(Default)]
struct RuleSnapshot {
    rules: Vec<BurstRule>,
    press_index: HashMap<KeyId, Vec<usize>>,
    hold_release_index: HashMap<KeyId, Vec<usize>>,
}

impl RuleSnapshot {
    fn new(rules: Vec<BurstRule>) -> Self {
        let mut snapshot = Self {
            rules,
            press_index: HashMap::new(),
            hold_release_index: HashMap::new(),
        };
        for (idx, rule) in snapshot.rules.iter().enumerate() {
            if !rule.enabled {
                continue;
            }
            match rule.mode {
                BurstMode::Hold => {
                    push_rule_index(&mut snapshot.press_index, rule.trigger_key, idx);
                    push_rule_index(&mut snapshot.hold_release_index, rule.trigger_key, idx);
                }
                BurstMode::Toggle => {
                    push_rule_index(&mut snapshot.press_index, rule.trigger_key, idx);
                    let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                    if stop != rule.trigger_key {
                        push_rule_index(&mut snapshot.press_index, stop, idx);
                    }
                }
            }
        }
        snapshot
    }
}

fn push_rule_index(index: &mut HashMap<KeyId, Vec<usize>>, key: KeyId, rule_idx: usize) {
    index.entry(key).or_default().push(rule_idx);
}

fn physical_key_down(physical_keys: &PhysicalKeys, key: KeyId) -> bool {
    revive(physical_keys.lock()).contains(&key)
}

fn record_simulated_down(simulated_keys: &SimulatedKeys, key: KeyId) {
    let mut keys = revive(simulated_keys.lock());
    *keys.entry(key).or_default() += 1;
}

fn record_simulated_up(simulated_keys: &SimulatedKeys, key: KeyId) -> bool {
    let mut keys = revive(simulated_keys.lock());
    let Some(count) = keys.get_mut(&key) else {
        return false;
    };
    if *count <= 1 {
        keys.remove(&key);
    } else {
        *count -= 1;
    }
    true
}

fn plan_key_down(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
    events: &mut Vec<KeyEvent>,
) -> bool {
    if !allow_while_physical_down && physical_key_down(physical_keys, key) {
        return false;
    }
    record_simulated_down(simulated_keys, key);
    events.push((key, false));
    true
}

fn plan_key_up(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) -> Option<KeyEvent> {
    if !record_simulated_up(simulated_keys, key) {
        return None;
    }
    if allow_while_physical_down || !physical_key_down(physical_keys, key) {
        Some((key, true))
    } else {
        None
    }
}

fn emit_key_events(events: &[KeyEvent]) {
    if !events.is_empty() {
        key_events(events);
    }
}

#[cfg(test)]
fn safe_key_down(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) -> bool {
    let mut events = Vec::new();
    let started = plan_key_down(
        key,
        physical_keys,
        simulated_keys,
        allow_while_physical_down,
        &mut events,
    );
    emit_key_events(&events);
    started
}

#[cfg(test)]
fn safe_key_up(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) {
    if let Some(event) = plan_key_up(
        key,
        physical_keys,
        simulated_keys,
        allow_while_physical_down,
    ) {
        emit_key_events(&[event]);
    }
}

fn release_simulated_key(key: KeyId, physical_keys: &PhysicalKeys, simulated_keys: &SimulatedKeys) {
    let was_down = revive(simulated_keys.lock()).remove(&key).is_some();
    if was_down && !physical_key_down(physical_keys, key) {
        emit_key_events(&[(key, true)]);
    }
}

fn release_simulated_keys(physical_keys: &PhysicalKeys, simulated_keys: &SimulatedKeys) {
    let keys: Vec<_> = revive(simulated_keys.lock())
        .drain()
        .map(|(key, _)| key)
        .collect();
    let mut events = Vec::new();
    for key in keys {
        if !physical_key_down(physical_keys, key) {
            events.push((key, true));
        }
    }
    emit_key_events(&events);
}

impl ScheduledRule {
    fn new(config: ScheduledRuleConfig, now: Instant) -> Self {
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

fn spawn_scheduler(
    rx: Receiver<SchedulerCommand>,
    wake: SchedulerWake,
    wait_mode: Arc<AtomicU8>,
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
                wait_mode,
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

fn run_scheduler(
    rx: Receiver<SchedulerCommand>,
    wake: SchedulerWake,
    wait_mode: Arc<AtomicU8>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    active_rules: &ActiveRules,
    metrics: &EngineMetrics,
) {
    let mut rules = HashMap::<String, ScheduledRule>::new();
    let mut target_holds = HashMap::<KeyId, TargetHold>::new();
    let mut waiter = SchedulerWaiter::new(
        wake.clone(),
        wait_mode_from_u8(wait_mode.load(Ordering::Relaxed)),
    );
    loop {
        while let Ok(cmd) = rx.try_recv() {
            if handle_scheduler_command(
                cmd,
                &wake,
                &mut waiter,
                &wait_mode,
                &mut rules,
                &mut target_holds,
                physical_keys,
                simulated_keys,
                active_rules,
                metrics,
            ) {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    physical_keys,
                    simulated_keys,
                    active_rules,
                    true,
                );
                return;
            }
        }

        let mut events = Vec::new();
        step_due_rules(
            &mut rules,
            &mut target_holds,
            physical_keys,
            simulated_keys,
            metrics,
            &mut events,
        );
        emit_key_events(&events);
        metrics.add_injected_events(events.len());

        let timeout = next_scheduler_timeout(&rules);
        let command = match waiter.wait(&rx, timeout) {
            SchedulerWaitOutcome::Command(cmd) => Some(cmd),
            SchedulerWaitOutcome::Timeout => None,
            SchedulerWaitOutcome::Disconnected => {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    physical_keys,
                    simulated_keys,
                    active_rules,
                    true,
                );
                return;
            }
        };

        if let Some(cmd) = command {
            if handle_scheduler_command(
                cmd,
                &wake,
                &mut waiter,
                &wait_mode,
                &mut rules,
                &mut target_holds,
                physical_keys,
                simulated_keys,
                active_rules,
                metrics,
            ) {
                cleanup_scheduler_rules(
                    &mut rules,
                    &mut target_holds,
                    physical_keys,
                    simulated_keys,
                    active_rules,
                    true,
                );
                return;
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "scheduler 命令处理需要同时更新等待器、规则表、目标持有账本和安全兜底状态"
)]
fn handle_scheduler_command(
    cmd: SchedulerCommand,
    wake: &SchedulerWake,
    waiter: &mut SchedulerWaiter,
    wait_mode: &AtomicU8,
    rules: &mut HashMap<String, ScheduledRule>,
    target_holds: &mut HashMap<KeyId, TargetHold>,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    active_rules: &ActiveRules,
    metrics: &EngineMetrics,
) -> bool {
    match cmd {
        SchedulerCommand::Start(config) => {
            rules.insert(
                config.id.clone(),
                ScheduledRule::new(config, Instant::now()),
            );
            false
        }
        SchedulerCommand::Stop(rule_id, sent_at) => {
            metrics.record_stop_response(sent_at.elapsed());
            if let Some(rule) = rules.remove(&rule_id) {
                stop_scheduled_rule(rule, target_holds, physical_keys, simulated_keys);
            }
            false
        }
        SchedulerCommand::StopAll(sent_at) => {
            metrics.record_stop_response(sent_at.elapsed());
            cleanup_scheduler_rules(
                rules,
                target_holds,
                physical_keys,
                simulated_keys,
                active_rules,
                false,
            );
            false
        }
        SchedulerCommand::SetWaitMode(mode) => {
            wait_mode.store(wait_mode_to_u8(mode), Ordering::Relaxed);
            waiter.set_mode(wake, mode);
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

fn step_due_rules(
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

/// 关键路径锁兜底：若前一持锁者 panic 导致 Mutex 中毒,强行复活并继续。
/// 按键工具最差故障是键盘卡死,值得给一层硬兜底；连发线程已被 catch_unwind
/// 包裹,持锁期间也仅做 HashMap/Vec 操作,正常路径不会中毒。
fn revive<T>(r: std::sync::LockResult<T>) -> T {
    r.unwrap_or_else(|e| e.into_inner())
}

pub struct BurstEngine {
    pub global_enabled: Arc<AtomicBool>,
    rules: Arc<Mutex<Arc<RuleSnapshot>>>,
    active_rules: ActiveRules,
    scheduler_tx: SchedulerCommandSender,
    scheduler_handle: Option<thread::JoinHandle<()>>,
    scheduler_wait_mode: Arc<AtomicU8>,
    metrics: Metrics,
    toggle_states: Arc<Mutex<HashMap<String, bool>>>,
    /// 当前物理按下的键；用于过滤 OS 生成的 key-repeat。
    pressed_keys: PhysicalKeys,
    /// 应用确认由自身模拟按下、尚未配对释放的键；异常停止时用于兜底释放。
    simulated_keys: SimulatedKeys,
    hotkeys: Arc<Mutex<Hotkeys>>,
    /// 全局开关状态被热键改变时调用（由 app 层注册，用于同步托盘与前端）。
    on_global_changed: GlobalChangedCb,
    /// 面板显隐热键触发时调用。
    on_panel_toggle: PanelToggleCb,
}

impl Default for BurstEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BurstEngine {
    pub fn new() -> Self {
        let pressed_keys = Arc::new(Mutex::new(HashSet::new()));
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));
        let active_rules = Arc::new(Mutex::new(HashSet::new()));
        let metrics = Arc::new(EngineMetrics::new());
        let (scheduler_tx, scheduler_rx) = mpsc::channel();
        let scheduler_wake = SchedulerWake::new();
        let scheduler_wait_mode =
            Arc::new(AtomicU8::new(wait_mode_to_u8(SchedulerWaitMode::default())));
        let scheduler_tx = SchedulerCommandSender::new(scheduler_tx, scheduler_wake.clone());
        let scheduler_handle = Some(spawn_scheduler(
            scheduler_rx,
            scheduler_wake,
            scheduler_wait_mode.clone(),
            pressed_keys.clone(),
            simulated_keys.clone(),
            active_rules.clone(),
            metrics.clone(),
        ));
        Self {
            global_enabled: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(Mutex::new(Arc::new(RuleSnapshot::default()))),
            active_rules,
            scheduler_tx,
            scheduler_handle,
            scheduler_wait_mode,
            metrics,
            toggle_states: Arc::new(Mutex::new(HashMap::new())),
            pressed_keys,
            simulated_keys,
            hotkeys: Arc::new(Mutex::new(Hotkeys::default())),
            on_global_changed: Arc::new(Mutex::new(None)),
            on_panel_toggle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_hotkeys(&self, hotkeys: Hotkeys) {
        *revive(self.hotkeys.lock()) = hotkeys;
    }

    /// 取消所有正在运行的连发循环并清空 toggle 状态。
    /// 在全局开关关闭时调用，防止连发线程继续注入按键。
    pub fn cancel_all_loops(&self) {
        revive(self.active_rules.lock()).clear();
        self.metrics.set_active_rules(0);
        self.metrics.add_stop_command();
        if self
            .scheduler_tx
            .send(SchedulerCommand::StopAll(Instant::now()))
            .is_err()
        {
            // The scheduler owns planned key-down/up ordering. Only fall back here if it is gone.
            release_simulated_keys(&self.pressed_keys, &self.simulated_keys);
        }
        revive(self.toggle_states.lock()).clear();
        #[cfg(windows)]
        clear_pending_injections();
    }

    /// 注册全局开关热键触发时的回调（供 app 层同步托盘与前端事件）。
    pub fn set_on_global_changed(&self, f: impl Fn(bool) + Send + Sync + 'static) {
        *revive(self.on_global_changed.lock()) = Some(Box::new(f));
    }

    /// 注册面板显隐热键触发时的回调。
    pub fn set_on_panel_toggle(&self, f: impl Fn() + Send + Sync + 'static) {
        *revive(self.on_panel_toggle.lock()) = Some(Box::new(f));
    }

    pub fn set_rules(&self, rules: Vec<BurstRule>) {
        let snapshot = Arc::new(RuleSnapshot::new(rules));
        let mut current = revive(self.rules.lock());
        // 持有规则锁期间取消旧循环，避免 cancel 后、快照替换前又按旧规则启动。
        self.cancel_all_loops();
        *current = snapshot;
    }

    pub fn get_rules(&self) -> Vec<BurstRule> {
        revive(self.rules.lock()).rules.clone()
    }

    /// 当前正在执行连发的规则 ID 集合：hold 模式表示触发键被按住，toggle 模式表示已开启。
    /// 用于前端轮询展示激活态视觉反馈。
    pub fn get_active_ids(&self) -> Vec<String> {
        revive(self.active_rules.lock()).iter().cloned().collect()
    }

    pub fn metrics_snapshot(&self) -> EngineMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn scheduler_wait_mode(&self) -> SchedulerWaitMode {
        wait_mode_from_u8(self.scheduler_wait_mode.load(Ordering::Relaxed))
    }

    pub fn set_scheduler_wait_mode(&self, mode: SchedulerWaitMode) {
        self.scheduler_wait_mode
            .store(wait_mode_to_u8(mode), Ordering::Relaxed);
        let _ = self.scheduler_tx.send(SchedulerCommand::SetWaitMode(mode));
    }

    #[cfg(windows)]
    fn record_hook_callback(&self, started_at: Instant) {
        self.metrics.record_hook_callback(started_at.elapsed());
    }

    /// 返回 true 表示引擎处理了本次按键（热键触发或规则匹配），false 表示未匹配或重复按下。
    /// 供中继调用方决定是否 preventDefault。
    pub fn on_key_press(&self, key: KeyId) -> bool {
        // 低级键盘 hook 没有可靠的 key-repeat 标志。用按下集合识别首次 down，
        // 避免长按全局热键时重复切换开关，也避免依赖 KBDLLHOOKSTRUCT.flags 保留位。
        if !revive(self.pressed_keys.lock()).insert(key) {
            return false;
        }

        // 全局热键检测：优先于规则处理，且不受 global_enabled 当前状态限制
        {
            let hk = revive(self.hotkeys.lock());
            let start = hk.global_toggle;
            let stop = hk.global_stop.or(start); // None 时停止键 = 开启键（切换模式）
            let panel = hk.panel_toggle;
            let enabled = self.global_enabled.load(Ordering::SeqCst);

            if panel == Some(key) {
                drop(hk);
                if let Some(cb) = revive(self.on_panel_toggle.lock()).as_ref() {
                    cb();
                }
                return true;
            }
            if start == Some(key) && !enabled {
                drop(hk);
                self.global_enabled.store(true, Ordering::SeqCst);
                if let Some(cb) = revive(self.on_global_changed.lock()).as_ref() {
                    cb(true);
                }
                return true;
            }
            if stop == Some(key) && enabled {
                drop(hk);
                self.global_enabled.store(false, Ordering::SeqCst);
                self.cancel_all_loops();
                if let Some(cb) = revive(self.on_global_changed.lock()).as_ref() {
                    cb(false);
                }
                return true;
            }
        }

        if !self.global_enabled.load(Ordering::SeqCst) {
            return false;
        }
        let mut handled = false;
        let rules = revive(self.rules.lock()).clone();
        let Some(indices) = rules.press_index.get(&key) else {
            return false;
        };
        for &idx in indices {
            let rule = &rules.rules[idx];
            match rule.mode {
                BurstMode::Hold => {
                    if rule.trigger_key == key {
                        self.start_hold_burst(rule);
                        handled = true;
                    }
                }
                BurstMode::Toggle => {
                    let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                    if rule.trigger_key == key || stop == key {
                        let started = revive(self.toggle_states.lock())
                            .get(&rule.id)
                            .copied()
                            .unwrap_or(false);
                        if started {
                            if stop == key {
                                self.handle_toggle_press(rule);
                                handled = true;
                            }
                        } else if rule.trigger_key == key {
                            self.handle_toggle_press(rule);
                            handled = true;
                        }
                    }
                }
            }
        }
        handled
    }

    pub fn on_key_release(&self, key: KeyId) {
        revive(self.pressed_keys.lock()).remove(&key);

        let rules = revive(self.rules.lock()).clone();
        let Some(indices) = rules.hold_release_index.get(&key) else {
            return;
        };
        for &idx in indices {
            let rule = &rules.rules[idx];
            self.stop_burst(&rule.id);
        }
    }

    fn start_hold_burst(&self, rule: &BurstRule) {
        self.start_scheduled_burst(rule, rule.trigger_key == rule.target_key);
    }

    fn handle_toggle_press(&self, rule: &BurstRule) {
        let mut states = revive(self.toggle_states.lock());
        let active = states.entry(rule.id.clone()).or_insert(false);
        if *active {
            *active = false;
            drop(states);
            self.stop_burst(&rule.id);
        } else {
            *active = true;
            drop(states);
            self.start_toggle_burst(rule);
        }
    }

    fn start_toggle_burst(&self, rule: &BurstRule) {
        self.start_scheduled_burst(rule, false);
    }

    fn start_scheduled_burst(&self, rule: &BurstRule, allow_while_physical_down: bool) {
        {
            let mut active = revive(self.active_rules.lock());
            if !active.insert(rule.id.clone()) {
                return;
            }
            self.metrics.set_active_rules(active.len());
        }

        let cmd = SchedulerCommand::Start(ScheduledRuleConfig {
            id: rule.id.clone(),
            target_key: rule.target_key,
            interval_ms: rule.interval_ms,
            allow_while_physical_down,
        });
        if self.scheduler_tx.send(cmd).is_err() {
            let mut active = revive(self.active_rules.lock());
            active.remove(&rule.id);
            self.metrics.set_active_rules(active.len());
            release_simulated_key(rule.target_key, &self.pressed_keys, &self.simulated_keys);
        }
    }

    fn stop_burst(&self, rule_id: &str) {
        let removed = {
            let mut active = revive(self.active_rules.lock());
            let removed = active.remove(rule_id);
            if removed {
                self.metrics.set_active_rules(active.len());
            }
            removed
        };
        if removed {
            self.metrics.add_stop_command();
            let _ = self
                .scheduler_tx
                .send(SchedulerCommand::Stop(rule_id.to_string(), Instant::now()));
        }
    }
}

impl Drop for BurstEngine {
    fn drop(&mut self) {
        revive(self.active_rules.lock()).clear();
        self.metrics.set_active_rules(0);
        let _ = self.scheduler_tx.send(SchedulerCommand::Shutdown);
        if let Some(handle) = self.scheduler_handle.take() {
            let _ = handle.join();
        }
        release_simulated_keys(&self.pressed_keys, &self.simulated_keys);
        #[cfg(windows)]
        clear_pending_injections();
    }
}

/// WH_KEYBOARD_LL 低级键盘钩子回调；运行在安装 hook 的线程（消息循环线程）上。
///
/// # Safety
///
/// 由 Windows 调用,调用方契约：当 `ncode >= 0` 时 `lparam` 指向 Windows 维护的
/// 有效 `KBDLLHOOKSTRUCT`,生命周期覆盖本次回调返回前。函数内不持有该指针的延长引用,
/// 也不跨线程发送借用。
#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if ncode >= 0 {
        // SAFETY: 上文 # Safety 契约保证 ncode>=0 时 lparam 是有效的
        // KBDLLHOOKSTRUCT 指针,借用 kb 不存活到回调返回之后
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        // SendInput / Interception：通过 dwExtraInfo 精确过滤自身注入；
        // DD-HID：dwExtraInfo 由驱动端置位，无法控制，转用 PENDING_INJECTIONS 队列匹配
        let is_sim_marker = kb.dwExtraInfo == SIM_MARKER;
        if !is_sim_marker {
            let key = KeyId::Keyboard(kb.vkCode);
            let is_up = matches!(wparam as u32, WM_KEYUP | WM_SYSKEYUP);
            let is_down_or_up = matches!(
                wparam as u32,
                WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
            );
            // 仅对 down/up 主事件调用消费，避免无关 wparam 误吃记录
            if is_down_or_up && try_consume_injection(key, is_up) {
                // SAFETY: WH_KEYBOARD_LL 文档允许传入 null hhk,Windows 会沿钩链向后传递
                return CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam);
            }

            let engine = ENGINE_HOOK
                .read()
                .unwrap()
                .as_ref()
                .and_then(|w| w.upgrade());
            if let Some(engine) = engine {
                let started_at = Instant::now();
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        engine.on_key_press(key);
                    }
                    WM_KEYUP | WM_SYSKEYUP => engine.on_key_release(key),
                    _ => {}
                }
                engine.record_hook_callback(started_at);
            }
        }
    }
    // SAFETY: 同上,fall-through 路径必须把事件继续传递给后续钩子,否则会吞掉键盘输入
    CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
}

/// 把 wparam + MSLLHOOKSTRUCT 解析为 (按钮, 是否抬起)。仅识别 5 个按钮事件，
/// 移动 / 滚轮 / 双击不映射，调用方应直接转发。
#[cfg(windows)]
fn classify_mouse_event(wparam: u32, mouse_data: u32) -> Option<(MouseButton, bool)> {
    match wparam {
        WM_LBUTTONDOWN => Some((MouseButton::Left, false)),
        WM_LBUTTONUP => Some((MouseButton::Left, true)),
        WM_RBUTTONDOWN => Some((MouseButton::Right, false)),
        WM_RBUTTONUP => Some((MouseButton::Right, true)),
        WM_MBUTTONDOWN => Some((MouseButton::Middle, false)),
        WM_MBUTTONUP => Some((MouseButton::Middle, true)),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            // MSLLHOOKSTRUCT.mouseData 高 16 位是 XBUTTON1 / XBUTTON2 标识
            let xbtn = ((mouse_data >> 16) & 0xFFFF) as u16;
            let btn = if xbtn == XBUTTON1 {
                MouseButton::X1
            } else if xbtn == XBUTTON2 {
                MouseButton::X2
            } else {
                return None;
            };
            Some((btn, wparam == WM_XBUTTONUP))
        }
        _ => None,
    }
}

/// WH_MOUSE_LL 低级鼠标钩子回调；与键盘 hook 共用同一消息循环线程。
///
/// # Safety
///
/// 由 Windows 调用：当 `ncode >= 0` 时 `lparam` 指向 Windows 维护的
/// 有效 `MSLLHOOKSTRUCT`，生命周期覆盖本次回调返回前。函数内不持有该指针的延长引用。
#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if ncode >= 0 {
        // SAFETY: 上文 # Safety 契约保证 ncode>=0 时 lparam 指向有效 MSLLHOOKSTRUCT
        let ms = &*(lparam as *const MSLLHOOKSTRUCT);
        let is_sim_marker = ms.dwExtraInfo == SIM_MARKER;
        if !is_sim_marker {
            if let Some((btn, is_up)) = classify_mouse_event(wparam as u32, ms.mouseData) {
                let key = KeyId::Mouse(btn);
                if try_consume_injection(key, is_up) {
                    // SAFETY: 文档允许 null hhk
                    return CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam);
                }
                let engine = ENGINE_HOOK
                    .read()
                    .unwrap()
                    .as_ref()
                    .and_then(|w| w.upgrade());
                if let Some(engine) = engine {
                    let started_at = Instant::now();
                    if is_up {
                        engine.on_key_release(key);
                    } else {
                        engine.on_key_press(key);
                    }
                    engine.record_hook_callback(started_at);
                }
            }

            // 滚轮触发：每格作为瞬发事件，发 press 后立即发 release
            // Toggle 规则每格切换一次；Hold 规则每格触发一个间隔周期
            if wparam as u32 == WM_MOUSEWHEEL {
                let delta = ((ms.mouseData >> 16) as u16) as i16;
                let btn = if delta > 0 {
                    MouseButton::WheelUp
                } else {
                    MouseButton::WheelDown
                };
                let key = KeyId::Mouse(btn);
                // DD-HID 注入的滚轮通过 PENDING_INJECTIONS 过滤
                if try_consume_injection(key, false) {
                    return CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam);
                }
                {
                    let engine = ENGINE_HOOK
                        .read()
                        .unwrap()
                        .as_ref()
                        .and_then(|w| w.upgrade());
                    if let Some(engine) = engine {
                        let started_at = Instant::now();
                        engine.on_key_press(key);
                        engine.on_key_release(key);
                        engine.record_hook_callback(started_at);
                    }
                }
            }
        }
    }
    // SAFETY: 同上,fall-through 路径必须把事件继续传递给后续钩子
    CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
}

#[cfg(windows)]
pub fn start_listener(engine: Arc<BurstEngine>) {
    {
        let mut guard = ENGINE_HOOK.write().unwrap();
        if guard.as_ref().and_then(|w| w.upgrade()).is_some() {
            error!("start_listener 重复调用：旧引擎仍存活，忽略以防双重 hook");
            return;
        }
        *guard = Some(Arc::downgrade(&engine));
    }
    thread::spawn(move || {
        // SAFETY: 在安装 hook 前记录线程 ID，供 rehook_keyboard() 跨线程投递消息
        HOOK_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

        // SAFETY: WH_KEYBOARD_LL 全局钩子允许 hmod=null + dwThreadId=0,Windows
        // 会自行加载本进程模块作为 hook owner;hook_proc 满足 # Safety 契约
        let mut kbd_hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                std::ptr::null_mut(),
                0,
            )
        };
        if kbd_hook.is_null() {
            error!("安装键盘 hook 失败");
            return;
        }
        info!("键盘 hook 已安装");

        // SAFETY: WH_MOUSE_LL 全局钩子规则与键盘相同
        let mouse_hook = unsafe {
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), std::ptr::null_mut(), 0)
        };
        if mouse_hook.is_null() {
            error!("安装鼠标 hook 失败，鼠标按键将无法触发连发");
        } else {
            info!("鼠标 hook 已安装");
        }

        // WH_KEYBOARD_LL / WH_MOUSE_LL 都要求安装线程持续运行消息循环，
        // 否则 Windows 会在超时后移除 hook
        // SAFETY: MSG 是 POD 结构,全 0 是合法初值,GetMessageW 会写入有效字段
        let mut msg = unsafe { std::mem::zeroed::<MSG>() };
        loop {
            // SAFETY: msg 来自上面 zeroed,后续 GetMessageW/Translate/Dispatch
            // 都按 Win32 文档以可变指针写入或只读消费,生命周期不超出本作用域
            let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if ret == 0 || ret == -1 {
                break;
            }
            // 面板获得焦点时触发：重装键盘 hook 使我们排在 Chromium hook 之后安装（LIFO 优先调用）
            if msg.hwnd.is_null() && msg.message == WM_REHOOK_KEYBOARD {
                if !kbd_hook.is_null() {
                    // SAFETY: kbd_hook 是之前 SetWindowsHookExW 返回的有效句柄
                    unsafe { UnhookWindowsHookEx(kbd_hook) };
                }
                kbd_hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(keyboard_hook_proc),
                        std::ptr::null_mut(),
                        0,
                    )
                };
                if kbd_hook.is_null() {
                    error!("rehook: 键盘 hook 重新安装失败");
                } else {
                    info!("rehook: 键盘 hook 已重新安装");
                }
                continue;
            }
            // SAFETY: msg 是上一步 GetMessageW 写入的合法消息
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        if !kbd_hook.is_null() {
            // SAFETY: kbd_hook 是上面 SetWindowsHookExW 返回的非空有效句柄
            unsafe { UnhookWindowsHookEx(kbd_hook) };
            info!("键盘 hook 已卸载");
        }
        if !mouse_hook.is_null() {
            // SAFETY: mouse_hook 上面已校验非空
            unsafe { UnhookWindowsHookEx(mouse_hook) };
            info!("鼠标 hook 已卸载");
        }
    });
    info!("连发引擎监听器已启动");
}

#[cfg(not(windows))]
pub fn start_listener(_engine: Arc<BurstEngine>) {
    info!("连发引擎监听器（当前平台暂不支持键盘 hook）");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn rule(id: &str, mode: BurstMode, trigger_key: KeyId, target_key: KeyId) -> BurstRule {
        BurstRule {
            id: id.to_string(),
            enabled: true,
            trigger_key,
            target_key,
            mode,
            stop_key: None,
            interval_ms: 10,
        }
    }

    fn engine_with_scheduler_tx(
        tx: std::sync::mpsc::Sender<SchedulerCommand>,
        simulated_keys: SimulatedKeys,
    ) -> BurstEngine {
        BurstEngine {
            global_enabled: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(Mutex::new(Arc::new(RuleSnapshot::default()))),
            active_rules: Arc::new(Mutex::new(HashSet::new())),
            scheduler_tx: SchedulerCommandSender::new(tx, SchedulerWake::new()),
            scheduler_handle: None,
            scheduler_wait_mode: Arc::new(AtomicU8::new(wait_mode_to_u8(
                SchedulerWaitMode::default(),
            ))),
            metrics: Arc::new(EngineMetrics::new()),
            toggle_states: Arc::new(Mutex::new(HashMap::new())),
            pressed_keys: Arc::new(Mutex::new(HashSet::new())),
            simulated_keys,
            hotkeys: Arc::new(Mutex::new(Hotkeys::default())),
            on_global_changed: Arc::new(Mutex::new(None)),
            on_panel_toggle: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn repeated_keydown_does_not_retrigger_global_toggle_before_release() {
        let engine = BurstEngine::new();
        let key = KeyId::Keyboard(0x51);
        engine.set_hotkeys(Hotkeys {
            global_toggle: Some(key),
            ..Default::default()
        });

        engine.on_key_press(key);
        assert!(engine.global_enabled.load(Ordering::SeqCst));

        engine.on_key_press(key);
        assert!(engine.global_enabled.load(Ordering::SeqCst));

        engine.on_key_release(key);
        engine.on_key_press(key);
        assert!(!engine.global_enabled.load(Ordering::SeqCst));
    }

    #[test]
    fn repeated_keydown_calls_panel_toggle_once_until_release() {
        let engine = BurstEngine::new();
        let key = KeyId::Keyboard(0x51);
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_cb = calls.clone();
        engine.set_hotkeys(Hotkeys {
            panel_toggle: Some(key),
            ..Default::default()
        });
        engine.set_on_panel_toggle(move || {
            calls_for_cb.fetch_add(1, Ordering::SeqCst);
        });

        engine.on_key_press(key);
        engine.on_key_press(key);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        engine.on_key_release(key);
        engine.on_key_press(key);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn hold_rule_still_starts_on_first_down_and_stops_on_up() {
        let engine = BurstEngine::new();
        let trigger = KeyId::Keyboard(0x51);
        let target = KeyId::Keyboard(0x45);
        engine.global_enabled.store(true, Ordering::SeqCst);
        engine.set_rules(vec![rule("hold-q", BurstMode::Hold, trigger, target)]);

        engine.on_key_press(trigger);
        assert_eq!(engine.get_active_ids(), vec!["hold-q".to_string()]);

        engine.on_key_press(trigger);
        assert_eq!(engine.get_active_ids(), vec!["hold-q".to_string()]);

        engine.on_key_release(trigger);
        assert!(engine.get_active_ids().is_empty());
    }

    #[test]
    fn hold_rule_allows_same_trigger_and_target() {
        let engine = BurstEngine::new();
        let key = KeyId::Keyboard(0x51);
        engine.global_enabled.store(true, Ordering::SeqCst);
        engine.set_rules(vec![rule("hold-same-q", BurstMode::Hold, key, key)]);

        assert!(engine.on_key_press(key));
        assert_eq!(engine.get_active_ids(), vec!["hold-same-q".to_string()]);

        engine.on_key_release(key);
        assert!(engine.get_active_ids().is_empty());
    }

    #[test]
    fn disabled_rule_is_not_indexed_for_press_or_release() {
        let engine = BurstEngine::new();
        let trigger = KeyId::Keyboard(0x51);
        let target = KeyId::Keyboard(0x45);
        let mut disabled = rule("disabled-hold-q", BurstMode::Hold, trigger, target);
        disabled.enabled = false;
        engine.global_enabled.store(true, Ordering::SeqCst);
        engine.set_rules(vec![disabled]);

        assert!(!engine.on_key_press(trigger));
        assert!(engine.get_active_ids().is_empty());

        engine.on_key_release(trigger);
        assert!(engine.get_active_ids().is_empty());
    }

    #[test]
    fn safe_key_down_skips_when_physical_target_is_down() {
        let key = KeyId::Keyboard(0x57);
        let physical_keys = Arc::new(Mutex::new(HashSet::from([key])));
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));

        assert!(!safe_key_down(key, &physical_keys, &simulated_keys, false));
        assert!(revive(simulated_keys.lock()).is_empty());
    }

    #[test]
    fn safe_key_down_allows_same_key_hold_pulse() {
        let key = KeyId::Keyboard(0x57);
        let physical_keys = Arc::new(Mutex::new(HashSet::from([key])));
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));

        assert!(safe_key_down(key, &physical_keys, &simulated_keys, true));
        assert_eq!(revive(simulated_keys.lock()).get(&key), Some(&1));

        safe_key_up(key, &physical_keys, &simulated_keys, true);
        assert!(revive(simulated_keys.lock()).is_empty());
    }

    #[test]
    fn release_simulated_keys_drains_ledger() {
        let key = KeyId::Keyboard(0x45);
        let physical_keys = Arc::new(Mutex::new(HashSet::new()));
        let simulated_keys = Arc::new(Mutex::new(HashMap::from([(key, 2)])));

        release_simulated_keys(&physical_keys, &simulated_keys);
        assert!(revive(simulated_keys.lock()).is_empty());
    }

    #[test]
    fn cancel_all_loops_leaves_simulated_ledger_for_scheduler_when_stop_all_is_queued() {
        let key = KeyId::Keyboard(0x45);
        let (tx, _rx) = mpsc::channel();
        let simulated_keys = Arc::new(Mutex::new(HashMap::from([(key, 1)])));
        let engine = engine_with_scheduler_tx(tx, simulated_keys.clone());

        engine.cancel_all_loops();

        assert_eq!(revive(simulated_keys.lock()).get(&key), Some(&1));
    }

    #[test]
    fn cancel_all_loops_releases_simulated_ledger_when_scheduler_is_unavailable() {
        let key = KeyId::Keyboard(0x45);
        let (tx, rx) = mpsc::channel();
        drop(rx);
        let simulated_keys = Arc::new(Mutex::new(HashMap::from([(key, 1)])));
        let engine = engine_with_scheduler_tx(tx, simulated_keys.clone());

        engine.cancel_all_loops();

        assert!(revive(simulated_keys.lock()).is_empty());
    }

    #[test]
    fn scheduler_waiter_receives_woken_command() {
        let (tx, rx) = mpsc::channel();
        let wake = SchedulerWake::new();
        let sender = SchedulerCommandSender::new(tx, wake.clone());
        let mut waiter = SchedulerWaiter::new(wake, SchedulerWaitMode::Standard);

        assert!(sender.send(SchedulerCommand::Shutdown).is_ok());

        assert!(matches!(
            waiter.wait(&rx, Some(Duration::from_secs(1))),
            SchedulerWaitOutcome::Command(SchedulerCommand::Shutdown)
        ));
    }

    #[test]
    fn standard_wait_zero_timeout_returns_timeout() {
        let (_tx, rx) = mpsc::channel();

        assert!(matches!(
            wait_standard(&rx, Some(Duration::ZERO)),
            SchedulerWaitOutcome::Timeout
        ));
    }

    #[test]
    fn scheduler_wait_mode_defaults_to_standard_and_can_change() {
        let engine = BurstEngine::new();
        assert_eq!(engine.scheduler_wait_mode(), SchedulerWaitMode::Standard);

        engine.set_scheduler_wait_mode(SchedulerWaitMode::HighPrecision);
        assert_eq!(
            engine.scheduler_wait_mode(),
            SchedulerWaitMode::HighPrecision
        );

        engine.set_scheduler_wait_mode(SchedulerWaitMode::Standard);
        assert_eq!(engine.scheduler_wait_mode(), SchedulerWaitMode::Standard);
    }

    #[test]
    fn step_due_rules_merges_same_target_pulses() {
        let target = KeyId::Keyboard(0x45);
        let physical_keys = Arc::new(Mutex::new(HashSet::new()));
        let simulated_keys = Arc::new(Mutex::new(HashMap::new()));
        let now = Instant::now();
        let mut rules = HashMap::from([
            (
                "a".to_string(),
                ScheduledRule::new(
                    ScheduledRuleConfig {
                        id: "a".to_string(),
                        target_key: target,
                        interval_ms: 10,
                        allow_while_physical_down: false,
                    },
                    now,
                ),
            ),
            (
                "b".to_string(),
                ScheduledRule::new(
                    ScheduledRuleConfig {
                        id: "b".to_string(),
                        target_key: target,
                        interval_ms: 10,
                        allow_while_physical_down: false,
                    },
                    now,
                ),
            ),
        ]);
        let mut target_holds = HashMap::new();
        let metrics = EngineMetrics::new();
        let mut events = Vec::new();

        step_due_rules(
            &mut rules,
            &mut target_holds,
            &physical_keys,
            &simulated_keys,
            &metrics,
            &mut events,
        );

        assert_eq!(events, vec![(target, false)]);
        assert_eq!(revive(simulated_keys.lock()).get(&target), Some(&1));
        assert_eq!(target_holds.get(&target).unwrap().owners.len(), 2);
        assert_eq!(metrics.snapshot().injected_events, 0);
        assert_eq!(metrics.snapshot().scheduler_steps, 2);
    }

    #[test]
    fn toggle_rule_still_toggles_after_release_and_next_down() {
        let engine = BurstEngine::new();
        let trigger = KeyId::Keyboard(0x51);
        let target = KeyId::Keyboard(0x45);
        engine.global_enabled.store(true, Ordering::SeqCst);
        engine.set_rules(vec![rule("toggle-q", BurstMode::Toggle, trigger, target)]);

        engine.on_key_press(trigger);
        assert_eq!(engine.get_active_ids(), vec!["toggle-q".to_string()]);

        engine.on_key_press(trigger);
        assert_eq!(engine.get_active_ids(), vec!["toggle-q".to_string()]);

        engine.on_key_release(trigger);
        engine.on_key_press(trigger);
        assert!(engine.get_active_ids().is_empty());
    }

    #[test]
    fn toggle_rule_with_distinct_stop_is_indexed_by_stop_key() {
        let engine = BurstEngine::new();
        let trigger = KeyId::Keyboard(0x51);
        let stop = KeyId::Keyboard(0x52);
        let target = KeyId::Keyboard(0x45);
        let mut toggle = rule("toggle-q", BurstMode::Toggle, trigger, target);
        toggle.stop_key = Some(stop);
        engine.global_enabled.store(true, Ordering::SeqCst);
        engine.set_rules(vec![toggle]);

        assert!(engine.on_key_press(trigger));
        assert_eq!(engine.get_active_ids(), vec!["toggle-q".to_string()]);

        assert!(engine.on_key_press(stop));
        assert!(engine.get_active_ids().is_empty());
    }

    #[test]
    fn repeated_mouse_down_is_filtered_until_release() {
        let engine = BurstEngine::new();
        let key = KeyId::Mouse(MouseButton::Left);
        engine.set_hotkeys(Hotkeys {
            global_toggle: Some(key),
            ..Default::default()
        });

        engine.on_key_press(key);
        assert!(engine.global_enabled.load(Ordering::SeqCst));

        engine.on_key_press(key);
        assert!(engine.global_enabled.load(Ordering::SeqCst));

        engine.on_key_release(key);
        engine.on_key_press(key);
        assert!(!engine.global_enabled.load(Ordering::SeqCst));
    }
}
