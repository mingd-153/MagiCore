//! `mgc install` ai — tách từ core/ai.rs (Phase 7 v5). Lock ghép: uv.lock → pip requirements.lock.

use anyhow::Result;

use super::super::shared;

pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = shared::ai_project_root()?;
    if !packages.is_empty() {
        mgc_ui::info(&format!(
            "[ai install] ignoring package args {:?}; install is driven by project lock files (05 §5)",
            packages
        ));
    }
    let (tool, args) = ai_install_command(&root)?;
    if dry_run {
        mgc_ui::info(&format!("[dry-run] {} {}", tool, args.join(" ")));
        return Ok(());
    }
    // Shared store (B-series, 2026-09-12): uv/pip caches point INSIDE
    // the mgc store (~/.magicore/store/pypi) — every ai/python project
    // on this machine reuses the same wheel/sdist bytes (one download,
    // many projects; identical root to the lib/python lane).
    // Store chia sẻ (B-series): cache uv/pip trỏ VÀO store mgc
    // (~/.magicore/store/pypi) — mọi project ai/python trên máy dùng
    // lại cùng byte wheel/sdist (tải một lần, nhiều project; cùng gốc
    // với lane lib/python).
    let shared_env = shared::shared_pypi_store_env()?;
    shared::ai_run_tool_with_env(&root, tool, &args, shared_env)?;
    Ok(())
}

fn ai_install_command(root: &std::path::Path) -> Result<(&'static str, Vec<String>)> {
    // pip3 fallback for systems without a `pip` alias (merged from the RC line).
    // pip3 fallback cho hệ thống không có alias `pip` (gộp từ nhánh RC).
    let pip_cmd = if std::process::Command::new("pip")
        .arg("--version")
        .output()
        .is_ok()
    {
        "pip"
    } else if std::process::Command::new("pip3")
        .arg("--version")
        .output()
        .is_ok()
    {
        "pip3"
    } else {
        "pip" // Fallback — missing tool surfaces a clear error at spawn time.
    };
    if root.join("uv.lock").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.lock").exists() {
        return Ok((pip_cmd, pip_requirements_args("requirements.lock")));
    }
    if root.join("pyproject.toml").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.txt").exists() {
        return Ok((pip_cmd, pip_requirements_args("requirements.txt")));
    }
    Err(crate::error::ai_no_lockfile())
}

fn pip_requirements_args(file: &str) -> Vec<String> {
    vec!["install".to_string(), "-r".to_string(), file.to_string()]
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
