//! `contract.rs` — The audit plan contract between adapters and the
//! shared engine: adapters DETECT manifests and DESCRIBE the scans they
//! need; the engine EXECUTES them and owns aggregation (Tech Lead
//! 2026-09-09 §11 — adapters must not run scanners inline).
//! Hợp đồng audit plan giữa adapter và engine: adapter DETECT manifest
//! và MÔ TẢ scan cần làm; engine THỰC THI và giữ phần tổng hợp.

use mgc_types::MgResult;
use mgc_types::adapter::AuditReport;

/// One scan step in an AuditPlan: name of the ecosystem/manifest group
/// plus the scanner closure that produces its report.
/// Một bước scan trong AuditPlan: tên nhóm ecosystem/manifest kèm closure
/// scanner sinh report của bước đó.
pub struct ScanStep<'a> {
    /// Ecosystem label recorded on findings (e.g. "kotlin", "python").
    /// Nhãn ecosystem ghi vào finding (vd "kotlin", "python").
    pub ecosystem: &'static str,
    /// Scanner name recorded on findings (e.g. "cargo-audit", "osv").
    /// Tên scanner ghi vào finding (vd "cargo-audit", "osv").
    pub scanner: &'static str,
    /// Execute this step — returns Err ONLY on infrastructure failure;
    /// scanner-internal states travel inside the AuditReport.
    /// Chạy bước này — chỉ trả Err khi lỗi hạ tầng; trạng thái scanner
    /// nằm bên trong AuditReport.
    pub run: Box<dyn FnOnce() -> MgResult<AuditReport> + Send + 'a>,
}
