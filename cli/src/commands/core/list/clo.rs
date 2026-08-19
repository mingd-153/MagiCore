//! `mg list` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mg_types::Ecosystem;

pub async fn list() -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::list(&*adapter, &root).await
}
