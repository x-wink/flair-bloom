//! L2 真 OS 往返冒烟测试（Windows，默认 `#[ignore]`）。
//!
//! 验证「`win_input` SendInput 注入 → 低级键盘 hook → 还原出正确 vk/抬起/`SIM_MARKER`」整条
//! 真实链路——这是平时只能手动验的部分。为**零副作用**：自装的 hook 对带 `SIM_MARKER` 的
//! 自注入事件捕获后**直接吞掉**（返回 1，不下传），注入的按键不会泄漏到前台窗口；真实物理
//! 按键照常放行。
//!
//! 为什么 `#[ignore]`：低级 hook 需要交互式桌面会话 + 消息循环，headless / 服务态 CI 收不到
//! 注入事件。在本机或**自建带活动会话的 Windows runner** 上显式运行：
//!
//! ```sh
//! cargo test -p burst-engine -- --ignored
//! ```

use qzh_profile::key_id::KeyId;
use std::ptr::null_mut;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use win_input::SIM_MARKER;
use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYUP, WM_QUIT,
};

/// 捕获到的自注入事件：`(vkCode, is_up)`。只收带 `SIM_MARKER` 的。
static CAPTURED: OnceLock<Mutex<Vec<(u32, bool)>>> = OnceLock::new();

fn captured() -> &'static Mutex<Vec<(u32, bool)>> {
    CAPTURED.get_or_init(|| Mutex::new(Vec::new()))
}

/// 低级键盘 hook：自注入（`SIM_MARKER`）的事件捕获并吞掉，物理事件放行。
///
/// # Safety
/// 由 Windows 调用，`ncode >= 0` 时 `lparam` 指向有效 `KBDLLHOOKSTRUCT`。
unsafe extern "system" fn capture_hook(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if ncode >= 0 {
        // SAFETY: ncode>=0 时 lparam 是有效 KBDLLHOOKSTRUCT 指针，借用不超出本次调用。
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        if kb.dwExtraInfo == SIM_MARKER {
            let is_up = wparam as u32 == WM_KEYUP;
            captured()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((kb.vkCode, is_up));
            return 1; // 吞掉自注入，避免泄漏到前台窗口
        }
    }
    // SAFETY: 文档允许 null hhk；非自注入事件必须继续下传，否则会吞掉用户真实按键。
    CallNextHookEx(null_mut(), ncode, wparam, lparam)
}

#[test]
#[ignore = "需交互式桌面会话；自装 hook 捕获并吞掉自注入，无副作用。运行：cargo test -p burst-engine -- --ignored"]
fn sendinput_roundtrips_through_ll_hook_with_marker() {
    captured().lock().unwrap_or_else(|e| e.into_inner()).clear();

    // hook 必须装在自带消息循环的线程上。
    let (tid_tx, tid_rx) = mpsc::channel::<u32>();
    let hook_thread = thread::spawn(move || {
        // SAFETY: 装 hook 前记录本线程 id，供主线程投递 WM_QUIT 退出。
        let tid = unsafe { GetCurrentThreadId() };
        // SAFETY: WH_KEYBOARD_LL 允许 hmod=null + dwThreadId=0；capture_hook 满足契约。
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(capture_hook), null_mut(), 0) };
        assert!(!hook.is_null(), "安装键盘 hook 失败");
        tid_tx.send(tid).unwrap();

        // SAFETY: MSG 全 0 是合法初值；GetMessageW 按 Win32 文档写入。
        let mut msg = unsafe { std::mem::zeroed::<MSG>() };
        // GetMessageW 收到 WM_QUIT 返回 0，退出循环。
        while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {}
        // SAFETY: hook 是上面 SetWindowsHookExW 返回的有效句柄。
        unsafe { UnhookWindowsHookEx(hook) };
    });

    let tid = tid_rx.recv().expect("hook 线程未回传线程 id");
    thread::sleep(Duration::from_millis(100)); // 等 hook 就位

    // 注入良性键 F24（0x87），走默认 SendInput 通道（带 SIM_MARKER）。
    let key = KeyId::Keyboard(0x87);
    win_input::key_down(key);
    win_input::key_up(key);

    // 轮询等待捕获 down+up（最多约 1s）。
    let mut got = Vec::new();
    for _ in 0..50 {
        got = captured().lock().unwrap_or_else(|e| e.into_inner()).clone();
        if got.len() >= 2 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    // SAFETY: tid 来自 hook 线程自身写入的有效线程 id；WM_QUIT 参数合法。
    unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) };
    hook_thread.join().ok();

    assert_eq!(got.len(), 2, "应捕获注入的 down+up，实得 {got:?}");
    assert_eq!(got[0], (0x87, false), "首个应为 F24 按下");
    assert_eq!(got[1], (0x87, true), "次个应为 F24 抬起");
}
