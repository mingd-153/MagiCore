#![allow(clippy::unwrap_used)]
//! T9a core signature marker tests — chữ kí chống nhầm core (user 2026-08-19).
//! Covers: write/read marker, save() auto-writes marker, detect priority,
//! ambiguous fail-closed, find_project_root marker propagation.

use mgc_config::project::ProjectConfig;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mgc-config-marker-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn project(name: &str) -> PathBuf {
    let dir = tmp_dir(name);
    let cfg = ProjectConfig::new(dir.file_name().unwrap().to_string_lossy().as_ref(), "web");
    cfg.save(&dir).unwrap();
    dir
}

#[test]
fn save_writes_marker_with_ecosystem() {
    let dir = project("saves-marker");
    let marker = dir.join(ProjectConfig::CORE_MARKER_FILE);
    assert!(marker.exists(), "save() must write .mgc.core");
    let content = fs::read_to_string(&marker).unwrap();
    assert_eq!(content.trim(), "web");
}

#[test]
fn read_marker_roundtrip_and_cloud_alias() {
    let dir = project("alias-cloud");
    ProjectConfig::write_core_marker_at(&dir, "cloud").unwrap();
    let core = ProjectConfig::read_core_marker(&dir).unwrap();
    assert_eq!(
        core.as_deref(),
        Some("clo"),
        "cloud must canonicalize to clo"
    );
}

#[test]
fn read_marker_invalid_core_fails_closed() {
    let dir = tmp_dir("invalid-core");
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "not-a-core\n").unwrap();
    let err = ProjectConfig::read_core_marker(&dir).unwrap_err();
    assert!(
        err.to_string().contains("invalid core signature"),
        "expected fail-closed on bad marker, got: {err}"
    );
}

#[test]
fn detect_core_priority_marker_over_signature() {
    let dir = project("priority-marker");
    let marker = fs::read_to_string(dir.join(ProjectConfig::CORE_MARKER_FILE)).unwrap();
    assert_eq!(marker.trim(), "web");
    // ngay cả khi thêm signature khác core — marker vẫn thắng
    fs::write(dir.join("package.json"), "{}").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("web")
    );
}

#[test]
fn detect_core_signature_single() {
    let dir = tmp_dir("detect-single");
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("lib"),
        "Cargo.toml signature → lib (game/iot must have their own marker)"
    );
}

#[test]
fn detect_core_ambiguous_fails_closed() {
    let dir = tmp_dir("detect-ambiguous");
    fs::write(dir.join("package.json"), "{}").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    let err = ProjectConfig::detect_core(&dir).unwrap_err();
    assert!(
        err.to_string().contains("Ambiguous"),
        "two signatures + no marker must fail, got: {err}"
    );
    // và auto_detect legacy không đoán trong case này
    assert_eq!(ProjectConfig::auto_detect(&dir), None);
}

#[test]
fn detect_core_marker_disambiguates() {
    let dir = tmp_dir("detect-disambiguate");
    fs::write(dir.join("package.json"), "{}").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    ProjectConfig::write_core_marker_at(&dir, "game").unwrap();
    assert_eq!(
        ProjectConfig::detect_core(&dir).unwrap().as_deref(),
        Some("game"),
        "marker must disambiguate: a game project with Cargo.toml must not be mistaken for lib"
    );
}

#[test]
fn find_project_root_sees_marker_in_parent() {
    let dir = tmp_dir("find-root");
    let inner = dir.join("a/b/c");
    fs::create_dir_all(&inner).unwrap();
    assert_eq!(
        ProjectConfig::find_project_root(&inner),
        None,
        "no marker yet"
    );
    fs::write(dir.join(ProjectConfig::CORE_MARKER_FILE), "iot\n").unwrap();
    assert_eq!(
        ProjectConfig::find_project_root(&inner),
        Some(dir),
        "marker in grandparent must anchor root (monorepo)"
    );
}

#[test]
fn write_marker_unknown_core_rejected() {
    let dir = tmp_dir("unknown-core");
    let err = ProjectConfig::write_core_marker_at(&dir, "monolith").unwrap_err();
    assert!(err.to_string().contains("Unknown core"), "got: {err}");
    assert!(!dir.join(ProjectConfig::CORE_MARKER_FILE).exists());
}
