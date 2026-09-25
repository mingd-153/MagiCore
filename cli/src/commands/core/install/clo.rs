//! `mgc install` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::dev::clo as clo_tools;
use super::super::shared;
use mgc_types::Ecosystem;

pub async fn install(
    packages: Vec<String>,
    dry_run: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    // All cloud dependency lifecycle operations pass the same owner gate;
    // only CDK/Pulumi with package.json use the embedded MGC JS engine.
    let root = shared::core_project_root("clo")?;
    let cloud_kind = clo_tools::cloud_type(&root)?;
    crate::commands::dep_gate::gate_cloud_project(
        &root,
        &cloud_kind,
        crate::commands::dep_gate::DepOp::Install,
        compat_runtime.as_deref(),
    )?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    if dry_run {
        if !packages.is_empty() {
            mgc_ui::info(&format!(
                "[dry-run] would add cloud packages: {:?}",
                packages
            ));
        }
        mgc_ui::info(&format!(
            "[dry-run] would run MagiCore-managed dependency install for {cloud_kind}"
        ));
        return Ok(());
    }
    if !packages.is_empty() {
        shared::add(
            &*adapter, &root, packages, None, false, false, false, false, false, false, false,
        )
        .await?;
    }
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        false,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}
