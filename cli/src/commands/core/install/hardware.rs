//! `mgc install-hardware` — materialize optimizer/bench vào project. Phase 7 v5.

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("generic"))?;
    Ok(root)
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    for pkg in &packages {
        if !matches!(pkg.as_str(), OPTIMIZER_PKG | BENCH_PKG) {
            return Err(crate::error::unknown_hardware_package(pkg));
        }
        shared::materialize_template(&root, pkg).await?;
    }
    shared::install_with_adapter(
        &*crate::factory::create_adapter(&mgc_types::Ecosystem::Hardware, None, None)
            .expect("hardware adapter always available in hardware core build"),
        &root,
        "mgc install-hardware",
        false,
        mgc_types::adapter::InstallOptions::default(),
    )
    .await
}
