//! `mg add-hardware <pkg>` — materialize optimizer/bench vào project. Phase 7 v5.

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mg_project_found(""))?;
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
        let spinner = mg_ui::create_spinner(&format!("  Materializing {pkg}..."));
        shared::materialize_template(&root, pkg).await?;
        spinner.finish_and_clear();
        mg_ui::success(&format!("{pkg} scaffolded at ./{pkg}"));
    }
    shared::install_with_adapter(
        &*crate::factory::create_adapter(&mg_types::Ecosystem::Hardware, None, None)
            .expect("hardware adapter always available in hardware core build"),
        &root,
        "mg add-hardware",
        false,
        mg_types::adapter::InstallOptions::default(),
    )
    .await
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
