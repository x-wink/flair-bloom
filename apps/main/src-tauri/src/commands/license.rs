//! 兑换码许可证：`{app_data_dir}/licenses/` 下一张许可证一个文件，存 billing 签发的原文。
//!
//! 许可证可以叠加：每兑换一张码就多一份，可用权益取所有有效许可证的并集（D19）。
//! 内存里只存原文，每次查询按当前时间重新验签：有期限的许可证在应用运行中到期时自动失效，
//! 不需要定时器；几次 Ed25519 验签的开销可以忽略。
//!
//! 联网只发生在用户点「激活」之后（先取会员服务协议版本、再提交兑换码），其余时候不碰网络。
//! 契约见 `docs/roadmaps/license.md` D11–D14、D19 与 T3。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crypto::License;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tracing::{info, warn};

use crate::commands::profile::now_secs;

const LICENSES_DIR: &str = "licenses";
const LICENSE_EXT: &str = "jws";
const AGREEMENT_CODE: &str = "membership";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

// 开发构建打 xwink-console 本地起的服务（billing 没有预发环境，D13）
#[cfg(debug_assertions)]
const BILLING_API: &str = "http://localhost:8400/api/v1";
#[cfg(not(debug_assertions))]
const BILLING_API: &str = "https://billing.xwink.fun/api/v1";
#[cfg(debug_assertions)]
const UC_API: &str = "http://localhost:8000/api/v1";
#[cfg(not(debug_assertions))]
const UC_API: &str = "https://uc.xwink.fun/api/v1";
/// 会员服务协议正文页，`open_link("membership")` 用。
#[cfg(debug_assertions)]
pub(crate) const MEMBERSHIP_TERMS_URL: &str = "http://localhost:3700/auth/terms/membership";
#[cfg(not(debug_assertions))]
pub(crate) const MEMBERSHIP_TERMS_URL: &str = "https://uc.xwink.fun/auth/terms/membership";

/// 已激活的许可证原文。
#[derive(Default)]
pub struct LicenseState(Mutex<Vec<String>>);

impl LicenseState {
    /// 启动时读许可证目录。目录不存在即未激活；验不过的只记日志、不弹窗，
    /// 按无权益处理——核心功能与许可证无关，不能因为它出错影响启动。
    pub fn load(app: &AppHandle) -> Self {
        let tokens = licenses_dir(app)
            .map(|d| read_tokens(&d))
            .unwrap_or_default();
        let now = now_secs();
        for token in &tokens {
            if let Err(e) = crypto::verify_license(token, now) {
                warn!("有一张许可证无效，按无权益处理：{e}");
            }
        }
        Self(Mutex::new(tokens))
    }

    /// 当前有效的许可证。
    pub fn current(&self) -> Vec<License> {
        let Ok(tokens) = self.0.lock() else {
            return Vec::new();
        };
        let now = now_secs();
        tokens
            .iter()
            .filter_map(|t| crypto::verify_license(t, now).ok())
            .collect()
    }

    /// 是否持有指定权益：任意一张有效许可证授予即可。
    pub fn grants(&self, entitlement: &str) -> bool {
        self.current().iter().any(|l| l.grants(entitlement))
    }

    fn set(&self, tokens: Vec<String>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = tokens;
        }
    }
}

fn licenses_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join(LICENSES_DIR))
}

/// 按文件名排序读出目录里的全部许可证原文，读不了的跳过。
fn read_tokens(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == LICENSE_EXT))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

/// 许可证文件名取兑换码的哈希。重复兑换要在发请求之前拦下（每次重下发都占用次数），
/// 而请求前手里只有码；同一张码在 billing 那边永远对应同一张许可证，按码命名天然去重。
/// billing 对码不区分大小写、忽略分组横线，这里同样归一化后再算。
fn code_file_name(code: &str) -> Option<String> {
    let normalized: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    if normalized.is_empty() {
        return None;
    }
    let digest = Sha256::digest(normalized.as_bytes());
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("{hex}.{LICENSE_EXT}"))
}

