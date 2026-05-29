//! FlairBloom packaged driver resource integrity checks.

use std::path::Path;

use ::resource_integrity as generic_resource_integrity;
#[allow(unused_imports)]
pub use generic_resource_integrity::{
    sha256_file_hex, ResourceHealth, ResourceIssue, ResourceIssueKind, ResourceSpec,
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct ExpectedResource {
    pub rel: &'static str,
    pub label: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn check_resources(resources_dir: &Path) -> ResourceHealth {
    let specs = EXPECTED_RESOURCES
        .iter()
        .map(|expected| ResourceSpec {
            rel: expected.rel,
            size: expected.size,
            sha256: expected.sha256,
        })
        .collect::<Vec<_>>();
    generic_resource_integrity::check_resources(resources_dir, &specs)
}

#[allow(dead_code)]
pub fn issue_label(issue: &ResourceIssue) -> String {
    let label = label_for_rel(issue.rel);
    match &issue.kind {
        ResourceIssueKind::Missing => format!("{label} 缺失"),
        ResourceIssueKind::SizeMismatch { actual, expected } => {
            format!("{label} 大小异常：实际 {actual}，期望 {expected}")
        }
        ResourceIssueKind::HashMismatch { actual, expected } => {
            format!("{label} SHA256 不匹配：实际 {actual}，期望 {expected}")
        }
        ResourceIssueKind::ReadError(e) => format!("{label} 读取失败：{e}"),
    }
}

fn label_for_rel(rel: &str) -> &str {
    EXPECTED_RESOURCES
        .iter()
        .find(|item| item.rel == rel)
        .map(|item| item.label)
        .unwrap_or(rel)
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
