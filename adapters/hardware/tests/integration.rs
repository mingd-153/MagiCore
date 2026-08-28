#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-hardware-adapter — sát với src/lib.rs
//! Kiểm thử: adapter_for (detect qua mgc.toml ecosystem), list, audit, PackageAdapter trait.

use mgc_hardware_adapter::{adapter_for, generate_sbom, HardwareAdapter};
use mgc_types::adapter::PackageAdapter;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-hw-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_for_hardware_ecosystem() {
    let dir = tmp("hw");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    assert!(adapter_for(&dir).is_some());
}

#[test]
fn adapter_for_returns_some_for_game_ecosystem_cross_core() {
    // hardware optimizer/bench là cross-core add-on — game project cũng supported
    let dir = tmp("game-cross");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"game\"\n").unwrap();
    assert!(
        adapter_for(&dir).is_some(),
        "cross-core: hardware optimizer can be added to game project"
    );
}

#[test]
fn adapter_for_returns_none_for_dir_without_mgc_toml() {
    let dir = tmp("empty");
    assert!(adapter_for(&dir).is_none());
}

#[test]
fn adapter_for_returns_none_for_non_mgc_ecosystem() {
    let dir = tmp("web-eco");
    // web ecosystem — hardware adapter không detect
    std::fs::write(dir.join("package.json"), r#"{"name":"web"}"#).unwrap();
    assert!(adapter_for(&dir).is_none());
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    assert_eq!(HardwareAdapter.name(), "hardware");
    assert_eq!(format!("{:?}", HardwareAdapter.ecosystem()), "Hardware");
}

#[test]
fn can_handle_returns_true_for_hardware_ecosystem() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    assert!(HardwareAdapter.can_handle(&dir));
}

#[test]
fn can_handle_returns_false_for_plain_dir() {
    let dir = tmp("ch-false");
    assert!(!HardwareAdapter.can_handle(&dir));
}

// ── list — scan optimizer/ and bench/ folders ──────────────────────────────

#[tokio::test]
async fn list_returns_empty_when_no_optimizer_or_bench() {
    let dir = tmp("list-empty");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    let pkgs = HardwareAdapter.list(&dir).await.unwrap();
    assert!(pkgs.is_empty());
}

#[tokio::test]
async fn list_returns_optimizer_when_folder_exists() {
    let dir = tmp("list-opt");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    std::fs::create_dir_all(dir.join("optimizer")).unwrap();
    let pkgs = HardwareAdapter.list(&dir).await.unwrap();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].id.name().as_str(), "optimizer");
}

#[tokio::test]
async fn list_returns_both_optimizer_and_bench_when_both_exist() {
    let dir = tmp("list-both");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    std::fs::create_dir_all(dir.join("optimizer")).unwrap();
    std::fs::create_dir_all(dir.join("bench")).unwrap();
    let pkgs = HardwareAdapter.list(&dir).await.unwrap();
    assert_eq!(pkgs.len(), 2);
    let names: Vec<&str> = pkgs.iter().map(|p| p.id.name().as_str()).collect();
    assert!(names.contains(&"optimizer"));
    assert!(names.contains(&"bench"));
}

#[tokio::test]
async fn list_ignores_other_directories() {
    let dir = tmp("list-ignore");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    std::fs::create_dir_all(dir.join("optimizer")).unwrap();
    // src/ không phải optimizer hay bench — phải bị bỏ qua
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let pkgs = HardwareAdapter.list(&dir).await.unwrap();
    assert_eq!(pkgs.len(), 1);
}

// ── audit ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn audit_returns_clean_for_hardware_project() {
    let dir = tmp("audit");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    let report = HardwareAdapter.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}

// ── parse_manifest ─────────────────────────────────────────────────────────

#[tokio::test]
async fn parse_manifest_uses_dir_name() {
    let dir = tmp("my-hardware-project");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    let manifest = HardwareAdapter.parse_manifest(&dir).await.unwrap();
    assert!(manifest.name.contains("my-hardware-project"));
}

// ── add/remove/update — fail-closed semantics ─────────────────────────────

#[tokio::test]
async fn add_fails_for_hardware_adapter() {
    use mgc_types::adapter::AddOptions;
    use mgc_types::PackageName;
    let dir = tmp("add-fail");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    let name = PackageName::new("hal-crate").unwrap();
    let result = HardwareAdapter
        .add(&dir, &name, None, AddOptions::default())
        .await;
    assert!(result.is_err(), "hardware adapter must block direct add");
}

#[tokio::test]
async fn remove_fails_for_hardware_adapter() {
    use mgc_types::PackageName;
    let dir = tmp("rem-fail");
    std::fs::write(dir.join("mgc.toml"), "ecosystem = \"hardware\"\n").unwrap();
    let name = PackageName::new("hal-crate").unwrap();
    let result = HardwareAdapter.remove(&dir, &name).await;
    assert!(result.is_err(), "hardware adapter must block direct remove");
}

#[test]
fn generate_sbom_uses_lockfile_v2_fixture() {
    let mut lockfile = mgc_lockfile::Lockfile::new();
    lockfile.add_package(mgc_lockfile::Package::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        "https://example.com/test.tgz".to_string(),
        "blake3:test123".to_string(),
    ));
    let json = generate_sbom(&lockfile, mgc_sbom::SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("test-pkg"));
    assert!(json.contains("1.0.0"));
}
