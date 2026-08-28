//! Deny package lifecycle scripts — trust deny command
//! Từ chối package chạy lifecycle scripts — lệnh trust deny

use anyhow::{bail, Context, Result};
use mgc_store::{Database, Layout};
use std::env;

/// Execute trust deny — Thực thi trust deny
pub fn execute(package: &str) -> Result<()> {
    // Validate package name — Xác thực tên package
    if package.is_empty() {
        bail!("package name cannot be empty");
    }

    // Get project root (current directory) — Lấy project root (thư mục hiện tại)
    let project_root = env::current_dir().context("failed to get current directory")?;

    // Layout for web cache — Layout cho cache web
    let cache_root = project_root.join(".magicore").join("cache").join("web");
    let layout = Layout::new(cache_root);

    // Open database — Mở database
    let db = Database::open(&layout.db_path()).context("failed to open trust policy database")?;

    // Upsert policy to 'denied' — Thêm/cập nhật policy thành 'denied'
    db.upsert_trust_policy(package, "denied")
        .context("failed to save trust policy")?;

    println!("✓ Denied lifecycle scripts for: {}", package);
    println!("  Package will not run install/postinstall scripts.");

    Ok(())
}
