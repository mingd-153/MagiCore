//! `mgc add library` — tách từ core/library.rs (Phase 7 v5).

use anyhow::Result;
fn project_root() -> Result<PathBuf> {
    shared::core_project_root("lib")
}

use std::path::PathBuf;

use crate::commands::core::shared;

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
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    // The ownership gate permits only a completed native dependency lane.
    // Tường lửa chỉ cho phép lane dependency native đã hoàn chỉnh.
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::lib_project_context(
            &root,
            crate::commands::dep_gate::DepOp::Add,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = shared::lib_adapter(&root)?;
    shared::add(
        &*adapter, &root, packages, version, dev, exact, optional, peer, no_save, true, global,
    )
    .await
}
