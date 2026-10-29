//! 面板窗口拖边时锁定宽高比，并以拖动开始时的窗口中心为准缩放。
//!
//! Windows 在拖边的每一帧发 `WM_SIZING`，`lParam` 指向系统拟定的新窗口矩形，窗口过程可以当场改写它。
//! 在这里按设计比例改写，窗口拖动中就一直是正确比例，不用等松手再吸附。Tauri 没有封装宽高比，
//! 所以给面板窗口挂一个 subclass 处理这条消息；矩形换算是纯函数 [`resize_centered`]，平台无关、可单测。

// 矩形换算只有 Windows 的 subclass 用，其它平台留着给单测跑（macOS 走系统自带的比例约束）
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::Mutex;

/// 设计尺寸（逻辑像素），`None` 表示还没设置、不锁比例。
static PANEL_BASE: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// 拖边松手时发给面板：前端据此判断是否拉到了最大、要不要转成最大化。
pub const RESIZE_ENDED_EVENT: &str = "panel-resize-ended";

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

    /// 被拖的竖边在拟定矩形里的横坐标；只拖上下边时为 `None`。
    fn dragged_x(self, rect: Rect) -> Option<i32> {
        match self {
            Self::Left | Self::TopLeft | Self::BottomLeft => Some(rect.left),
            Self::Right | Self::TopRight | Self::BottomRight => Some(rect.right),
            Self::Top | Self::Bottom => None,
        }
    }

    /// 被拖的横边在拟定矩形里的纵坐标；只拖左右边时为 `None`。
    fn dragged_y(self, rect: Rect) -> Option<i32> {
        match self {
            Self::Top | Self::TopLeft | Self::TopRight => Some(rect.top),
            Self::Bottom | Self::BottomLeft | Self::BottomRight => Some(rect.bottom),
            Self::Left | Self::Right => None,
        }
    }
}

/// 拖动开始时记下的锚点：窗口中心与当时所在屏幕的工作区，物理像素。
#[derive(Clone, Copy, Debug)]
pub struct DragAnchor {
    pub center: (f64, f64),
    pub work_area: Rect,
}

