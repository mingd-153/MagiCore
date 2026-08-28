//! `mgc add-hardware <pkg>` — materialize optimizer/bench vào project. Phase 7 v5.

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

pub async fn add(packages: Vec<String>) -> Result<()> {
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
    if has_materialized_pkg {
        if let Ok(adapter) =
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
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
