//! `mgc list app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    gate_react_native, language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    // tool_command is PURE (zero spawn). Order: React Native gates first
    // (P0#2); unimplemented verbs hint without gating; implemented verbs
    // gate with the exact tool.
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    gate_react_native(
        &root,
        lang,
        crate::commands::dep_gate::DepOp::List,
        &compat,
    )?;
    let Some(cmd) = tool_command(lang, "list") else {
        return Err(manifest_hint(lang, "list"));
    };
    // C0 ownership firewall (T0.3): list spawns the provider tool — the
    // explicit --compat-runtime flag is REQUIRED (P0#3).
    // (Tường lửa C0: list spawn tool — cờ tường minh BẮT BUỘC.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        Some(cmd.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
