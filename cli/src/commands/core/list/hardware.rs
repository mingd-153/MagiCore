//! `mgc list-hardware` — báo optimizer/bench hiện có qua adapter. Phase 7 v5.
//!
//! GATE-EXEMPT: adapter directory read — no toolchain spawn, no package
//! mutation in this lane.
//! (GATE-EXEMPT: đọc thư mục qua adapter — lane này không spawn
//! toolchain, không đổi package.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared;

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("generic"))?;
    Ok(root)
}

pub async fn list() -> Result<()> {
    let root = project_root()?;
    shared::list(
        &*crate::factory::create_adapter(&mgc_types::Ecosystem::Hardware, None, None)
            .expect("hardware adapter always available in hardware core build"),
        &root,
    )
    .await
}
