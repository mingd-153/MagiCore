//! `mg add` ai — tách từ core/ai.rs (Phase 7 v5). 05 §5: chốt 1 tool theo lock (uv/pip).

use anyhow::Result;

use super::super::shared;

fn add_args(packages: &[String], tool: &str) -> Vec<String> {
    let mut args = vec![if tool == "uv" { "add" } else { "install" }.to_string()];
    args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    args
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    _version: Option<String>,
    _dev: bool,
    _exact: bool,
    _optional: bool,
    _peer: bool,
    _no_save: bool,
    _global: bool,
) -> Result<()> {
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    let args = add_args(&packages, tool);
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
