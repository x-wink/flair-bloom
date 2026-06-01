use super::*;
use crate::safety::{safe_key_down, safe_key_up};
use crate::scheduler::{
    handle_scheduler_command, step_due_rules, wait_standard, ScheduledRule, SchedulerContext,
    SchedulerWaitOutcome, SchedulerWaiter,
};
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
        scheduler_handle: Mutex::new(None),
        lifecycle_state: AtomicU8::new(LIFECYCLE_PAUSED),
        scheduler_hp_degraded: Arc::new(AtomicBool::new(false)),
        on_scheduler_degraded: Arc::new(Mutex::new(None)),
        stop_all_depth: Arc::new(AtomicUsize::new(0)),
        stop_all_generation: Arc::new(AtomicU64::new(0)),
        metrics: Arc::new(EngineMetrics::new()),
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
fn shutdown_is_idempotent() {
    let engine = BurstEngine::new();
    assert!(engine.enable_runtime());

    engine.shutdown();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Shutdown);

    engine.shutdown();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Shutdown);
}

#[test]
fn on_key_press_returns_false_after_shutdown() {
    let engine = BurstEngine::new();
    let trigger = KeyId::Keyboard(0x51);
    let target = KeyId::Keyboard(0x45);
    engine.global_enabled.store(true, Ordering::SeqCst);
    engine.set_rules(vec![rule("hold-q", BurstMode::Hold, trigger, target)]);
    engine.shutdown();

    assert!(!engine.on_key_press(trigger));
    assert!(engine.get_active_ids().is_empty());
}

#[test]
fn cancel_all_loops_is_noop_after_shutdown() {
    let engine = BurstEngine::new();
    assert!(engine.enable_runtime());
    engine.shutdown();

    engine.cancel_all_loops();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Shutdown);
}

#[test]
fn pause_runtime_is_noop_after_shutdown() {
    let engine = BurstEngine::new();
    assert!(engine.enable_runtime());
    engine.shutdown();

    engine.pause_runtime();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Shutdown);
    assert!(!engine.global_enabled.load(Ordering::SeqCst));
}

#[test]
fn toggle_rule_blocked_by_stop_all_depth() {
    let engine = BurstEngine::new();
    let trigger = KeyId::Keyboard(0x51);
    let target = KeyId::Keyboard(0x45);
    engine.global_enabled.store(true, Ordering::SeqCst);
    engine.set_rules(vec![rule("toggle-q", BurstMode::Toggle, trigger, target)]);
    engine.stop_all_depth.store(1, Ordering::SeqCst);

    assert!(!engine.on_key_press(trigger));
    assert!(engine.get_active_ids().is_empty());
}

#[test]
fn global_stop_hotkey_ignored_when_already_disabled() {
    let engine = BurstEngine::new();
    let start_key = KeyId::Keyboard(0x70);
    let stop_key = KeyId::Keyboard(0x71);
    engine.set_hotkeys(Hotkeys {
        global_toggle: Some(start_key),
        global_stop: Some(stop_key),
        ..Default::default()
    });
    // global_enabled=false 时停止热键不应生效
    assert!(!engine.on_key_press(stop_key));
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
fn global_stop_takes_priority_over_panel_when_hotkeys_conflict() {
    let engine = BurstEngine::new();
    let start = KeyId::Keyboard(0x70);
    let stop = KeyId::Keyboard(0x71);
    let panel_calls = Arc::new(AtomicUsize::new(0));
    let panel_calls_for_cb = panel_calls.clone();
    engine.set_hotkeys(Hotkeys {
        global_toggle: Some(start),
        global_stop: Some(stop),
        panel_toggle: Some(stop),
    });
    engine.set_on_panel_toggle(move || {
        panel_calls_for_cb.fetch_add(1, Ordering::SeqCst);
    });
    engine.global_enabled.store(true, Ordering::SeqCst);

    assert!(engine.on_key_press(stop));

    assert!(!engine.global_enabled.load(Ordering::SeqCst));
    assert_eq!(panel_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn set_rules_normalizes_unsafe_boundaries() {
    let engine = BurstEngine::new();
    let rules = (0..=MAX_RULES)
        .map(|i| {
            let mut r = rule(
                &format!("r{i}"),
                BurstMode::Hold,
                KeyId::Keyboard(0x51),
                KeyId::Keyboard(0x45),
            );
            r.interval_ms = if i == 0 { 0 } else { 100_000 };
            r
        })
        .collect();

    engine.set_rules(rules);
    let rules = engine.get_rules();

    assert_eq!(rules.len(), MAX_RULES);
    assert_eq!(rules[0].interval_ms, MIN_BURST_INTERVAL_MS);
    assert!(rules[1..]
        .iter()
        .all(|rule| rule.interval_ms == MAX_BURST_INTERVAL_MS));
}

#[test]
fn set_rules_deduplicates_rule_ids_before_snapshotting() {
    let engine = BurstEngine::new();
    let trigger = KeyId::Keyboard(0x51);
    let target = KeyId::Keyboard(0x45);
    let duplicate_trigger = KeyId::Keyboard(0x57);
    let duplicate_target = KeyId::Keyboard(0x52);

    engine.set_rules(vec![
        rule("same-id", BurstMode::Hold, trigger, target),
        rule(
            "same-id",
            BurstMode::Toggle,
            duplicate_trigger,
            duplicate_target,
        ),
    ]);
    let rules = engine.get_rules();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].trigger_key, trigger);
    assert_eq!(rules[0].target_key, target);
}

#[test]
fn stopping_all_gate_blocks_new_rule_starts() {
    let engine = BurstEngine::new();
    let trigger = KeyId::Keyboard(0x51);
    let target = KeyId::Keyboard(0x45);
    engine.global_enabled.store(true, Ordering::SeqCst);
    engine.set_rules(vec![rule("hold-q", BurstMode::Hold, trigger, target)]);
    engine.stop_all_depth.store(1, Ordering::SeqCst);

    assert!(!engine.on_key_press(trigger));
    assert!(engine.get_active_ids().is_empty());
}

#[test]
fn runtime_lifecycle_distinguishes_pause_from_shutdown() {
    let engine = BurstEngine::new();

    assert_eq!(engine.lifecycle(), EngineLifecycle::Paused);
    assert!(engine.enable_runtime());
    assert_eq!(engine.lifecycle(), EngineLifecycle::Running);
    assert!(engine.global_enabled.load(Ordering::SeqCst));

    engine.pause_runtime();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Paused);
    assert!(!engine.global_enabled.load(Ordering::SeqCst));

    assert!(engine.enable_runtime());
    engine.shutdown();
    assert_eq!(engine.lifecycle(), EngineLifecycle::Shutdown);
    assert!(!engine.global_enabled.load(Ordering::SeqCst));
    assert!(!engine.enable_runtime());
}

