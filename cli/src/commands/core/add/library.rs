//! `mg add library` — tách từ core/library.rs (Phase 7 v5).

use anyhow::Result;
use mg_types::adapter::PackageAdapter;
use mg_types::Ecosystem;
fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root =
        shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mg_project_found("lib"))?;
    Ok(root)
}

fn lib_adapter() -> Arc<dyn PackageAdapter> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let (registry_url, token) = crate::context::ProjectContext::load_at(
        &cwd,
        mg_config::project::ProjectConfig::find_project_root(&cwd).as_ref(),
        None,
    )
    .ok()
    .map(|ctx| {
        (
            ctx.config.registries.first().map(|r| r.url.clone()),
            ctx.config.registries.first().and_then(|r| r.token.clone()),
        )
    })
    .map(|(u, t)| {
        (
            u.or_else(|| std::env::var("MEGAGATE_LIB_REGISTRY_URL").ok()),
            t.or_else(|| std::env::var("MEGAGATE_LIB_REGISTRY_TOKEN").ok()),
        )
    })
    .unwrap_or_else(|| {
        (
            std::env::var("MEGAGATE_LIB_REGISTRY_URL").ok(),
            std::env::var("MEGAGATE_LIB_REGISTRY_TOKEN").ok(),
        )
    });
    crate::factory::create_adapter(&Ecosystem::Lib, registry_url.as_deref(), token.as_deref())
        .expect("lib adapter always available in lib core build")
}

use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

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
    let adapter = lib_adapter();
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
