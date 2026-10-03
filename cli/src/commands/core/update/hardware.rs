//! `mgc update-hardware <pkg>` — refresh materialized optimizer/bench dirs.
//!
//! GATE-EXEMPT: template refresh is not a package lifecycle (no registry,
//! no toolchain spawn in this lane, no adapter install call). Refresh is
//! remove + re-materialize because `materialize_template` never overwrites.
//! (Làm mới template — gỡ rồi materialize lại vì materialize không ghi đè.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root =
        shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mgc_project_found(""))?;
    Ok(root)
}

pub async fn update(
    packages: Vec<String>,
    install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    update_at(&project_root()?, packages, install, compat_runtime).await
}

/// Template-only refresh anchored at an explicit root (no cwd dependence
/// — unit-testable).
/// Làm mới template với root tường minh (không phụ thuộc cwd — test được).
pub async fn update_at(
    root: &std::path::Path,
    packages: Vec<String>,
    install: bool,
    compat_runtime: Option<String>,
) -> Result<()> {
    // Validate package names before touching the filesystem: unknown
    // input fails fast without requiring anything else.
    // (Kiểm tra tên package trước khi đụng filesystem.)
    for pkg in &packages {
        if !matches!(pkg.as_str(), OPTIMIZER_PKG | BENCH_PKG) {
            return Err(crate::error::unknown_hardware_package(pkg));
        }
    }
    // Both flags are meaningless on a template-only lane — warn instead
    // of silently ignoring them.
    // (Cả 2 cờ vô nghĩa trên lane template-only — cảnh báo thay vì bỏ qua.)
    if let Some(tool) = compat_runtime.as_deref() {
        mgc_ui::warning(&format!(
            "`update-hardware` is template-only and ignores `--compat-runtime {tool}` (no package lifecycle to delegate)."
        ));
    }
    if install {
        mgc_ui::warning(
            "`update-hardware` is template-only and ignores `--install` (materialization already lands in place; there is no install phase).",
        );
    }
    for pkg in &packages {
        let dir = root.join(pkg);
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| crate::error::hardware_template_remove_failed(pkg, &e))?;
        }
        shared::materialize_template(root, pkg).await?;
        mgc_ui::success(&format!("{pkg} refreshed at ./{pkg}"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
