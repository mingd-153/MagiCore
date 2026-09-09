//! `audit/mod.rs` — Security audit for lib adapter.
//! Scans Rust/Python dependencies for known vulnerabilities; when BOTH
//! manifests exist the engine aggregates instead of first-match
//! (Tech Lead 2026-09-09 §5 multi-language lib parity).

pub mod scanner;

pub use mgc_audit::scanners::{audit_python, audit_rust};

use mgc_types::adapter::AuditReport;
use mgc_types::{MgError, MgResult};
use std::path::Path;

use crate::language::LibLanguage;

/// Run security audit for lib project.
/// Chạy security audit cho lib project.
pub(crate) async fn run_audit(language: LibLanguage, project_root: &Path) -> MgResult<AuditReport> {
    match language {
        LibLanguage::Ts => {
            // TypeScript: delegate to web adapter audit
            // TypeScript: ủy quyền cho web adapter audit
            Err(MgError::Other(
                "TypeScript audit should be delegated to web adapter".to_string(),
            ))
        }
        LibLanguage::Rust => scanner::audit_rust(project_root).await,
        LibLanguage::Python => {
            // A polyglot lib (pyproject + Cargo) must scan BOTH sets —
            // aggregate, never first-match.
            // Lib đa ngôn ngữ (pyproject + Cargo) phải quét CẢ HAI tập —
            // tổng hợp, không first-match.
            if project_root.join("Cargo.toml").is_file()
                && (project_root.join("requirements.txt").is_file()
                    || project_root.join("pylock.toml").is_file()
                    || pylock_variant_exists(project_root)
                    || project_root.join("uv.lock").is_file()
                    || project_root.join("pyproject.toml").is_file())
            {
                aggregate_python_rust(project_root).await
            } else {
                scanner::audit_python(project_root).await
            }
        }
    }
}

/// True when any PEP 751 pylock.*.toml variant exists (mirrors the
/// scanner's discovery rule so the aggregate path triggers identically).
/// Có biến thể pylock.*.toml PEP 751 nào tồn tại (khớp luật discovery
/// của scanner để đường aggregate kích hoạt y hệt).
fn pylock_variant_exists(project_root: &Path) -> bool {
    std::fs::read_dir(project_root)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("pylock.") && n.ends_with(".toml"))
            })
        })
        .unwrap_or(false)
}

/// Aggregate python + rust scans through the shared engine.
/// Tổng hợp scan python + rust qua engine chung.
async fn aggregate_python_rust(project_root: &Path) -> MgResult<AuditReport> {
    use futures_util::future::FutureExt;

    let root_py = project_root.to_path_buf();
    let root_rs = project_root.to_path_buf();
    let mut plan = mgc_audit::AuditPlan::new();

    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "python",
        scanner: "pip-audit",
        run: Box::new(
            move || match Box::pin(scanner::audit_python(&root_py)).now_or_never() {
                Some(result) => result,
                None => Err(MgError::Other(
                    "python scanner requires async execution".to_string(),
                )),
            },
        ),
    });
    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "rust",
        scanner: "cargo-audit",
        run: Box::new(
            move || match Box::pin(scanner::audit_rust(&root_rs)).now_or_never() {
                Some(result) => result,
                None => Err(MgError::Other(
                    "rust scanner requires async execution".to_string(),
                )),
            },
        ),
    });

    plan.execute().await
}
