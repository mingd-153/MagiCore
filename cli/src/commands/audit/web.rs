use anyhow::{bail, Result};
use colored::*;
use mg_types::adapter::AuditReport;
use mg_ui::{info, success, warning};
use std::path::Path;

pub async fn audit(
    adapter: &dyn mg_types::adapter::PackageAdapter,
    project_root: &Path,
) -> Result<()> {
    if !mg_ui::is_quiet() {
        mg_ui::blank_line();
        println!("🛡️  {}", "MegaGate Security Audit (Web Core)".bold().cyan());
    }

    info("Auditing lockfile through the native web adapter...");
    let report = adapter.audit(project_root).await?;
    print_report(&report);

    if report.vulnerability_count > 0 {
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
    mg_ui::blank_line();
    println!("{}", "Audit Report".bold().underline());
    println!("  Packages audited: {}", report.packages_audited);
    println!("  Vulnerabilities: {}", report.vulnerability_count);

    for vuln in &report.vulnerabilities {
        let severity = match vuln.severity_level {
            mg_types::adapter::VulnerabilitySeverity::Critical
            | mg_types::adapter::VulnerabilitySeverity::High => vuln.severity.red().bold(),
            mg_types::adapter::VulnerabilitySeverity::Medium => vuln.severity.yellow().bold(),
            _ => vuln.severity.normal(),
        };
        mg_ui::blank_line();
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
