//! 外部配置导入命令。支持从第三方按键助手解析并导入配置。
//!
//! 扩展方式：在 [`PARSERS`] 中追加新的 [`ParserDef`] 条目即可支持新格式。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, State};

use qzh_profile::{
    Advanced, BurstMode, BurstRule, Hotkeys, KeyId, MouseButton, Profile, ProfileMeta,
    CURRENT_SCHEMA_VERSION, MAX_RULES,
};

use super::engine::EngineState;
use super::profile::{
    now_secs, pick_unique_name, profiles_dir, set_active_path, write_profile_file_to_path,
};

static IMPORT_SEQ: AtomicU64 = AtomicU64::new(0);

fn make_id() -> String {
    let n = IMPORT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}-imp{n}", now_secs())
}

// ─── 公共结果类型 ─────────────────────────────────────────────────────────────

/// 扫描到的一份外部配置。
#[derive(Debug, Serialize)]
pub struct FoundConfig {
    /// 文件绝对路径。
    pub path: String,
    /// 来源软件名称，如「丐帮高手」。
    pub source_app: String,
    /// 用于列表展示的简短说明（含文件所在目录名）。
    pub display_name: String,
}

/// 解析后的导入预览，不含完整规则列表，用于在 UI 做确认展示。
#[derive(Debug, Serialize)]
pub struct ImportPreview {
    /// 来源软件名称。
    pub source_app: String,
    /// 建议的配置文件名（取自所在目录名，可由用户修改）。
    pub suggested_name: String,
    /// 将会生成的规则条数（已截断到 MAX_RULES）。
    pub rule_count: usize,
    /// 因超出上限而丢弃的规则条数。
    pub skipped_count: usize,
    /// 连发间隔（ms）。
    pub interval_ms: u32,
    /// 检测到的全局开启键。
    pub global_toggle: Option<KeyId>,
    /// 检测到的全局停止键（与开启键相同时为 None）。
    pub global_stop: Option<KeyId>,
    /// 检测到的面板显隐键。
    pub panel_toggle: Option<KeyId>,
}

// ─── 解析器定义（可扩展注册表）────────────────────────────────────────────────

struct ParserDef {
    /// 该格式的配置文件名（大小写不敏感匹配）。
    config_filenames: &'static [&'static str],
    /// 来源软件名称。
    source_app: &'static str,
    /// 快速检测：返回 true 则认为该文件属于此格式。
    detect: fn(data: &str) -> bool,
    /// 解析为预览信息。
    parse_preview: fn(data: &str, dir_name: &str) -> ImportPreview,
    /// 解析为完整规则列表（用于实际导入）。
    parse_rules: fn(data: &str) -> Vec<BurstRule>,
    /// 提取全局开关键。
    parse_hotkeys: fn(data: &str) -> Hotkeys,
}

/// 解析器注册表。新增格式支持：在此追加一个 `ParserDef`。
static PARSERS: &[ParserDef] = &[ParserDef {
    config_filenames: &["config.json"],
    source_app: "丐帮高手",
    detect: gaibang_detect,
    parse_preview: gaibang_parse_preview,
    parse_rules: gaibang_parse_rules,
    parse_hotkeys: gaibang_parse_hotkeys,
}];

// ─── 丐帮高手解析器 ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GaibangConfig {
    #[serde(default = "default_delay_us")]
    delay_us: u64,
    #[serde(default)]
    exclude: Vec<u32>,
    #[serde(default)]
    hk_toggle: u32,
    #[serde(default)]
    hk_stop: u32,
    #[serde(default)]
    hk_game: u32,
}

fn default_delay_us() -> u64 {
    50_000
}

/// 将 Win32 VK 码映射为 KeyId；鼠标按钮单独映射，其余均视为键盘键。
fn vk_to_key_id(vk: u32) -> KeyId {
    match vk {
        1 => KeyId::Mouse(MouseButton::Left),
        2 => KeyId::Mouse(MouseButton::Right),
        4 => KeyId::Mouse(MouseButton::Middle),
        5 => KeyId::Mouse(MouseButton::X1),
        6 => KeyId::Mouse(MouseButton::X2),
        k => KeyId::Keyboard(k),
    }
}

/// 「可连发」VK 码范围：字符键、功能键、数字键盘、OEM 标点。
fn burstable_vks() -> impl Iterator<Item = u32> {
    (1u32..=2) // 鼠标左/右键
        .chain(4..=6) // 鼠标中/侧键
        .chain(0x30..=0x39) // 数字行 0-9
        .chain(0x41..=0x5A) // 字母 A-Z
        .chain(0x60..=0x6B) // 数字键盘 0-9, *, +
        .chain(0x6D..=0x6F) // 数字键盘 -, ., /（跳过 0x6C VK_SEPARATOR，非标准键）
        .chain(0x70..=0x7B) // F1-F12
        .chain(0xBA..=0xC0) // OEM 标点 ;=,-./ `
        .chain(0xDB..=0xDE) // OEM 标点 [{ \| ]} '"
}

