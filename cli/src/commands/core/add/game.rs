//! `mg add` game — tách từ core/game.rs (Phase 7 v5). Optimizer là local template,
//! không gửi qua adapter (bevy `cargo add optimizer` fail vì crate không tồn tại).

use anyhow::Result;

use super::super::shared;
use mg_types::Ecosystem;

const OPTIMIZER_PKG: &str = "optimizer";

fn game_split(packages: &[String]) -> (Vec<String>, bool) {
    let mut adapter_pkgs = Vec::new();
    let mut has_optimizer = false;
    for pkg in packages {
        if pkg == OPTIMIZER_PKG {
            has_optimizer = true;
        } else {
            adapter_pkgs.push(pkg.clone());
        }
    }
    (adapter_pkgs, has_optimizer)
}

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
) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);

    // optimizer/bench: không phải crate registry — materialize template + hook dep
    let (adapter_pkgs, has_optimizer) = game_split(&packages);
    if has_optimizer {
        shared::game_optimizer_template(&root).await?;
    }
    if adapter_pkgs.is_empty() {
        return Ok(());
    }

    shared::add(
        &*adapter,
        &root,
        adapter_pkgs,
        version,
        dev,
        exact,
        optional,
        peer,
        no_save,
        true,
        global,
    )
    .await
}

#[cfg(test)]
#[path = "test/game.rs"]
mod tests;