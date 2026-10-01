//! Prune stale trust policies — remove policies for uninstalled packages
//! Dọn dẹp trust policy cũ — xóa policy của package đã gỡ

use anyhow::{Context, Result};
use std::env;

/// Execute trust prune — Thực thi trust prune
pub fn execute() -> Result<()> {
    // Get project root (current directory) — Lấy project root (thư mục hiện tại)
    let project_root = env::current_dir().context("failed to get current directory")?;

    // Serialize pruning with installs that update the package table.
    // (Tuần tự hóa prune với install đang cập nhật bảng package.)
    let pruned_count = super::script_policy::prune_stale_policies(&project_root)?;

    if pruned_count > 0 {
        println!("✓ Pruned {} stale trust policies", pruned_count);
        println!("  Removed policies for uninstalled packages.");
    } else {
        println!("✓ No stale trust policies found");
        println!("  All policies are for currently installed packages.");
    }

    Ok(())
}
