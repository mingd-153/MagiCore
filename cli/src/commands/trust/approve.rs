//! Approve package for lifecycle scripts — trust approve command
//! Cho phép package chạy lifecycle scripts — lệnh trust approve

use anyhow::{Context, Result};
use std::env;

/// Execute trust approve — Thực thi trust approve
pub fn execute(package: &str) -> Result<()> {
    // Get project root (current directory) — Lấy project root (thư mục hiện tại)
    let project_root = env::current_dir().context("failed to get current directory")?;

    // Serialize the decision with install's lifecycle-script gate.
    // (Tuần tự hóa quyết định với cổng lifecycle script của install.)
    super::script_policy::set_policy(
        &project_root,
        package,
        super::script_policy::ScriptPolicy::Approved,
    )?;

    println!("✓ Approved lifecycle scripts for: {}", package);
    println!("  Package can now run install/postinstall scripts.");

    Ok(())
}
