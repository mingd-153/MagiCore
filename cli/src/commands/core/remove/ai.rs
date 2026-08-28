//! `mgc remove` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

fn remove_args(packages: &[String], tool: &str) -> Vec<String> {
    let mut args = vec![if tool == "uv" {
        "remove".to_string()
    } else {
        "uninstall".to_string()
    }];
    if tool == "pip" {
        args.push("-y".to_string());
    }
    args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    args
}

pub async fn remove(packages: Vec<String>) -> Result<()> {
    if packages.is_empty() {
        return Err(crate::error::remove_ai_usage());
    }
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    let args = remove_args(&packages, tool);
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
