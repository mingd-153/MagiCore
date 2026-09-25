//! `mgc update app` — passthrough tool theo language (Q18 allowlist §5.1). Phase 7 v5.

use anyhow::Result;

use crate::commands::core::install::app::{
    gate_react_native, language, manifest_hint, project_root, run_tool, tool_command,
};

pub async fn update(
    packages: Vec<String>,
    _install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = project_root()?;
    let lang = language(&root)?;
    // Flutter updates natively (pub.dev resolve-latest + mgc-side pubspec
    // edit + native install tail, zero `flutter` spawn). Other languages
    // keep the legacy delegated path below.
    // (Flutter update native.)
    if lang == mgc_app_adapter::AppLanguage::Flutter {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "app",
                Some(lang.ecosystem()),
                None,
                None,
                crate::commands::dep_gate::DepOp::Update,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let adapter = mgc_app_adapter::adapter_for(&root)
            .ok_or_else(crate::error::app_project_not_detected)?;
        return crate::commands::core::shared::native_update(&adapter, &root, packages, true).await;
    }
    // Swift/Kotlin source/catalog mutations do not yet share the crash
    // journal. The ownership gate below rejects these cells before any
    // direct writer or provider package-manager command can run.
    // (Mutation Swift/Kotlin chưa qua journal; gate từ chối trước khi ghi.)
    // tool_command is PURE (zero spawn). Order: React Native gates first
    // (P0#2), then gate with the resolved tool (None when the verb has no
    // command — the exact table cell answers Unsupported). The manifest
    // hint below is defensive fallback.
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    gate_react_native(
        &root,
        lang,
        crate::commands::dep_gate::DepOp::Update,
        &compat,
    )?;
    let cmd_opt = tool_command(lang, "update");
    // C0 ownership firewall (T0.3): single control path.
    // (Tường lửa C0: đường điều khiển duy nhất.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "app",
            Some(lang.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::Update,
        ),
        cmd_opt.as_ref().map(|c| c.tool.as_str()),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let Some(mut cmd) = cmd_opt else {
        return Err(manifest_hint(lang, "update"));
    };
    cmd.args.extend(
        packages
            .iter()
            .flat_map(|p| p.split_whitespace().map(String::from)),
    );
    run_tool(&root, &cmd.tool, &cmd.args)?;
    Ok(())
}
