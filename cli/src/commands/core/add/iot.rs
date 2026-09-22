//! `mgc add iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use mgc_types::Ecosystem;
use mgc_types::adapter::PackageAdapter;
use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("iot"))?;
    Ok(root)
}

fn iot_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Iot, None, None)
        .expect("iot adapter always available in iot core build")
}

#[allow(clippy::too_many_arguments)]
pub async fn add(
    packages: Vec<String>,
    version: Option<String>,
    dev: bool,
    exact: bool,
    optional: bool,
    peer: bool,
    no_save: bool,
    global: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = project_root()?;
    // Native lane (mgc.lock, no Cargo.lock): esp32-rust deps book through
    // the NATIVE crates engine (resolve-first + mgc-side Cargo edit) —
    // zero `cargo` spawn. Other frameworks keep the legacy delegated
    // path below.
    // (Lane native: esp32-rust + Cargo.toml → engine crates native.)
    {
        let iot = mgc_iot_adapter::adapter_for(&root);
        let framework = iot.as_ref().map(|a| a.framework());
        let target_owned = iot.as_ref().and_then(|a| a.target(&root));
        if framework == Some("esp32-rust") && root.join("Cargo.toml").is_file() {
            let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
            crate::commands::dep_gate::gate(
                &crate::commands::dep_gate::DepContext::new(
                    "iot",
                    Some("esp32-rust"),
                    Some("esp32-rust"),
                    target_owned.as_deref(),
                    crate::commands::dep_gate::DepOp::Add,
                ),
                None,
                &compat,
                Some(&root.join(".magicore").join("exec.log")),
            )?;
            let lib_adapter =
                crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None).map_err(
                    |e| anyhow::anyhow!("iot native add needs the lib Cargo engine: {e}"),
                )?;
            return shared::add(
                &*lib_adapter,
                &root,
                packages,
                version,
                dev,
                exact,
                optional,
                peer,
                no_save,
                true,
                global,
            )
            .await;
        }
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context (P0#2): detected framework id + board target.
    let iot = mgc_iot_adapter::adapter_for(&root);
    let framework = iot.as_ref().map(|a| a.framework());
    let target_owned = iot.as_ref().and_then(|a| a.target(&root));
    // C0 ownership firewall (T0.3): the IoT add lane routes to the
    // adapter, whose frameworks delegate (cargo/pio/west).
    // (Tường lửa C0: lane add IoT gọi adapter, framework trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "iot",
            framework,
            framework,
            target_owned.as_deref(),
            crate::commands::dep_gate::DepOp::Add,
        ),
        // Exact tool for the detected framework (never None on a
        // spawning lane).
        framework.and_then(shared::iot_framework_tool),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = iot_adapter();
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
