//! `mgc remove-hardware <pkg>` — remove materialized optimizer/bench dirs.
//!
//! GATE-EXEMPT: template filesystem removal is not a package lifecycle (no
//! registry, no toolchain spawn in this lane, no adapter install call).
//! (Gỡ thư mục template — không phải package lifecycle.)

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root =
        shared::find_project_root(&cwd)?.ok_or_else(|| crate::error::no_mgc_project_found(""))?;
    Ok(root)
}

pub async fn remove(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    remove_at(&project_root()?, packages, compat_runtime).await
}

/// Template-only remove anchored at an explicit root (no cwd dependence —
/// unit-testable).
/// Gỡ template với root tường minh (không phụ thuộc cwd — test được).
pub async fn remove_at(
    root: &std::path::Path,
    packages: Vec<String>,
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
    // An explicit compat flag on a template-only lane is meaningless —
    // warn instead of silently ignoring it.
    // (Cờ compat trên lane template-only là vô nghĩa — cảnh báo thay vì
    // bỏ qua âm thầm.)
    if let Some(tool) = compat_runtime.as_deref() {
        mgc_ui::warning(&format!(
            "`remove-hardware` is template-only and ignores `--compat-runtime {tool}` (no package lifecycle to delegate)."
        ));
    }
    for pkg in &packages {
        let dir = root.join(pkg);
        if !dir.is_dir() {
            return Err(crate::error::hardware_template_not_materialized(pkg, root));
        }
        std::fs::remove_dir_all(&dir)
            .map_err(|e| crate::error::hardware_template_remove_failed(pkg, &e))?;
        mgc_ui::success(&format!("{pkg} removed from ./{}", pkg));
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