/// 前端看到的一张许可证。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LicenseStatus {
    /// 许可证 id。
    pub id: String,
    /// 这张许可证授予的权益键。
    pub entitlements: Vec<String>,
    /// 签发时间（Unix 秒）；重新激活时 billing 会重签，取最近一次。
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

/// 一项权益合并后的有效期。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct EntitlementGrant {
    /// 权益键。
    pub key: String,
    /// 授予它的许可证里最晚的到期时间；有一张永久即永久（`None`）。
    pub expires_at: Option<u64>,
}

/// 全部有效许可证的汇总。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LicenseSummary {
    /// 合并后的权益，按首次出现的顺序。
    pub entitlements: Vec<EntitlementGrant>,
    /// 各张许可证，按签发时间排序。
    pub licenses: Vec<LicenseStatus>,
}

fn summarize(mut licenses: Vec<License>) -> LicenseSummary {
    licenses.sort_by_key(|l| l.issued_at);
    let mut entitlements: Vec<EntitlementGrant> = Vec::new();
    for license in &licenses {
        for key in &license.entitlements {
            match entitlements.iter_mut().find(|g| &g.key == key) {
                Some(grant) => {
                    grant.expires_at = match (grant.expires_at, license.expires_at) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        _ => None,
                    }
                }
                None => entitlements.push(EntitlementGrant {
                    key: key.clone(),
                    expires_at: license.expires_at,
                }),
            }
        }
    }
    LicenseSummary {
        entitlements,
        licenses: licenses.into_iter().map(Into::into).collect(),
    }
}

/// 当前全部有效许可证的汇总；未激活、验不过或已到期的不计入。
#[tauri::command]
pub fn get_license_status(state: State<LicenseState>) -> LicenseSummary {
    summarize(state.current())
}

/// 激活失败的分类，前端按 `kind` 出文案。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivationError {
    /// 码不存在或不是许可证类型的码。
    InvalidCode,
    /// 码已作废。
    Void,
    /// 码已过期（只挡首次激活）。
    Expired,
    /// 这张码的许可证已到期，不能再重新激活。
    LicenseExpired,
    /// 这张码本机已经激活过；带上它现有的许可证（验得过时），由用户决定是否覆盖。
    AlreadyActivated {
        /// 本机已有的那张许可证。
        license: Option<LicenseStatus>,
    },
    /// 重新激活次数用完。
    ReissueLimit,
    /// 协议版本缺失或已更新，要用户按新版本重新勾选。
    AgreementChanged {
        /// billing 给出的现行版本。
        current_version: Option<String>,
    },
    /// 激活服务暂时不可用（503 等）。
    Unavailable,
    /// 触发限流。
    RateLimited {
        /// `Retry-After` 的秒数。
        retry_after_secs: Option<u64>,
    },
    /// 连不上服务或超时。
    Network,
    /// billing 已签发、许可证也已保存，但本机时间早于签发时间太多，校正系统时间后自动生效。
    ClockSkew,
    /// billing 已签发、许可证也已保存，但本地验不过：多半是构建里的公钥与 billing 的签名密钥对不上。
    InvalidLicense,
    /// 其余失败，带服务端消息与 traceId 便于排查。
    Failed {
        /// 服务端给的说明。
        message: String,
        /// 服务端 traceId。
        trace_id: Option<String>,
    },
}

/// 会员服务协议的现行版本。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MembershipAgreement {
    /// 协议标题。
    pub title: String,
    /// 版本号，激活时原样提交。
    pub version: String,
}

