//! Audit log — ghi mỗi lần chạy exec (00-index §5.4)
//! (exec audit vào .megagate/exec.log — args đã REDACTED, không bao giờ chứa secret)

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Một dòng audit (JSON) — args nhận SẴN đã redact từ sanitizer.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub dry_run: bool,
    pub ts: u64,
}

/// Append một dòng audit vào `path` (tạo parent dir nếu thiếu).
pub fn append(path: &Path, entry: &AuditEntry) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry)?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Timestamp hiện tại (giây kể từ epoch).
pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
