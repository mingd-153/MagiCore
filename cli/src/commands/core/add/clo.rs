//! `mgc add` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

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
    let root = shared::core_project_root("clo")?;
    let cloud_kind = super::super::dev::clo::cloud_type(&root)?;
    crate::commands::dep_gate::gate_cloud_project(
        &root,
        &cloud_kind,
        crate::commands::dep_gate::DepOp::Add,
        compat_runtime.as_deref(),
    )?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
