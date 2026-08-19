//! `mg install` ai — tách từ core/ai.rs (Phase 7 v5). Lock ghép: uv.lock → pip requirements.lock.

use anyhow::Result;

use super::super::shared;

pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = shared::ai_project_root()?;
    if !packages.is_empty() {
        mg_ui::info(&format!(
            "[ai install] ignoring package args {:?} — install theo lock (05 §5)",
            packages
        ));
    }
    let (tool, args): (&str, Vec<String>) = if root.join("uv.lock").exists() {
        ("uv", vec!["sync".to_string()])
    } else if root.join("requirements.lock").exists() {
        (
            "pip",
            vec![
                "install".to_string(),
                "-r".to_string(),
                "requirements.lock".to_string(),
            ],
        )
    } else {
        return Err(crate::error::ai_no_lockfile());
    };
    if dry_run {
        mg_ui::info(&format!("[dry-run] {} {}", tool, args.join(" ")));
        return Ok(());
    }
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
