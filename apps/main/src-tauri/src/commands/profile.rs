use crypto::aes;
use qzh_format::{
    header::{FileHeader, MAGIC, VERSION},
    migrate::migrate_profile,
    profile::{
        Advanced, BurstMode, BurstRule, Hotkeys, Profile, ProfileMeta, CURRENT_SCHEMA_VERSION,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_store::StoreExt;
use tracing::warn;

use super::engine::EngineState;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn profiles_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("profiles"))
        .map_err(|e| format!("无法获取应用数据目录: {}", e))
}

fn now_secs() -> u64 {
    // 时钟早于 UNIX epoch 是 invariant 违反（比 1970 还早或被恶意回拨），
    // 静默返回 0 会导致 created_at/updated_at 错乱、list_profiles 排序失真，
    // 故选择显式 panic 让上层 panic hook 记录现场。
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟早于 UNIX epoch")
        .as_secs()
}

fn make_id() -> String {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}-{}", now_secs(), n)
}

/// AAD 仅含 magic + version + flags，不含 nonce，所以可预先计算。
fn compute_aad() -> Vec<u8> {
    let mut aad = Vec::with_capacity(7);
    aad.extend_from_slice(MAGIC);
    aad.push(VERSION);
    aad.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
    aad
}

fn write_profile_file_to_path(file_path: &Path, profile: &Profile) -> Result<String, String> {
    let json = serde_json::to_vec(profile).map_err(|e| format!("序列化失败: {}", e))?;
    let aad = compute_aad();
    let (ciphertext, nonce) = aes::encrypt(&json, &aad).map_err(|e| e.to_string())?;
    let header = FileHeader::new(nonce);

    let mut data = header.to_bytes();
    data.extend_from_slice(&ciphertext);

    let tmp_path = file_path.with_extension("qzh.tmp");
    std::fs::write(&tmp_path, &data).map_err(|e| format!("写入临时文件失败: {}", e))?;
    std::fs::rename(&tmp_path, file_path).map_err(|e| format!("替换配置文件失败: {}", e))?;
    Ok(file_path.to_string_lossy().to_string())
}

fn load_meta_from_file(file_path: &Path) -> Option<ProfileMeta> {
    let data = std::fs::read(file_path).ok()?;
    let header = FileHeader::from_bytes(&data)?;
    let aad = header.aad();
    let ciphertext = &data[FileHeader::SIZE..];
    let plaintext = aes::decrypt(ciphertext, &header.nonce, &aad).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&plaintext).ok()?;
    serde_json::from_value::<ProfileMeta>(value.get("meta")?.clone()).ok()
}

#[tauri::command]
pub fn save_profile(
    app: AppHandle,
    state: State<EngineState>,
    name: String,
    mut profile: Profile,
) -> Result<String, String> {
    profile.validate().map_err(|e| e.to_string())?;

    let now = now_secs();
    let dir = profiles_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {}", e))?;

    let file_path = dir.join(format!("{}.qzh", sanitize_filename(&name)));

    // 若文件已存在，保留原始 created_at；否则设为当前时间
    if profile.meta.created_at == 0 {
        if let Some(old_meta) = load_meta_from_file(&file_path) {
            profile.meta.created_at = old_meta.created_at;
        } else {
            profile.meta.created_at = now;
        }
    }
    profile.meta.updated_at = now;
    profile.meta.app_version = env!("CARGO_PKG_VERSION").to_string();
    profile.schema_version = CURRENT_SCHEMA_VERSION;

    let rules = profile.rules.clone();
    let path = write_profile_file_to_path(&file_path, &profile)?;

    // 更新 store 中记录的活跃配置路径
    if let Ok(store) = app.store("settings.json") {
        store.set("activeProfilePath", serde_json::json!(path));
        let _ = store.save();
    }

    state.0.set_rules(rules);
    Ok(path)
}

