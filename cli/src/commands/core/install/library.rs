//! `mgc install library` — tách từ core/library.rs (Phase 7 v5).

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

pub async fn install(
    packages: Vec<String>,
    compat_runtime: Option<String>,
    frozen: bool,
) -> Result<()> {
    let root = project_root()?;
    // P0 install/add split: `install-lib` NEVER adds packages — it replays
    // the existing graph/lock through the native pipeline. Package args
    // used to flow into adapter.add() (cargo/pip/go spawn) UNDER the
    // native Install gate: a direct bypass. Fail closed with the exact
    // command instead.
    // (P0: install-lib KHÔNG BAO GIỜ add package — chỉ cài graph/lock
    // hiện có. Package args phải đi `add-lib`.)
    if !packages.is_empty() {
        return Err(crate::error::install_lib_packages_use_add(&packages));
    }
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): TypeScript rides the native web engine;
    // protocol languages run the native resolve/fetch/CAS pipeline
    // (adapters/lib/src/install spawns NO toolchain — verified by scan).
    // (Tường lửa C0: pipeline native, không spawn toolchain.)
    let language = mgc_lib_adapter::detect_language(&root).map(|lang| lang.ecosystem());
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "lib",
            language,
            None,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = lib_adapter();
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        frozen,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}
