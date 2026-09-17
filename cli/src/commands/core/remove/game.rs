//! `mgc remove` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context: detected engine id (non-bevy hits Unsupported).
    let engine = mgc_game_adapter::adapter_for(&root).map(|a| a.engine());
    // C0 ownership firewall (T0.3): the game remove lane routes to the
    // adapter, whose engines delegate (Bevy → cargo).
    // (Tường lửa C0: lane remove game gọi adapter, engine trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Remove,
        ),
        // Exact tool the bevy lane spawns (never None on a spawning lane).
        Some("cargo"),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::remove(&*adapter, &root, packages, true).await
}
