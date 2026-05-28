//! Packaged driver resource integrity checks.

use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy)]
pub struct ExpectedResource {
    pub rel: &'static str,
    pub label: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const EXPECTED_RESOURCES: &[ExpectedResource] = &[
    ExpectedResource {
        rel: "install-interception.exe",
        label: "install-interception.exe",
        size: 470_528,
        sha256: "E137863A79DA797F08E7A137280FF2A123809044A888FD75CE9C973198915ABE",
    },
    ExpectedResource {
        rel: "ddhid.63340.dll",
        label: "ddhid.63340.dll",
        size: 2_242_088,
        sha256: "01E8DB6893CF79E9E7AA3AFBEE76BEA6C4220C4D1A2C63BC2E5B7C109FDB831E",
    },
    ExpectedResource {
        rel: "ddhid-driver/ddc.exe",
        label: "ddhid-driver/ddc.exe",
        size: 97_240,
        sha256: "3C535B334F0897B8A0870BCB476C30EA79AFD09CFC18F8E00190BDC7C6C46785",
    },
    ExpectedResource {
        rel: "ddhid-driver/ddhid63340.inf",
        label: "ddhid-driver/ddhid63340.inf",
        size: 1_685,
        sha256: "17FE3814F57E98DD2AF97F56B63502E474EA5E41CDA1A510FFE435EE6AD7A104",
    },
    ExpectedResource {
        rel: "ddhid-driver/ddhid63340.cat",
        label: "ddhid-driver/ddhid63340.cat",
        size: 12_110,
        sha256: "6135C664711127A62E0988F6844521E345D78ACE9D3747A392400CE99BE96983",
    },
    ExpectedResource {
        rel: "ddhid-driver/ddhid63340.sys",
        label: "ddhid-driver/ddhid63340.sys",
        size: 1_190_080,
        sha256: "FBE510402B3822C63E94752051B7D5895B67875F22EC48593DE19764A649F8B1",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceIssueKind {
    Missing,
    SizeMismatch {
        actual: u64,
        expected: u64,
    },
    HashMismatch {
        actual: String,
        expected: &'static str,
    },
    ReadError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceIssue {
    pub rel: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
    pub kind: ResourceIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceHealth {
    pub checked: usize,
    pub issues: Vec<ResourceIssue>,
}

impl ResourceHealth {
    pub fn ok(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_missing(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| matches!(issue.kind, ResourceIssueKind::Missing))
    }
}

pub fn check_resources(resources_dir: &Path) -> ResourceHealth {
    let mut issues = Vec::new();
    for expected in EXPECTED_RESOURCES {
        let path = resources_dir.join(expected.rel);
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                issues.push(ResourceIssue {
                    rel: expected.rel,
                    label: expected.label,
                    path,
                    kind: ResourceIssueKind::Missing,
                });
                continue;
            }
            Err(e) => {
                issues.push(ResourceIssue {
                    rel: expected.rel,
                    label: expected.label,
                    path,
                    kind: ResourceIssueKind::ReadError(e.to_string()),
                });
                continue;
            }
        };

        let actual_size = meta.len();
        if actual_size != expected.size {
            issues.push(ResourceIssue {
                rel: expected.rel,
                label: expected.label,
                path,
                kind: ResourceIssueKind::SizeMismatch {
                    actual: actual_size,
                    expected: expected.size,
                },
            });
            continue;
        }

        match sha256_file_hex(&path) {
            Ok(actual_hash) if actual_hash == expected.sha256 => {}
            Ok(actual_hash) => issues.push(ResourceIssue {
                rel: expected.rel,
                label: expected.label,
                path,
                kind: ResourceIssueKind::HashMismatch {
                    actual: actual_hash,
                    expected: expected.sha256,
                },
            }),
            Err(e) => issues.push(ResourceIssue {
                rel: expected.rel,
                label: expected.label,
                path,
                kind: ResourceIssueKind::ReadError(e.to_string()),
            }),
        }
    }
    ResourceHealth {
        checked: EXPECTED_RESOURCES.len(),
        issues,
    }
}

pub fn issue_label(issue: &ResourceIssue) -> String {
    match &issue.kind {
        ResourceIssueKind::Missing => format!("{} 缺失", issue.label),
        ResourceIssueKind::SizeMismatch { actual, expected } => {
            format!(
                "{} 大小异常：实际 {}，期望 {}",
                issue.label, actual, expected
            )
        }
        ResourceIssueKind::HashMismatch { actual, expected } => {
            format!(
                "{} SHA256 不匹配：实际 {}，期望 {}",
                issue.label, actual, expected
            )
        }
        ResourceIssueKind::ReadError(e) => format!("{} 读取失败：{}", issue.label, e),
    }
}

pub(crate) fn sha256_file_hex(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02X}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_manifest_pins_ddhid_inf_original_bytes() {
        let inf = EXPECTED_RESOURCES
            .iter()
            .find(|item| item.rel == "ddhid-driver/ddhid63340.inf")
            .unwrap();
        assert_eq!(inf.size, 1_685);
        assert_eq!(
            inf.sha256,
            "17FE3814F57E98DD2AF97F56B63502E474EA5E41CDA1A510FFE435EE6AD7A104"
        );
    }
}
