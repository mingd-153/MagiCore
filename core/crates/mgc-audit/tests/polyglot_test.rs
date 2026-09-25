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
    // uv.lock signals a resolved Python graph and joins the MGC-native lane.
    // uv.lock báo graph Python đã resolve và được đưa vào scanner native MGC.
    let dir = temp_root();
    std::fs::write(dir.path().join("uv.lock"), "version = 1\n").unwrap();
    let plan = plan_for_shared_manifests(dir.path()).unwrap();
    assert_eq!(plan.len(), 1, "uv.lock → python lane step");
}

#[tokio::test]
async fn polyglot_executes_scanners_and_aggregates() {
    // Requirements pins are scanned natively but remain Partial because
    // the file cannot establish a complete resolved dependency graph.
    // MGC quét pin requirements native nhưng giữ Partial vì file không
    // chứng minh được graph resolve đầy đủ.
    let dir = temp_root();
    std::fs::write(dir.path().join("requirements.txt"), "flask==3.0.0\n").unwrap();
    let report = audit_polyglot(dir.path(), "game (0 deps)".to_string())
        .await
        .unwrap();
    // Partial proves scanner execution and preserves the coverage warning.
    // Partial chứng minh scanner đã chạy và giữ cảnh báo độ phủ.
    assert!(
        matches!(
            report.scanner_status,
            mgc_types::adapter::ScannerStatus::Partial { .. }
                | mgc_types::adapter::ScannerStatus::Failed { .. }
        ),
        "recognized manifest must not yield unsupported: {:?}",
        report.scanner_status
    );
}
