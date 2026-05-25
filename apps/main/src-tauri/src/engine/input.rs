#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
};

/// 写入 SendInput 的 dwExtraInfo，hook 据此过滤程序自身模拟的按键，消除竞态
pub const SIM_MARKER: usize = 0x5148_5844;

#[cfg(windows)]
unsafe fn send(vk: u32, flags: u32) {
    let scan_ex = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX);
    let scan = (scan_ex & 0xFF) as u16;
    let prefix = (scan_ex >> 8) & 0xFF;
    // scan==0 或 E1 前缀（Pause 等需特殊序列）→ 回退到纯 VK 模式
    let (w_vk, w_scan, scan_flags) = if scan == 0 || prefix == 0xE1 {
        (vk as u16, 0u16, 0u32)
    } else {
        let ext = if prefix == 0xE0 { KEYEVENTF_EXTENDEDKEY } else { 0 };
        (0u16, scan, KEYEVENTF_SCANCODE | ext)
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: w_vk,
                wScan: w_scan,
                dwFlags: scan_flags | flags,
                time: 0,
                dwExtraInfo: SIM_MARKER,
            },
        },
    };
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

pub fn key_down(vk: u32) {
    #[cfg(windows)]
    unsafe {
        send(vk, 0);
    }
    #[cfg(not(windows))]
    let _ = vk;
}

pub fn key_up(vk: u32) {
    #[cfg(windows)]
    unsafe {
        send(vk, KEYEVENTF_KEYUP);
    }
    #[cfg(not(windows))]
    let _ = vk;
}
