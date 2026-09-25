//! `mgc remove app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    gate_react_native, language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    if packages.is_empty() {
        return Err(crate::error::remove_app_usage());
    }
    // Flutter remove uses the shared journaled manifest writer. Swift and
    // Kotlin mutations stay blocked in the ownership gate until their
    // source/catalog edits use the same crash-recovery transaction.
    // (Flutter qua journal chung; Swift/Kotlin mutation đang fail-closed.)
    if lang == mgc_app_adapter::AppLanguage::Flutter {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "app",
                Some(lang.ecosystem()),
                None,
                None,
                crate::commands::dep_gate::DepOp::Remove,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let adapter = mgc_app_adapter::adapter_for(&root)
            .ok_or_else(crate::error::app_project_not_detected)?;
        return crate::commands::core::shared::remove(&adapter, &root, packages, true).await;
    }
    // tool_command is PURE (zero spawn). Order: React Native gates first
    // (P0#2), then gate with the resolved tool (None when the verb has no
    // command — the exact table cell answers Unsupported). The manifest
    // hint below is defensive fallback.
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    gate_react_native(
        &root,
        lang,
        crate::commands::dep_gate::DepOp::Remove,
        &compat,
    )?;
    let cmd_opt = tool_command(lang, "remove");
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Remove,
        ),
        cmd_opt.as_ref().map(|c| c.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let Some(mut cmd) = cmd_opt else {
        return Err(manifest_hint(lang, "remove"));
    };
    cmd.args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
