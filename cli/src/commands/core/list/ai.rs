//! `mgc list` ai — tách từ core/ai.rs (Phase 7 v5).

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
    // C0 ownership firewall (T0.3): list spawns the provider tool.
    // List lanes take no --compat-runtime flag (unit variants) — compat
    // flows via MGC_COMPAT_RUNTIME only.
    // (Tường lửa C0: list spawn tool. Lane list không có cờ — compat chỉ
    // qua env.)
    let compat = crate::commands::dep_gate::from_dep_flag(None)?;
    crate::commands::dep_gate::gate(
        "ai",
        crate::commands::dep_gate::DepOp::List,
        Some(tool),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let args = list_args(tool);
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
