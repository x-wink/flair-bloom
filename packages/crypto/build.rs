//! 把许可证公钥列表编进 `$OUT_DIR/license_keys.rs`。
//!
//! - `FLAIR_LICENSE_PUBKEYS`：billing 交付的 JWK 数组（`[{kty,crv,x,kid}]`），发版构建必填。
//!   缺失或为空串即构建失败——GitHub 未配置的 secret 展开成空串，漏配时不能悄悄发出一个
//!   所有激活都失败的版本。本地发版构建不需要激活时显式写 `[]`。
//! - `license-dev-keys.json`：本地 billing 开发密钥的公钥，只在 `debug_assertions` 下编入，
//!   发版构建不认开发密钥签的许可证。

use std::{env, fmt::Write as _, fs, path::Path};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

#[path = "src/kid.rs"]
mod kid;

const ENV_KEYS: &str = "FLAIR_LICENSE_PUBKEYS";
const DEV_KEYS_FILE: &str = "license-dev-keys.json";

fn main() {
    println!("cargo:rerun-if-env-changed={ENV_KEYS}");
    println!("cargo:rerun-if-changed={DEV_KEYS_FILE}");
    println!("cargo:rerun-if-changed=src/kid.rs");

    let debug = env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some();
    let mut keys = match env::var(ENV_KEYS) {
        Ok(json) if !json.trim().is_empty() => parse_jwks(&json, ENV_KEYS),
        _ if debug => Vec::new(),
        _ => panic!(
            "发版构建必须设置 {ENV_KEYS}（billing 许可证公钥的 JWK 数组）；本地发版构建不需要激活时显式设为 []"
        ),
    };
    if debug {
        let json = fs::read_to_string(DEV_KEYS_FILE)
            .unwrap_or_else(|e| panic!("读取 {DEV_KEYS_FILE} 失败：{e}"));
        keys.extend(parse_jwks(&json, DEV_KEYS_FILE));
    }
    for (i, (kid, _)) in keys.iter().enumerate() {
        assert!(
            keys[..i].iter().all(|(k, _)| k != kid),
            "许可证公钥 kid {kid} 重复"
        );
    }

    let mut out = String::from("const EMBEDDED_KEYS: &[(&str, [u8; 32])] = &[\n");
    for (kid, key) in &keys {
        writeln!(out, "    ({kid:?}, {key:?}),").unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("license_keys.rs");
    fs::write(dest, out).unwrap();
}

fn parse_jwks(json: &str, source: &str) -> Vec<(String, [u8; 32])> {
    let jwks: Vec<serde_json::Value> =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("{source} 不是 JWK 数组：{e}"));
    jwks.iter()
        .map(|jwk| {
            let field = |name: &str| {
                jwk.get(name)
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("{source} 里的 JWK 缺少字段 {name}"))
            };
            assert!(
                field("kty") == "OKP" && field("crv") == "Ed25519",
                "{source} 里的 JWK 不是 Ed25519 公钥"
            );
            let key: [u8; 32] = URL_SAFE_NO_PAD
                .decode(field("x"))
                .ok()
                .and_then(|raw| raw.try_into().ok())
                .unwrap_or_else(|| panic!("{source} 里的 JWK x 不是 32 字节 base64url"));
            // 由 x 重算 kid 核对：Secrets 里贴错一个字符要在构建期暴露，而不是等亲友激活时
            let kid = kid::key_id(&key);
            assert!(
                field("kid") == kid,
                "{source} 里的 JWK kid {} 与按 x 算出的 {kid} 不一致",
                field("kid")
            );
            (kid, key)
        })
        .collect()
}
