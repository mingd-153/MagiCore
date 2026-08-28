//! Approve package for lifecycle scripts — trust approve command
//! Cho phép package chạy lifecycle scripts — lệnh trust approve

use anyhow::{bail, Context, Result};
use mgc_store::{Database, Layout};
use std::env;

/// Execute trust approve — Thực thi trust approve
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

    // Upsert policy to 'approved' — Thêm/cập nhật policy thành 'approved'
    db.upsert_trust_policy(package, "approved")
        .context("failed to save trust policy")?;

    println!("✓ Approved lifecycle scripts for: {}", package);
    println!("  Package can now run install/postinstall scripts.");

    Ok(())
}
