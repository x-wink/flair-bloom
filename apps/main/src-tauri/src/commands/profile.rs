use crypto::aes;
use qzh_format::header::{FileHeader, MAGIC, VERSION};
use qzh_profile::{
    migrate_profile, Advanced, BurstMode, BurstRule, Hotkeys, KeyId, Profile, ProfileMeta,
    CURRENT_SCHEMA_VERSION,
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

/// 默认配置名（同时是文件名 stem）。受保护：不能改名也不能删除，
/// 用户对它的修改会触发自动 fork（见 [`fork_active_profile`]）。
pub const DEFAULT_PROFILE_NAME: &str = "defults";

/// store 中存储「当前激活配置文件绝对路径」的键名。
pub const ACTIVE_PATH_KEY: &str = "activeProfilePath";

pub(crate) fn profiles_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("profiles"))
        .map_err(|e| format!("无法获取应用数据目录: {e}"))
}

pub(crate) fn now_secs() -> u64 {
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

pub(crate) fn write_profile_file_to_path(
    file_path: &Path,
    profile: &Profile,
) -> Result<String, String> {
    let json = serde_json::to_vec(profile).map_err(|e| format!("序列化失败: {e}"))?;
    let aad = compute_aad();
    let (ciphertext, nonce) = aes::encrypt(&json, &aad).map_err(|e| e.to_string())?;
    let header = FileHeader::new(nonce);

    let mut data = header.to_bytes();
    data.extend_from_slice(&ciphertext);

    let tmp_path = file_path.with_extension("qzh.tmp");
    std::fs::write(&tmp_path, &data).map_err(|e| format!("写入临时文件失败: {e}"))?;
    std::fs::rename(&tmp_path, file_path).map_err(|e| format!("替换配置文件失败: {e}"))?;
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

/// 读取并完整解密一个 .qzh 文件，返回带迁移与校验的 [`Profile`]。
/// 与 [`load_profile`] 命令的差别：不触发 engine.set_rules，也不更新 active path，
/// 适合 rename/fork/delete 等内部流程消费。
fn read_profile_from_file(file_path: &Path) -> Result<Profile, String> {
    let data = std::fs::read(file_path).map_err(|e| format!("读取文件失败: {e}"))?;
    let header = FileHeader::from_bytes(&data).ok_or("文件格式无效，可能已损坏")?;
    let aad = header.aad();
    let ciphertext = &data[FileHeader::SIZE..];
    let plaintext = aes::decrypt(ciphertext, &header.nonce, &aad).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("解析失败: {e}"))?;
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(CURRENT_SCHEMA_VERSION as u64) as u32;
    let value = if version < CURRENT_SCHEMA_VERSION {
        migrate_profile(value, version).map_err(|e| format!("配置迁移失败: {e}"))?
    } else if version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "配置版本 {version} 高于当前支持的版本 {CURRENT_SCHEMA_VERSION}，请升级应用"
        ));
    } else {
        value
    };
    let profile: Profile =
        serde_json::from_value(value).map_err(|e| format!("反序列化失败: {e}"))?;
    profile.validate().map_err(|e| e.to_string())?;
    Ok(profile)
}

fn profile_path_for_name(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.qzh", sanitize_filename(name)))
}

