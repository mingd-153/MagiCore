//! `mg list-hardware` — báo optimizer/bench hiện có qua adapter. Phase 7 v5.

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mg_project_found("generic"))?;
    Ok(root)
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    shared::list(
        &*crate::factory::create_adapter(&mg_types::Ecosystem::Hardware, None, None)
            .expect("hardware adapter always available in hardware core build"),
        &root,
    )
    .await
}
