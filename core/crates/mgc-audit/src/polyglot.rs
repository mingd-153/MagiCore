//! `polyglot.rs` — Shared manifest-dispatch for the extended cores
//! (game/iot/hardware/cloud/cicd — Tech Lead P2 2026-09-09).
//!
//! These cores own ecosystems with NO dedicated scanner yet (Unity,
//! Unreal, Terraform...), but their PROJECTS routinely carry manifests
//! the shared scanners DO understand (Bevy games ship Cargo.toml, IoT
//! backends ship go.mod, AI pipelines inside cloud projects ship
//! requirements.txt). The honest contract: scan EVERY recognized
//! manifest via the shared engine — a Bevy game gets a REAL cargo-audit
//! run; a Unity-only project stays UnsupportedEcosystem (never a fake
//! clean).
//!
//! Core mở rộng chưa có scanner riêng cho ecosystem của mình, nhưng
//! project của chúng thường mang manifest mà scanner chung HIỂU (game
//! Bevy có Cargo.toml, backend IoT có go.mod). Hợp đồng trung thực:
//! quét MỌI manifest nhận diện được qua engine chung — game Bevy được
//! cargo-audit THẬT; project chỉ Unity thì giữ UnsupportedEcosystem
//! (không bao giờ bịa sạch).

use crate::contract::ScanStep;
use crate::engine::AuditPlan;
use mgc_types::MgResult;
use mgc_types::adapter::AuditReport;
use std::path::Path;

/// Build an AuditPlan covering every SHARED-SCANNER-recognized manifest
/// in the project: Cargo.toml (cargo-audit), requirements/pylock
/// (pip-audit), go.mod (govulncheck). Adapters for the extended cores
/// call this instead of hand-rolling detection — one dispatch table,
/// every core gets the same lanes.
/// Dựng AuditPlan phủ mọi manifest được scanner chung nhận diện:
/// Cargo.toml (cargo-audit), requirements/pylock (pip-audit), go.mod
/// (govulncheck). Adapter của core mở rộng gọi hàm này thay vì tự dò —
/// một bảng dispatch, mọi core cùng lane.
pub fn plan_for_shared_manifests(project_root: &Path) -> MgResult<AuditPlan<'static>> {
    let mut plan = AuditPlan::new();

    if project_root.join("Cargo.toml").is_file() {
        let root = project_root.to_path_buf();
        plan.add_step(ScanStep {
            ecosystem: "rust",
            scanner: "cargo-audit",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { crate::scanners::audit_rust(&root).await })
            }),
        });
    }
    if crate::scanners::find_requirements_file(project_root).is_some()
        || crate::scanners::find_pylock_file(project_root).is_some()
        || project_root.join("uv.lock").is_file()
        || project_root.join("pyproject.toml").is_file()
    {
        let root = project_root.to_path_buf();
        plan.add_step(ScanStep {
            ecosystem: "python",
            scanner: "pip-audit",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { crate::scanners::audit_python(&root).await })
            }),
        });
    }
    if project_root.join("go.mod").is_file() {
        let root = project_root.to_path_buf();
        plan.add_step(ScanStep {
            ecosystem: "go",
            scanner: "govulncheck",
            run: Box::new(move || {
                let root = root.clone();
                Box::pin(async move { crate::scanners::audit_go(&root).await })
            }),
        });
    }

    Ok(plan)
}

/// Execute the polyglot plan, falling back to the caller's honest
/// UnsupportedEcosystem label when NO shared manifest exists.
/// Thực thi plan polyglot, rơi về nhãn UnsupportedEcosystem trung thực
/// của caller khi KHÔNG có manifest chung nào.
pub async fn audit_polyglot(
    project_root: &Path,
    unsupported_label: String,
) -> MgResult<AuditReport> {
    let plan = plan_for_shared_manifests(project_root)?;
    if plan.is_empty() {
        return Ok(AuditReport::unsupported_ecosystem(unsupported_label));
    }
    plan.execute().await
}
