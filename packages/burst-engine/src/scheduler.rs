use qzh_profile::key_id::KeyId;
use qzh_profile::profile::BurstRule;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{error, warn};
use win_input::{key_events, DispatchResult, InputEvent};

const ACK_TIMEOUT: Duration = Duration::from_millis(500);

/// 规则被调度器自行判定失活（liveness 兜底）时回调，参数为规则 ID。
/// 由引擎注册，用于同步清理 `active_rules` / `toggle_states`，使该规则可被重新触发。
pub(crate) type RuleExpiredCb = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulePhase {
    DownPhase,
    UpPhase,
}

#[derive(Debug)]
struct ScheduledRule {
    rule: Arc<BurstRule>,
    generation: u64,
    /// 该规则是 Hold(trigger==target) 连发：物理触发键被按住期间持续注入同一键。
    /// 这类规则的物理松开事件可能被注入回灌吞掉而收不到 on_key_release，需 liveness 兜底自停。
    hold_same_key: bool,
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
    pub fn start(
        physical_keys: PhysicalKeys,
        simulated_keys: SimulatedKeys,
        on_expired: RuleExpiredCb,
    ) -> Self {
        Self::start_with_dispatcher(
            Arc::new(WinInputDispatcher),
            None,
            physical_keys,
            simulated_keys,
            on_expired,
        )
    }

    pub(crate) fn start_with_dispatcher(
        dispatcher: Arc<dyn EventDispatcher>,
        stats: Option<Arc<SchedulerStats>>,
        physical_keys: PhysicalKeys,
        simulated_keys: SimulatedKeys,
        on_expired: RuleExpiredCb,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let hp_degraded = Arc::new(AtomicBool::new(false));
        let worker_degraded = hp_degraded.clone();
        let command_waker = platform_wait::CommandWaker::new(&hp_degraded);
        let worker_waker = command_waker.clone();
        // panic 兜底：worker 栈一旦展开，target_holds 随之丢失，靠 simulated_keys 账本
        // 释放所有已按下的模拟键，避免按键卡死（连发工具最差故障）。
        let panic_fallback = simulated_keys.clone();
        let join = thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker_loop(
                    rx,
                    worker_degraded,
                    worker_waker,
                    dispatcher,
                    stats,
                    physical_keys,
                    simulated_keys,
                    on_expired,
                )
            }));
            if outcome.is_err() {
                release_simulated_keys(&panic_fallback);
                error!("调度器 worker 线程 panic，已按账本兜底释放模拟按键");
            }
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

