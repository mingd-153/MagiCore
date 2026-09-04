use anyhow::{bail, Result};
use colored::*;
use mgc_types::adapter::AuditReport;
use mgc_ui::{info, success, warning};
use std::path::Path;

pub async fn audit(
    adapter: &dyn mgc_types::adapter::PackageAdapter,
    project_root: &Path,
    fix: bool,
) -> Result<()> {
    if !mgc_ui::is_quiet() {
        mgc_ui::blank_line();
        println!("🛡️  {}", "MagiCore Security Audit (Web Core)".bold().cyan());
    }

    info("Auditing lockfile through the native web adapter...");
    let report = adapter.audit(project_root).await?;

    // Check scanner availability before reporting results
    if !report.scanner_available() {
        if let mgc_types::adapter::ScannerStatus::Unavailable(reason) = &report.scanner_status {
            let strict_mode = std::env::var("MGC_AUDIT_STRICT")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);

            eprintln!("⚠ Audit scanner unavailable: {}", reason);
            eprintln!("  Audit NOT performed - cannot verify security");

            if strict_mode {
                anyhow::bail!(
                    "Audit failed: scanner unavailable in strict mode\n\
                     Set MGC_AUDIT_STRICT=0 to allow unverified state (not recommended in CI)"
                );
            } else {
                eprintln!("  Status: UNVERIFIED (pass with warning)");
                eprintln!("  Set MGC_AUDIT_STRICT=1 to fail on unavailable scanner (recommended for CI)");
                return Ok(());
            }
        }
    }

    print_report(&report);

    if report.vulnerability_count > 0 {
        if fix {
            info("Bumping vulnerable packages to latest (fail-closed)...");
            let ids: Vec<_> = report
                .vulnerabilities
                .iter()
                .map(|v| v.package.clone())
                .collect();
            let fixed = adapter.audit_fix(project_root, &ids).await?;
            success(&format!(
                "audit --fix bumped {} package(s); lockfile rewritten",
                fixed
            ));
            return Ok(());
        }
        bail!(
            "audit found {} vulnerabilities across {} packages",
            report.vulnerability_count,
            report.packages_audited
        );
    }

    success("No vulnerabilities reported by the configured provider");
    Ok(())
}

fn print_report(report: &AuditReport) {
    mgc_ui::blank_line();
    println!("{}", "Audit Report".bold().underline());
    println!("  Packages audited: {}", report.packages_audited);
    println!("  Vulnerabilities: {}", report.vulnerability_count);

    for vuln in &report.vulnerabilities {
        let severity = match vuln.severity_level {
            mgc_types::adapter::VulnerabilitySeverity::Critical
            | mgc_types::adapter::VulnerabilitySeverity::High => vuln.severity.red().bold(),
            mgc_types::adapter::VulnerabilitySeverity::Medium => vuln.severity.yellow().bold(),
            _ => vuln.severity.normal(),
        };
        mgc_ui::blank_line();
        println!("{} {} in {}", severity, vuln.title.bold(), vuln.package);
        if !vuln.cve.is_empty() {
            println!("  CVE: {}", vuln.cve);
        }
        if let Some(patched) = &vuln.patched_versions {
            println!("  Patched versions: {}", patched);
        }
        if let Some(url) = &vuln.url {
            println!("  Advisory: {}", url);
        }
    }

    if report.vulnerability_count > 0 {
        warning("Audit failed. Review the advisories above before shipping.");
    }
}
