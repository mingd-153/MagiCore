//! Prune stale trust policies — remove policies for uninstalled packages
//! Dọn dẹp trust policy cũ — xóa policy của package đã gỡ

use anyhow::{Context, Result};
use mgc_store::{Database, Layout};
use std::env;

/// Execute trust prune — Thực thi trust prune
pub fn execute() -> Result<()> {
    // Get project root (current directory) — Lấy project root (thư mục hiện tại)
    let project_root = env::current_dir().context("failed to get current directory")?;

    // Layout for web cache — Layout cho cache web
    let cache_root = project_root.join(".magicore").join("cache").join("web");
    let layout = Layout::new(cache_root);

    // Open database — Mở database
    let db = Database::open(&layout.db_path()).context("failed to open trust policy database")?;

    // Prune stale policies (packages not installed) — Dọn policy cũ (package đã gỡ)
    let pruned_count = db
        .prune_trust_policies()
        .context("failed to prune trust policies")?;

    if pruned_count > 0 {
        println!("✓ Pruned {} stale trust policies", pruned_count);
        println!("  Removed policies for uninstalled packages.");
    } else {
        println!("✓ No stale trust policies found");
        println!("  All policies are for currently installed packages.");
    }

    Ok(())
}