fn gaibang_detect(data: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(data)
        .map(|v| v.get("exclude").is_some() && v.get("delay_us").is_some())
        .unwrap_or(false)
}

fn parse_gaibang_cfg(data: &str) -> Option<GaibangConfig> {
    serde_json::from_str(data).ok()
}

fn gaibang_active_vks(cfg: &GaibangConfig) -> Vec<u32> {
    let mut excluded: HashSet<u32> = cfg.exclude.iter().copied().collect();
    for hk in [cfg.hk_toggle, cfg.hk_stop, cfg.hk_game] {
        if hk != 0 {
            excluded.insert(hk);
        }
    }
    burstable_vks().filter(|k| !excluded.contains(k)).collect()
}

fn gaibang_interval(cfg: &GaibangConfig) -> u32 {
    ((cfg.delay_us / 1000) as u32).clamp(10, 10000)
}

fn gaibang_parse_preview(data: &str, dir_name: &str) -> ImportPreview {
    let cfg = parse_gaibang_cfg(data).unwrap_or(GaibangConfig {
        delay_us: default_delay_us(),
        exclude: vec![],
        hk_toggle: 0,
        hk_stop: 0,
        hk_game: 0,
    });
    let active = gaibang_active_vks(&cfg);
    let rule_count = active.len().min(MAX_RULES);
    let skipped_count = active.len().saturating_sub(MAX_RULES);
    let interval_ms = gaibang_interval(&cfg);
    let global_toggle = (cfg.hk_toggle != 0).then(|| vk_to_key_id(cfg.hk_toggle));
    let global_stop =
        (cfg.hk_stop != 0 && cfg.hk_stop != cfg.hk_toggle).then(|| vk_to_key_id(cfg.hk_stop));
    let panel_toggle = (cfg.hk_game != 0).then(|| vk_to_key_id(cfg.hk_game));
    ImportPreview {
        source_app: "丐帮高手".to_string(),
        suggested_name: dir_name.to_string(),
        rule_count,
        skipped_count,
        interval_ms,
        global_toggle,
        global_stop,
        panel_toggle,
    }
}

fn gaibang_parse_rules(data: &str) -> Vec<BurstRule> {
    let Some(cfg) = parse_gaibang_cfg(data) else {
        return vec![];
    };
    let interval_ms = gaibang_interval(&cfg);
    gaibang_active_vks(&cfg)
        .into_iter()
        .take(MAX_RULES)
        .map(|vk| BurstRule {
            id: make_id(),
            enabled: true,
            trigger_key: vk_to_key_id(vk),
            target_key: vk_to_key_id(vk),
            mode: BurstMode::Hold,
            stop_key: None,
            interval_ms,
            group: None,
        })
        .collect()
}

fn gaibang_parse_hotkeys(data: &str) -> Hotkeys {
    let Some(cfg) = parse_gaibang_cfg(data) else {
        return Hotkeys::default();
    };
    let global_toggle = (cfg.hk_toggle != 0).then(|| vk_to_key_id(cfg.hk_toggle));
    let global_stop =
        (cfg.hk_stop != 0 && cfg.hk_stop != cfg.hk_toggle).then(|| vk_to_key_id(cfg.hk_stop));
    let panel_toggle = (cfg.hk_game != 0).then(|| vk_to_key_id(cfg.hk_game));
    Hotkeys {
        global_toggle,
        global_stop,
        panel_toggle,
    }
}

// ─── 工具函数 ─────────────────────────────────────────────────────────────────

/// 检测并返回能处理 `path` 的解析器。
fn find_parser(path: &Path) -> Option<&'static ParserDef> {
    let filename = path.file_name()?.to_string_lossy().to_lowercase();
    let data = std::fs::read_to_string(path).ok()?;
    PARSERS
        .iter()
        .find(|p| p.config_filenames.contains(&filename.as_str()) && (p.detect)(&data))
}

/// 在目录的一级子目录中扫描所有已知外部配置文件。
fn scan_dir(dir: &Path) -> Vec<FoundConfig> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // 直接在该目录下的文件
        if path.is_file() {
            if let Some(p) = to_found_config(&path) {
                found.push(p);
            }
        }
        // 进入一级子目录
        if path.is_dir() {
            let Ok(sub) = std::fs::read_dir(&path) else {
                continue;
            };
            for sub_entry in sub.flatten() {
                let sub_path = sub_entry.path();
                if sub_path.is_file() {
                    if let Some(p) = to_found_config(&sub_path) {
                        found.push(p);
                    }
                }
            }
        }
    }
    found
}

