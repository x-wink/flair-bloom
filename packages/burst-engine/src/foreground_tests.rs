use super::*;
use crate::test_support::{rule, setup};

fn key(vk: u32) -> KeyId {
    KeyId::Keyboard(vk)
}

#[test]
fn window_pause_retains_toggle_cursor_and_requires_new_hold_press() {
    let (engine, scheduler) = setup(vec![
        rule("toggle", BurstMode::Toggle, key(81), key(69)),
        rule("hold", BurstMode::Hold, key(82), key(70)),
    ]);
    engine.on_key_press(key(81));
    engine.on_key_release(key(81));
    engine.on_key_press(key(82));
    scheduler.clear();
    engine.set_foreground_open(false);
    assert!(engine.global_enabled.load(Ordering::SeqCst));
    assert_eq!(engine.get_rule_states().paused, ["toggle"]);
    assert!(scheduler.cmds().contains(&"pause:toggle:g1".into()));
    assert!(scheduler.cmds().contains(&"stop:hold:g1".into()));
    scheduler.clear();
    engine.set_foreground_open(true);
    assert_eq!(scheduler.cmds(), ["resume:toggle:g1"]);
    assert_eq!(engine.get_rule_states().running, ["toggle"]);
    engine.on_key_release(key(82));
    assert!(engine.on_key_press(key(82)));
}

#[test]
fn suspended_stop_key_and_global_stop_cancel_restore() {
    let (engine, scheduler) = setup(vec![rule("toggle", BurstMode::Toggle, key(81), key(69))]);
    engine.on_key_press(key(81));
    engine.on_key_release(key(81));
    engine.set_foreground_open(false);
    assert!(engine.on_key_press(key(81)));
    assert!(engine.get_active_ids().is_empty());
    engine.set_foreground_open(true);
    engine.on_key_release(key(81));
    engine.on_key_press(key(81));
    engine.set_foreground_open(false);
    engine.set_global_enabled(false, false);
    scheduler.clear();
    engine.set_foreground_open(true);
    assert!(scheduler.cmds().is_empty());
    assert!(engine.get_active_ids().is_empty());
}

#[test]
fn window_pause_unwinds_hold_preemption_and_preserves_pending_toggle() {
    for pending in [false, true] {
        let mut toggle = rule("toggle", BurstMode::Toggle, key(81), key(69));
        toggle.group = Some("group".into());
        let mut hold = rule("hold", BurstMode::Hold, key(82), key(70));
        hold.group = Some("group".into());
        let (engine, scheduler) = setup(vec![toggle, hold]);
        if pending {
            engine.on_key_press(key(82));
            engine.on_key_press(key(81));
        } else {
            engine.on_key_press(key(81));
            engine.on_key_press(key(82));
        }
        engine.set_foreground_open(false);
        scheduler.clear();
        engine.set_foreground_open(true);
        assert_eq!(
            scheduler.cmds(),
            [if pending {
                "start:toggle:g1"
            } else {
                "resume:toggle:g1"
            }]
        );
        assert_eq!(engine.get_rule_states().running, ["toggle"]);
    }
}

#[test]
fn routed_input_cannot_start_after_focus_round_trip() {
    let (engine, scheduler) = setup(vec![rule("toggle", BurstMode::Toggle, key(81), key(69))]);
    let engine = Arc::new(engine);
    let (tx, rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    *revive(engine.routing_pause.lock()) = Some(Box::new(move || {
        tx.send(()).unwrap();
        resume_rx.recv().unwrap();
    }));
    let thread = {
        let engine = engine.clone();
        std::thread::spawn(move || engine.on_key_press(key(81)))
    };
    rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    engine.set_foreground_open(false);
    engine.set_foreground_open(true);
    scheduler.clear();
    resume_tx.send(()).unwrap();
    assert!(!thread.join().unwrap());
    assert!(scheduler.cmds().is_empty());
}
