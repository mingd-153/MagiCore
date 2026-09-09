//! `engine.rs` — Executes an AuditPlan: run every ScanStep, collect the
//! per-ecosystem reports (labeled), and delegate to the aggregator.
//! The engine itself holds no business logic per scanner — adapters
//! register closures, the engine just runs and aggregates (Tech Lead
//! §11).
//! Thực thi AuditPlan: chạy từng ScanStep, gom report theo ecosystem
//! (có nhãn) rồi đưa cho aggregator. Engine không chứa logic scanner —
//! adapter đăng ký closure, engine chạy và tổng hợp.

use mgc_types::MgResult;
use mgc_types::adapter::AuditReport;

use crate::aggregate::aggregate_reports;
use crate::contract::ScanStep;

/// An executable audit plan: the full set of scan steps for one project.
/// Audit plan thực thi được: toàn bộ bước scan của một project.
pub struct AuditPlan<'a> {
    steps: Vec<ScanStep<'a>>,
}

impl<'a> AuditPlan<'a> {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Register one scan step (ecosystem label + scanner closure).
    /// Đăng ký một bước scan (nhãn ecosystem + closure scanner).
    pub fn add_step(&mut self, step: ScanStep<'a>) {
        self.steps.push(step);
    }

    /// Number of registered steps — lets adapters assert they scan
    /// EVERY manifest they detected (no silent first-match).
    /// Số bước đã đăng ký — adapter dùng để khẳng định quét MỌI manifest
    /// đã nhận diện (không rơi vào first-match).
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Execute all steps and aggregate. Infrastructure errors (MgResult
    /// Err) fail the WHOLE audit — they mean the engine could not even
    /// run a step, which is stricter than scanner-internal failures.
    /// Chạy tất cả bước rồi tổng hợp. Lỗi hạ tầng (Err) fail CẢ audit —
    /// nghĩa là engine không chạy được bước nào đó, chặt hơn lỗi scanner.
    pub async fn execute(self) -> MgResult<AuditReport> {
        let mut labeled: Vec<(String, AuditReport)> = Vec::new();
        for step in self.steps {
            let report = (step.run)()?;
            labeled.push((format!("{}:{}", step.ecosystem, step.scanner), report));
        }
        Ok(aggregate_reports(labeled))
    }
}

impl Default for AuditPlan<'_> {
    fn default() -> Self {
        Self::new()
    }
}
