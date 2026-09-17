//! `mgc list app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn list() -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    let Some(cmd) = tool_command(lang, "list") else {
        return Err(manifest_hint(lang, "list"));
    };
    // C0 ownership firewall (T0.3): list spawns the provider tool.
    // List lanes take no --compat-runtime flag (unit variants) — compat
    // flows via MGC_COMPAT_RUNTIME only.
    // (Tường lửa C0: list spawn tool. Lane list không có cờ — compat chỉ
    // qua env.)
    let compat = crate::commands::dep_gate::from_dep_flag(None)?;
    crate::commands::dep_gate::gate(
        "app",
        crate::commands::dep_gate::DepOp::List,
        Some(cmd.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
