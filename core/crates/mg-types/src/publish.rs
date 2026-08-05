//! Publish types — dùng chung giữa mg-publish, mg-pack, CLI (01 §5, Phase 0)
//! (Cấu hình publish + kết quả tóm tắt — một nguồn types cho publish pipeline)

use serde::{Deserialize, Serialize};

/// Cấu hình publish cho client (mg-publish) — không phải CLI args; CLI build từ đây.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishOptions {
    pub registry_url: String,
    #[serde(default = "default_tag")]
    pub tag: String,
    pub access: Option<String>,
    pub dry_run: bool,
    pub otp: Option<String>,
    pub ignore_scripts: bool,
    pub force: bool,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_tag() -> String {
    "latest".to_string()
}

fn default_retries() -> u32 {
    3
}

impl Default for PublishOptions {
    fn default() -> Self {
        Self {
            registry_url: String::new(),
            tag: default_tag(),
            access: None,
            dry_run: false,
            otp: None,
            ignore_scripts: false,
            force: false,
            retries: default_retries(),
        }
    }
}

/// Tóm tắt kết quả publish — migrate từ cli/commands/publish.rs để tái dùng (Phase 3+).
#[derive(Debug, Clone, Serialize)]
pub struct PublishSummary {
    pub name: String,
    pub version: String,
    pub tag: String,
    pub size: u64,
    pub unpacked_size: u64,
    pub shasum: String,
    pub integrity: String,
    pub files: usize,
    pub entry_count: usize,
}