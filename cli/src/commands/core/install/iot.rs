//! `mgc install iot` — tách từ core/iot.rs (Phase 7 v5).

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
    // C0 ownership firewall (T0.3): the IoT install lane routes to the
    // adapter, whose frameworks delegate (cargo/pio/west).
    // (Tường lửa C0: lane install IoT gọi adapter, framework trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "iot",
            framework,
            framework,
            target_owned.as_deref(),
            crate::commands::dep_gate::DepOp::Install,
        ),
        // Exact tool for the detected framework (never None on a
        // spawning lane): esp32-rust→cargo, platformio→pio, zephyr→west.
        framework.and_then(shared::iot_framework_tool),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = iot_adapter();
    for pkg in &packages {
        let spinner = mgc_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mgc_types::PackageName::new(pkg)?;
        let opts = mgc_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    // Delegated install (esp32-rust→cargo, platformio→pio,
    // zephyr→west): the iot adapter has no mgc-native resolve — routing
    // through install_with_adapter would die in prepare_install_execution
    // with "does not support 'resolve'" on the first real dependency.
    // Install straight into the adapter with an empty graph (each
    // framework's installer owns its lifecycle) and print the same
    // honest summary footer. Behavior for dependency-free manifests is
    // unchanged (that path already reached adapter.install with an
    // empty graph).
    // (Install ủy thác: gọi adapter trực tiếp với graph rỗng.)
    let started_at = std::time::Instant::now();
    let mut summary = adapter
        .install(
            &mgc_types::adapter::ResolvedGraph::empty(),
            &root,
            mgc_types::adapter::InstallOptions {
                legacy_flat: false,
                ..Default::default()
            },
        )
        .await?;
    summary.duration_ms = started_at.elapsed().as_millis() as u64;
    let cache_source = match summary.cache_mode {
        mgc_types::adapter::InstallCacheMode::MgCStore => "shared mgc store",
        mgc_types::adapter::InstallCacheMode::Delegated => "native toolchain cache",
    };
    mgc_ui::print_install_summary_source(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
        Some(cache_source),
    );
    mgc_ui::blank_line();
    mgc_ui::success("All dependencies installed");
    Ok(())
}
