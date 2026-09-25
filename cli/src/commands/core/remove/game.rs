//! `mgc remove` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context: detected engine id (non-bevy hits Unsupported).
    let engine = mgc_game_adapter::adapter_for(&root).map(|a| a.engine());
    // Native lane (mgc.lock, no Cargo.lock): bevy removals edit Cargo.toml
    // mgc-side with zero `cargo` spawn. Other engines keep the legacy
    // delegated path below.
    // (Lane native: bevy + Cargo.toml → edit mgc-side.)
    if engine == Some("bevy") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "game",
                Some("bevy"),
                Some("bevy"),
                None,
                crate::commands::dep_gate::DepOp::Remove,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("game native remove needs the lib Cargo engine: {e}"))?;
        return shared::remove(&*lib_adapter, &root, packages, true).await;
    }
    if engine == Some("bevy") {
        anyhow::bail!("native Bevy dependency removal requires Cargo.toml at the project root");
    }
    // Non-native engines are rejected by the ownership gate.
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Remove,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let _ = packages;
    anyhow::bail!("MagiCore does not yet own dependency removal for this game engine")
}
