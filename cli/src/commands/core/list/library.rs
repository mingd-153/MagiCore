//! `mgc list library` — tách từ core/library.rs (Phase 7 v5).
//!
//! GATED NATIVE READ (P0#3): adapter manifest/lock read — no toolchain
//! spawn, no package mutation in this lane. The gate records the native
//! decision (and logs the ignore notice when --compat-runtime is passed).
//! (Đọc manifest/lock qua adapter, có gate — lane này không spawn
//! toolchain, không đổi package.)

use anyhow::Result;
fn project_root() -> Result<PathBuf> {
    shared::core_project_root("lib")
}

use std::path::PathBuf;

use crate::commands::core::shared;

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::lib_project_context(
            &root,
            crate::commands::dep_gate::DepOp::List,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    let adapter = shared::lib_adapter(&root)?;
    shared::list(&*adapter, &root).await
}
