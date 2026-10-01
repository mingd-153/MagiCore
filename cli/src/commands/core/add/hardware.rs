//! `mgc add-hardware <pkg>` — materialize optimizer/bench vào project. Phase 7 v5.
//!
//! GATE-EXEMPT: template materialization is not a package lifecycle (no
//! registry, no toolchain spawn in this lane).
//! (GATE-EXEMPT: materialize template không phải package lifecycle.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root =
        shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mgc_project_found(""))?;
    Ok(root)
}

fn optimizer_owner(root: &std::path::Path) -> Result<String> {
    mgc_config::project::ProjectConfig::detect_core(root)?
        .ok_or_else(|| crate::error::project_core_identity_missing(root, "hardware optimizer"))
}

fn hardware_kind(pkg: &str) -> Result<()> {
    match pkg {
        OPTIMIZER_PKG | BENCH_PKG => Ok(()),
        other => Err(crate::error::unknown_hardware_package(other)),
    }
}

pub async fn add(
    packages: Vec<String>,
    _compat_runtime: Option<String>,
    version: Option<String>,
) -> Result<()> {
    // Templates have no versions — a pinned version here is a user error,
    // failed loudly instead of silently ignored.
    // (Template không có version — version ghim ở đây là lỗi user, fail
    // rõ thay vì bỏ qua âm thầm.)
    if let Some(pinned) = version.as_deref() {
        return Err(crate::error::add_version_unsupported("hardware", pinned));
    }
    let root = project_root()?;
    for pkg in &packages {
        hardware_kind(pkg)?;
        if pkg == OPTIMIZER_PKG {
            // Resolve the full project identity; a missing marker must not
            // silently route another ecosystem through the Web profile.
            // (Đọc identity đầy đủ; thiếu marker không được âm thầm chọn Web.)
            let core = optimizer_owner(&root)?;
            crate::commands::optimizer::optimize_project(&root, &core, false)?;
        } else {
            let spinner = mgc_ui::create_spinner(&format!("  Materializing {pkg}..."));
            shared::materialize_template(&root, pkg).await?;
            spinner.finish_and_clear();
            mgc_ui::success(&format!("{pkg} scaffolded at ./{pkg}"));
        }
    }
    // There is deliberately NO trailing adapter install call: hardware
    // has no dependency lifecycle (resolve/install are Unsupported by
    // design), so materialization above IS the complete operation — the
    // old tail failed AFTER materializing (side effects plus an error).
    // (Cố ý KHÔNG gọi adapter install ở cuối — materialize ĐÃ là toàn bộ.)
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
