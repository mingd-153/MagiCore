//! `audit/mod.rs` — Unified security audit pipeline for ALL cores.
//!
//! One contract, one pipeline (Tech Lead 2026-09-09 §1):
//! detect core → adapter → scanner → typed report → policy → exit code.
//! The CLI only dispatches and renders; the adapters run the scanners and
//! the shared finisher enforces the fail-closed exit contract.
//!
//! P1-B (2026-09-09): `--format json|sarif|cyclonedx` prints ONLY the
//! machine payload (no table) while keeping the SAME exit contract —
//! CI can ingest evidence without parsing terminal art.
//!
//! Pipeline audit THỐNG NHẤT cho mọi core: CLI chỉ dispatch + render;
//! adapter chạy scanner; finisher chung giữ exit contract fail-closed
//! (Available → exit theo finding; Unavailable → warning, strict thì
//! fail). `--format` in đúng payload máy, giữ nguyên exit contract.

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

/// Machine output format for `mgc audit --format` (P1-B).
/// Định dạng máy cho `mgc audit --format` (P1-B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OutputFormat {
    #[default]
    Table,
    Json,
    Sarif,
    CycloneDx,
}

impl OutputFormat {
    /// Parse the CLI flag — unknown values are a hard usage error, never
    /// a silent fallback (RULE §9: guess nothing).
    /// Parse cờ CLI — giá trị lạ là lỗi usage cứng, không âm thầm rơi về
    /// mặc định (RULE §9: không đoán).
    pub(crate) fn from_flag(value: Option<&str>) -> Result<Self> {
        match value {
            None | Some("table") => Ok(Self::Table),
            Some("json") => Ok(Self::Json),
            Some("sarif") => Ok(Self::Sarif),
            Some("cyclonedx") => Ok(Self::CycloneDx),
            Some(other) => Err(crate::error::audit_unknown_format(other)),
        }
    }

    /// True when the format prints ONLY a machine payload (no table).
    /// Đúng khi format chỉ in payload máy (không có bảng).
    pub(crate) fn is_machine(self) -> bool {
        !matches!(self, Self::Table)
    }
}

/// Entry: detect core (via ProjectContext) → dispatch per-core → finisher.
/// Điểm vào: detect core qua ProjectContext → dispatch per-core → finisher.
pub async fn run(core: Option<&str>, fix: bool, format: Option<&str>) -> Result<()> {
    let fmt = OutputFormat::from_flag(format)?;
    let ctx = ProjectContext::load_with_core(core)?;
    match ctx.adapter().name() {
        "web" => web::audit(ctx.adapter(), ctx.root(), fix, fmt).await,
        "game" => game::audit(&ctx, fmt).await,
        "ai" => ai::audit(&ctx, fmt).await,
        // The cloud adapter's name() is "cloud"; the CLI core id is
        // "clo" — accept BOTH (the adapter name is the runtime truth,
        // the short id is the user-facing core key).
        // name() của adapter cloud là "cloud"; id core CLI là "clo" —
        // nhận cả hai (tên adapter là sự thật runtime, id ngắn là khóa
        // core phía người dùng).
        "clo" | "cloud" => clo::audit(&ctx, fmt).await,
        "cicd" => cicd::audit(&ctx, fmt).await,
        "iot" => iot::audit(&ctx, fmt).await,
        "app" => app::audit(&ctx, fmt).await,
        "lib" => lib::audit(&ctx, fmt).await,
        "hardware" => hardware::audit(&ctx, fmt).await,
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
/// - Machine formats (json/sarif/cyclonedx) print ONLY the payload —
///   the state travels INSIDE the payload (schema_version / UNVERIFIED
///   markers), never as extra terminal chatter.
///
/// Hợp đồng finisher (chung mọi core — không copy-paste từng core):
/// Available sạch → exit 0; có finding → exit 1; các trạng thái chưa xác
/// thực → cảnh báo, ngoài strict exit 0 (escape hatch local kèm warning),
/// strict thì exit 2. Format máy chỉ in payload — trạng thái nằm TRONG
/// payload, không thêm text terminal.
async fn finish_and_print(
    core: &str,
    report: &AuditReport,
    strict: StrictMode,
    fmt: OutputFormat,
) -> Result<()> {
    // Machine formats: emit the payload and keep the SAME exit contract —
    // the human table is skipped entirely, CI parses structured data.
    // Format máy: phát payload và giữ NGUYÊN exit contract — bỏ bảng
    // hiển thị, CI parse dữ liệu có cấu trúc.
    if fmt.is_machine() {
        return finish_machine(report, strict, fmt);
    }

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
        eprintln!("WARN: {headline}");
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

/// Machine-path finisher: print the structured payload, then apply the
/// SAME exit contract as the table path (fail-closed five-state policy).
/// Finisher đường máy: in payload cấu trúc rồi áp DÚNG exit contract
/// như đường table (policy 5 trạng thái fail-closed).
fn finish_machine(report: &AuditReport, strict: StrictMode, fmt: OutputFormat) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            let envelope = mgc_audit::output::json::JsonAuditEnvelope::from_report(report);
            println!("{}", envelope.to_json()?);
        }
        OutputFormat::Sarif => {
            let log = mgc_audit::output::sarif::to_sarif(report);
            println!("{}", serde_json::to_string_pretty(&log)?);
        }
        OutputFormat::CycloneDx => {
            let bom = mgc_audit::output::cyclonedx::to_cyclonedx(report);
            println!("{}", serde_json::to_string_pretty(&bom)?);
        }
        OutputFormat::Table => unreachable!("table handled by the human path"),
    }

    // Exit contract mirrors finish_and_print exactly: unverified states
    // still exit 2 under strict (CI ingests the payload AND gates on it).
    // Exit contract phản chiếu finish_and_print: trạng thái chưa xác thực
    // vẫn exit 2 dưới strict (CI vừa ingest payload vừa gate trên nó).
    if !report.scanner_available() && strict.enabled() {
        let headline = format!(
            "audit did not complete (status: {})",
            mgc_audit::output::json::JsonAuditEnvelope::from_report(report).scanner_status
        );
        return Err(crate::error::audit_scanner_unavailable_strict(&headline));
    }
    if report.scanner_available() && report.vulnerability_count > 0 {
        return Err(crate::error::audit_found_vulnerabilities(
            report.vulnerability_count,
            report.packages_audited,
        ));
    }
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
    run(Some(core), false, None).await
}

#[cfg(test)]
#[path = "test/audit_format_test.rs"]
mod tests;
