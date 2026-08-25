//! `mgc update` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    shared::update(&*adapter, &root, packages, install).await
}