#[allow(clippy::too_many_arguments)]
fn worker_loop(
    rx: Receiver<SchedulerCommand>,
    hp_degraded: Arc<AtomicBool>,
    command_waker: platform_wait::CommandWaker,
    dispatcher: Arc<dyn EventDispatcher>,
    stats: Option<Arc<SchedulerStats>>,
    physical_keys: PhysicalKeys,
    simulated_keys: SimulatedKeys,
    on_expired: RuleExpiredCb,
) {
    let mut worker = SchedulerWorker {
        rules: HashMap::new(),
        target_holds: HashMap::new(),
        current_generation: 0,
        dispatcher,
        stats,
        physical_keys,
        simulated_keys,
        physical_down_probe: key_physically_down,
        on_expired,
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
    rules: HashMap<Arc<str>, ScheduledRule>,
    /// 目标键 -> 当前持有它的规则 ID 集合。集合非空即该键处于注入按下态，
    /// 多条规则共享同一目标键时只发一次 down，最后一个 owner 释放时才发 up。
    target_holds: HashMap<KeyId, HashSet<Arc<str>>>,
    current_generation: u64,
    dispatcher: Arc<dyn EventDispatcher>,
    stats: Option<Arc<SchedulerStats>>,
    /// hook 维护的物理按键集合。引擎不为业务取舍兜底，已删除高频注入门控
    /// （is_physically_blocked）；这里仅在 liveness 兜底自停时低频清理陈旧触发键记录。
    physical_keys: PhysicalKeys,
    simulated_keys: SimulatedKeys,
    /// 物理按下状态探针，默认 [`key_physically_down`]（GetAsyncKeyState）。
    /// 抽成字段以便单元测试在无真实键盘输入时注入确定的物理状态。
    physical_down_probe: fn(KeyId) -> bool,
    /// 规则被 liveness 兜底自停时通知引擎清理（见 [`RuleExpiredCb`]）。
    on_expired: RuleExpiredCb,
    /// 智能降频状态：注入持续迟到时自动放慢，守住物理输入优先与连发稳定。
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

    /// 复位到不降频。停止 / 切代 / 退出等会话边界调用，避免瞬时承压跨会话残留拖慢。
    fn reset(&mut self) {
        self.dynamic_floor_ms = THROTTLE_FLOOR_MIN_MS;
    }
}

impl SchedulerWorker {
    fn handle_command(&mut self, command: SchedulerCommand) -> bool {
        match command {
            SchedulerCommand::Start { rule, generation } => {
                if generation == self.current_generation {
                    let hold_same_key = rule.mode == qzh_profile::profile::BurstMode::Hold
                        && rule.trigger_key == rule.target_key;
                    let id: Arc<str> = Arc::from(rule.id.as_str());
                    self.rules.entry(id).or_insert(ScheduledRule {
                        rule,
                        generation,
                        hold_same_key,
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
        // 会话结束：复位降频，下一轮连发从不降频开始，避免上次瞬时承压残留拖慢。
        self.throttle.reset();
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

        // 智能降频：effective interval = max(请求 interval, floor)，只拉长「下一拍的间隔」，
        // 绝不快过规则请求；floor>=1ms 保证 next_at 单调推进。
        let floor_ms = self.throttle.floor_ms();
        let mut max_late = Duration::ZERO;
        let mut events = Vec::new();
        for id in due_ids {
            let Some(mut rule) = self.rules.remove(&id) else {
                continue;
            };
            let late = now.saturating_duration_since(rule.next_at);
            if let Some(stats) = &self.stats {
                stats.record_delay(late);
            }
            if rule.generation != self.current_generation {
                if rule.is_down {
                    if let Some(event) = self.release_owner(rule.rule.target_key, &id) {
                        events.push(event);
                    }
                }
                continue;
            }

            // liveness 兜底：DD 等驱动后端的注入回灌泄漏可能吞掉物理抬键，使
            // Hold(trigger==target) 规则永不收到 on_key_release 而失控。仅在松开相位
            // （is_down == false，此时 GetAsyncKeyState 反映用户真实状态而非自身注入的下压）
            // 复核物理触发键；已抬起则自停并通知引擎清理，使规则可被重新触发。这是 stop
            // 安全网（守住「不卡死」安全底线），非注入门控——不在高频路径持锁。
            if rule.hold_same_key
                && !rule.is_down
                && !(self.physical_down_probe)(rule.rule.trigger_key)
            {
                self.physical_keys
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&rule.rule.trigger_key);
                (self.on_expired)(&id);
                warn!("Hold 触发键已抬起但未收到释放事件，自停规则: id={id}");
                continue; // 不重新插入 → 规则停止；目标键此前已在 UpPhase 释放
            }

            // 信号 B：仅统计真正参与注入的规则的调度迟到。
            max_late = max_late.max(late);

            // hold（按下时长）按规则请求的 interval 计算，降频不拉长按下、保持点按特性；
            // 仅把拍与拍之间的「间隔」放慢。
            let interval = Duration::from_millis(rule.rule.interval_ms.max(floor_ms) as u64);
            let hold = hold_duration(rule.rule.interval_ms);
            match rule.phase {
                RulePhase::DownPhase if hold.is_zero() => {
                    match self.acquire_owner(rule.rule.target_key, &id) {
                        AcquireOutcome::Acquired => {
                            events.push(InputEvent::down(rule.rule.target_key));
                            if let Some(event) = self.release_owner(rule.rule.target_key, &id) {
                                events.push(event);
                            }
                        }
                        AcquireOutcome::Shared => {
                            let _ = self.release_owner(rule.rule.target_key, &id);
                        }
                    }
                    rule.is_down = false;
                    rule.phase = RulePhase::DownPhase;
                    rule.next_at = now + interval;
                }
                RulePhase::DownPhase => {
                    if let AcquireOutcome::Acquired = self.acquire_owner(rule.rule.target_key, &id)
                    {
                        events.push(InputEvent::down(rule.rule.target_key));
                    }
                    rule.is_down = true;
                    rule.phase = RulePhase::UpPhase;
                    rule.next_at = now + hold;
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
        // 信号 B/C：调度持续迟到（线程跟不上节奏）则升高下限，否则衰减恢复。
        self.throttle.observe(max_late > THROTTLE_LATE_THRESHOLD);
    }

    /// 申请目标键的注入所有权。引擎不为业务取舍兜底：不判断该键是否被用户物理按住
    /// （把某键设为连发 target 即声明它会被不断按下弹起，是业务层取舍）。同一目标键被多条
    /// 规则共享时只发一次 down，避免相互打断。
    fn acquire_owner(&mut self, key: KeyId, owner: &Arc<str>) -> AcquireOutcome {
        if let Some(owners) = self.target_holds.get_mut(&key) {
            owners.insert(owner.clone());
            return AcquireOutcome::Shared;
        }
        self.target_holds
            .entry(key)
            .or_default()
            .insert(owner.clone());
        AcquireOutcome::Acquired
    }

    fn release_owner(&mut self, key: KeyId, owner: &str) -> Option<InputEvent> {
        let owners = self.target_holds.get_mut(&key)?;
        owners.remove(owner);
        if !owners.is_empty() {
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
            // 默认信任真实探针；需要确定物理状态的用例自行覆盖 physical_down_probe。
            physical_down_probe: key_physically_down,
            on_expired: Arc::new(|_| {}),
            throttle: Throttle::new(Arc::new(AtomicBool::new(false))),
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
    fn throttle_raises_floor_under_stress_and_decays_when_healthy() {
        let mut t = Throttle::new(Arc::new(AtomicBool::new(false)));
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MIN_MS);
        for _ in 0..100 {
            t.observe(true);
        }
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MAX_MS);
        for _ in 0..(THROTTLE_FLOOR_MAX_MS as usize) {
            t.observe(false);
        }
        assert_eq!(t.floor_ms(), THROTTLE_FLOOR_MIN_MS);
    }

    #[test]
    fn throttle_hp_degraded_enforces_min_floor() {
        let t = Throttle::new(Arc::new(AtomicBool::new(true)));
        assert!(t.floor_ms() >= HP_DEGRADED_FLOOR_MS);
    }

    #[test]
    fn cleanup_all_resets_throttle() {
        let (mut worker, _, _) = test_worker();
        for _ in 0..100 {
            worker.throttle.observe(true);
        }
        assert!(worker.throttle.floor_ms() > THROTTLE_FLOOR_MIN_MS);
        worker.cleanup_all();
        assert_eq!(worker.throttle.floor_ms(), THROTTLE_FLOOR_MIN_MS);
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
            worker.acquire_owner(key, &Arc::from("a")),
            AcquireOutcome::Acquired
        );
        assert_eq!(
            worker.acquire_owner(key, &Arc::from("b")),
            AcquireOutcome::Shared
        );
        assert_eq!(worker.release_owner(key, "a"), None);
        assert_eq!(worker.release_owner(key, "b"), Some(InputEvent::up(key)));
    }

    #[test]
    fn acquire_owner_no_longer_gates_on_physical_state() {
        // 引擎不为业务取舍兜底：target 键即便被物理按住也照常注入（由业务层声明风险）。
        let (mut worker, physical_keys, _) = test_worker();
        let key = KeyId::Keyboard(0x45);
        physical_keys.lock().unwrap().insert(key);
        worker.physical_down_probe = |_| true;

        assert_eq!(
            worker.acquire_owner(key, &Arc::from("different-trigger")),
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

    fn hold_same_key_rule(id: &str, key: KeyId) -> Arc<BurstRule> {
        Arc::new(BurstRule {
            id: id.to_string(),
            enabled: true,
            trigger_key: key,
            target_key: key,
            mode: BurstMode::Hold,
            stop_key: None,
            interval_ms: 10,
            group: None,
        })
    }

    // liveness 兜底：Hold(trigger==target) 的物理触发键已抬起（注入回灌泄漏吞掉抬键事件）时，
    // 调度器自停规则、清理物理集合并回调 on_expired，闭合唯一缺少 is_physically_blocked 的路径。
    #[test]
    fn hold_same_key_auto_stops_when_physical_key_released() {
        let (mut worker, physical_keys, _) = test_worker();
        let key = KeyId::Keyboard(0x51);
        let expired = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = expired.clone();
        worker.on_expired = Arc::new(move |id: &str| {
            sink.lock().unwrap().push(id.to_string());
        });
        physical_keys.lock().unwrap().insert(key);
        // 探针报告键已抬起（物理抬键被吞）
        worker.physical_down_probe = |_| false;
        worker.handle_command(SchedulerCommand::Start {
            rule: hold_same_key_rule("hold", key),
            generation: 0,
        });

        worker.process_due(Instant::now());

        assert!(worker.rules.is_empty());
        assert!(!physical_keys.lock().unwrap().contains(&key));
        assert_eq!(expired.lock().unwrap().as_slice(), &["hold".to_string()]);
    }

    // 物理触发键仍按住时，liveness 兜底不得误停规则。
    #[test]
    fn hold_same_key_keeps_running_while_physical_key_down() {
        let (mut worker, physical_keys, _) = test_worker();
        let key = KeyId::Keyboard(0x51);
        physical_keys.lock().unwrap().insert(key);
        worker.physical_down_probe = |_| true;
        worker.handle_command(SchedulerCommand::Start {
            rule: hold_same_key_rule("hold", key),
            generation: 0,
        });

        worker.process_due(Instant::now());

        assert!(worker.rules.contains_key("hold"));
    }
}