pub(crate) fn set_active_path(app: &AppHandle, path: &str) {
    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set(ACTIVE_PATH_KEY, serde_json::json!(path));
        let _ = store.save();
    }
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
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {e}"))?;

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
    let hotkeys = profile.hotkeys.clone();
    let path = write_profile_file_to_path(&file_path, &profile)?;

    // 更新 store 中记录的活跃配置路径
    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set(ACTIVE_PATH_KEY, serde_json::json!(path));
        let _ = store.save();
    }

    state.0.set_rules(rules);
    state.0.set_hotkeys(hotkeys);
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

    let data = std::fs::read(&safe_path).map_err(|e| format!("读取文件失败: {e}"))?;

    let header = FileHeader::from_bytes(&data).ok_or("文件格式无效，可能已损坏")?;
    let aad = header.aad();
    let ciphertext = &data[FileHeader::SIZE..];

    let plaintext = aes::decrypt(ciphertext, &header.nonce, &aad).map_err(|e| e.to_string())?;

    let value: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("解析失败: {e}"))?;

    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(CURRENT_SCHEMA_VERSION as u64) as u32;

    let value = if version < CURRENT_SCHEMA_VERSION {
        migrate_profile(value, version).map_err(|e| format!("配置迁移失败: {e}"))?
    } else if version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "配置版本 {version} 高于当前支持的版本 {CURRENT_SCHEMA_VERSION}，请升级应用"
        ));
    } else {
        value
    };

    let profile: Profile =
        serde_json::from_value(value).map_err(|e| format!("反序列化失败: {e}"))?;
    profile.validate().map_err(|e| e.to_string())?;

    state.0.set_rules(profile.rules.clone());
    state.0.set_hotkeys(profile.hotkeys.clone());
    set_active_path(&app, &safe_path.to_string_lossy());
    Ok(profile)
}

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileEntry>, String> {
    let dir = profiles_dir(&app)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    let read = std::fs::read_dir(&dir).map_err(|e| format!("无法读取配置目录: {e}"))?;
    for entry in read.flatten() {
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
                            entries.push(ProfileEntry {
                                meta,
                                path: path.to_string_lossy().to_string(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                warn!("跳过无法读取的配置文件 {}: {}", path.display(), e);
            }
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.meta.updated_at));
    Ok(entries)
}

#[derive(serde::Serialize)]
pub struct ProfileEntry {
    pub meta: ProfileMeta,
    pub path: String,
}

/// 出厂默认规则（不含 id，调用方负责注入）。抽出此函数避免与
/// [`create_default_profile`] 漂移。
fn factory_default_rules() -> Vec<BurstRule> {
    vec![
        BurstRule {
            id: make_id(),
            enabled: false,
            trigger_key: KeyId::Keyboard(0x51), // Q
            target_key: KeyId::Keyboard(0x51),
            mode: BurstMode::Hold,
            stop_key: None,
            interval_ms: 10,
        },
        BurstRule {
            id: make_id(),
            enabled: false,
            trigger_key: KeyId::Keyboard(0x46), // F
            target_key: KeyId::Keyboard(0x46),
            mode: BurstMode::Toggle,
            stop_key: None,
            interval_ms: 10,
        },
    ]
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
            name: DEFAULT_PROFILE_NAME.to_string(),
            created_at: now,
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        rules: factory_default_rules(),
        hotkeys: Hotkeys::default(),
        advanced: Advanced::default(),
    };

    let dir = profiles_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {e}"))?;
    let file_path = dir.join(format!("{DEFAULT_PROFILE_NAME}.qzh"));

    let rules = profile.rules.clone();
    let hotkeys = profile.hotkeys.clone();
    let path = write_profile_file_to_path(&file_path, &profile)?;
    engine.set_rules(rules);
    engine.set_hotkeys(hotkeys);

    if let Ok(store) = app.store(crate::STORE_PATH) {
        store.set(ACTIVE_PATH_KEY, serde_json::json!(path));
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
        .store(crate::STORE_PATH)
        .map_err(|e| format!("无法读取存储: {e}"))?;
    Ok(store
        .get(ACTIVE_PATH_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string())))
}

/// 重命名一个配置文件：sanitize 新名 → 改 meta.name 重写新文件 → 删除旧文件 →
/// 如改的是当前激活配置则同步 `activeProfilePath`。返回新文件绝对路径。
#[tauri::command]
pub fn rename_profile(
    app: AppHandle,
    old_name: String,
    new_name: String,
) -> Result<String, String> {
    if old_name == DEFAULT_PROFILE_NAME {
        return Err("默认配置不可重命名".into());
    }
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("配置名不能为空".into());
    }
    if trimmed == DEFAULT_PROFILE_NAME {
        return Err("不能使用默认配置名".into());
    }
    let dir = profiles_dir(&app)?;
    let old_path = profile_path_for_name(&dir, &old_name);
    let new_path = profile_path_for_name(&dir, trimmed);

    if !old_path.exists() {
        return Err(format!("配置不存在：{old_name}"));
    }
    if old_path != new_path && new_path.exists() {
        return Err(format!("已存在同名配置：{trimmed}"));
    }

    let mut profile = read_profile_from_file(&old_path)?;
    profile.meta.name = trimmed.to_string();
    profile.meta.updated_at = now_secs();
    profile.meta.app_version = env!("CARGO_PKG_VERSION").to_string();

    let saved_path = write_profile_file_to_path(&new_path, &profile)?;
    if old_path != new_path {
        std::fs::remove_file(&old_path).map_err(|e| format!("删除旧文件失败: {e}"))?;
    }

    // 如果改的是当前激活配置，同步 store
    let was_active = app
        .store(crate::STORE_PATH)
        .ok()
        .and_then(|s| {
            s.get(ACTIVE_PATH_KEY)
                .and_then(|v| v.as_str().map(String::from))
        })
        .map(|p| Path::new(&p) == old_path)
        .unwrap_or(false);
    if was_active {
        set_active_path(&app, &saved_path);
    }
    Ok(saved_path)
}

