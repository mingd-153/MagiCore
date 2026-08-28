#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for build command logic

use super::*;
use mgc_config::project::ProjectExecutionConfig;
use std::{fs, path::Path};

fn execution(lane: &str) -> ProjectExecutionConfig {
    ProjectExecutionConfig {
        architecture: "rust-first".to_string(),
        lane: lane.to_string(),
        compatibility_layer: "ts".to_string(),
        native_targets: vec!["frontend-executable".to_string()],
    }
}

#[test]
fn explicit_build_target_overrides_execution_lane() {
    let execution = execution("compatibility-shell");
    assert_eq!(
        resolve_web_build_target(&execution, Some("compiled-executable")),
        WebBuildTarget::CompiledExecutable
    );
    assert_eq!(
        resolve_web_build_target(&execution, Some("native-ready")),
        WebBuildTarget::NativeReady
    );
}

#[test]
fn execution_lane_drives_default_build_target() {
    assert_eq!(
        resolve_web_build_target(&execution("compatibility-shell"), None),
        WebBuildTarget::CompatibilityShell
    );
    assert_eq!(
        resolve_web_build_target(&execution("native-ready"), None),
        WebBuildTarget::NativeReady
    );
    assert_eq!(
        resolve_web_build_target(&execution("compiled-executable"), None),
        WebBuildTarget::CompiledExecutable
    );
}

#[test]
fn detects_native_engine_crate_in_frontend_layouts() {
    let dir = tempfile::tempdir().unwrap();
    let crate_dir = dir.path().join("crates").join("engine");
    fs::create_dir_all(crate_dir.join("src")).unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname=\"mgc-web-engine\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();

    assert_eq!(find_native_engine_crate(dir.path()), Some(crate_dir));
}

#[test]
fn framework_build_script_maps_vite_and_next() {
    let dir = tempfile::tempdir().unwrap();
    let bin_dir = dir.path().join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("vite"), "").unwrap();
    fs::write(bin_dir.join("next"), "").unwrap();

    let vite = map_framework_build_script(dir.path(), &["vite", "build"])
        .unwrap()
        .unwrap();
    assert_eq!(vite.0, Path::new("node"));
    assert_eq!(vite.1, {
        let args = vite
            .1
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(args[0], "--preserve-symlinks");
        assert_eq!(args[1], "--preserve-symlinks-main");
        assert!(args[2].contains("node_modules"));
        assert_eq!(args[3], "build");
        vite.1.clone()
    });

    let next = map_framework_build_script(dir.path(), &["next", "build"])
        .unwrap()
        .unwrap();
    assert_eq!(next.0, Path::new("node"));
    assert_eq!(next.1, {
        let args = next
            .1
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(args[0], "--preserve-symlinks");
        assert_eq!(args[1], "--preserve-symlinks-main");
        assert!(args[2].contains("node_modules"));
        assert_eq!(args[3], "build");
        next.1.clone()
    });
}

#[test]
fn framework_build_script_rejects_external_pm_wrappers() {
    let err = reject_external_package_manager_script(
        "npm run build:inner",
        Path::new("/tmp/package.json"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("delegates to 'npm'"));
}

#[test]
fn framework_build_script_rejects_external_pm_wrappers_after_separator() {
    let err = reject_external_package_manager_script(
        "vite build && yarn install",
        Path::new("/tmp/package.json"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("delegates to 'yarn'"));
}

#[test]
fn tool_unavailable_false_for_known_tool_in_path() {
    assert!(!tool_unavailable("sh"));
}

#[test]
fn tool_unavailable_true_for_nonsense_tool() {
    assert!(tool_unavailable("definitely-not-a-real-tool-mgc"));
}

#[cfg(feature = "app")]
#[test]
fn build_multi_fails_when_no_platform_artifact_is_created() {
    let tmp = std::env::temp_dir().join(format!("mgc-build-multi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("mgc.toml"), "[app]\nlanguage=\"multi\"\n").unwrap();
    let v: toml::Value =
        toml::from_str(&std::fs::read_to_string(tmp.join("mgc.toml")).unwrap()).unwrap();
    assert!(super::build_multi_app(&tmp, &v).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(feature = "clo")]
#[test]
fn build_cloud_fails_when_toolchain_missing() {
    let tmp = std::env::temp_dir().join(format!("mgc-build-cloud-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // terraform: không có CLI trên máy → cảnh báo, không fail
    std::fs::write(tmp.join("mgc.toml"), "[cloud]\ntype = \"terraform\"\n").unwrap();
    std::fs::write(tmp.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(super::build_cloud(&tmp)).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(feature = "game")]
#[test]
fn game_build_fails_when_engine_is_not_implemented() {
    let tmp = std::env::temp_dir().join(format!("mgc-build-game-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("mgc.toml"), "ecosystem = \"game\"\n").unwrap();
    std::fs::write(tmp.join("project.godot"), "[application]\n").unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(super::build_game(&tmp)).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(feature = "iot")]
#[test]
fn iot_build_fails_when_toolchain_missing() {
    let tmp = std::env::temp_dir().join(format!("mgc-build-iot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("mgc.toml"), "ecosystem = \"iot\"\n").unwrap();
    std::fs::write(
        tmp.join("platformio.ini"),
        "[env:esp32dev]\nplatform = espressif32\n",
    )
    .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(super::build_iot(&tmp)).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[cfg(feature = "lib")]
#[test]
fn lib_ts_build_fails_when_tsc_missing() {
    let tmp = std::env::temp_dir().join(format!("mgc-build-libts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("mgc.toml"), "ecosystem = \"lib\"\n").unwrap();
    std::fs::write(
        tmp.join("package.json"),
        "{\"name\":\"x\",\"version\":\"0.1.0\"}",
    )
    .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(super::build_lib(&tmp)).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}