#[test]
fn stopping_active_rules_keeps_runtime_enabled() {
    let engine = BurstEngine::new();
    assert!(engine.enable_runtime());

    engine.cancel_all_loops();

    assert_eq!(engine.lifecycle(), EngineLifecycle::Running);
    assert!(engine.global_enabled.load(Ordering::SeqCst));
}

#[test]
fn scheduler_drops_start_from_previous_stop_generation() {
    let target = KeyId::Keyboard(0x45);
    let physical_keys = Arc::new(Mutex::new(HashSet::new()));
    let simulated_keys = Arc::new(Mutex::new(HashMap::new()));
    let active_rules = Arc::new(Mutex::new(HashSet::new()));
    let metrics = EngineMetrics::new();
    let stop_all_generation = AtomicU64::new(1);
    let context = SchedulerContext {
        stop_all_generation: &stop_all_generation,
        physical_keys: &physical_keys,
        simulated_keys: &simulated_keys,
        active_rules: &active_rules,
        metrics: &metrics,
    };
    let mut scheduled_rules = HashMap::new();
    let mut target_holds = HashMap::new();

    let should_shutdown = handle_scheduler_command(
        SchedulerCommand::Start(ScheduledRuleConfig {
            id: "stale".to_string(),
            target_key: target,
            interval_ms: 10,
            allow_while_physical_down: false,
            stop_generation: 0,
        }),
        &mut scheduled_rules,
        &mut target_holds,
        &context,
    );

    assert!(!should_shutdown);
    assert!(scheduled_rules.is_empty());
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
fn cancel_all_loops_waits_for_scheduler_to_release_simulated_ledger() {
    let key = KeyId::Keyboard(0x45);
    let engine = BurstEngine::new();
    revive(engine.simulated_keys.lock()).insert(key, 1);

    engine.cancel_all_loops();

    assert!(revive(engine.simulated_keys.lock()).is_empty());
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
    let mut waiter = SchedulerWaiter::new(
        wake,
        Arc::new(Mutex::new(None)),
        Arc::new(AtomicBool::new(false)),
    );

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
                    stop_generation: 0,
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
                    stop_generation: 0,
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
fn plan_key_up_releases_even_when_physically_held() {
    // 回归测试：Toggle 连发激活时物理按压 target 键导致驱动侧按键卡死。
    // plan_key_up 不得以"非物理按下"为条件跳过 key_up。
    use crate::safety::plan_key_up;
    let key = KeyId::Keyboard(0x57);
    let physical_keys = Arc::new(Mutex::new(HashSet::from([key])));
    let simulated_keys = Arc::new(Mutex::new(HashMap::from([(key, 1usize)])));

    let event = plan_key_up(key, &physical_keys, &simulated_keys, false);

    assert_eq!(event, Some((key, true)));
    assert!(revive(simulated_keys.lock()).is_empty());
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
