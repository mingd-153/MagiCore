//! `mgc update` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

pub async fn update(
    packages: Vec<String>,
    install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context: detected engine id (non-bevy hits Unsupported).
    let engine = mgc_game_adapter::adapter_for(&root).map(|a| a.engine());
    // Native lane (mgc.lock, no Cargo.lock): bevy updates resolve through
    // the NATIVE crates engine (resolve-latest + mgc-side edit) — zero
    // `cargo` spawn. Other engines keep the legacy delegated path below.
    // (Lane native: bevy + Cargo.toml → engine crates native.)
    if engine == Some("bevy") && root.join("Cargo.toml").is_file() {
        let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
        crate::commands::dep_gate::gate(
            &crate::commands::dep_gate::DepContext::new(
                "game",
                Some("bevy"),
                Some("bevy"),
                None,
                crate::commands::dep_gate::DepOp::Update,
            ),
            None,
            &compat,
            Some(&root.join(".magicore").join("exec.log")),
        )?;
        let lib_adapter = crate::factory::create_adapter(&mgc_types::Ecosystem::Lib, None, None)
            .map_err(|e| anyhow::anyhow!("game native update needs the lib Cargo engine: {e}"))?;
        // Mutation gateway: direct native_update callers hold the writer
        // lock themselves (shared::update is bypassed here by design).
        // (Gọi native trực tiếp thì tự giữ lock writer.)
        let write_lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
            &root,
            crate::commands::core::shared::writer_lock_timeout(&root),
        )
        .map_err(|e| anyhow::anyhow!("update cannot acquire the project writer lock: {e}"))?;
        return crate::commands::core::shared::native_update(
            &*lib_adapter,
            &root,
            packages,
            install,
            &write_lock,
        )
        .await;
    }
    // C0 ownership firewall (T0.3): the game update lane routes to the
    // adapter, whose engines delegate (Bevy → cargo).
    // (Tường lửa C0: lane update game gọi adapter, engine trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Update,
        ),
        // Exact tool the bevy lane spawns (never None on a spawning lane).
        Some("cargo"),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = super::super::shared::core_adapter(&Ecosystem::Game);
    shared::update(&*adapter, &root, packages, install).await
}
