//! `mgc remove` ai — tách từ core/ai.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

// DELEGATED: the args feed a REAL uv/pip run (ai_run_tool) — mgc
// orchestrates only, it does not own this dependency lifecycle.
// (DELEGATED: args này nuôi lệnh uv/pip THẬT (ai_run_tool) — mgc chỉ điều
// phối, không sở hữu lifecycle dependency này.)
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

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    if packages.is_empty() {
        return Err(crate::error::remove_ai_usage());
    }
    let root = shared::ai_project_root()?;
    let tool = shared::ai_pick_tool(&root);
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        "ai",
        crate::commands::dep_gate::DepOp::Remove,
        Some(tool),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let args = remove_args(&packages, tool);
    shared::ai_run_tool(&root, tool, &args)?;
    Ok(())
}

#[cfg(test)]
#[path = "test/ai.rs"]
mod tests;
