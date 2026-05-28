//! 气质花（FlairBloom）业务 Profile schema：连发规则、KeyId、宏序列与 schema 迁移。
//!
//! 提供高层 I/O helper [`load_from_path`] / [`save_to_path`]，封装
//! read+decrypt+migrate+validate / serialize+encrypt+atomic-rename 全流程。
//!
//! 文件容器格式（[`qzh_format::header::FileHeader`] / AES-GCM）由 `qzh-format` crate 负责。

pub mod key_id;
pub mod macro_seq;
pub mod profile;
pub mod schema_migrate;

pub use key_id::{KeyId, MouseButton};
pub use profile::{
    Advanced, BurstMode, BurstRule, Hotkeys, Profile, ProfileError, ProfileMeta,
    CURRENT_SCHEMA_VERSION, MAX_RULES,
};
pub use schema_migrate::migrate_profile;

use std::path::Path;

impl From<qzh_format::FormatError> for ProfileError {
    fn from(e: qzh_format::FormatError) -> Self {
        match e {
            qzh_format::FormatError::Io(e) => Self::Io(e),
            qzh_format::FormatError::InvalidHeader => Self::InvalidFormat,
            qzh_format::FormatError::Crypto(e) => Self::Crypto(e),
            qzh_format::FormatError::Json(e) => Self::Json(e),
        }
    }
}

/// 读取、解密、迁移、校验一个 `.qzh` 文件，返回 [`Profile`]。
pub fn load_from_path(path: &Path) -> Result<Profile, ProfileError> {
    let (value, schema_version) = qzh_format::read_encrypted(path)?;
    let value = if schema_version < CURRENT_SCHEMA_VERSION {
        migrate_profile(value, schema_version)?
    } else if schema_version > CURRENT_SCHEMA_VERSION {
        return Err(ProfileError::TooNew(schema_version));
    } else {
        value
    };
    let profile: Profile = serde_json::from_value(value)?;
    profile.validate()?;
    Ok(profile)
}

/// 校验、序列化、加密、原子写入 [`Profile`] 到 `.qzh` 文件。
pub fn save_to_path(path: &Path, profile: &Profile) -> Result<(), ProfileError> {
    profile.validate()?;
    qzh_format::write_encrypted(path, profile)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Advanced, Hotkeys, ProfileMeta};

    fn minimal_profile() -> Profile {
        Profile {
            schema_version: CURRENT_SCHEMA_VERSION,
            meta: ProfileMeta {
                name: "test".into(),
                created_at: 0,
                updated_at: 0,
                app_version: "0".into(),
            },
            rules: vec![],
            hotkeys: Hotkeys::default(),
            advanced: Advanced::default(),
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir();
        let path = dir.join("qzh_profile_test_roundtrip.qzh");
        let profile = minimal_profile();
        save_to_path(&path, &profile).unwrap();
        let loaded = load_from_path(&path).unwrap();
        assert_eq!(loaded.meta.name, "test");
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_from_nonexistent_file_returns_io_error() {
        let path = std::path::PathBuf::from("/tmp/qzh_profile_nonexistent.qzh");
        let _ = std::fs::remove_file(&path);
        assert!(matches!(load_from_path(&path), Err(ProfileError::Io(_))));
    }

    #[test]
    fn save_rejects_invalid_profile() {
        use crate::key_id::KeyId;
        use crate::profile::{BurstMode, BurstRule};
        let dir = std::env::temp_dir();
        let path = dir.join("qzh_profile_test_invalid.qzh");
        let mut profile = minimal_profile();
        // 间隔超范围
        profile.rules.push(BurstRule {
            id: "r".into(),
            enabled: true,
            trigger_key: KeyId::Keyboard(0x51),
            target_key: KeyId::Keyboard(0x51),
            mode: BurstMode::Hold,
            stop_key: None,
            interval_ms: 5, // < 10
        });
        assert!(matches!(
            save_to_path(&path, &profile),
            Err(ProfileError::InvalidInterval(5))
        ));
        let _ = std::fs::remove_file(&path);
    }
}
