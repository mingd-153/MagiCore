//! `mgc install` clo — tách từ core/clo.rs (Phase 7 v5).

use anyhow::Result;

use super::super::dev::clo as clo_tools;
use super::super::shared;
use mgc_types::Ecosystem;

pub async fn install(packages: Vec<String>, dry_run: bool) -> Result<()> {
    let root = shared::core_project_root("clo")?;
    let adapter = shared::core_adapter(&Ecosystem::Cloud);
    if dry_run {
        // terraform passthrough — in lệnh init/get KHÔNG chạy (spec §5 criterion)
        if !packages.is_empty() {
            mgc_ui::info(&format!(
                "[dry-run] ignoring package args {:?} — terraform install = init/get",
                packages
            ));
        }
        let kind = clo_tools::cloud_type(&root)?;
        if kind != "terraform" {
            mgc_ui::info(&format!(
                "[dry-run] would run npm-registry install via mgc-resolver for {kind}"
            ));
            return Ok(());
        }
        mgc_ui::info("[dry-run] would run: terraform init");
        mgc_ui::info("[dry-run] would run: terraform get");
        return Ok(());
    }
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
