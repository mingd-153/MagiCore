//! `mgc install iot` — tách từ core/iot.rs (Phase 7 v5).

use anyhow::Result;
use mgc_types::adapter::PackageAdapter;
use mgc_types::Ecosystem;
use std::path::PathBuf;
use std::sync::Arc;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("iot"))?;
    Ok(root)
}

fn iot_adapter() -> Arc<dyn PackageAdapter> {
    crate::factory::create_adapter(&Ecosystem::Iot, None, None)
        .expect("iot adapter always available in iot core build")
}

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = project_root()?;
    let adapter = iot_adapter();
    for pkg in &packages {
        let spinner = mgc_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mgc_types::PackageName::new(pkg)?;
        let opts = mgc_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mgc add",
        false,
        mgc_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}
