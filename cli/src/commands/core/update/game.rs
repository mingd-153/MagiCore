//! `mg update` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mg_types::Ecosystem;

pub async fn update(packages: Vec<String>, install: bool) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::update(&*adapter, &root, packages, install).await
}