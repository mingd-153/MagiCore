//! `mgc list` clo — tách từ core/clo.rs (Phase 7 v5).
//!
//! GATE-EXEMPT: adapter manifest read — no toolchain spawn, no package
//! mutation in this lane.
//! (GATE-EXEMPT: đọc manifest qua adapter — lane này không spawn
//! toolchain, không đổi package.)

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn list() -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::list(&*adapter, &root).await
}
