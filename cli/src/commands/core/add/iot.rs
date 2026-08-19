//! `mg add iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mg_project_found("iot"))?;
    Ok(root)
}

fn iot_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Iot, None, None)
        .expect("iot adapter always available in iot core build")
}

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
) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
