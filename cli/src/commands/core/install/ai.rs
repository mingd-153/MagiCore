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
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

fn ai_install_command(root: &std::path::Path) -> Result<(&'static str, Vec<String>)> {
    if root.join("uv.lock").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.lock").exists() {
        return Ok(("pip", pip_requirements_args("requirements.lock")));
    }
    if root.join("pyproject.toml").exists() {
        return Ok(("uv", vec!["sync".to_string()]));
    }
    if root.join("requirements.txt").exists() {
        return Ok(("pip", pip_requirements_args("requirements.txt")));
    }
    Err(crate::error::ai_no_lockfile())
}

fn pip_requirements_args(file: &str) -> Vec<String> {
    vec!["install".to_string(), "-r".to_string(), file.to_string()]
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
