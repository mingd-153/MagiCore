//! `mgc install` game — tách từ core/game.rs (Phase 7 v5).

use anyhow::Result;

use super::super::shared;
use mgc_types::Ecosystem;

const OPTIMIZER_PKG: &str = "optimizer";

pub async fn install(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = super::super::shared::core_project_root("game")?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // Full gate context: the DETECTED engine id (bevy/godot/unity/unreal)
    // — godot/unity/unreal hit Unsupported at the gate (no package
    // manager), never a post-gate adapter error.
    // (Context gate đầy đủ: engine id detect được.)
    let engine = mgc_game_adapter::adapter_for(&root).map(|a| a.engine());
    // C0 ownership firewall (T0.3): the game install lane routes to the
    // adapter, whose engines delegate (Bevy → cargo).
    // (Tường lửa C0: lane install game gọi adapter, engine trong đó
    // delegate.)
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "game",
            engine,
            engine,
            None,
            crate::commands::dep_gate::DepOp::Install,
        ),
        // Exact tool the bevy lane spawns (never None on a spawning lane).
        Some("cargo"),
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
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
        let spinner = mgc_ui::create_spinner(&format!("  Adding {}...", pkg));
        let name = mgc_types::PackageName::new(pkg)?;
        let opts = mgc_types::adapter::AddOptions::default();
        adapter.add(&root, &name, None, opts).await?;
        spinner.finish_and_clear();
    }
    // Delegated install (Bevy → `cargo fetch`): the game adapter has no
    // mgc-native resolve — routing through install_with_adapter would die
    // in prepare_install_execution with "does not support 'resolve'".
    // Install straight into the adapter with an empty graph (the bevy
    // engine ignores the graph; `cargo fetch` owns the lifecycle) and
    // print the same honest summary footer.
    // (Install ủy thác: gọi adapter trực tiếp với graph rỗng — engine
    // bevy bỏ qua graph, `cargo fetch` sở hữu lifecycle.)
    let started_at = std::time::Instant::now();
    let mut summary = adapter
        .install(
            &mgc_types::adapter::ResolvedGraph::empty(),
            &root,
            mgc_types::adapter::InstallOptions {
                legacy_flat: false,
                ..Default::default()
            },
        )
        .await?;
    summary.duration_ms = started_at.elapsed().as_millis() as u64;
    let cache_source = match summary.cache_mode {
        mgc_types::adapter::InstallCacheMode::MgCStore => "shared mgc store",
        mgc_types::adapter::InstallCacheMode::Delegated => "native toolchain cache",
    };
    mgc_ui::print_install_summary_source(
        summary.added.len(),
        summary.bytes_from_cache as usize,
        summary.duration_ms,
        "0 B",
        Some(cache_source),
    );
    mgc_ui::blank_line();
    mgc_ui::success("All dependencies installed");
    Ok(())
}