impl DragAnchor {
    pub fn new(window: Rect, work_area: Rect) -> Self {
        Self {
            center: (
                f64::from(window.left + window.right) / 2.0,
                f64::from(window.top + window.bottom) / 2.0,
            ),
            work_area,
        }
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

/// 按比例改写拟定矩形，以锚点中心为准向四周缩放：被拖的边到中心的距离是半个窗口，拖左右边
/// 以宽为准，拖上下边以高为准，拖角取两者里较大的那个，都按比例补齐另一边。摆好后若越出
/// 锚点屏幕的工作区就整体推回——中心让位于不出屏，否则放在角落的窗口几乎放不大。
pub fn resize_centered(rect: Rect, edge: Edge, anchor: DragAnchor, limits: AspectLimits) -> Rect {
    let AspectLimits {
        ratio,
        frame: (frame_w, frame_h),
        min_width,
        max_width,
    } = limits;
    let (cx, cy) = anchor.center;
    let from_x = edge
        .dragged_x(rect)
        .map(|x| 2.0 * (f64::from(x) - cx).abs() - f64::from(frame_w));
    let from_y = edge
        .dragged_y(rect)
        .map(|y| (2.0 * (f64::from(y) - cy).abs() - f64::from(frame_h)) * ratio);
    let wanted = match (from_x, from_y) {
        (Some(w), Some(h)) => w.max(h),
        (Some(w), None) => w,
        (None, Some(h)) => h,
        (None, None) => f64::from(rect.right - rect.left - frame_w),
    };
    let width = (wanted.round() as i32).clamp(min_width, max_width.max(min_width));
    let height = (f64::from(width) / ratio).round() as i32;
    let outer_w = width + frame_w;
    let outer_h = height + frame_h;

    let work = anchor.work_area;
    let fit = |start: f64, len: i32, lo: i32, hi: i32| -> i32 {
        let start = start.round() as i32;
        // 先贴远端再贴近端：窗口比工作区还大（最小尺寸大过小屏）时宁可露出左上角的标题栏
        start.min(hi - len).max(lo)
    };
    let left = fit(
        cx - f64::from(outer_w) / 2.0,
        outer_w,
        work.left,
        work.right,
    );
    let top = fit(
        cy - f64::from(outer_h) / 2.0,
        outer_h,
        work.top,
        work.bottom,
    );
    Rect {
        left,
        top,
        right: left + outer_w,
        bottom: top + outer_h,
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
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_SIZING,
    };

    use tauri::Emitter;

    use super::{
        resize_centered, AspectLimits, DragAnchor, Edge, Rect, PANEL_BASE, RESIZE_ENDED_EVENT,
    };

    const SUBCLASS_ID: usize = 0x00F1_A1B0;
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    /// 本次拖动的锚点。消息都在窗口线程上串行到达，锁只为满足 `static` 的 `Sync`。
    static ANCHOR: std::sync::Mutex<Option<DragAnchor>> = std::sync::Mutex::new(None);
    /// 本次 move/size 循环里是否拖过边；只移动窗口时松手不发通知。
    static SIZED: AtomicBool = AtomicBool::new(false);
    /// 拖边松手时用来给面板发事件。
    static WINDOW: std::sync::OnceLock<tauri::WebviewWindow> = std::sync::OnceLock::new();

    /// subclass 必须在窗口所属线程（主线程）上挂，否则 `SetWindowSubclass` 直接失败。
    pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let _ = WINDOW.set(window.clone());
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
        match msg {
            // 移动窗口也会进出这对消息；锚点只在 WM_SIZING 里用，记下无妨
            WM_ENTERSIZEMOVE => set_anchor(anchor_now(hwnd)),
            WM_EXITSIZEMOVE => {
                set_anchor(None);
                if SIZED.swap(false, Ordering::Relaxed) {
                    if let Some(window) = WINDOW.get() {
                        let _ = window.emit_to(window.label(), RESIZE_ENDED_EVENT, ());
                    }
                }
            }
            _ => {}
        }
        if msg == WM_SIZING && lparam != 0 {
            SIZED.store(true, Ordering::Relaxed);
            let anchor = ANCHOR
                .lock()
                .ok()
                .and_then(|a| *a)
                .or_else(|| anchor_now(hwnd));
            let edge = Edge::from_wmsz(wparam as u32);
            let limits = anchor.and_then(|a| limits(hwnd, a));
            if let (Some(edge), Some(anchor), Some(limits)) = (edge, anchor, limits) {
                // SAFETY: WM_SIZING 的 lParam 是系统给的可写 RECT 指针，非空已判断
                let r = &mut *(lparam as *mut RECT);
                let locked = resize_centered(to_rect(r), edge, anchor, limits);
                r.left = locked.left;
                r.top = locked.top;
                r.right = locked.right;
                r.bottom = locked.bottom;
                return 1;
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    fn set_anchor(anchor: Option<DragAnchor>) {
        if let Ok(mut slot) = ANCHOR.lock() {
            *slot = anchor;
        }
    }

    fn to_rect(r: &RECT) -> Rect {
        Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }

    /// 以窗口当前的矩形和所在屏幕的工作区作锚点；拖动中窗口始终留在这块屏幕上。
    unsafe fn anchor_now(hwnd: HWND) -> Option<DragAnchor> {
        let mut window: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut window) == 0 {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        Some(DragAnchor::new(to_rect(&window), to_rect(&info.rcWork)))
    }

    /// 每帧现算 DPI：拖动开始前窗口可能刚从另一块 DPI 不同的屏幕上挪过来。
    unsafe fn limits(hwnd: HWND, anchor: DragAnchor) -> Option<AspectLimits> {
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

        let work = anchor.work_area;
        let work_w = f64::from(work.right - work.left - frame_w);
        let work_h = f64::from(work.bottom - work.top - frame_h);
        let max_width = work_w.min(work_h * ratio).floor() as i32;
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
