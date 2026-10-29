//! 面板窗口拖边时锁定宽高比。
//!
//! Windows 在拖边的每一帧发 `WM_SIZING`，`lParam` 指向系统拟定的新窗口矩形，窗口过程可以当场改写它。
//! 在这里按设计比例改写，窗口拖动中就一直是正确比例，不用等松手再吸附。Tauri 没有封装宽高比，
//! 所以给面板窗口挂一个 subclass 处理这条消息；矩形换算是纯函数 [`lock_aspect`]，平台无关、可单测。

// 矩形换算只有 Windows 的 subclass 用，其它平台留着给单测跑（macOS 走系统自带的比例约束）
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

/// 设计尺寸（逻辑像素），`None` 表示还没设置、不锁比例。
static PANEL_BASE: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// 屏幕坐标下的窗口矩形（物理像素），与 Win32 `RECT` 同义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// 正在拖的边或角，对应 `WM_SIZING` 的 `WMSZ_*`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    TopLeft,
    TopRight,
    Bottom,
    BottomLeft,
    BottomRight,
}

impl Edge {
    pub fn from_wmsz(value: u32) -> Option<Self> {
        Some(match value {
            1 => Self::Left,
            2 => Self::Right,
            3 => Self::Top,
            4 => Self::TopLeft,
            5 => Self::TopRight,
            6 => Self::Bottom,
            7 => Self::BottomLeft,
            8 => Self::BottomRight,
            _ => return None,
        })
    }

    fn drags_left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    fn drags_top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }
}

/// 锁比例的约束，全部是物理像素。
#[derive(Clone, Copy, Debug)]
pub struct AspectLimits {
    /// 客户区宽高比（宽 / 高）。
    pub ratio: f64,
    /// 窗口矩形比客户区多出的宽、高（边框）；无边框窗口通常为 0。
    pub frame: (i32, i32),
    /// 客户区最小宽度，即设计宽度。
    pub min_width: i32,
    /// 客户区最大宽度，由显示器工作区装得下的最大等比尺寸决定。
    pub max_width: i32,
}

