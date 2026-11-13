use super::{WindowIcon, WindowInfo};
use std::{
    collections::HashMap,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    },
    System::{
        Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        LibraryLoader::GetModuleHandleW,
        Threading::{
            GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE},
        Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON},
        WindowsAndMessaging::*,
    },
};

static NOTIFY: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);
static PICKER: Mutex<Option<mpsc::Sender<usize>>> = Mutex::new(None);
static PICKING: AtomicBool = AtomicBool::new(false);
static PICK_DOWN: AtomicBool = AtomicBool::new(false);
static ICONS: OnceLock<Mutex<HashMap<String, WindowIcon>>> = OnceLock::new();

pub struct Watcher {
    thread_id: u32,
    thread: Option<thread::JoinHandle<()>>,
}

impl Watcher {
    pub fn start(notify: impl Fn() + Send + 'static) -> Result<Self, String> {
        *NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(notify));
        let (tx, rx) = mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            // SAFETY: 回调只投递消息，生命周期由本线程上的 hook 与消息循环共同约束。
            let mut message: MSG = std::mem::zeroed();
            PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE);
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                null_mut(),
                Some(foreground_changed),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let _ = tx.send(if hook.is_null() {
                Err("无法监听前台窗口".to_string())
            } else {
                Ok(GetCurrentThreadId())
            });
            if hook.is_null() {
                return;
            }
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            UnhookWinEvent(hook);
        });
        let thread_id = rx.recv().map_err(|_| "前台监听线程未就绪".to_string())??;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: 线程 ID 由已建立消息队列的自有线程返回；只停止本实例。
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        *NOTIFY.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

unsafe extern "system" fn foreground_changed(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(notify) = NOTIFY.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        notify();
    }
}

pub fn current() -> Option<WindowInfo> {
    // SAFETY: Win32 返回值可能为空，统一交给 read_window 验证。
    unsafe { read_window(GetForegroundWindow(), false) }
}

pub fn window_by_id(id: usize) -> Option<WindowInfo> {
    // SAFETY: 枚举来的句柄可能已失效，read_window 在读取前重新检查。
    unsafe { read_window(id as HWND, true) }
}

pub fn list_windows(icons: bool) -> Vec<WindowInfo> {
    unsafe extern "system" fn enumerate(window: HWND, data: LPARAM) -> i32 {
        // SAFETY: data 是 EnumWindows 同步调用期间独占存活的收集器。
        let (icons, found) = &mut *(data as *mut (bool, Vec<WindowInfo>));
        if IsWindowVisible(window) != 0
            && GetWindowTextLengthW(window) > 0
            && GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize == 0
        {
            if let Some(info) = read_window(window, *icons) {
                if info.pid != std::process::id() {
                    found.push(info);
                }
            }
        }
        1
    }
    let mut data = (icons, Vec::new());
    // SAFETY: 枚举为同步调用，不保留收集器指针。
    unsafe {
        EnumWindows(Some(enumerate), &mut data as *mut _ as LPARAM);
    }
    data.1
}

unsafe fn read_window(window: HWND, icons: bool) -> Option<WindowInfo> {
    if window.is_null() || IsWindow(window) == 0 {
        return None;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    let mut title = [0u16; 512];
    let length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
    let mut class = [0u16; 256];
    let class_length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    let path = process_path(pid);
    let exe = path
        .as_ref()
        .and_then(|path| path.rsplit('\\').next())
        .map(str::to_string);
    let icon = if icons {
        window_icon(window, path.as_deref())
    } else {
        None
    };
    Some(WindowInfo {
        id: window as usize,
        pid,
        exe,
        class: String::from_utf16_lossy(&class[..class_length.max(0) as usize]),
        title: String::from_utf16_lossy(&title[..length as usize]),
        path,
        icon,
    })
}

unsafe fn process_path(pid: u32) -> Option<String> {
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    let success = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
    CloseHandle(process);
    (success != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

unsafe fn window_icon(window: HWND, path: Option<&str>) -> Option<WindowIcon> {
    let key = path
        .map(str::to_string)
        .unwrap_or_else(|| format!("hwnd:{}", window as usize));
    let cache = ICONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(icon) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Some(icon.clone());
    }
    let mut icon = null_mut();
    for kind in [ICON_SMALL2, ICON_SMALL, ICON_BIG] {
        let mut result = 0;
        // 超时避免无响应的目标程序拖住窗口选择器。
        if SendMessageTimeoutW(
            window,
            WM_GETICON,
            kind as usize,
            0,
            SMTO_ABORTIFHUNG,
            60,
            &mut result,
        ) != 0
            && result != 0
        {
            icon = result as HICON;
            break;
        }
    }
    if icon.is_null() {
        icon = GetClassLongPtrW(window, GCLP_HICONSM) as HICON;
    }
    if icon.is_null() {
        icon = GetClassLongPtrW(window, GCLP_HICON) as HICON;
    }
    let mut owned = false;
    if icon.is_null() {
        if let Some(path) = path {
            // Shell 图标可能使用 COM 扩展；后台线程独立配对初始化，不依赖 Tauri 的主线程。
            let com = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
            let path: Vec<_> = path.encode_utf16().chain(Some(0)).collect();
            let mut info: SHFILEINFOW = std::mem::zeroed();
            if SHGetFileInfoW(
                path.as_ptr(),
                0,
                &mut info,
                std::mem::size_of_val(&info) as u32,
                SHGFI_ICON | SHGFI_SMALLICON,
            ) != 0
            {
                icon = info.hIcon;
                owned = true;
            }
            if com >= 0 {
                CoUninitialize();
            }
        }
    }
    let result = render_icon(icon);
    if owned && !icon.is_null() {
        DestroyIcon(icon);
    }
    if let Some(result) = &result {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, result.clone());
    }
    result
}

unsafe fn render_icon(icon: HICON) -> Option<WindowIcon> {
    if icon.is_null() {
        return None;
    }
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return None;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 32,
            biHeight: -32,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        },
        ..std::mem::zeroed()
    };
    let mut bits = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(dc);
        return None;
    }
    let previous = SelectObject(dc, bitmap);
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, 32 * 32 * 4);
    pixels.fill(0);
    let black_ok = DrawIconEx(dc, 0, 0, icon, 32, 32, 0, null_mut(), DI_NORMAL);
    GdiFlush();
    let black = pixels.to_vec();
    pixels.fill(255);
    let white_ok = DrawIconEx(dc, 0, 0, icon, 32, 32, 0, null_mut(), DI_NORMAL);
    GdiFlush();
    let rgba = super::icon_rgba(&black, pixels);
    SelectObject(dc, previous);
    DeleteObject(bitmap);
    DeleteDC(dc);
    (black_ok != 0 && white_ok != 0).then_some(WindowIcon { rgba, size: 32 })
}