#[tauri::command]
pub fn load_profile(
    app: AppHandle,
    path: String,
    state: State<EngineState>,
) -> Result<Profile, String> {
    // 只允许加载 profiles_dir 下的文件，防止路径遍历
    let dir = profiles_dir(&app)?;
    let file_name = Path::new(&path)
        .file_name()
        .ok_or("无效文件路径")?
        .to_string_lossy();
    let safe_path = dir.join(sanitize_filename(&file_name));

    let data = std::fs::read(&safe_path).map_err(|e| format!("读取文件失败: {}", e))?;

    let header = FileHeader::from_bytes(&data).ok_or("文件格式无效，可能已损坏")?;
    let aad = header.aad();
    let ciphertext = &data[FileHeader::SIZE..];

    let plaintext = aes::decrypt(ciphertext, &header.nonce, &aad).map_err(|e| e.to_string())?;

    let value: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("解析失败: {}", e))?;

    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(CURRENT_SCHEMA_VERSION as u64) as u32;

    let value = if version < CURRENT_SCHEMA_VERSION {
        migrate_profile(value, version).map_err(|e| format!("配置迁移失败: {}", e))?
    } else if version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "配置版本 {} 高于当前支持的版本 {}，请升级应用",
            version, CURRENT_SCHEMA_VERSION
        ));
    } else {
        value
    };

    let profile: Profile =
        serde_json::from_value(value).map_err(|e| format!("反序列化失败: {}", e))?;
    profile.validate().map_err(|e| e.to_string())?;

    state.0.set_rules(profile.rules.clone());
    Ok(profile)
}

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileMeta>, String> {
    let dir = profiles_dir(&app)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut metas = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("无法读取配置目录: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "qzh") {
            continue;
        }
        match std::fs::read(&path) {
            Ok(data) => {
                if let Some(header) = FileHeader::from_bytes(&data) {
                    let aad = header.aad();
                    let ciphertext = &data[FileHeader::SIZE..];
                    if let Ok(plaintext) = aes::decrypt(ciphertext, &header.nonce, &aad) {
                        if let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&plaintext)
                            .and_then(|v| {
                                serde_json::from_value::<ProfileMeta>(
                                    v.get("meta").cloned().unwrap_or_default(),
                                )
                            })
                        {
                            metas.push(meta);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("跳过无法读取的配置文件 {}: {}", path.display(), e);
            }
        }
    }
    metas.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
    Ok(metas)
}

/// 创建并落盘默认配置。供 Tauri command (`init_default_profile`) 与启动时
/// 兜底初始化（`lib.rs` 中加载失败/首启动）共享，避免双份 Profile 字面量与加密落盘代码漂移。
pub(crate) fn create_default_profile(
    app: &AppHandle,
    engine: &crate::engine::BurstEngine,
) -> Result<Profile, String> {
    let now = now_secs();

    let profile = Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        meta: ProfileMeta {
            name: "defults".to_string(),
            created_at: now,
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        rules: vec![
            BurstRule {
                id: make_id(),
                enabled: false,
                trigger_key: 0x51, // Q
                target_key: 0x51,
                mode: BurstMode::Hold,
                stop_key: None,
                interval_ms: 10,
            },
            BurstRule {
                id: make_id(),
                enabled: false,
                trigger_key: 0x46, // F
                target_key: 0x46,
                mode: BurstMode::Toggle,
                stop_key: None,
                interval_ms: 10,
            },
        ],
        hotkeys: Hotkeys::default(),
        advanced: Advanced::default(),
    };

    let dir = profiles_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {}", e))?;
    let file_path = dir.join("defults.qzh");

    let rules = profile.rules.clone();
    let path = write_profile_file_to_path(&file_path, &profile)?;
    engine.set_rules(rules);

    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set("activeProfilePath", serde_json::json!(path));
        let _ = store.save();
    }

    Ok(profile)
}

#[tauri::command]
pub fn init_default_profile(app: AppHandle, state: State<EngineState>) -> Result<Profile, String> {
    create_default_profile(&app, &state.0)
}

#[tauri::command]
pub fn get_active_profile_path(app: AppHandle) -> Result<Option<String>, String> {
    let store = app
        .store("settings.json")
        .map_err(|e| format!("无法读取存储: {}", e))?;
    Ok(store
        .get("activeProfilePath")
        .and_then(|v| v.as_str().map(|s| s.to_string())))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_ascii_control() => '_',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_windows_reserved_chars() {
        assert_eq!(
            sanitize_filename(r#"a<b>c:d"e/f\g|h?i*j"#),
            "a_b_c_d_e_f_g_h_i_j"
        );
    }

    #[test]
    fn sanitize_replaces_ascii_control_chars() {
        // 包含 NUL, \n, \t 等控制字符必须被替换
        assert_eq!(sanitize_filename("a\nb\tc\x00d"), "a_b_c_d");
    }

    #[test]
    fn sanitize_preserves_chinese_and_normal_chars() {
        // 防止越来越严的过滤误伤合法文件名
        assert_eq!(sanitize_filename("默认配置-v2"), "默认配置-v2");
    }

    #[test]
    fn sanitize_blocks_path_traversal_via_separators() {
        // ../../etc/passwd 类形态:斜杠 / 反斜杠都会被替换
        assert_eq!(sanitize_filename("../../etc/passwd"), "..__..__etc_passwd");
        assert_eq!(
            sanitize_filename(r"..\..\windows\system32"),
            "..__..__windows_system32"
        );
    }

    #[test]
    fn sanitize_keeps_dots_and_dashes() {
        // . 和 - 是合法文件名字符
        assert_eq!(sanitize_filename("my.profile-1"), "my.profile-1");
    }

    #[test]
    fn sanitize_handles_empty_string() {
        assert_eq!(sanitize_filename(""), "");
    }
}
