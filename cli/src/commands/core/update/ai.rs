//! `mgc update` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

fn update_args(packages: &[String], tool: &str) -> Vec<String> {
    if packages.is_empty() {
        return if tool == "uv" {
            vec!["lock".to_string(), "--upgrade".to_string()]
        } else {
            vec!["list".to_string(), "--outdated".to_string()]
        };
    }
    if tool == "uv" {
        let mut args = vec!["lock".to_string()];
        for p in packages.iter().flat_map(|p| p.split_whitespace()) {
            args.push("--upgrade-package".to_string());
            args.push(p.to_string());
        }
        args
    } else {
        let mut args = vec!["install".to_string(), "--upgrade".to_string()];
        args.extend(
            packages
                .iter()
                .flat_map(|p| p.split_whitespace().map(String::from)),
        );
        args
    }
}

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    let args = update_args(&packages, tool);
    shared::ai_run_tool(&root, tool, &args)?;
    if packages.is_empty() {
        if tool == "uv" {
            if install {
                shared::ai_run_tool(&root, tool, &["sync".to_string()])?;
            }
        } else {
            mgc_ui::info(
                "Run `pip list --outdated` to see newer versions — pip does not auto-upgrade the lock.",
            );
        }
        return Ok(());
    }
    if tool == "uv" {
        if install {
            shared::ai_run_tool(&root, tool, &["sync".to_string()])?;
        }
    } else {
        // pip: cập nhật lock sau khi upgrade
        let locked = shared::ai_run_tool_capture(&root, tool, &["freeze".to_string()])?;
        std::fs::write(root.join("requirements.lock"), locked)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
