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
        // Mutation gateway: direct native_update callers hold the writer
        // lock themselves (shared::update is bypassed here by design).
        // (Gọi native trực tiếp thì tự giữ lock writer.)
        let write_lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
            &root,
            crate::commands::core::shared::writer_lock_timeout(&root),
        )
        .map_err(|e| anyhow::anyhow!("update cannot acquire the project writer lock: {e}"))?;
        return crate::commands::core::shared::native_update(
            &adapter,
            &root,
            packages,
            true,
            &write_lock,
        )
        .await;
    }
    // Swift updates natively for registry pins (resolve-latest +
    // Package.swift text bump verified by re-scan + native install
    // tail). Git/branch pins report honest skips; an unconfigured
    // registry fails the op closed (never a silent partial pass).
    // (Swift update native cho pin registry.)
    if lang == mgc_app_adapter::AppLanguage::Swift {
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
        let (updated, skipped) = adapter.update_swift_native(&root, &packages).await?;
        for (name, from, to) in &updated {
            mgc_ui::info(&format!("  {name}: {from} → {to}"));
        }
        for (name, reason) in &skipped {
            mgc_ui::info(&format!("  {name}: skipped ({reason})"));
        }
        if updated.is_empty() {
            mgc_ui::info("All packages are up to date");
            return Ok(());
        }
        mgc_ui::success(&format!("Updated {} package(s)", updated.len()));
        return crate::commands::core::shared::install_with_adapter(
            &adapter,
            &root,
            "mgc add",
            false,
            mgc_types::adapter::InstallOptions {
                legacy_flat: false,
                ..Default::default()
            },
        )
        .await;
    }
    // Kotlin updates natively through the version catalog
    // (resolve-latest + bump + re-parse verify, zero `gradle` spawn).
    // No install tail (install stays delegated-gradle). Projects without
    // a catalog fail closed with guidance.
    // (Kotlin update native qua version catalog.)
    if lang == mgc_app_adapter::AppLanguage::Kotlin {
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
        let updated = adapter.update_kotlin_native(&root, &packages).await?;
        if updated.is_empty() {
            mgc_ui::info("All packages are up to date");
            return Ok(());
        }
        for (name, from, to) in &updated {
            mgc_ui::info(&format!("  {name}: {from} → {to}"));
        }
        mgc_ui::success(&format!("Updated {} package(s)", updated.len()));
        return Ok(());
    }
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
