//! 兑换码许可证的本地状态：`{app_data_dir}/license.jws` 存 billing 签发的原文。
//!
//! 内存里只存原文，每次查询按当前时间重新验签：有期限的许可证在应用运行中到期时自动失效，
//! 不需要定时器；一次 Ed25519 验签的开销可以忽略。

use std::path::PathBuf;
use std::sync::Mutex;

use crypto::License;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracing::warn;

use crate::commands::profile::now_secs;

const LICENSE_FILE: &str = "license.jws";

/// 当前许可证原文；`None` 即没有激活过。
#[derive(Default)]
pub struct LicenseState(Mutex<Option<String>>);

impl LicenseState {
    /// 启动时读许可证文件。文件不存在即未激活；验不过只记一次日志、不弹窗，
    /// 按无权益处理——核心功能与许可证无关，不能因为它出错影响启动。
    pub fn load(app: &AppHandle) -> Self {
        let Some(token) = license_path(app).and_then(|p| std::fs::read_to_string(p).ok()) else {
            return Self::default();
        };
        if let Err(e) = crypto::verify_license(&token, now_secs()) {
            warn!("许可证无效，按无权益处理：{e}");
        }
        Self(Mutex::new(Some(token)))
    }

    /// 当前有效的许可证。
    pub fn current(&self) -> Option<License> {
        let token = self.0.lock().ok()?.clone()?;
        crypto::verify_license(&token, now_secs()).ok()
    }

    /// 是否持有指定权益。
    pub fn grants(&self, entitlement: &str) -> bool {
        self.current().is_some_and(|l| l.grants(entitlement))
    }
}

fn license_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(LICENSE_FILE))
}

/// 前端看到的许可证状态。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LicenseStatus {
    /// 许可证 id。
    pub id: String,
    /// 已授权的权益键。
    pub entitlements: Vec<String>,
    /// 签发时间（Unix 秒）。
    pub issued_at: u64,
    /// 到期时间（Unix 秒），永久为 `None`。
    pub expires_at: Option<u64>,
}

impl From<License> for LicenseStatus {
    fn from(l: License) -> Self {
        Self {
            id: l.id,
            entitlements: l.entitlements,
            issued_at: l.issued_at,
            expires_at: l.expires_at,
        }
    }
}

/// 当前有效的许可证；未激活、验不过或已到期都返回 `None`。
#[tauri::command]
pub fn get_license_status(state: State<LicenseState>) -> Option<LicenseStatus> {
    state.current().map(Into::into)
}

#[cfg(test)]
#[path = "license_tests.rs"]
mod tests;
