use qzh_profile::key_id::KeyId;
#[cfg(test)]
use qzh_profile::key_id::MouseButton;
use qzh_profile::profile::{BurstMode, BurstRule, Hotkeys};
#[cfg(test)]
use qzh_profile::MAX_RULES;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
use win_input::{clear_pending_injections, clear_relay_injections};

#[cfg(windows)]
mod hooks;
mod lifecycle;
mod metrics;
mod rules;
mod safety;
mod scheduler;

#[cfg(windows)]
pub use hooks::start_listener;
pub use lifecycle::EngineLifecycle;
pub use metrics::EngineMetricsSnapshot;

#[cfg(not(windows))]
pub fn start_listener(_engine: Arc<BurstEngine>) {
    tracing::info!("连发引擎监听器（当前平台暂不支持键盘 hook）");
}

use lifecycle::LIFECYCLE_PAUSED;
use metrics::EngineMetrics;
use rules::{normalize_rules_for_engine, RuleSnapshot};
use safety::{release_simulated_key, release_simulated_keys};
use scheduler::{
    spawn_scheduler, ScheduledRuleConfig, SchedulerCommand, SchedulerCommandSender, SchedulerWake,
};

type GlobalChangedCb = Arc<Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>>>;
type PanelToggleCb = Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>;
type PhysicalKeys = Arc<Mutex<HashSet<KeyId>>>;
type SimulatedKeys = Arc<Mutex<HashMap<KeyId, usize>>>;
type ActiveRules = Arc<Mutex<HashSet<String>>>;
type KeyEvent = (KeyId, bool);
type Metrics = Arc<EngineMetrics>;
const STOP_ALL_ACK_TIMEOUT: Duration = Duration::from_millis(500);
const MIN_BURST_INTERVAL_MS: u32 = 10;
const MAX_BURST_INTERVAL_MS: u32 = 10_000;

struct StopAllDepthGuard<'a> {
    depth: &'a AtomicUsize,
}

impl<'a> StopAllDepthGuard<'a> {
    fn new(depth: &'a AtomicUsize) -> Self {
        depth.fetch_add(1, Ordering::SeqCst);
        Self { depth }
    }
}

