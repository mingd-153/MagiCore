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

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    // C0 ownership firewall (T0.3): list spawns the provider tool
    // (`uv pip list` / `pip list`) — the explicit --compat-runtime flag
    // is REQUIRED (P0#3), the env var stays as automation fallback only.
    // (Tường lửa C0: list spawn tool — cờ tường minh BẮT BUỘC.)
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "ai",
            Some(crate::commands::dep_gate::eco::PYTHON),
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
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