unsafe extern "system" fn pick_mouse(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0 {
        if message as u32 == WM_LBUTTONDOWN {
            let point = (*(data as *const MSLLHOOKSTRUCT)).pt;
            let mut pid = 0;
            GetWindowThreadProcessId(GetAncestor(WindowFromPoint(point), GA_ROOT), &mut pid);
            // 自身点击交给捕获遮罩处理取消，不作为候选，也不吞掉遮罩的点击。
            if pid == std::process::id() {
                return CallNextHookEx(null_mut(), code, message, data);
            }
            PICK_DOWN.store(true, Ordering::SeqCst);
            return 1;
        }
        if message as u32 == WM_LBUTTONUP && PICK_DOWN.swap(false, Ordering::SeqCst) {
            // SAFETY: WH_MOUSE_LL 的有效回调参数指向 MSLLHOOKSTRUCT。
            let point = (*(data as *const MSLLHOOKSTRUCT)).pt;
            let window = GetAncestor(WindowFromPoint(point), GA_ROOT);
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            if pid == std::process::id() {
                return 1;
            }
            if let Some(sender) = PICKER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                let _ = sender.send(window as usize);
            }
            return 1;
        }
    }
    CallNextHookEx(null_mut(), code, message, data)
}

pub fn pick_window(cancelled: Arc<AtomicBool>) -> Result<Option<WindowInfo>, String> {
    if cancelled.load(Ordering::SeqCst) {
        return Ok(None);
    }
    if PICKING.swap(true, Ordering::SeqCst) {
        return Err("已有窗口选取正在进行".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PICKING.store(false, Ordering::SeqCst);
            *PICKER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
    let _reset = Reset;
    PICK_DOWN.store(false, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    *PICKER.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    // SAFETY: hook 和消息泵位于本次阻塞任务的同一线程，退出前始终卸载。
    unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(pick_mouse), GetModuleHandleW(null()), 0);
        if hook.is_null() {
            return Err("无法开始窗口选取".into());
        }
        let start = Instant::now();
        let result = loop {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if let Ok(window) = rx.try_recv() {
                break read_window(window as HWND, true);
            }
            if cancelled.load(Ordering::SeqCst)
                || GetAsyncKeyState(VK_ESCAPE as i32) < 0
                || start.elapsed() > Duration::from_secs(20)
            {
                break None;
            }
            thread::sleep(Duration::from_millis(8));
        };
        UnhookWindowsHookEx(hook);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_icon_is_extracted_with_transparency_and_color() {
        // SAFETY: 系统共享图标不由调用者销毁，绘制资源在 render_icon 内成对释放。
        let icon = unsafe { render_icon(LoadIconW(null_mut(), IDI_APPLICATION)) }.unwrap();
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        let (pixels, _) = icon.rgba.as_chunks::<4>();
        assert!(pixels.iter().any(|pixel| pixel[3] > 0));
        assert!(pixels.iter().any(|pixel| pixel[3] < 255));
    }

    #[test]
    fn cancellation_before_picker_thread_starts_does_not_arm_global_mouse_hook() {
        assert!(pick_window(Arc::new(AtomicBool::new(true)))
            .unwrap()
            .is_none());
        assert!(!PICKING.load(Ordering::SeqCst));
    }
}