fn to_found_config(path: &Path) -> Option<FoundConfig> {
    let parser = find_parser(path)?;
    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Some(FoundConfig {
        path: path.to_string_lossy().to_string(),
        source_app: parser.source_app.to_string(),
        display_name: if dir_name.is_empty() {
            path.to_string_lossy().to_string()
        } else {
            dir_name.to_string()
        },
    })
}

/// 默认扫描目录：桌面 + 下载（跨平台）。
fn default_scan_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let base = PathBuf::from(&profile);
        dirs.push(base.join("Desktop"));
        dirs.push(base.join("Downloads"));
        dirs.push(PathBuf::from(r"C:\Users\Public\Desktop"));
    }

    #[cfg(not(windows))]
    if let Ok(home) = std::env::var("HOME") {
        let base = PathBuf::from(&home);
        dirs.push(base.join("Desktop"));
        dirs.push(base.join("Downloads"));
    }

    dirs
}

// ─── Tauri 命令 ───────────────────────────────────────────────────────────────

/// 扫描指定目录（为空则扫描桌面/下载等默认位置）中已知的外部配置文件。
#[tauri::command]
pub fn scan_import_configs(dirs: Vec<String>) -> Vec<FoundConfig> {
    let targets: Vec<PathBuf> = if dirs.is_empty() {
        default_scan_dirs()
    } else {
        dirs.into_iter().map(PathBuf::from).collect()
    };
    let mut result = Vec::new();
    for dir in targets {
        result.extend(scan_dir(&dir));
    }
    result
}

/// 导入文件大小上限（4 MB），防止大文件导致 OOM。
const IMPORT_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 解析指定文件，返回导入预览。路径可来自 [`scan_import_configs`] 或用户手动输入。
#[tauri::command]
pub fn preview_import(path: String) -> Result<ImportPreview, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("文件不存在：{path}"));
    }
    let parser =
        find_parser(p).ok_or_else(|| "无法识别的配置格式，目前支持：丐帮高手".to_string())?;
    let size = p
        .metadata()
        .map_err(|e| format!("读取文件信息失败：{e}"))?
        .len();
    if size > IMPORT_MAX_BYTES {
        return Err(format!("文件过大（{size} 字节），超过 4 MB 上限"));
    }
    let data = std::fs::read_to_string(p).map_err(|e| format!("读取文件失败：{e}"))?;
    let dir_name = p
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty() && n != "." && n != "/" && n != "\\")
        .unwrap_or_else(|| "导入配置".to_string());
    Ok((parser.parse_preview)(&data, &dir_name))
}

/// 将外部配置导入为一份新的 FlairBloom 配置文件，立即切换为活跃配置。
#[tauri::command]
pub fn import_external_config(
    app: AppHandle,
    state: State<EngineState>,
    path: String,
    profile_name: String,
) -> Result<Profile, String> {
    let trimmed = profile_name.trim();
    if trimmed.is_empty() {
        return Err("配置名不能为空".into());
    }

    let p = Path::new(&path);
    let parser = find_parser(p).ok_or_else(|| "无法识别的配置格式".to_string())?;
    let size = p
        .metadata()
        .map_err(|e| format!("读取文件信息失败：{e}"))?
        .len();
    if size > IMPORT_MAX_BYTES {
        return Err(format!("文件过大（{size} 字节），超过 4 MB 上限"));
    }
    let data = std::fs::read_to_string(p).map_err(|e| format!("读取文件失败：{e}"))?;

    let rules = (parser.parse_rules)(&data);
    if rules.len() > MAX_RULES {
        return Err(format!("规则数量 {} 超过上限 {MAX_RULES}", rules.len()));
    }
    for (i, rule) in rules.iter().enumerate() {
        if !(10..=10000).contains(&rule.interval_ms) {
            return Err(format!(
                "第 {} 条规则间隔 {}ms 超出范围",
                i + 1,
                rule.interval_ms
            ));
        }
    }

    let hotkeys = (parser.parse_hotkeys)(&data);
    let now = now_secs();
    let dir = profiles_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录：{e}"))?;

    let (final_name, final_path) = pick_unique_name(&dir, trimmed);

    let profile = Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        meta: ProfileMeta {
            name: final_name,
            created_at: now,
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        rules: rules.clone(),
        hotkeys: hotkeys.clone(),
        advanced: Advanced::default(),
    };

    let saved_path = write_profile_file_to_path(&final_path, &profile)?;
    set_active_path(&app, &saved_path);
    state.0.set_rules(rules);
    state.0.set_hotkeys(hotkeys);

    Ok(profile)
}
