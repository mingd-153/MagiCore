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

use crate::engine::AuditPlan;
use mgc_types::MgResult;
use mgc_types::adapter::AuditReport;
use std::path::Path;

/// Build an AuditPlan covering every SHARED-SCANNER-recognized manifest
/// in the project — one dispatch table, every core gets the same lanes
/// (R1: built on the shared constructors in `crate::detect`, never
/// hand-rolled file checks).
/// Dựng AuditPlan phủ mọi manifest được scanner chung nhận diện — một
/// bảng dispatch trên constructor chung, không tự kiểm file.
pub fn plan_for_shared_manifests(project_root: &Path) -> MgResult<AuditPlan<'static>> {
    let mut plan = AuditPlan::new();

    for step in [
        crate::detect::rust_step(project_root),
        crate::detect::python_step(project_root),
        crate::detect::go_step(project_root),
        // P0/F4: JVM + .NET join the shared plan — a Bevy game with a
        // Gradle sidecar and a cloud repo with a .sln get real OSV scans,
        // not silent skips.
        // JVM + .NET vào plan chung — sidecar Gradle / .sln được scan
        // OSV thật.
        crate::detect::java_step(project_root),
        crate::detect::dotnet_step(project_root),
    ]
    .into_iter()
    .flatten()
    {
        plan.add_step(step);
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
