use super::input::{rdev_key_to_vk, simulate_keypress, SIM_COUNT};
use qzh_format::profile::{BurstMode, BurstRule};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tracing::{error, info};

pub struct BurstEngine {
    pub global_enabled: Arc<AtomicBool>,
    rules: Arc<Mutex<Vec<BurstRule>>>,
    // rule_id -> cancel flag; Hold mode loops, Toggle mode loops
    active_loops: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    // Toggle mode state: rule_id -> active
    toggle_states: Arc<Mutex<HashMap<String, bool>>>,
}

impl BurstEngine {
    pub fn new() -> Self {
        Self {
            global_enabled: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(Mutex::new(Vec::new())),
            active_loops: Arc::new(Mutex::new(HashMap::new())),
            toggle_states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_rules(&self, rules: Vec<BurstRule>) {
        *self.rules.lock().unwrap() = rules;
    }

    pub fn get_rules(&self) -> Vec<BurstRule> {
        self.rules.lock().unwrap().clone()
    }

    pub fn on_key_press(&self, vk: u32) {
        if !self.global_enabled.load(Ordering::SeqCst) {
            return;
        }
        let rules = self.rules.lock().unwrap().clone();
        for rule in rules.iter().filter(|r| r.enabled) {
            match rule.mode {
                BurstMode::Hold => {
                    if rule.trigger_key == vk {
                        self.start_hold_burst(rule);
                    }
                }
                BurstMode::Toggle => {
                    let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                    if rule.trigger_key == vk || stop == vk {
                        let started = self
                            .toggle_states
                            .lock()
                            .unwrap()
                            .get(&rule.id)
                            .copied()
                            .unwrap_or(false);
                        if started {
                            if stop == vk {
                                self.handle_toggle_press(rule);
                            }
                        } else if rule.trigger_key == vk {
                            self.handle_toggle_press(rule);
                        }
                    }
                }
            }
        }
    }

    pub fn on_key_release(&self, vk: u32) {
        let rules = self.rules.lock().unwrap().clone();
        for rule in rules
            .iter()
            .filter(|r| r.enabled && r.trigger_key == vk && r.mode == BurstMode::Hold)
        {
            self.stop_burst(&rule.id);
        }
    }

    fn start_hold_burst(&self, rule: &BurstRule) {
        let mut loops = self.active_loops.lock().unwrap();
        if loops.contains_key(&rule.id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        loops.insert(rule.id.clone(), cancel.clone());
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        thread::spawn(move || {
            while !cancel.load(Ordering::SeqCst) {
                simulate_keypress(target_key);
                thread::sleep(Duration::from_millis(interval_ms as u64));
            }
        });
    }

    fn handle_toggle_press(&self, rule: &BurstRule) {
        let mut states = self.toggle_states.lock().unwrap();
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
        let mut loops = self.active_loops.lock().unwrap();
        if loops.contains_key(&rule.id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        loops.insert(rule.id.clone(), cancel.clone());
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        thread::spawn(move || {
            while !cancel.load(Ordering::SeqCst) {
                simulate_keypress(target_key);
                thread::sleep(Duration::from_millis(interval_ms as u64));
            }
        });
    }

    fn stop_burst(&self, rule_id: &str) {
        if let Some(cancel) = self.active_loops.lock().unwrap().remove(rule_id) {
            cancel.store(true, Ordering::SeqCst);
        }
    }
}

pub fn start_listener(engine: Arc<BurstEngine>) {
    thread::spawn(move || loop {
        let engine_ref = engine.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let callback = move |event: rdev::Event| {
                if SIM_COUNT.load(Ordering::SeqCst) > 0 {
                    return;
                }
                match event.event_type {
                    rdev::EventType::KeyPress(key) => {
                        let vk = rdev_key_to_vk(key);
                        if vk != 0 {
                            engine_ref.on_key_press(vk);
                        }
                    }
                    rdev::EventType::KeyRelease(key) => {
                        let vk = rdev_key_to_vk(key);
                        if vk != 0 {
                            engine_ref.on_key_release(vk);
                        }
                    }
                    _ => {}
                }
            };
            rdev::listen(callback)
        }));
        match result {
            Ok(Ok(())) => {
                info!("engine listener exited cleanly");
                break;
            }
            Ok(Err(e)) => {
                error!("rdev listen error: {:?}", e);
                break;
            }
            Err(_) => {
                error!("engine listener panicked, restarting in 1s");
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
    info!("burst engine listener started");
}
