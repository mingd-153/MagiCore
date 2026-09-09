//! `audit/mod.rs` — Unified security audit pipeline for ALL cores.
//!
//! One contract, one pipeline (Tech Lead 2026-09-09 §1):
//! detect core → adapter → scanner → typed report → policy → exit code.
//! The CLI only dispatches and renders; the adapters run the scanners and
//! the shared finisher enforces the fail-closed exit contract.
//!
//! Pipeline audit THỐNG NHẤT cho mọi core: CLI chỉ dispatch + render;
//! adapter chạy scanner; finisher chung giữ exit contract fail-closed
//! (Available → exit theo finding; Unavailable → warning, strict thì fail).

use anyhow::Result;
use colored::Colorize;
use mgc_types::Ecosystem;
use mgc_types::adapter::AuditReport;

use crate::context::ProjectContext;

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod hardware;
pub mod iot;
pub mod lib;
pub mod web;

/// Entry: detect core (via ProjectContext) → dispatch per-core → finisher.
/// Điểm vào: detect core qua ProjectContext → dispatch per-core → finisher.
pub async fn run(core: Option<&str>, fix: bool) -> Result<()> {
    let ctx = ProjectContext::load_with_core(core)?;
    match ctx.adapter().name() {
        "web" => web::audit(ctx.adapter(), ctx.root(), fix).await,
        "game" => game::audit(&ctx).await,
        "ai" => ai::audit(&ctx).await,
        "clo" => clo::audit(&ctx).await,
        "cicd" => cicd::audit(&ctx).await,
        "iot" => iot::audit(&ctx).await,
        "app" => app::audit(&ctx).await,
        "lib" => lib::audit(&ctx).await,
        "hardware" => hardware::audit(&ctx).await,
        other => Err(crate::error::unknown_core(other)),
    }
}

/// Shared per-core body: run the adapter's real scanner, then enforce the
/// exit contract via the finisher. Per-core files call this so the CLI
/// layer holds zero business logic (Tech Lead §11).
/// Body chung từng core: chạy scanner thật của adapter rồi giữ exit
/// contract qua finisher — file per-core chỉ dispatch, không business logic.
async fn run_adapter_audit(ctx: &ProjectContext) -> Result<AuditReport> {
    ctx.adapter()
        .audit(ctx.root())
        .await
        .map_err(|e| anyhow::anyhow!("{} adapter audit failed: {e}", ctx.adapter().name()))
}

/// Finisher contract (shared by every core — no per-core copy-paste):
/// - Available + 0 findings  → exit 0, "clean" printed.
/// - Available + findings    → exit 1 with the vulnerability list.
/// - Partial / ToolMissing / UnsupportedEcosystem / Failed → UNVERIFIED:
///   exit 0 only outside strict mode (local escape hatch with loud warning),
///   exit 2 in strict mode (CI must never pass on an unverified audit).
///
/// Hợp đồng finisher (chung mọi core — không copy-paste từng core):
/// Available sạch → exit 0; có finding → exit 1; các trạng thái chưa xác
/// thực → cảnh báo, ngoài strict exit 0 (escape hatch local kèm warning),
/// strict thì exit 2.
async fn finish_and_print(core: &str, report: &AuditReport, strict: StrictMode) -> Result<()> {
    if !mgc_ui::is_quiet() {
        mgc_ui::blank_line();
        println!(
            "🛡️  {}",
            format!("MagiCore Security Audit ({core} core)")
                .bold()
                .cyan()
        );
    }

    // Fail-closed classification over the five-state contract (Tech Lead
    // §1). Only `Available` may print "clean"; every other state is
    // UNVERIFIED — strict mode fails with exit 2, local mode warns loudly
    // and exits 0 as the documented escape hatch.
    // Phân loại fail-closed trên contract 5 trạng thái. Chỉ `Available` mới
    // được in "clean"; mọi trạng thái khác là UNVERIFIED — strict fail
    // exit 2, local cảnh báo to và exit 0 như escape hatch đã ghi rõ.
    if !report.scanner_available() {
        use mgc_types::adapter::ScannerStatus;

        let (headline, detail) = match &report.scanner_status {
            ScannerStatus::Partial {
                scanned,
                skipped,
                reasons,
            } => (
                format!(
                    "{core} audit PARTIAL: {scanned} scanned, {skipped} skipped (strict CI must fail)"
                ),
                format!("  Skipped reasons: {}", reasons.join("; ")),
            ),
            ScannerStatus::ToolMissing { tool, remediation } => (
                format!("{core} audit NOT performed — required tool missing: {tool}"),
                format!("  Remediation: {remediation}"),
            ),
            ScannerStatus::UnsupportedEcosystem { ecosystem } => (
                format!(
                    "{core} audit NOT performed — no scanner implemented for ecosystem '{ecosystem}' (dev-only state)"
                ),
                "  This core cannot be claimed complete while this state is reachable.".to_string(),
            ),
            ScannerStatus::Failed { scanner, reason } => (
                format!("{core} audit scanner FAILED: {scanner}"),
                format!("  Reason: {reason}"),
            ),
            ScannerStatus::Available => {
                return Err(crate::error::audit_unknown_scanner_state(core));
            }
        };
        eprintln!("⚠ {headline}");
        eprintln!("{detail}");
        eprintln!("  Audit NOT performed/complete — this run is UNVERIFIED, not clean.");
        if strict.enabled() {
            return Err(crate::error::audit_scanner_unavailable_strict(&headline));
        }
        eprintln!("  Status: UNVERIFIED (exit 0 — local escape hatch)");
        eprintln!(
            "  Set MGC_AUDIT_STRICT=1 (or pass --audit-strict) to fail on unavailable scanner in CI."
        );
        return Ok(());
    }

    print_report(report);

    if report.vulnerability_count > 0 {
        return Err(crate::error::audit_found_vulnerabilities(
            report.vulnerability_count,
            report.packages_audited,
        ));
    }

    mgc_ui::success("No vulnerabilities reported by the configured provider");
    Ok(())
}