/// 按比例改写拟定矩形：拖左右边以宽为准，拖上下边以高为准，拖角取两者里较大的那个，
/// 都按比例补齐另一边。被拖的那条边移动，对边保持不动；只拖左右边时高度变化落在下边，
/// 只拖上下边时宽度变化落在右边。
pub fn lock_aspect(rect: Rect, edge: Edge, limits: AspectLimits) -> Rect {
    let AspectLimits {
        ratio,
        frame: (frame_w, frame_h),
        min_width,
        max_width,
    } = limits;
    let client_w = f64::from(rect.right - rect.left - frame_w);
    let client_h = f64::from(rect.bottom - rect.top - frame_h);
    let wanted = match edge {
        Edge::Left | Edge::Right => client_w,
        Edge::Top | Edge::Bottom => client_h * ratio,
        _ => client_w.max(client_h * ratio),
    };
    let width = (wanted.round() as i32).clamp(min_width, max_width.max(min_width));
    let height = (f64::from(width) / ratio).round() as i32;
    let outer_w = width + frame_w;
    let outer_h = height + frame_h;

    let (left, right) = if edge.drags_left() {
        (rect.right - outer_w, rect.right)
    } else {
        (rect.left, rect.left + outer_w)
    };
    let (top, bottom) = if edge.drags_top() {
        (rect.bottom - outer_h, rect.bottom)
    } else {
        (rect.top, rect.top + outer_h)
    };
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

/// 设定面板的设计尺寸并开始锁比例；切换布局时再调一次换成新比例。
/// Windows 拦 `WM_SIZING`，macOS 设 NSWindow 的内容宽高比，其它平台只记下尺寸。
#[tauri::command]
pub fn set_panel_aspect(
    window: tauri::WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if !(width > 0.0 && height > 0.0) {
        return Err(format!("设计尺寸无效：{width}×{height}"));
    }
    if let Ok(mut base) = PANEL_BASE.lock() {
        *base = Some((width, height));
    }
    #[cfg(windows)]
    win::install(&window)?;
    #[cfg(target_os = "macos")]
    mac::lock(&window, width, height)?;
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = window;
    Ok(())
}

/// macOS 的 NSWindow 自带内容区宽高比约束，拖边时由系统锁住，不需要像 Windows 那样拦消息。
#[cfg(target_os = "macos")]
mod mac {
    use objc2_app_kit::NSWindow;
    use objc2_foundation::NSSize;

    pub fn lock(window: &tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
        let ptr = window
            .ns_window()
            .map_err(|e| format!("取窗口句柄失败：{e}"))? as usize;
        window
            .run_on_main_thread(move || {
                // SAFETY: 指针来自仍然存活的面板窗口，AppKit 调用在主线程执行
                let ns_window = unsafe { &*(ptr as *const NSWindow) };
                ns_window.setContentAspectRatio(NSSize::new(width, height));
            })
            .map_err(|e| format!("派发到主线程失败：{e}"))
    }
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, WM_SIZING};

    use super::{lock_aspect, AspectLimits, Edge, Rect, PANEL_BASE};

    const SUBCLASS_ID: usize = 0x00F1_A1B0;
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// subclass 必须在窗口所属线程（主线程）上挂，否则 `SetWindowSubclass` 直接失败。
    pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let hwnd = window.hwnd().map_err(|e| format!("取窗口句柄失败：{e}"))?.0 as isize;
        window
            .run_on_main_thread(move || {
                // SAFETY: hwnd 来自仍然存活的面板窗口，回调在其所属线程执行
                let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(proc), SUBCLASS_ID, 0) };
                if ok == 0 {
                    INSTALLED.store(false, Ordering::SeqCst);
                    tracing::warn!("面板窗口挂 subclass 失败，拖边不锁比例");
                }
            })
            .map_err(|e| format!("派发到主线程失败：{e}"))
    }

    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_SIZING && lparam != 0 {
            if let (Some(edge), Some(limits)) = (Edge::from_wmsz(wparam as u32), limits(hwnd)) {
                // SAFETY: WM_SIZING 的 lParam 是系统给的可写 RECT 指针，非空已判断
                let r = &mut *(lparam as *mut RECT);
                let locked = lock_aspect(
                    Rect {
                        left: r.left,
                        top: r.top,
                        right: r.right,
                        bottom: r.bottom,
                    },
                    edge,
                    limits,
                );
                r.left = locked.left;
                r.top = locked.top;
                r.right = locked.right;
                r.bottom = locked.bottom;
                return 1;
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    /// 每帧现算：拖到另一块 DPI 不同的屏幕上时，最小尺寸与工作区都要跟着换。
    unsafe fn limits(hwnd: HWND) -> Option<AspectLimits> {
        let (base_w, base_h) = (*PANEL_BASE.lock().ok()?)?;
        let scale = f64::from(GetDpiForWindow(hwnd).max(96)) / 96.0;
        let ratio = base_w / base_h;

        let mut window: RECT = std::mem::zeroed();
        let mut client: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut window) == 0 || GetClientRect(hwnd, &mut client) == 0 {
            return None;
        }
        let frame_w = (window.right - window.left) - (client.right - client.left);
        let frame_h = (window.bottom - window.top) - (client.bottom - client.top);

        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let max_width = if GetMonitorInfoW(monitor, &mut info) != 0 {
            let work_w = f64::from(info.rcWork.right - info.rcWork.left - frame_w);
            let work_h = f64::from(info.rcWork.bottom - info.rcWork.top - frame_h);
            work_w.min(work_h * ratio).floor() as i32
        } else {
            i32::MAX
        };
        Some(AspectLimits {
            ratio,
            frame: (frame_w, frame_h),
            min_width: (base_w * scale).round() as i32,
            max_width,
        })
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
