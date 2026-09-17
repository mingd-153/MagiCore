//! `mgc list-hardware` — báo optimizer/bench hiện có qua adapter. Phase 7 v5.
//!
//! GATED NATIVE READ (P0#3): adapter directory read — no toolchain spawn,
//! no package mutation in this lane. The gate records the native decision.
//! (Đọc thư mục qua adapter, có gate — lane này không spawn toolchain,
//! không đổi package.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("generic"))?;
    Ok(root)
}

pub async fn list(compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    let compat = crate::commands::dep_gate::from_dep_flag(compat_runtime.as_deref())?;
    crate::commands::dep_gate::gate(
        &crate::commands::dep_gate::DepContext::new(
            "hardware",
            None,
            None,
            None,
            crate::commands::dep_gate::DepOp::List,
        ),
        None,
        &compat,
        Some(&root.join(".magicore").join("exec.log")),
    )?;
    shared::list(
        &*crate::factory::create_adapter(&mgc_types::Ecosystem::Hardware, None, None)
            .expect("hardware adapter always available in hardware core build"),
        &root,
    )
    .await
}
