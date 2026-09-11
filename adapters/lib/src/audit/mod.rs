//! `audit/mod.rs` — Security audit for lib adapter.
//! Scans Rust/Python dependencies for known vulnerabilities; when BOTH
//! manifests exist the engine aggregates instead of first-match
//! (Tech Lead 2026-09-09 §5 multi-language lib parity).

pub mod scanner;

pub use mgc_audit::scanners::{audit_go, audit_python, audit_rust};
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
        LibLanguage::Go => {
            // Go lane (P1 matrix row "Lib Go govulncheck"): shared scanner
            // through the engine; a go.mod + Cargo.toml polyglot
            // aggregates BOTH.
            // Lane Go (P1 matrix "Lib Go govulncheck"): scanner chung qua
            // engine; polyglot go.mod + Cargo.toml gộp CẢ HAI.
            if project_root.join("Cargo.toml").is_file() {
                aggregate_go_rust(project_root).await
            } else {
                scanner::audit_go(project_root).await
            }
        }
        // Java/Kotlin + .NET lanes (P2 2026-09-10 matrix rows "Lib
        // Java/Kotlin Gradle/Maven Dependency Check" + "Lib .NET dotnet
        // list package --vulnerable"): OSV-backed shared scanners —
        // verification-metadata.xml / packages.lock.json pins, no heavy
        // CLI needed. Polyglot roots (Cargo.toml kề) aggregate BOTH.
        // Lane Java/Kotlin + .NET: scanner chung chạy OSV — ghim từ
        // lockfile, không cần CLI nặng. Root polyglot (có Cargo.toml
        // kề) gộp CẢ HAI.
        LibLanguage::Java => {
            if project_root.join("Cargo.toml").is_file() {
                aggregate_java_rust(project_root).await
            } else {
                mgc_audit::scanners::audit_java(project_root).await
            }
        }
        LibLanguage::DotNet => {
            if project_root.join("Cargo.toml").is_file() {
                aggregate_dotnet_rust(project_root).await
            } else {
                mgc_audit::scanners::audit_dotnet(project_root).await
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
    let root_py = project_root.to_path_buf();
    let root_rs = project_root.to_path_buf();
    let mut plan = mgc_audit::AuditPlan::new();

    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "python",
        scanner: "pip-audit",
        run: Box::new(move || {
            let root = root_py.clone();
            Box::pin(async move { scanner::audit_python(&root).await })
        }),
    });
    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "rust",
        scanner: "cargo-audit",
        run: Box::new(move || {
            let root = root_rs.clone();
            Box::pin(async move { scanner::audit_rust(&root).await })
        }),
    });

    plan.execute().await
}

/// Aggregate go + rust scans through the shared engine (polyglot lib
/// with go.mod AND Cargo.toml — never first-match).
/// Tổng hợp scan go + rust qua engine chung (lib đa ngôn ngữ có go.mod
/// VÀ Cargo.toml — không first-match).
async fn aggregate_go_rust(project_root: &Path) -> MgResult<AuditReport> {
    let root_go = project_root.to_path_buf();
    let root_rs = project_root.to_path_buf();
    let mut plan = mgc_audit::AuditPlan::new();

    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "go",
        scanner: "govulncheck",
        run: Box::new(move || {
            let root = root_go.clone();
            Box::pin(async move { scanner::audit_go(&root).await })
        }),
    });
    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "rust",
        scanner: "cargo-audit",
        run: Box::new(move || {
            let root = root_rs.clone();
            Box::pin(async move { scanner::audit_rust(&root).await })
        }),
    });

    plan.execute().await
}

/// Aggregate java + rust scans (polyglot lib with gradle verification
/// metadata AND Cargo.toml).
/// Tổng hợp scan java + rust (lib đa ngôn ngữ có metadata verification
/// gradle VÀ Cargo.toml).
async fn aggregate_java_rust(project_root: &Path) -> MgResult<AuditReport> {
    let root_java = project_root.to_path_buf();
    let root_rs = project_root.to_path_buf();
    let mut plan = mgc_audit::AuditPlan::new();

    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "java",
        scanner: "osv-maven",
        run: Box::new(move || {
            let root = root_java.clone();
            Box::pin(async move { mgc_audit::scanners::audit_java(&root).await })
        }),
    });
    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "rust",
        scanner: "cargo-audit",
        run: Box::new(move || {
            let root = root_rs.clone();
            Box::pin(async move { scanner::audit_rust(&root).await })
        }),
    });

    plan.execute().await
}

/// Aggregate dotnet + rust scans (polyglot lib with packages.lock.json
/// AND Cargo.toml).
/// Tổng hợp scan dotnet + rust (lib đa ngôn ngữ có packages.lock.json
/// VÀ Cargo.toml).
async fn aggregate_dotnet_rust(project_root: &Path) -> MgResult<AuditReport> {
    let root_net = project_root.to_path_buf();
    let root_rs = project_root.to_path_buf();
    let mut plan = mgc_audit::AuditPlan::new();

    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "dotnet",
        scanner: "osv-nuget",
        run: Box::new(move || {
            let root = root_net.clone();
            Box::pin(async move { mgc_audit::scanners::audit_dotnet(&root).await })
        }),
    });
    plan.add_step(mgc_audit::ScanStep {
        ecosystem: "rust",
        scanner: "cargo-audit",
        run: Box::new(move || {
            let root = root_rs.clone();
            Box::pin(async move { scanner::audit_rust(&root).await })
        }),
    });

    plan.execute().await
}
