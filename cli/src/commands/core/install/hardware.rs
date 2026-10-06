//! `mgc install-hardware` — materialize optimizer/bench vào project. Phase 7 v5.
//!
//! GATE-EXEMPT: template materialization is not a package lifecycle (no
//! registry, no toolchain spawn in this lane, no adapter install call).
//! (GATE-EXEMPT: materialize template không phải package lifecycle.)

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::commands::core::shared::{self, BENCH_PKG, OPTIMIZER_PKG};

fn project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| crate::error::cwd_deleted(&e))?;
    let root = shared::find_project_root(&cwd)?
        .ok_or_else(|| crate::error::no_mgc_project_found("generic"))?;
    Ok(root)
}

pub async fn install(packages: Vec<String>, compat_runtime: Option<String>) -> Result<()> {
    let root = project_root()?;
    install_at(&root, packages, compat_runtime).await
}

/// Template-only install anchored at an explicit root (no cwd dependence —
/// unit-testable).
/// Install template với root tường minh (không phụ thuộc cwd — test được).
///
/// There is deliberately NO trailing adapter install call: hardware has
/// no dependency lifecycle (resolve/install are Unsupported by design),
/// so materialization above IS the complete operation. The old tail
/// (`install_with_adapter`) failed AFTER materializing — side effects
/// plus an error.
/// (Cố ý KHÔNG gọi adapter install ở cuối: hardware không có lifecycle
/// dependency, materialize ở trên ĐÃ là toàn bộ operation. Tail cũ fail
/// SAU khi materialize — vừa có side effect vừa báo lỗi.)
pub async fn install_at(
    root: &Path,
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
            "`install-hardware` is template-only and ignores `--compat-runtime {tool}` (no package lifecycle to delegate)."
        ));
    }
    for pkg in &packages {
        shared::materialize_template(root, pkg).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/hardware.rs"]
mod tests;
