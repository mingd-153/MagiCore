//! `mgc remove` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // C0 ownership firewall (T0.3): the game remove lane routes to the
    // adapter, whose engines delegate (Bevy → cargo).
    // (Tường lửa C0: lane remove game gọi adapter, engine trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        "game",
        crate::commands::dep_gate::DepOp::Remove,
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::remove(&*adapter, &root, packages, true).await
}