/// Strict audit mode flag — `MGC_AUDIT_STRICT` env (set by `--audit-strict`).
/// Escape hatch contract (RULE §11): default open locally, closed in CI.
/// Cờ strict — env `MGC_AUDIT_STRICT` (bật qua `--audit-strict`).
/// Hợp đồng escape hatch (RULE §11): mặc định mở locally, đóng trong CI.
#[derive(Debug, Clone, Copy)]
struct StrictMode(bool);

impl StrictMode {
    fn from_env() -> Self {
        Self(
            std::env::var("MGC_AUDIT_STRICT")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
        )
    }

    fn enabled(self) -> bool {
        self.0
    }
}

/// Render the typed report — shared across cores so every ecosystem gets
/// the same table: package counts, per-finding severity, advisory links.
/// Render report typed — dùng chung mọi core: mọi ecosystem cùng một bảng
/// kết quả: số package, severity từng finding, link advisory.
pub(crate) fn print_report(report: &AuditReport) {
    use colored::*;
    use mgc_types::adapter::VulnerabilitySeverity;

    mgc_ui::blank_line();
    println!("{}", "Audit Report".bold().underline());
    println!("  Packages audited: {}", report.packages_audited);
    println!("  Vulnerabilities: {}", report.vulnerability_count);

    for vuln in &report.vulnerabilities {
        let severity = match vuln.severity_level {
            VulnerabilitySeverity::Critical | VulnerabilitySeverity::High => {
                vuln.severity.red().bold()
            }
            VulnerabilitySeverity::Medium => vuln.severity.yellow().bold(),
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
        mgc_ui::warning("Audit failed. Review the advisories above before shipping.");
    }
}

/// Legacy per-core entry for external callers that already hold an
/// Ecosystem (kept public for library consumers; the CLI itself and the
/// MCP server route through `run` directly).
/// Entry theo Ecosystem cho caller ngoài còn giữ Ecosystem (giữ public cho
/// library consumer; CLI và MCP gọi thẳng `run`).
#[allow(dead_code)]
pub async fn execute_audit(ecosystem: &Ecosystem) -> Result<()> {
    let core = match ecosystem {
        Ecosystem::Web => "web",
        Ecosystem::Game => "game",
        Ecosystem::Ai => "ai",
        Ecosystem::Cloud => "clo",
        Ecosystem::Cicd => "cicd",
        Ecosystem::Iot => "iot",
        Ecosystem::App => "app",
        Ecosystem::Lib => "lib",
        Ecosystem::Hardware => "hardware",
    };
    run(Some(core), false).await
}
