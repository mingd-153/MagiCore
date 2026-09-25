//! `mgc list` clo — tách từ core/clo.rs (Phase 7 v5).
//!
//! GATED NATIVE READ (P0#3): adapter manifest read — no toolchain spawn,
//! no package mutation in this lane. The gate records the native decision.
//! (Đọc manifest qua adapter, có gate — lane này không spawn toolchain,
//! không đổi package.)

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let cloud_type = mgc_cloud_adapter::detect_type(&root)
        .map(|kind| kind.as_str())
        .ok_or_else(|| crate::error::no_mgc_project_found("clo"))?;
    crate::commands::dep_gate::gate_cloud_project(
        &root,
        cloud_type,
        crate::commands::dep_gate::DepOp::List,
        compat_runtime.as_deref(),
    )?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::list(&*adapter, &root).await
}
