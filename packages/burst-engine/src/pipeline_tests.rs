//! 引擎管线确定性测试（L1.5）：注入「命令录制」调度器替身，对
//! 「合成物理按键 → 引擎状态机 → 发给调度器的命令（含 generation）」做 golden 断言。
//!
//! 与 `lib.rs` 里的 `active_ids` 测试互补：那些只断言引擎自身状态，这里断言**实际下发到
//! 调度器的命令序列**——能抓住「状态对但命令错/漏/generation 不匹配」这类引擎↔调度契约 bug。
//! 真实注入时序由 `scheduler/sim_tests.rs`（L1）覆盖。

use crate::scheduler::Scheduler;
use crate::BurstEngine;
use qzh_profile::key_id::KeyId;
use qzh_profile::profile::{BurstMode, BurstRule};
use std::sync::{Arc, Mutex};

/// 只记录引擎下发命令、不做任何调度的替身。阻塞类命令一律返回 true（成功），
/// 使引擎走正常路径而非 simulated_keys 兜底。
#[derive(Default)]
struct RecordingScheduler {
    cmds: Mutex<Vec<String>>,
}

impl RecordingScheduler {
    fn clear(&self) {
        self.cmds.lock().unwrap().clear();
    }
    fn cmds(&self) -> Vec<String> {
        self.cmds.lock().unwrap().clone()
    }
    fn log(&self, s: String) {
        self.cmds.lock().unwrap().push(s);
    }
}

impl Scheduler for RecordingScheduler {
    fn start_rule(&self, rule: Arc<BurstRule>, generation: u64) {
        self.log(format!("start:{}:g{generation}", rule.id));
    }
    fn stop_rule(&self, rule_id: String, generation: u64) {
        self.log(format!("stop:{rule_id}:g{generation}"));
    }
    fn stop_all_async(&self, generation: u64) {
        self.log(format!("stopall:g{generation}"));
    }
    fn stop_all_blocking(&self, generation: u64) -> bool {
        self.log(format!("stopall_blocking:g{generation}"));
        true
    }
    fn shutdown_blocking(&self, generation: u64) -> bool {
        self.log(format!("shutdown:g{generation}"));
        true
    }
    fn hp_degraded(&self) -> bool {
        false
    }
}

fn rule(id: &str, mode: BurstMode, trigger: KeyId, target: KeyId) -> BurstRule {
    BurstRule {
        id: id.to_string(),
        enabled: true,
        trigger_key: trigger,
        target_key: target,
        mode,
        stop_key: None,
        interval_ms: 10,
        group: None,
    }
}

/// 建好引擎、装规则、开全局开关，并清掉装规则引发的 stop_all，
/// 使后续断言只看「按键引发的命令」。装规则会把 generation 推进到 1。
fn setup(rules: Vec<BurstRule>) -> (BurstEngine, Arc<RecordingScheduler>) {
    let rec = Arc::new(RecordingScheduler::default());
    let engine = BurstEngine::new_with_scheduler(rec.clone());
    engine.set_rules(rules);
    engine.set_global_enabled(true, false);
    rec.clear();
    (engine, rec)
}

#[test]
fn hold_press_release_emits_start_then_stop_same_generation() {
    let trigger = KeyId::Keyboard(0x51);
    let (engine, rec) = setup(vec![rule(
        "h",
        BurstMode::Hold,
        trigger,
        KeyId::Keyboard(0x45),
    )]);

    engine.on_key_press(trigger);
    engine.on_key_release(trigger);

    assert_eq!(rec.cmds(), vec!["start:h:g1", "stop:h:g1"]);
}

#[test]
fn toggle_same_key_starts_then_stops_on_next_press() {
    let trigger = KeyId::Keyboard(0x51);
    let (engine, rec) = setup(vec![rule(
        "t",
        BurstMode::Toggle,
        trigger,
        KeyId::Keyboard(0x45),
    )]);

    engine.on_key_press(trigger); // 开
    engine.on_key_release(trigger);
    engine.on_key_press(trigger); // 关

    assert_eq!(rec.cmds(), vec!["start:t:g1", "stop:t:g1"]);
}

#[test]
fn toggle_group_displacement_emits_stop_old_then_start_new() {
    let mut a = rule(
        "a",
        BurstMode::Toggle,
        KeyId::Keyboard(0x51),
        KeyId::Keyboard(0x41),
    );
    let mut b = rule(
        "b",
        BurstMode::Toggle,
        KeyId::Keyboard(0x45),
        KeyId::Keyboard(0x42),
    );
    a.group = Some("g".into());
    b.group = Some("g".into());
    let (engine, rec) = setup(vec![a, b]);

    engine.on_key_press(KeyId::Keyboard(0x51)); // 启动 a
    engine.on_key_release(KeyId::Keyboard(0x51));
    engine.on_key_press(KeyId::Keyboard(0x45)); // 顶替 a、启动 b

    // 契约：顶替必须先给调度器发 stop:a，再发 start:b（否则共享/残留会错乱）。
    assert_eq!(rec.cmds(), vec!["start:a:g1", "stop:a:g1", "start:b:g1"]);
}

#[test]
fn disabling_global_switch_issues_stop_all_with_bumped_generation() {
    let trigger = KeyId::Keyboard(0x51);
    let (engine, rec) = setup(vec![rule(
        "t",
        BurstMode::Toggle,
        trigger,
        KeyId::Keyboard(0x45),
    )]);

    engine.on_key_press(trigger); // toggle 开
    rec.clear();
    engine.set_global_enabled(false, false); // 暂停 → stop_all（generation 递增到 2）

    assert_eq!(rec.cmds(), vec!["stopall:g2"]);
}

#[test]
fn dedicated_stop_hotkey_issues_stop_all() {
    let toggle = KeyId::Keyboard(0x51);
    let stop = KeyId::Keyboard(0x71);
    let (engine, rec) = setup(vec![rule(
        "t",
        BurstMode::Toggle,
        toggle,
        KeyId::Keyboard(0x45),
    )]);
    engine.set_hotkeys(qzh_profile::profile::Hotkeys {
        global_toggle: Some(toggle),
        global_stop: Some(stop),
        ..Default::default()
    });

    engine.on_key_press(toggle); // toggle 开
    rec.clear();
    engine.on_key_press(stop); // 专用停止键 → 关全局 → stop_all

    assert_eq!(rec.cmds(), vec!["stopall:g2"]);
}
