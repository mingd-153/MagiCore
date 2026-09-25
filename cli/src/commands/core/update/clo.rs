//! `mgc update` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn update(
    packages: Vec<String>,
    install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let cloud_kind = super::super::dev::clo::cloud_type(&root)?;
    crate::commands::dep_gate::gate_cloud_project(
        &root,
        &cloud_kind,
        crate::commands::dep_gate::DepOp::Update,
        compat_runtime.as_deref(),
    )?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::update(&*adapter, &root, packages, install).await
}
