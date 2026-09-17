//! `mgc list` game — tách từ core/game.rs (Phase 7 v5).
//!
//! GATED NATIVE READ (P0#3): adapter manifest read — no toolchain spawn,
//! no package mutation in this lane. The gate records the native decision
//! (and logs the ignore notice when --compat-runtime is passed).
//! (Đọc manifest qua adapter, có gate — lane này không spawn toolchain,
//! không đổi package.)

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    // C0 gate (P0#3): adapter manifest read — spawn-free, so native; the
    // explicit flag is accepted for CLI uniformity (gate logs the ignore
    // notice when passed).
    // (Gate C0: đọc manifest qua adapter — native; chấp nhận cờ để đồng
    // nhất CLI.)
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            Some(crate::commands::dep_gate::eco::BEVY),
            Some(crate::commands::dep_gate::eco::BEVY),
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::list(&*adapter, &root).await
}
