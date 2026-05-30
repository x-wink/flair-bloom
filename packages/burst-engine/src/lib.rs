use qzh_profile::key_id::KeyId;
#[cfg(any(test, windows))]
use qzh_profile::key_id::MouseButton;
use qzh_profile::profile::{BurstMode, BurstRule, Hotkeys};
#[cfg(windows)]
use std::sync::{atomic::AtomicU32, RwLock, Weak};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
#[cfg(windows)]
use tracing::error;
use tracing::info;
#[cfg(windows)]
use win_input::{clear_pending_injections, try_consume_injection, SIM_MARKER};
use win_input::{key_down, key_up};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
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

type ActiveLoops = Arc<
    Mutex<
        HashMap<
            String,
            (
                Arc<AtomicBool>,
                thread::Thread,
                KeyId,
                thread::JoinHandle<()>,
            ),
        >,
    >,
>;
type GlobalChangedCb = Arc<Mutex<Option<Box<dyn Fn(bool) + Send + Sync>>>>;
type PanelToggleCb = Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>;

/// 关键路径锁兜底：若前一持锁者 panic 导致 Mutex 中毒,强行复活并继续。
/// 按键工具最差故障是键盘卡死,值得给一层硬兜底；连发线程已被 catch_unwind
/// 包裹,持锁期间也仅做 HashMap/Vec 操作,正常路径不会中毒。
fn revive<T>(r: std::sync::LockResult<T>) -> T {
    r.unwrap_or_else(|e| e.into_inner())
}

pub struct BurstEngine {
    pub global_enabled: Arc<AtomicBool>,
    rules: Arc<Mutex<Vec<BurstRule>>>,
    /// rule_id -> (cancel_flag, thread_handle, target_key, join_handle)
    active_loops: ActiveLoops,
    toggle_states: Arc<Mutex<HashMap<String, bool>>>,
    /// 当前物理按下的键；用于过滤 OS 生成的 key-repeat。
    pressed_keys: Arc<Mutex<HashSet<KeyId>>>,
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
        Self {
            global_enabled: Arc::new(AtomicBool::new(false)),
            rules: Arc::new(Mutex::new(Vec::new())),
            active_loops: Arc::new(Mutex::new(HashMap::new())),
            toggle_states: Arc::new(Mutex::new(HashMap::new())),
            pressed_keys: Arc::new(Mutex::new(HashSet::new())),
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
        let mut loops = revive(self.active_loops.lock());
        for (cancel, thread_handle, _, _) in loops.values() {
            cancel.store(true, Ordering::SeqCst);
            thread_handle.unpark();
            thread_handle.unpark();
        }
        loops.clear();
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
        self.cancel_all_loops();
        *revive(self.rules.lock()) = rules;
    }

    pub fn get_rules(&self) -> Vec<BurstRule> {
        revive(self.rules.lock()).clone()
    }

    /// 当前正在执行连发的规则 ID 集合：hold 模式表示触发键被按住，toggle 模式表示已开启。
    /// 用于前端轮询展示激活态视觉反馈。
    pub fn get_active_ids(&self) -> Vec<String> {
        revive(self.active_loops.lock()).keys().cloned().collect()
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
        let rules = revive(self.rules.lock()).clone();
        let mut handled = false;
        for rule in rules.iter().filter(|r| r.enabled) {
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
        for rule in rules
            .iter()
            .filter(|r| r.enabled && r.trigger_key == key && r.mode == BurstMode::Hold)
        {
            self.stop_burst(&rule.id);
        }
    }

    fn start_hold_burst(&self, rule: &BurstRule) {
        if revive(self.active_loops.lock()).contains_key(&rule.id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        // spawn 在锁外执行，避免 thread::spawn panic 时中毒 Mutex
        let handle = thread::spawn(move || {
            let hold_ms = (interval_ms as u64 / 3)
                .clamp(5, 30)
                .min((interval_ms as u64).saturating_sub(1));
            let rest_ms = interval_ms as u64 - hold_ms;
            let result = std::panic::catch_unwind(|| {
                while !cancel_clone.load(Ordering::SeqCst) {
                    key_down(target_key);
                    thread::park_timeout(Duration::from_millis(hold_ms));
                    key_up(target_key);
                    if cancel_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::park_timeout(Duration::from_millis(rest_ms));
                }
            });
            if result.is_err() {
                key_up(target_key);
            }
        });
        let mut loops = revive(self.active_loops.lock());
        if loops.contains_key(&rule.id) {
            // spawn 后极罕见的并发插入：取消刚启动的线程
            cancel.store(true, Ordering::SeqCst);
            handle.thread().unpark();
            return;
        }
        loops.insert(
            rule.id.clone(),
            (cancel, handle.thread().clone(), target_key, handle),
        );
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
        if revive(self.active_loops.lock()).contains_key(&rule.id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let target_key = rule.target_key;
        let interval_ms = rule.interval_ms;
        let handle = thread::spawn(move || {
            let hold_ms = (interval_ms as u64 / 3)
                .clamp(5, 30)
                .min((interval_ms as u64).saturating_sub(1));
            let rest_ms = interval_ms as u64 - hold_ms;
            let result = std::panic::catch_unwind(|| {
                while !cancel_clone.load(Ordering::SeqCst) {
                    key_down(target_key);
                    thread::park_timeout(Duration::from_millis(hold_ms));
                    key_up(target_key);
                    if cancel_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::park_timeout(Duration::from_millis(rest_ms));
                }
            });
            if result.is_err() {
                key_up(target_key);
            }
        });
        let mut loops = revive(self.active_loops.lock());
        if loops.contains_key(&rule.id) {
            cancel.store(true, Ordering::SeqCst);
            handle.thread().unpark();
            return;
        }
        loops.insert(
            rule.id.clone(),
            (cancel, handle.thread().clone(), target_key, handle),
        );
    }

    fn stop_burst(&self, rule_id: &str) {
        if let Some((cancel, thread_handle, _, _join)) =
            revive(self.active_loops.lock()).remove(rule_id)
        {
            cancel.store(true, Ordering::SeqCst);
            // 调用两次覆盖 hold_ms 和 rest_ms 两个 park 点：
            // token 是单 bit，若线程在 hold_ms 睡眠则第一次唤醒、第二次 no-op；
            // 若 token 已被 hold_ms 消耗而线程进入 rest_ms，第二次唤醒 rest_ms。
            thread_handle.unpark();
            thread_handle.unpark();
        }
    }
}

impl Drop for BurstEngine {
    fn drop(&mut self) {
        let entries: Vec<_> = {
            let mut loops = revive(self.active_loops.lock());
            loops.drain().map(|(_, v)| v).collect()
        };
        // 先全部发出取消信号
        for (cancel, thread_handle, _, _) in &entries {
            cancel.store(true, Ordering::SeqCst);
            thread_handle.unpark();
            thread_handle.unpark();
        }
        // join 后再补发 key_up，确保线程已完成自身的 key_up，避免与线程竞态
        for (_, _, target_key, join_handle) in entries {
            // 仅在线程 panic 时补发 key_up；正常退出路径线程已自行调用
            if join_handle.join().is_err() {
                key_up(target_key);
            }
        }
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
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        engine.on_key_press(key);
                    }
                    WM_KEYUP | WM_SYSKEYUP => engine.on_key_release(key),
                    _ => {}
                }
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
                    if is_up {
                        engine.on_key_release(key);
                    } else {
                        engine.on_key_press(key);
                    }
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
                        engine.on_key_press(key);
                        engine.on_key_release(key);
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
