//! `mgc add` game — tách từ core/game.rs (Phase 7 v5). Optimizer là local template,
//! không gửi qua adapter (bevy `cargo add optimizer` fail vì crate không tồn tại).

use anyhow::Result;

use super::super::shared;

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
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let engine = mgc_game_adapter::adapter_for(&root).map(|a| a.engine());

    // optimizer/bench: không phải crate registry — materialize template + hook dep
    let (adapter_pkgs, has_optimizer) = game_split(&packages);
    if adapter_pkgs.is_empty() {
        if has_optimizer {
            shared::game_optimizer_template(&root).await?;
        }
        return Ok(());
    }

    // Native lane: a Bevy project with Cargo.toml uses the Lib/Rust native
    // engine. Other game engines fail closed; no adapter passthrough.
    // (Lane native: bevy + Cargo.toml → engine crates native.)
    if engine == Some("bevy") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "game",
                Some("bevy"),
                Some("bevy"),
                None,
                crate::commands::dep_gate::DepOp::Add,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("game native add needs the lib Cargo engine: {e}"))?;
        shared::add(
            &*lib_adapter,
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
        .await?;
        if has_optimizer {
            shared::game_optimizer_template(&root).await?;
        }
        return Ok(());
    }
    if engine == Some("bevy") {
        anyhow::bail!("native Bevy dependency operations require Cargo.toml at the project root");
    }

    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context: detected engine id (non-Bevy hits Unsupported).
    // Gate rejects non-native engines before optimizer/template mutation.
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Add,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let _ = (
        adapter_pkgs,
        version,
        dev,
        exact,
        optional,
        peer,
        no_save,
        global,
    );
    anyhow::bail!("MagiCore does not yet own dependency addition for this game engine")
}

#[cfg(test)]
#[path = "test/game.rs"]
mod tests;