/// 取会员服务协议的现行版本；线上没有已发布的协议时返回 `None`，此时激活不带版本（D14）。
#[tauri::command]
pub async fn get_membership_agreement() -> Result<Option<MembershipAgreement>, ActivationError> {
    let url = format!("{UC_API}/auth/agreements/current?codes={AGREEMENT_CODE}");
    let resp = http_client()?
        .get(url)
        .send()
        .await
        .map_err(network_error)?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp.bytes().await.map_err(network_error)?;
    if status == 429 {
        return Err(rate_limited(retry_after.as_deref()));
    }
    if !(200..300).contains(&status) {
        warn!("取会员服务协议失败：HTTP {status}");
        return Err(ActivationError::Unavailable);
    }
    // 解析不了不能当成「没有已发布的协议」：那样会不带版本去激活，白占一次 billing 的限流额度
    pick_agreement(&body).ok_or_else(|| {
        warn!("会员服务协议接口的返回格式不认识");
        ActivationError::Unavailable
    })
}

/// 外层 `None` 是返回体解析不了；内层 `None` 是线上没有已发布的会员服务协议。
fn pick_agreement(body: &[u8]) -> Option<Option<MembershipAgreement>> {
    #[derive(Deserialize)]
    struct Item {
        code: String,
        title: Option<String>,
        version: String,
    }
    let items: Vec<Item> = serde_json::from_slice(body).ok()?;
    Some(
        items
            .into_iter()
            .find(|i| i.code == AGREEMENT_CODE)
            .map(|i| MembershipAgreement {
                title: i.title.unwrap_or_else(|| "会员服务协议".into()),
                version: i.version,
            }),
    )
}

/// 激活结果：这张码带来的权益与激活后的汇总。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ActivationResult {
    /// 这次激活的许可证授予的权益键，前端据此提示怎么用。
    pub granted: Vec<String>,
    /// 激活后全部有效许可证的汇总。
    pub summary: LicenseSummary,
}

/// 用兑换码换许可证。本机已激活过同一张码时，
/// `overwrite` 为假直接返回 [`ActivationError::AlreadyActivated`]、不发请求。
#[tauri::command]
pub async fn activate_license(
    app: AppHandle,
    state: State<'_, LicenseState>,
    code: String,
    agreement_version: Option<String>,
    overwrite: bool,
) -> Result<ActivationResult, ActivationError> {
    let file_name = code_file_name(&code).ok_or(ActivationError::InvalidCode)?;
    let dir = licenses_dir(&app).ok_or_else(|| failed("无法获取应用数据目录".into()))?;
    if !overwrite {
        if let Some(err) = already_activated(&dir.join(&file_name)) {
            return Err(err);
        }
    }
    let (token, verified) = request_license(&code, agreement_version).await?;
    // 验签之前先落盘：billing 那边已经记下这次激活，丢掉它用户再点一次就白占一次重新激活次数；
    // 留着则校正时钟后自动生效，再兑换也会先走重复确认
    write_license_file(&dir, &file_name, &token)
        .map_err(|e| failed(format!("保存许可证失败：{e}")))?;
    state.set(read_tokens(&dir));
    let license = verified.map_err(|e| {
        warn!("billing 返回的许可证验签失败，已保存待校正：{e}");
        match e {
            crypto::LicenseError::ClockRollback => ActivationError::ClockSkew,
            _ => ActivationError::InvalidLicense,
        }
    })?;
    info!("兑换码激活成功，许可证 {}", license.id);
    Ok(ActivationResult {
        granted: license.entitlements,
        summary: summarize(state.current()),
    })
}

/// 这张码的许可证文件已在本机时返回 [`ActivationError::AlreadyActivated`]，附上验得过的现有许可证。
fn already_activated(path: &Path) -> Option<ActivationError> {
    if !path.exists() {
        return None;
    }
    let license = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| crypto::verify_license(&t, now_secs()).ok())
        .map(Into::into);
    Some(ActivationError::AlreadyActivated { license })
}

