//! `mgc add library` — tách từ core/library.rs (Phase 7 v5).

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
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): TypeScript rides the native web engine;
    // every other lib language delegates to its toolchain (compat only).
    // (Tường lửa C0: TypeScript đi engine web native; ngôn ngữ lib khác
    // delegate toolchain.)
    let detected = mgc_lib_adapter::detect_language(&root);
    let language = detected.map(|lang| lang.ecosystem());
    // Actual tool the adapter WILL spawn (never None on a spawning
    // lane): the gate must see `pip` for python — a `--compat-runtime
    // uv` opt-in must NOT open a pip spawn (flag==process contract).
    let tool = detected.and_then(shared::lib_edit_tool);
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "lib",
            language,
            None,
            None,
            crate::commands::dep_gate::DepOp::Add,
        ),
        tool,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = lib_adapter();
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
