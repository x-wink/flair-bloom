use super::input::{simulate_keypress, SIM_MARKER};
use qzh_format::profile::{BurstMode, BurstRule};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    thread,
    time::Duration,
};
use tracing::{error, info};
use windows_sys::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

/// hook 回调通过静态 Weak 引用访问引擎，避免 Arc 延长生命周期
static ENGINE_HOOK: OnceLock<Weak<BurstEngine>> = OnceLock::new();

type ActiveLoops = Arc<Mutex<HashMap<String, (Arc<AtomicBool>, thread::Thread)>>>;

pub struct BurstEngine {
    pub global_enabled: Arc<AtomicBool>,
    rules: Arc<Mutex<Vec<BurstRule>>>,
    /// rule_id -> (cancel_flag, thread_handle)；thread_handle 用于 unpark 即时停止
    active_loops: ActiveLoops,
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
        let mut loops = self.active_loops.lock().unwrap();
        for (cancel, thread_handle) in loops.values() {
            cancel.store(true, Ordering::SeqCst);
            thread_handle.unpark();
        }
        loops.clear();
        drop(loops);
        self.toggle_states.lock().unwrap().clear();
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
        let cancel_clone = cancel.clone();
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        let handle = thread::spawn(move || {
            while !cancel_clone.load(Ordering::SeqCst) {
                simulate_keypress(target_key);
                // park_timeout 可被 unpark() 立即打断，确保停止命令即时响应
                thread::park_timeout(Duration::from_millis(interval_ms as u64));
            }
        });
        loops.insert(rule.id.clone(), (cancel, handle.thread().clone()));
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
        let cancel_clone = cancel.clone();
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        let handle = thread::spawn(move || {
            while !cancel_clone.load(Ordering::SeqCst) {
                simulate_keypress(target_key);
                thread::park_timeout(Duration::from_millis(interval_ms as u64));
            }
        });
        loops.insert(rule.id.clone(), (cancel, handle.thread().clone()));
    }

    fn stop_burst(&self, rule_id: &str) {
        if let Some((cancel, thread_handle)) = self.active_loops.lock().unwrap().remove(rule_id) {
            cancel.store(true, Ordering::SeqCst);
            // unpark 立即唤醒处于 park_timeout 中的连发线程
            thread_handle.unpark();
        }
    }
}

/// KF_REPEAT (0x4000) >> 8：KBDLLHOOKSTRUCT.flags 第 6 位，OS key-repeat 时置位。
/// Microsoft SDK 未定义命名常量，但与 LLKHF_EXTENDED/ALTDOWN/UP 的推导规律一致。
const LLKHF_REPEAT: u32 = 0x40;

/// WH_KEYBOARD_LL 低级键盘钩子回调；运行在安装 hook 的线程（消息循环线程）上
unsafe extern "system" fn hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if ncode >= 0 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        // 通过 dwExtraInfo 精确过滤 SendInput 模拟事件，无竞态
        if kb.dwExtraInfo != SIM_MARKER {
            if let Some(engine) = ENGINE_HOOK.get().and_then(|w| w.upgrade()) {
                match wparam as u32 {
                    // key-repeat 时跳过：Toggle 模式下持续按键会反复开关连发
                    WM_KEYDOWN | WM_SYSKEYDOWN if (kb.flags & LLKHF_REPEAT) == 0 => {
                        engine.on_key_press(kb.vkCode)
                    }
                    WM_KEYUP | WM_SYSKEYUP => engine.on_key_release(kb.vkCode),
                    _ => {}
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), ncode, wparam, lparam)
}

pub fn start_listener(engine: Arc<BurstEngine>) {
    let _ = ENGINE_HOOK.set(Arc::downgrade(&engine));
    thread::spawn(move || {
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0) };
        if hook.is_null() {
            error!("安装键盘 hook 失败");
            return;
        }
        info!("键盘 hook 已安装");
        // WH_KEYBOARD_LL 要求安装线程持续运行消息循环，否则 Windows 会在超时后移除 hook
        let mut msg = unsafe { std::mem::zeroed::<MSG>() };
        loop {
            let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if ret == 0 || ret == -1 {
                break;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        unsafe { UnhookWindowsHookEx(hook) };
        info!("键盘 hook 已卸载");
    });
    info!("连发引擎监听器已启动");
}
