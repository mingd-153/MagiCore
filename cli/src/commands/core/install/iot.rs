//! `mgc install iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("iot"))?;
    Ok(root)
}

pub async fn install(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context (P0#2): the detected framework id IS the iot
    // ecosystem (no separate language layer) plus the board target.
    // Undetectable framework ⇒ Unsupported, never generic-delegated.
    // (Context gate đầy đủ: framework detect được là ecosystem iot.)
    let iot = mgc_iot_adapter::adapter_for(&root);
    let framework = iot.as_ref().map(|a| a.framework());
    let target_owned = iot.as_ref().and_then(|a| a.target(&root));
    // Native lane (mgc.lock, no Cargo.lock): esp32-rust projects ARE
    // Cargo projects — deps resolve through the NATIVE crates engine
    // (same as lib/rust) with zero `cargo` spawn. Other frameworks keep
    // the legacy delegated path below.
    // (Lane native: esp32-rust + Cargo.toml → engine crates native.)
    if framework == Some("esp32-rust") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "iot",
                Some("esp32-rust"),
                Some("esp32-rust"),
                target_owned.as_deref(),
                crate::commands::dep_gate::DepOp::Install,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("iot native install needs the lib Cargo engine: {e}"))?;
        if !packages.is_empty() {
            crate::commands::core::shared::add(
                &*lib_adapter,
                &root,
                packages,
                None,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
            )
            .await?;
        }
        return crate::commands::core::shared::install_with_adapter(
            &*lib_adapter,
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
    if framework == Some("esp32-rust") {
        anyhow::bail!(
            "native ESP32-Rust dependency installation requires Cargo.toml at the project root"
        );
    }
    // Unsupported frameworks are rejected by the ownership gate before
    // mutation; `--compat-runtime` is not an implementation claim.
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "iot",
            framework,
            framework,
            target_owned.as_deref(),
            crate::commands::dep_gate::DepOp::Install,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let _ = packages;
    anyhow::bail!("MagiCore does not yet own dependency installation for this IoT framework")
}
