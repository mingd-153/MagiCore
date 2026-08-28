//! `audit/mod.rs` — Security audit for lib adapter.
//! Scans Rust/Python dependencies for known vulnerabilities.

pub mod scanner;

use mgc_types::adapter::AuditReport;
use mgc_types::{MgError, MgResult};
use std::path::Path;

use crate::language::LibLanguage;

/// Run security audit for lib project.
/// Chạy security audit cho lib project.
#[allow(dead_code)] // P2: adapter.rs sẽ nối vào PackageAdapter::audit — wired in P2
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
        LibLanguage::Python => scanner::audit_python(project_root).await,
    }
}
