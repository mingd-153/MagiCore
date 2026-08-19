//! `mg remove` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mg_types::Ecosystem;

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::remove(&*adapter, &root, packages, true).await
}
