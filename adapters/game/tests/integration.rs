#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-game-adapter — sát với src/lib.rs
//! Kiểm thử: detect_engine (Bevy, Godot, Unity, Unreal), adapter_for, PackageAdapter trait.

use mgc_game_adapter::{adapter_for, detect_engine, GameEngine};
use mgc_types::adapter::{AddOptions, PackageAdapter};
use mgc_types::PackageName;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-game-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_engine — file markers ──────────────────────────────────────────

#[test]
fn detect_godot_via_project_godot() {
    let dir = tmp("godot");
    std::fs::write(
        dir.join("project.godot"),
        "[application]\nconfig/name=\"Demo\"\n",
    )
    .unwrap();
    assert_eq!(detect_engine(&dir), Some(GameEngine::Godot));
}

#[test]
fn detect_unity_via_packages_manifest() {
    let dir = tmp("unity");
    std::fs::create_dir_all(dir.join("Packages")).unwrap();
    std::fs::write(
        dir.join("Packages").join("manifest.json"),
        "{\"dependencies\":{}}\n",
    )
    .unwrap();
    assert_eq!(detect_engine(&dir), Some(GameEngine::Unity));
}

#[test]
fn detect_unreal_via_uproject() {
    let dir = tmp("unreal");
    std::fs::write(dir.join("Game.uproject"), "{\"FileVersion\":3}\n").unwrap();
    assert_eq!(detect_engine(&dir), Some(GameEngine::Unreal));
}

#[test]
fn detect_bevy_via_cargo_toml() {
    let dir = tmp("bevy");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"game\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    assert_eq!(detect_engine(&dir), Some(GameEngine::Bevy));
}

// ── detect_engine — mgc.toml priority ──────────────────────────────────────

#[test]
fn detect_via_mgc_toml_overrides_file_marker() {
    let dir = tmp("mgc-override");
    // Cargo.toml -> Bevy, mgc.toml -> Godot
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"game\"\n\n[game]\nengine = \"godot\"\n",
    )
    .unwrap();
    assert_eq!(detect_engine(&dir), Some(GameEngine::Godot));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_engine(&dir).is_none());
}

// ── adapter_for & engine() ─────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_for_godot() {
    let dir = tmp("af-godot");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.engine(), "godot");
}

#[test]
fn adapter_for_returns_none_for_plain_dir() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let dir = tmp("name-eco");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.name(), "game");
    assert_eq!(format!("{:?}", a.ecosystem()), "Game");
}

#[test]
fn can_handle_returns_true_for_game_project() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert!(a.can_handle(&dir));
}

#[tokio::test]
async fn parse_manifest_godot_returns_game_manifest() {
    let dir = tmp("my-godot-game");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert!(manifest.name.contains("my-godot-game"));
}

#[tokio::test]
async fn parse_manifest_bevy_reads_cargo_toml_dependencies() {
    let dir = tmp("my-bevy-game");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"space-game\"\nversion = \"0.1.0\"\n\n[dependencies]\nbevy = \"0.14\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert_eq!(manifest.name, "space-game");
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dependencies[0].name.as_str(), "bevy");
}

#[tokio::test]
async fn godot_install_returns_ok() {
    let dir = tmp("install-godot");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    let graph = a.resolve(&manifest).await.unwrap();
    assert!(a.install(&graph, &dir, Default::default()).await.is_ok());
}

#[tokio::test]
async fn godot_add_fails_closed_directing_to_editor() {
    let dir = tmp("add-godot");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("godot-plugin").unwrap();
    let err = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("godot") || msg.contains("Asset Library") || msg.contains("dev"),
        "error must mention asset library/dev: {msg}"
    );
}

#[tokio::test]
async fn unity_add_fails_closed_directing_to_upm() {
    let dir = tmp("add-unity");
    std::fs::create_dir_all(dir.join("Packages")).unwrap();
    std::fs::write(
        dir.join("Packages").join("manifest.json"),
        "{\"dependencies\":{}}\n",
    )
    .unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("com.unity.cinemachine").unwrap();
    let err = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unity") || msg.contains("Packages/manifest.json") || msg.contains("UPM"),
        "error must mention UPM/Packages: {msg}"
    );
}

#[tokio::test]
async fn audit_returns_clean_for_game_project() {
    let dir = tmp("audit-game");
    std::fs::write(dir.join("project.godot"), "[application]\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}