/// 删除指定配置文件；如删的是当前激活配置，回退到默认配置（必要时重新创建）。
/// 返回删除后引擎应当装载的 [`Profile`]（仅当删的是激活配置时返回 Some，
/// 前端据此刷新 UI；否则返回 None）。
#[tauri::command]
pub fn delete_profile(
    app: AppHandle,
    state: State<EngineState>,
    name: String,
) -> Result<Option<Profile>, String> {
    if name == DEFAULT_PROFILE_NAME {
        return Err("默认配置不可删除".into());
    }
    let dir = profiles_dir(&app)?;
    let target_path = profile_path_for_name(&dir, &name);
    if !target_path.exists() {
        return Err(format!("配置不存在：{name}"));
    }

    let active_path = app.store(crate::STORE_PATH).ok().and_then(|s| {
        s.get(ACTIVE_PATH_KEY)
            .and_then(|v| v.as_str().map(String::from))
    });
    let was_active = active_path
        .as_deref()
        .map(|p| Path::new(p) == target_path)
        .unwrap_or(false);

    std::fs::remove_file(&target_path).map_err(|e| format!("删除配置失败: {e}"))?;

    if !was_active {
        return Ok(None);
    }

    // 删的是激活配置：加载默认配置；若不存在或损坏则重建
    let default_path = dir.join(format!("{DEFAULT_PROFILE_NAME}.qzh"));
    let profile = match read_profile_from_file(&default_path) {
        Ok(p) => {
            state.0.set_rules(p.rules.clone());
            state.0.set_hotkeys(p.hotkeys.clone());
            set_active_path(&app, &default_path.to_string_lossy());
            p
        }
        Err(e) => {
            if default_path.exists() {
                warn!("默认配置损坏将重建: {}", e);
            }
            // create_default_profile 内部会 set_rules + set_active_path
            create_default_profile(&app, &state.0)?
        }
    };
    Ok(Some(profile))
}

/// 基于当前激活配置 fork 出一份副本：选一个不冲突的名字，落盘新文件，
/// 把 `activeProfilePath` 切到新文件。返回新 [`Profile`] 与新路径。
/// 用于「修改默认配置时自动新建一份」的 fork-on-edit 流程。
#[tauri::command]
pub fn fork_active_profile(app: AppHandle, suggested_name: String) -> Result<ForkResult, String> {
    let dir = profiles_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {e}"))?;

    let active_path_str = app
        .store(crate::STORE_PATH)
        .ok()
        .and_then(|s| {
            s.get(ACTIVE_PATH_KEY)
                .and_then(|v| v.as_str().map(String::from))
        })
        .ok_or("当前没有激活配置")?;
    let active_path = Path::new(&active_path_str);
    let mut profile = read_profile_from_file(active_path)?;

    let base = {
        let trimmed = suggested_name.trim();
        if trimmed.is_empty() || trimmed == DEFAULT_PROFILE_NAME {
            "我的配置"
        } else {
            trimmed
        }
    };
    let (final_name, final_path) = pick_unique_name(&dir, base);

    let now = now_secs();
    profile.meta.name = final_name.clone();
    profile.meta.created_at = now;
    profile.meta.updated_at = now;
    profile.meta.app_version = env!("CARGO_PKG_VERSION").to_string();
    profile.schema_version = CURRENT_SCHEMA_VERSION;

    let saved_path = write_profile_file_to_path(&final_path, &profile)?;
    set_active_path(&app, &saved_path);

    Ok(ForkResult {
        profile,
        path: saved_path,
    })
}

#[derive(serde::Serialize)]
pub struct ForkResult {
    pub profile: Profile,
    pub path: String,
}

/// 在 `dir` 下挑一个不冲突的文件名：base、base 2、base 3 ...
pub(crate) fn pick_unique_name(dir: &Path, base: &str) -> (String, PathBuf) {
    let first = profile_path_for_name(dir, base);
    if !first.exists() {
        return (base.to_string(), first);
    }
    for i in 2.. {
        let candidate = format!("{base} {i}");
        let path = profile_path_for_name(dir, &candidate);
        if !path.exists() {
            return (candidate, path);
        }
    }
    unreachable!()
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
#[path = "profile_tests.rs"]
mod tests;
