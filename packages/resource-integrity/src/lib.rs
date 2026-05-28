//! Generic filesystem resource integrity checks.

use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy)]
pub struct ResourceSpec {
    pub rel: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

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

pub fn check_resources(resources_dir: &Path, specs: &[ResourceSpec]) -> ResourceHealth {
    let mut issues = Vec::new();
    for spec in specs {
        let path = resources_dir.join(spec.rel);
        let meta = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                issues.push(ResourceIssue {
                    rel: spec.rel,
                    path,
                    kind: ResourceIssueKind::Missing,
                });
                continue;
            }
            Err(e) => {
                issues.push(ResourceIssue {
                    rel: spec.rel,
                    path,
                    kind: ResourceIssueKind::ReadError(e.to_string()),
                });
                continue;
            }
        };

        let actual_size = meta.len();
        if actual_size != spec.size {
            issues.push(ResourceIssue {
                rel: spec.rel,
                path,
                kind: ResourceIssueKind::SizeMismatch {
                    actual: actual_size,
                    expected: spec.size,
                },
            });
            continue;
        }

        match sha256_file_hex(&path) {
            Ok(actual_hash) if actual_hash == spec.sha256 => {}
            Ok(actual_hash) => issues.push(ResourceIssue {
                rel: spec.rel,
                path,
                kind: ResourceIssueKind::HashMismatch {
                    actual: actual_hash,
                    expected: spec.sha256,
                },
            }),
            Err(e) => issues.push(ResourceIssue {
                rel: spec.rel,
                path,
                kind: ResourceIssueKind::ReadError(e.to_string()),
            }),
        }
    }
    ResourceHealth {
        checked: specs.len(),
        issues,
    }
}

pub fn sha256_file_hex(path: &Path) -> Result<String, io::Error> {
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
    fn check_resources_reports_ok_for_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.bin");
        std::fs::write(&path, b"abc").unwrap();

        let health = check_resources(
            dir.path(),
            &[ResourceSpec {
                rel: "ok.bin",
                size: 3,
                sha256: "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
            }],
        );

        assert!(health.ok());
    }

    #[test]
    fn check_resources_reports_size_before_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.bin");
        std::fs::write(&path, b"abc").unwrap();

        let health = check_resources(
            dir.path(),
            &[ResourceSpec {
                rel: "short.bin",
                size: 4,
                sha256: "unused",
            }],
        );

        assert_eq!(
            health.issues[0].kind,
            ResourceIssueKind::SizeMismatch {
                actual: 3,
                expected: 4
            }
        );
    }

    #[test]
    fn check_resources_reports_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let health = check_resources(
            dir.path(),
            &[ResourceSpec {
                rel: "missing.bin",
                size: 1,
                sha256: "unused",
            }],
        );

        assert!(matches!(health.issues[0].kind, ResourceIssueKind::Missing));
    }
}
