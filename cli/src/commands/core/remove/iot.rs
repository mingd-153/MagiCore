//! `mgc remove iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("iot"))?;
    Ok(root)
}

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context (P0#2): detected framework id + board target.
    let iot = mgc_iot_adapter::adapter_for(&root);
    let framework = iot.as_ref().map(|a| a.framework());
    let target_owned = iot.as_ref().and_then(|a| a.target(&root));
    // Native lane (mgc.lock, no Cargo.lock): esp32-rust removals edit
    // Cargo.toml mgc-side with zero `cargo` spawn. Other frameworks keep
    // the legacy delegated path below.
    // (Lane native: esp32-rust + Cargo.toml → edit mgc-side.)
    if framework == Some("esp32-rust") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "iot",
                Some("esp32-rust"),
                Some("esp32-rust"),
                target_owned.as_deref(),
                crate::commands::dep_gate::DepOp::Remove,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("iot native remove needs the lib Cargo engine: {e}"))?;
        return shared::remove(&*lib_adapter, &root, packages, true).await;
    }
    if framework == Some("esp32-rust") {
        anyhow::bail!(
            "native ESP32-Rust dependency removal requires Cargo.toml at the project root"
        );
    }
    // Non-native IoT package operations fail closed.
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "iot",
            framework,
            framework,
            target_owned.as_deref(),
            crate::commands::dep_gate::DepOp::Remove,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let _ = packages;
    anyhow::bail!("MagiCore does not yet own dependency removal for this IoT framework")
}
