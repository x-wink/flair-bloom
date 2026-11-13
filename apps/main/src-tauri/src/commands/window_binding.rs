use qzh_profile::window_binding::WindowBinding;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use win_foreground::{WindowIcon, WindowInfo};

const ICON_CACHE_KEY: &str = "windowBindingIcons";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetWindow {
    pub name: String,
    pub icon: Option<WindowIcon>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingStatus {
    pub revision: u64,
    pub supported: bool,
    pub binding: Option<WindowBinding>,
    pub target: Option<TargetWindow>,
    pub open: bool,
    pub foreground: bool,
    pub present: bool,
    pub reason: String,
    pub picking: bool,
    pub error: Option<String>,
}

pub struct WindowBindingState {
    inner: Mutex<BindingStatus>,
    refresh: Mutex<()>,
    picker: AtomicBool,
    pick_cancel: Arc<AtomicBool>,
    sender: mpsc::Sender<bool>,
    watcher: Mutex<Option<win_foreground::Watcher>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WindowBindingState {
    pub fn start(app: &AppHandle) -> Self {
        let (sender, receiver) = mpsc::channel();
        let notify = sender.clone();
        let watcher = win_foreground::Watcher::start(move || {
            let _ = notify.send(true);
        });
        let error = watcher.as_ref().err().cloned();
        if let Some(error) = &error {
            tracing::warn!("窗口监听不可用：{error}");
        }
        let handle = app.clone();
        let worker = std::thread::spawn(move || {
            while let Ok(refresh) = receiver.recv() {
                if !refresh {
                    break;
                }
                if let Some(state) = handle.try_state::<WindowBindingState>() {
                    state.recompute(&handle);
                }
            }
        });
        Self {
            inner: Mutex::new(BindingStatus {
                revision: 0,
                supported: cfg!(windows),
                binding: None,
                target: None,
                open: true,
                foreground: false,
                present: false,
                reason: "unbound".into(),
                picking: false,
                error,
            }),
            refresh: Mutex::new(()),
            picker: AtomicBool::new(false),
            pick_cancel: Arc::new(AtomicBool::new(false)),
            sender,
            watcher: Mutex::new(watcher.ok()),
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn shutdown(&self) {
        self.pick_cancel.store(true, Ordering::SeqCst);
        self.watcher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let _ = self.sender.send(false);
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }

    pub fn status(&self) -> BindingStatus {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_binding(&self, app: &AppHandle, binding: Option<WindowBinding>) {
        let _refresh = self.refresh.lock().unwrap_or_else(|e| e.into_inner());
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).binding = binding;
        self.recompute_locked(app);
    }

    pub fn recompute(&self, app: &AppHandle) {
        let _refresh = self.refresh.lock().unwrap_or_else(|e| e.into_inner());
        self.recompute_locked(app);
    }

    fn recompute_locked(&self, app: &AppHandle) {
        let mut next = self.status();
        next.picking = self.picker.load(Ordering::SeqCst);
        let current = win_foreground::current();
        let self_focused = current
            .as_ref()
            .is_some_and(|info| info.pid == std::process::id());
        let matched = current.as_ref().is_some_and(|info| {
            next.binding.as_ref().is_some_and(|binding| {
                binding.matches(info.exe.as_deref(), &info.class, &info.title)
            })
        });
        next.foreground = matched;
        next.open = !next.picking
            && (!next.supported
                || next.binding.is_none()
                || (next.error.is_none() && (self_focused || matched)));
        // 先关输出闸门，再补列表与图标，避免慢窗口查询拖延切走后的停止。
        app.state::<super::engine::EngineState>()
            .0
            .set_foreground_open(next.open);
        let windows = if next.binding.is_some() && next.supported {
            win_foreground::list_windows(false)
        } else {
            Vec::new()
        };
        let matching = |info: &&WindowInfo| {
            next.binding.as_ref().is_some_and(|binding| {
                binding.matches(info.exe.as_deref(), &info.class, &info.title)
            })
        };
        let found = windows
            .iter()
            .filter(|info| info.pid != std::process::id())
            .find(matching);
        next.present = found.is_some();
        next.target = next.binding.as_ref().map(|binding| {
            let cached = read_icon_cache(app, &binding.exe);
            if let Some(info) = found {
                if cached.as_ref().is_none_or(|target| target.icon.is_none()) {
                    if let Some(info) = win_foreground::window_by_id(info.id) {
                        cache_target(app, &info);
                        return TargetWindow {
                            name: info.title,
                            icon: info.icon,
                        };
                    }
                }
            }
            cached.unwrap_or_else(|| TargetWindow {
                name: binding.exe.clone(),
                icon: None,
            })
        });
        next.reason = if next.picking {
            "picking"
        } else if !next.supported {
            "unsupported"
        } else if next.binding.is_none() {
            "unbound"
        } else if next.error.is_some() {
            "error"
        } else if self_focused {
            "self"
        } else if matched {
            "matched"
        } else if current.is_none() {
            "unknown"
        } else if next.present {
            "waiting"
        } else {
            "closed"
        }
        .into();
        let mut stored = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if *stored != next {
            next.revision = stored.revision.saturating_add(1);
            *stored = next.clone();
            let _ = app.emit("window-binding-changed", next);
        }
    }
}

fn read_icon_cache(app: &AppHandle, exe: &str) -> Option<TargetWindow> {
    let store = app.store(crate::STORE_PATH).ok()?;
    serde_json::from_value(
        store
            .get(ICON_CACHE_KEY)?
            .get(exe.to_ascii_lowercase())?
            .clone(),
    )
    .ok()
}

fn cache_target(app: &AppHandle, info: &WindowInfo) {
    let Some(exe) = &info.exe else {
        return;
    };
    if let Ok(store) = app.store(crate::STORE_PATH) {
        let mut cache = store
            .get(ICON_CACHE_KEY)
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        if cache.len() >= 64 && !cache.contains_key(&exe.to_ascii_lowercase()) {
            cache.clear();
        }
        cache.insert(
            exe.to_ascii_lowercase(),
            serde_json::json!(TargetWindow {
                name: info.title.clone(),
                icon: info.icon.clone()
            }),
        );
        store.set(ICON_CACHE_KEY, serde_json::Value::Object(cache));
        if let Err(error) = store.save() {
            tracing::warn!("目标窗口图标缓存失败：{error}");
        }
    }
}

#[tauri::command]
pub fn get_window_binding_status(state: tauri::State<WindowBindingState>) -> BindingStatus {
    state.status()
}

#[tauri::command]
pub async fn list_bindable_windows() -> Result<Vec<WindowInfo>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        win_foreground::list_windows(true)
            .into_iter()
            .filter(|window| window.pid != std::process::id())
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_binding_pick(state: tauri::State<WindowBindingState>) {
    state.pick_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn pick_binding_window(app: AppHandle) -> Result<Option<WindowInfo>, String> {
    let state = app.state::<WindowBindingState>();
    if state.picker.swap(true, Ordering::SeqCst) {
        return Err("正在选取窗口".into());
    }
    state.pick_cancel.store(false, Ordering::SeqCst);
    state.recompute(&app);
    app.state::<super::engine::EngineState>()
        .0
        .cancel_all_loops();
    let cancelled = state.pick_cancel.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || win_foreground::pick_window(cancelled))
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result);
    state.picker.store(false, Ordering::SeqCst);
    state.recompute(&app);
    result.map(|window| window.filter(|window| window.pid != std::process::id()))
}

#[tauri::command]
pub async fn set_profile_window_binding(
    app: AppHandle,
    name: String,
    window_id: Option<usize>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let selected = window_id
            .map(|id| {
                win_foreground::window_by_id(id).ok_or("目标窗口已关闭，请重新选择".to_string())
            })
            .transpose()?;
        if selected
            .as_ref()
            .is_some_and(|window| window.pid == std::process::id())
        {
            return Err("请选择游戏或其他程序的窗口".into());
        }
        let binding = selected
            .as_ref()
            .map(|info| {
                let exe = info
                    .exe
                    .clone()
                    .ok_or("无法识别目标程序，请尝试以管理员身份运行".to_string())?;
                WindowBinding {
                    exe,
                    class: (!info.class.is_empty()).then(|| info.class.clone()),
                    title_contains: None,
                }
                .sanitized()
                .ok_or("窗口标识无效".to_string())
            })
            .transpose()?;
        super::profile::write_window_binding(&app, &name, binding.clone())?;
        if let Some(selected) = &selected {
            cache_target(&app, selected);
        }
        app.state::<super::engine::EngineState>()
            .0
            .cancel_all_loops();
        app.state::<WindowBindingState>().set_binding(&app, binding);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
