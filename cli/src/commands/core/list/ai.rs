//! `mg list` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

fn list_args(tool: &str) -> Vec<String> {
    if tool == "uv" {
        vec!["pip".to_string(), "list".to_string()]
    } else {
        vec!["list".to_string()]
    }
}

pub async fn list() -> Result<()> {
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    let args = list_args(tool);
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
