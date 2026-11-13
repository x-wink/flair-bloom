use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowIcon {
    pub rgba: Vec<u8>,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WindowInfo {
    pub id: usize,
    pub pid: u32,
    pub exe: Option<String>,
    pub class: String,
    pub title: String,
    #[serde(skip_serializing)]
    pub path: Option<String>,
    pub icon: Option<WindowIcon>,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{current, list_windows, pick_window, window_by_id, Watcher};

#[cfg(not(windows))]
pub fn current() -> Option<WindowInfo> {
    None
}
#[cfg(not(windows))]
pub fn window_by_id(_id: usize) -> Option<WindowInfo> {
    None
}
#[cfg(not(windows))]
pub fn list_windows(_icons: bool) -> Vec<WindowInfo> {
    Vec::new()
}
#[cfg(not(windows))]
pub fn pick_window(
    _cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Option<WindowInfo>, String> {
    Err("窗口绑定仅支持 Windows".into())
}
#[cfg(not(windows))]
pub struct Watcher;
#[cfg(not(windows))]
impl Watcher {
    pub fn start(_notify: impl Fn() + Send + 'static) -> Result<Self, String> {
        Ok(Self)
    }
}

// 黑白底各绘制一次，从差值恢复 alpha，兼容旧式 AND 掩码图标与预乘 alpha 图标。
#[cfg(any(windows, test))]
fn icon_rgba(black: &[u8], white: &[u8]) -> Vec<u8> {
    black
        .chunks_exact(4)
        .zip(white.chunks_exact(4))
        .flat_map(|(b, w)| {
            let alpha = 255
                - (0..3)
                    .map(|i| w[i].saturating_sub(b[i]))
                    .max()
                    .unwrap_or(255);
            let channel = |i| {
                if alpha == 0 {
                    0
                } else {
                    (u32::from(b[i]) * 255 / u32::from(alpha)).min(255) as u8
                }
            };
            [channel(2), channel(1), channel(0), alpha]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_transparent_opaque_and_premultiplied_icon_pixels() {
        assert_eq!(icon_rgba(&[0, 0, 0, 0], &[255, 255, 255, 0]), [0, 0, 0, 0]);
        assert_eq!(icon_rgba(&[0, 0, 0, 0], &[0, 0, 0, 0]), [0, 0, 0, 255]);
        assert_eq!(
            icon_rgba(&[0, 0, 128, 128], &[127, 127, 255, 128]),
            [255, 0, 0, 128]
        );
    }
    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_exposes_no_native_window_and_reports_picker_unavailable() {
        assert!(current().is_none());
        assert!(window_by_id(1).is_none());
        assert!(list_windows(true).is_empty());
        assert!(
            pick_window(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false
            )))
            .is_err()
        );
        assert!(Watcher::start(|| {}).is_ok());
    }
}
