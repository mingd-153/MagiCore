//! `mgc remove` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn remove(packages: Vec<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::remove(&*adapter, &root, packages, true).await
}
