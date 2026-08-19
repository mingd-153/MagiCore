//! `mg install` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mg_types::Ecosystem;

const OPTIMIZER_PKG: &str = "optimizer";

pub async fn install(packages: Vec<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);

    // optimizer: materialize + hook dep; không gửi qua adapter (không phải registry crate)
    let mut adapter_pkgs = Vec::new();
    for pkg in &packages {
        if pkg == OPTIMIZER_PKG {
            shared::game_optimizer_template(&root).await?;
        } else {
            adapter_pkgs.push(pkg.clone());
        }
    }

    for pkg in &adapter_pkgs {
        let spinner = mg_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mg_types::PackageName::new(pkg)?;
        let opts = mg_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    shared::install_with_adapter(
        &*adapter,
        &root,
        "mg add",
        false,
        mg_types::adapter::InstallOptions {
            legacy_flat: false,
            ..Default::default()
        },
    )
    .await
}