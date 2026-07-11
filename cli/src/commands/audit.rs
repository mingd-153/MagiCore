use crate::context::ProjectContext;
use anyhow::Result;
use mg_ui::{info, success, warning};

/// mg audit — security audit
pub async fn run(core: Option<&str>) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();

    info("Running security audit...");
    let report = adapter.audit(ctx.root()).await?;

    if report.is_clean() {
        success("No vulnerabilities found");
        info(&format!("{} packages audited", report.packages_audited));
    } else {
        warning(&format!(
            "Found {} vulnerabilities:",
            report.vulnerabilities.len()
        ));
        for vuln in &report.vulnerabilities {
            warning(&format!(
                "  {}@{} — {} ({}) {}",
                vuln.package.name_str(),
                vuln.package.version(),
                vuln.title,
                vuln.severity,
                vuln.cve,
            ));
        }
    }

    Ok(())
}