impl Drop for StopAllDepthGuard<'_> {
    fn drop(&mut self) {
        self.depth.fetch_sub(1, Ordering::SeqCst);
    }
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
    scheduler_handle: Mutex<Option<thread::JoinHandle<()>>>,
    /// App 级生命周期；全局关闭只是 Paused，只有退出流程进入 Shutdown。
    lifecycle_state: AtomicU8,
    /// 高精度 timer 是否已降级为标准等待。
    scheduler_hp_degraded: Arc<AtomicBool>,
    /// 调度器降级时调用（由 app 层注册，用于通知前端）。
    on_scheduler_degraded: PanelToggleCb,
    /// StopAll 进行中的计数闸门；重叠停止流程全部结束前禁止新规则启动。
    stop_all_depth: Arc<AtomicUsize>,
    /// StopAll 世代号；scheduler 丢弃和当前世代不一致的陈旧 Start。
    stop_all_generation: Arc<AtomicU64>,
    metrics: Metrics,
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
        let scheduler_hp_degraded = Arc::new(AtomicBool::new(false));
        let stop_all_depth = Arc::new(AtomicUsize::new(0));
        let stop_all_generation = Arc::new(AtomicU64::new(0));
        let on_scheduler_degraded: PanelToggleCb = Arc::new(Mutex::new(None));
        let scheduler_tx = SchedulerCommandSender::new(scheduler_tx, scheduler_wake.clone());
        let scheduler_handle = Some(spawn_scheduler(
            scheduler_rx,
            scheduler_wake,
            scheduler_hp_degraded.clone(),
            on_scheduler_degraded.clone(),
            stop_all_generation.clone(),
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
            scheduler_handle: Mutex::new(scheduler_handle),
            lifecycle_state: AtomicU8::new(LIFECYCLE_PAUSED),
            scheduler_hp_degraded,
            on_scheduler_degraded,
            stop_all_depth,
            stop_all_generation,
            metrics,
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

    /// 取消所有正在运行的连发循环。
    /// 用于配置热更新、输入模式切换等需要停止活动任务但不改变全局开关语义的场景。
    pub fn cancel_all_loops(&self) {
        if matches!(
            self.lifecycle(),
            EngineLifecycle::ShuttingDown | EngineLifecycle::Shutdown
        ) {
            return;
        }
        let _gate = self.begin_stop_all();
        self.cancel_all_loops_inner();
    }

    fn begin_stop_all(&self) -> StopAllDepthGuard<'_> {
        let gate = StopAllDepthGuard::new(&self.stop_all_depth);
        self.stop_all_generation.fetch_add(1, Ordering::SeqCst);
        gate
    }

    fn cancel_all_loops_inner(&self) {
        self.metrics.add_stop_command();
        let (ack_tx, ack_rx) = mpsc::channel();
        {
            let mut active = revive(self.active_rules.lock());
            active.clear();
            self.metrics.set_active_rules(0);
        }
        let sent = self.scheduler_tx.send(SchedulerCommand::StopAll {
            sent_at: Instant::now(),
            ack: Some(ack_tx),
        });
        if sent.is_err() || ack_rx.recv_timeout(STOP_ALL_ACK_TIMEOUT).is_err() {
            // scheduler 已挂死或超时：fallback 兜底发 key_up，防止驱动侧按键卡住。
            // 注意：release_simulated_keys 会 drain 账本。若 scheduler 之后仍恢复处理
            // StopAll，plan_key_up 会发现账本为空，但不会跳过 key_up（已修复该逻辑）。
            release_simulated_keys(&self.pressed_keys, &self.simulated_keys);
        }
        // 第二次清空：消除 StopAll 等待期间并发 on_key_press 可能遗留的 active_rules 插入，
        // 保证方法返回后 active_rules 与实际调度器状态一致（均为空）。
        revive(self.active_rules.lock()).clear();
        self.metrics.set_active_rules(0);
        #[cfg(windows)]
        {
            clear_pending_injections();
            clear_relay_injections();
        }
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
        let _gate = self.begin_stop_all();
        let snapshot = Arc::new(RuleSnapshot::new(normalize_rules_for_engine(rules)));
        {
            let mut current = revive(self.rules.lock());
            *current = snapshot;
        }
        self.cancel_all_loops_inner();
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

    /// 返回调度器是否已降级为标准等待（高精度 timer 不可用）。
    pub fn scheduler_hp_degraded(&self) -> bool {
        self.scheduler_hp_degraded.load(Ordering::SeqCst)
    }

    /// 注册调度器降级时的回调（由 app 层注册，用于通知前端）。
    /// 若调度器已降级，注册后立即调用一次。
    pub fn set_on_scheduler_degraded(&self, f: impl Fn() + Send + Sync + 'static) {
        *revive(self.on_scheduler_degraded.lock()) = Some(Box::new(f));
        if self.scheduler_hp_degraded.load(Ordering::SeqCst) {
            if let Some(cb) = revive(self.on_scheduler_degraded.lock()).as_ref() {
                cb();
            }
        }
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

            if stop == Some(key) && enabled {
                drop(hk);
                self.pause_runtime();
                if let Some(cb) = revive(self.on_global_changed.lock()).as_ref() {
                    cb(false);
                }
                return true;
            }
            if start == Some(key) && !enabled {
                drop(hk);
                if self.enable_runtime() {
                    if let Some(cb) = revive(self.on_global_changed.lock()).as_ref() {
                        cb(true);
                    }
                    return true;
                }
                return false;
            }
            if panel == Some(key) {
                drop(hk);
                if let Some(cb) = revive(self.on_panel_toggle.lock()).as_ref() {
                    cb();
                }
                return true;
            }
        }

        if !self.global_enabled.load(Ordering::SeqCst) {
            return false;
        }
        if matches!(
            self.lifecycle(),
            EngineLifecycle::Stopping | EngineLifecycle::ShuttingDown | EngineLifecycle::Shutdown
        ) {
            return false;
        }
        if self.stop_all_depth.load(Ordering::SeqCst) > 0 {
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
                        // ⚠️ Toggle 状态以 active_rules 为唯一来源，检查与变更必须在同一把锁内完成。
                        //
                        // 陷阱：若单独维护一个"toggle_states"标志再分别更新 active_rules，
                        // 两把锁之间存在 TOCTOU 窗口：并发调用可能各自读到旧状态，
                        // 一个认为"未运行→Start"，另一个认为"运行→Stop"，最终
                        // active_rules 与调度器实际状态撕裂，用户需多按才能停止。
                        enum ToggleAction {
                            Start { generation: u64 },
                            Stop,
                        }
                        let action: Option<ToggleAction> = {
                            let mut active = revive(self.active_rules.lock());
                            if active.contains(&rule.id) {
                                if stop == key {
                                    active.remove(&rule.id);
                                    self.metrics.set_active_rules(active.len());
                                    Some(ToggleAction::Stop)
                                } else {
                                    None
                                }
                            } else if rule.trigger_key == key {
                                if self.stop_all_depth.load(Ordering::SeqCst) > 0 {
                                    None
                                } else {
                                    let generation =
                                        self.stop_all_generation.load(Ordering::SeqCst);
                                    active.insert(rule.id.clone());
                                    self.metrics.set_active_rules(active.len());
                                    Some(ToggleAction::Start { generation })
                                }
                            } else {
                                None
                            }
                        };
                        match action {
                            Some(ToggleAction::Stop) => {
                                self.metrics.add_stop_command();
                                let _ = self
                                    .scheduler_tx
                                    .send(SchedulerCommand::Stop(rule.id.clone(), Instant::now()));
                                handled = true;
                            }
                            Some(ToggleAction::Start { generation }) => {
                                handled = self.try_send_start_command(rule, false, generation);
                            }
                            None => {}
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

    fn start_scheduled_burst(&self, rule: &BurstRule, allow_while_physical_down: bool) {
        let generation = {
            let mut active = revive(self.active_rules.lock());
            if self.stop_all_depth.load(Ordering::SeqCst) > 0 {
                return;
            }
            if !active.insert(rule.id.clone()) {
                return;
            }
            self.metrics.set_active_rules(active.len());
            self.stop_all_generation.load(Ordering::SeqCst)
        };

        let _ = self.try_send_start_command(rule, allow_while_physical_down, generation);
    }

    fn remove_active_rule_after_start_failure(&self, rule_id: &str) {
        let mut active = revive(self.active_rules.lock());
        if active.remove(rule_id) {
            self.metrics.set_active_rules(active.len());
        }
    }

    fn try_send_start_command(
        &self,
        rule: &BurstRule,
        allow_while_physical_down: bool,
        generation: u64,
    ) -> bool {
        if self.stop_all_depth.load(Ordering::SeqCst) > 0
            || self.stop_all_generation.load(Ordering::SeqCst) != generation
        {
            self.remove_active_rule_after_start_failure(&rule.id);
            return false;
        }

        let cmd = SchedulerCommand::Start(ScheduledRuleConfig {
            id: rule.id.clone(),
            target_key: rule.target_key,
            interval_ms: rule.interval_ms,
            allow_while_physical_down,
            stop_generation: generation,
        });
        if self.scheduler_tx.send(cmd).is_err() {
            self.remove_active_rule_after_start_failure(&rule.id);
            release_simulated_key(rule.target_key, &self.pressed_keys, &self.simulated_keys);
            return false;
        }
        true
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
        self.shutdown_inner();
    }
}

#[cfg(test)]
mod tests;
