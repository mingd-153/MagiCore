//! `polyglot_test.rs` — Shared polyglot dispatch contract (P2): every
//! shared-scanner manifest in an extended-core project joins ONE plan
//! (game Bevy + Cargo.toml → cargo-audit; IoT + go.mod → govulncheck);
//! no shared manifest → honest UnsupportedEcosystem, never a fake clean.
//! Hợp đồng dispatch polyglot chung: mọi manifest scanner-chung trong
//! project core-mở rộng gộp vào MỘT plan; không còn manifest chung →
//! UnsupportedEcosystem trung thực, không bịa sạch.
#![allow(clippy::unwrap_used)]

use mgc_audit::polyglot::{audit_polyglot, plan_for_shared_manifests};

fn temp_root() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[tokio::test]
async fn no_shared_manifest_keeps_unsupported_label() {
    let dir = temp_root();
    let report = audit_polyglot(
        dir.path(),
        "game (0 dependencies not scanned — no scanner implemented yet)".to_string(),
    )
    .await
    .unwrap();
    assert!(!report.scanner_available());
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::UnsupportedEcosystem { ecosystem } => {
            assert!(
                ecosystem.starts_with("game"),
                "label must ride through: {ecosystem}"
            );
        }
        other => panic!("expected UnsupportedEcosystem, got {other:?}"),
    }
}

#[tokio::test]
async fn rust_manifest_joins_plan() {
    let dir = temp_root();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let plan = plan_for_shared_manifests(dir.path()).unwrap();
    assert_eq!(plan.len(), 1, "Cargo.toml → one cargo-audit step");
}

#[tokio::test]
async fn all_three_manifests_join_one_plan() {
    // A polyglot repo (game with rust + python + go tooling) must scan
    // ALL recognized manifests — three steps, never first-match.
    // Repo đa ngôn ngữ phải quét MỌI manifest nhận diện — ba bước,
    // không first-match.
    let dir = temp_root();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("requirements.txt"), "flask==3.0.0\n").unwrap();
    std::fs::write(dir.path().join("go.mod"), "module x\n\ngo 1.26\n").unwrap();
    let plan = plan_for_shared_manifests(dir.path()).unwrap();
    assert_eq!(plan.len(), 3, "rust+python+go manifests = three steps");
}

#[tokio::test]
async fn uv_lock_without_requirements_still_joins_python_lane() {
    // uv.lock signals a resolved python dep set — the python scanner
    // route reports honest Failed guidance (uv unsupported by pip-audit)
    // instead of skipping the lane.
    // uv.lock báo tập dep python đã resolve — lane python vẫn vào để
    // scanner báo Failed hướng dẫn trung thực, không âm thầm bỏ.
    let dir = temp_root();
    std::fs::write(dir.path().join("uv.lock"), "version = 1\n").unwrap();
    let plan = plan_for_shared_manifests(dir.path()).unwrap();
    assert_eq!(plan.len(), 1, "uv.lock → python lane step");
}

#[tokio::test]
async fn polyglot_executes_scanners_and_aggregates() {
    // Hermetic machine without pip-audit: the python lane reports
    // ToolMissing INSIDE the aggregate (strictest-wins), NOT a silent
    // clean — proof the plan actually executes scanner futures.
    // Máy hermetic không pip-audit: lane python báo ToolMissing trong
    // aggregate (chặt nhất thắng), KHÔNG sạch giả — chứng minh plan
    // thực sự chạy future scanner.
    let dir = temp_root();
    std::fs::write(dir.path().join("requirements.txt"), "flask==3.0.0\n").unwrap();
    let report = audit_polyglot(dir.path(), "game (0 deps)".to_string())
        .await
        .unwrap();
    // Either ToolMissing (no pip-audit) or Available (installed) — but
    // NEVER an unsupported empty: the manifest WAS recognized.
    // Hoặc ToolMissing (không pip-audit) hoặc Available (có cài) —
    // nhưng KHÔNG BAO GIỜ unsupported rỗng: manifest ĐÃ được nhận diện.
    assert!(
        matches!(
            report.scanner_status,
            mgc_types::adapter::ScannerStatus::ToolMissing { .. }
                | mgc_types::adapter::ScannerStatus::Available
                | mgc_types::adapter::ScannerStatus::Failed { .. }
        ),
        "recognized manifest must not yield unsupported: {:?}",
        report.scanner_status
    );
}
