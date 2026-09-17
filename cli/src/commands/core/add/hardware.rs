//! `mgc add-hardware <pkg>` — materialize optimizer/bench vào project. Phase 7 v5.
//!
//! GATE-EXEMPT: template materialization is not a package lifecycle (no
//! registry, no toolchain spawn in this lane).
//! (GATE-EXEMPT: materialize template không phải package lifecycle.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root =
        shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mgc_project_found(""))?;
    Ok(root)
}

fn hardware_kind(pkg: &str) -> Result<()> {
    match pkg {
        OPTIMIZER_PKG | BENCH_PKG => Ok(()),
        other => Err(crate::error::unknown_hardware_package(other)),
    }
}

pub async fn add(
    packages: Vec<String>,
    _compat_runtime: Option<String>,
    version: Option<String>,
) -> Result<()> {
    // Templates have no versions — a pinned version here is a user error,
    // failed loudly instead of silently ignored.
    // (Template không có version — version ghim ở đây là lỗi user, fail
    // rõ thay vì bỏ qua âm thầm.)
    if let Some(pinned) = version.as_deref() {
        return Err(crate::error::add_version_unsupported("hardware", pinned));
    }
    let root = project_root()?;
    for pkg in &packages {
        hardware_kind(pkg)?;
        if pkg == OPTIMIZER_PKG {
            // Xác định core hiện tại của project để optimize đúng profile
            let core = mgc_config::project::ProjectConfig::read_core_marker(&root)?
                .unwrap_or_else(|| "web".to_string());
            crate::commands::optimizer::optimize_project(&root, &core, false)?;
        } else {
            let spinner = mgc_ui::create_spinner(&format!("  Materializing {pkg}..."));
            shared::materialize_template(&root, pkg).await?;
            spinner.finish_and_clear();
            mgc_ui::success(&format!("{pkg} scaffolded at ./{pkg}"));
        }
    }
    let has_materialized_pkg = packages.iter().any(|pkg| pkg != OPTIMIZER_PKG);
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if has_materialized_pkg
        && let Ok(adapter) =
            crate::factory::create_adapter(&mgc_types::Ecosystem::Hardware, None, None)
    {
        shared::install_with_adapter(
            &*adapter,
            &root,
            "mgc add-hardware",
            false,
            mgc_types::adapter::InstallOptions::default(),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
