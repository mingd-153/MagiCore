//! `mgc list library` — tách từ core/library.rs (Phase 7 v5).
//!
//! GATED NATIVE READ (P0#3): adapter manifest/lock read — no toolchain
//! spawn, no package mutation in this lane. The gate records the native
//! decision (and logs the ignore notice when --compat-runtime is passed).
//! (Đọc manifest/lock qua adapter, có gate — lane này không spawn
//! toolchain, không đổi package.)

use anyhow::Result;
use mgc_types::Ecosystem;
use mgc_types::adapter::PackageAdapter;
fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("lib"))?;
    Ok(root)
}

fn lib_adapter() -> Arc<dyn PackageAdapter> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let (registry_url, token) = crate::context::ProjectContext::load_at(
        &cwd,
        mgc_config::project::ProjectConfig::find_project_root(&cwd).as_ref(),
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
            u.or_else(|| std::env::var("MAGICORE_LIB_REGISTRY_URL").ok()),
            t.or_else(|| std::env::var("MAGICORE_LIB_REGISTRY_TOKEN").ok()),
        )
    })
    .unwrap_or_else(|| {
        (
            std::env::var("MAGICORE_LIB_REGISTRY_URL").ok(),
            std::env::var("MAGICORE_LIB_REGISTRY_TOKEN").ok(),
        )
    });
    crate::factory::create_adapter(&Ecosystem::Lib, registry_url.as_deref(), token.as_deref())
        .expect("lib adapter always available in lib core build")
}

use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "lib",
            mgc_lib_adapter::detect_language(&root).map(|l| l.ecosystem()),
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = lib_adapter();
    shared::list(&*adapter, &root).await
}