/// 提交兑换码，返回 billing 签发的原文与本地验签结果；不落盘、不改状态。
async fn request_license(
    code: &str,
    agreement_version: Option<String>,
) -> Result<(String, Result<License, crypto::LicenseError>), ActivationError> {
    let code = code.trim();
    if code.is_empty() {
        return Err(ActivationError::InvalidCode);
    }
    // 不带 licenseId：本地还有许可证时再激活只会是换码，带上反而撞 LICENSE_MISMATCH；
    // 重装时文件已不在，同码不带它也按重下发处理
    let mut body = serde_json::json!({ "code": code });
    if let Some(v) = agreement_version {
        body["agreementVersion"] = v.into();
    }
    let resp = http_client()?
        .post(format!("{BILLING_API}/billing/licenses/activations"))
        .json(&body)
        .send()
        .await
        .map_err(network_error)?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = resp.bytes().await.map_err(network_error)?;
    if !(200..300).contains(&status) {
        let err = map_activation_error(status, retry_after.as_deref(), &bytes);
        warn!("兑换码激活失败：HTTP {status} {err:?}");
        return Err(err);
    }

    let token = parse_activation(&bytes)?;
    let verified = crypto::verify_license(&token, now_secs());
    Ok((token, verified))
}

fn http_client() -> Result<reqwest::Client, ActivationError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("FlairBloom/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| failed(format!("初始化网络客户端失败：{e}")))
}

fn network_error(e: reqwest::Error) -> ActivationError {
    warn!("激活请求网络错误：{e}");
    ActivationError::Network
}

fn failed(message: String) -> ActivationError {
    ActivationError::Failed {
        message,
        trace_id: None,
    }
}

fn parse_activation(body: &[u8]) -> Result<String, ActivationError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Activation {
        license: String,
        reissue_count: Option<u32>,
        reissue_limit: Option<u32>,
    }
    let a: Activation =
        serde_json::from_slice(body).map_err(|e| failed(format!("激活响应格式不对：{e}")))?;
    if let (Some(count), Some(limit)) = (a.reissue_count, a.reissue_limit) {
        info!("许可证重新激活次数 {count}/{limit}");
    }
    Ok(a.license)
}

fn rate_limited(retry_after: Option<&str>) -> ActivationError {
    ActivationError::RateLimited {
        retry_after_secs: retry_after.and_then(|v| v.trim().parse().ok()),
    }
}

/// 按 billing 的错误响应分类：业务码在 `error.details.code`，`error.code` 只是通用码。
fn map_activation_error(status: u16, retry_after: Option<&str>, body: &[u8]) -> ActivationError {
    if status == 429 {
        return rate_limited(retry_after);
    }
    let json: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let error = &json["error"];
    let details = &error["details"];
    match details["code"].as_str() {
        Some("REDEMPTION_CODE_NOT_FOUND" | "LICENSE_CODE_TYPE_MISMATCH") => {
            ActivationError::InvalidCode
        }
        Some("REDEMPTION_CODE_VOID") => ActivationError::Void,
        Some("REDEMPTION_CODE_EXPIRED") => ActivationError::Expired,
        Some("LICENSE_EXPIRED") => ActivationError::LicenseExpired,
        Some("LICENSE_REISSUE_LIMIT") => ActivationError::ReissueLimit,
        Some("AGREEMENT_VERSION_REQUIRED" | "AGREEMENT_VERSION_OUTDATED") => {
            ActivationError::AgreementChanged {
                current_version: details["currentVersion"].as_str().map(str::to_string),
            }
        }
        Some("LICENSE_APP_UNAVAILABLE" | "AGREEMENT_UNAVAILABLE") => ActivationError::Unavailable,
        _ if matches!(status, 502..=504) => ActivationError::Unavailable,
        _ => ActivationError::Failed {
            message: error["message"]
                .as_str()
                .map_or_else(|| format!("HTTP {status}"), str::to_string),
            trace_id: error["traceId"].as_str().map(str::to_string),
        },
    }
}

/// 先写同目录临时文件再改名，断电或磁盘满时不留下半截许可证。
fn write_license_file(dir: &Path, file_name: &str, token: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{file_name}.tmp"));
    std::fs::write(&tmp, token)?;
    std::fs::rename(tmp, dir.join(file_name))
}

#[cfg(test)]
#[path = "license_tests.rs"]
mod tests;
