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
fn standalone_rust_fast_path_requires_no_mgc_project_identity() {
    let plain = tempfile::tempdir().unwrap();
    fs::write(
        plain.path().join("Cargo.toml"),
        "[package]\nname='plain'\nversion='0.1.0'\n",
    )
    .unwrap();
    assert!(should_use_legacy_rust_build(plain.path()).unwrap());

    let configured = tempfile::tempdir().unwrap();
    fs::write(
        configured.path().join("Cargo.toml"),
        "[package]\nname='ai-project'\nversion='0.1.0'\n",
    )
    .unwrap();
    mgc_config::project::ProjectConfig::new("ai-project", "ai")
        .save(configured.path())
        .unwrap();
    fs::remove_file(configured.path().join(".mgc.core")).unwrap();
    assert!(!should_use_legacy_rust_build(configured.path()).unwrap());
}

#[test]
fn build_rejects_legacy_compat_runtime_before_core_dispatch() {
    let native = crate::commands::compat::CompatMode::Native;
    assert!(ensure_native_build_mode(&native).is_ok());

    for runtime in ["bun", "deno"] {
        let mode = crate::commands::compat::CompatMode::Explicit(runtime.to_string());
        let error = ensure_native_build_mode(&mode).unwrap_err();
        assert!(
            error.to_string().contains("native MagiCore runtime"),
            "unexpected refusal for {runtime}: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn build_manifest_presence_rejects_external_symlinks() {
    let project = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("platformio.ini"), "[env:test]\n").unwrap();
    std::os::unix::fs::symlink(
        external.path().join("platformio.ini"),
        project.path().join("platformio.ini"),
    )
    .unwrap();

    assert!(project_manifest_present(project.path(), "platformio.ini").is_err());
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
fn framework_build_script_rejects_package_manager_javascript_entrypoint() {
    let error = reject_external_package_manager_script(
        "node ./node_modules/npm/bin/npm-cli.js install",
        Path::new("/tmp/package.json"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("delegates to 'npm'"));
}

#[test]
fn tool_unavailable_false_for_known_tool_in_path() {
    assert!(!tool_unavailable("sh"));
}

#[test]
fn tool_unavailable_true_for_nonsense_tool() {
    assert!(tool_unavailable("definitely-not-a-real-tool-mgc"));
}

#[test]
// SAFETY (edition 2024): `env::set_var` is unsafe — this test mutates
// PATH/PATHEXT with a unique probe name, restoring both vars after
// (repo-wide test env convention: install_test.rs).
#[allow(unsafe_code)]
fn tool_unavailable_honors_pathext_wrappers() {
    // P0#4 Windows: a `<tool>.bat` shim counts as present when PATHEXT
    // names .BAT — verified on every OS by setting PATHEXT explicitly
    // (the helper reads it whenever present). The probe name is unique
    // per process so concurrent tests sharing env are unaffected.
    let probe = format!("mgc-pathext-probe-{}", std::process::id());
    let dir = std::env::temp_dir().join(format!("mgc-pathext-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{probe}.bat")), "@echo off\n").unwrap();
    let old_path = std::env::var_os("PATH");
    let old_pathext = std::env::var_os("PATHEXT");
    let mut paths = vec![dir.clone()];
    if let Some(existing) = &old_path {
        paths.extend(std::env::split_paths(existing));
    }
    // SAFETY (edition 2024): `env::set_var` is unsafe — this test mutates
    // process env with a unique probe name, restoring both vars after.
    // (An toàn: test đổi env với tên probe duy nhất, restore sau.)
    unsafe {
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        std::env::set_var("PATHEXT", ".COM;.EXE;.BAT;.CMD");
    }
    assert!(
        !tool_unavailable(&probe),
        "{probe}.bat must satisfy the lookup under PATHEXT"
    );
    assert!(
        tool_unavailable("definitely-not-a-real-tool-mgc-pathext"),
        "absent tool must stay unavailable"
    );
    unsafe {
        if let Some(path) = old_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }
        if let Some(pathext) = old_pathext {
            std::env::set_var("PATHEXT", pathext);
        } else {
            std::env::remove_var("PATHEXT");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
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

#[cfg(feature = "app")]
#[test]
fn build_multi_fails_when_any_requested_platform_was_skipped() {
    assert!(super::finish_multi_build(1, &["ios (missing toolchain)".to_string()]).is_err());
}

#[cfg(feature = "app")]
#[test]
fn build_multi_rejects_malformed_platform_configuration_before_building() {
    for source in [
        "[app]\nlanguage = \"multi\"\nplatforms = \"ios\"\n",
        "[app]\nlanguage = \"multi\"\nplatforms = []\n",
        "[app]\nlanguage = \"multi\"\nplatforms = [\"ios\", 7]\n",
        "[app]\nlanguage = \"multi\"\nplatforms = [\"ios\", \"ios\"]\n",
        "[app]\nlanguage = \"multi\"\nplatforms = [\"ios\", \"macos\"]\n",
    ] {
        let config: toml::Value = toml::from_str(source).unwrap();
        assert!(super::resolve_multi_platforms(&config).is_err(), "{source}");
    }
}

#[cfg(feature = "app")]
#[test]
fn build_multi_defaults_only_when_platforms_are_omitted() {
    let config: toml::Value = toml::from_str("[app]\nlanguage = \"multi\"\n").unwrap();
    assert_eq!(
        super::resolve_multi_platforms(&config).unwrap(),
        ["android", "ios", "react-native", "flutter"]
    );
}

#[cfg(feature = "app")]
#[test]
fn build_multi_preserves_explicit_platform_selection_and_order() {
    let config: toml::Value =
        toml::from_str("[app]\nlanguage = \"multi\"\nplatforms = [\"flutter\", \"ios\"]\n")
            .unwrap();
    assert_eq!(
        super::resolve_multi_platforms(&config).unwrap(),
        ["flutter", "ios"]
    );
}

#[cfg(feature = "app")]
#[test]
fn app_build_does_not_guess_flutter_when_project_is_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    assert!(super::resolve_app_language(dir.path()).is_err());
}

#[cfg(feature = "app")]
#[test]
fn app_build_rejects_malformed_project_config_instead_of_falling_back() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mgc.toml"), "[app\nlanguage = flutter").unwrap();
    assert!(super::resolve_app_language(dir.path()).is_err());
}

#[cfg(feature = "app")]
#[test]
fn app_build_rejects_unknown_explicit_language() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("mgc.toml"),
        "[app]\nlanguage = \"react-native\"\n",
    )
    .unwrap();
    assert!(super::resolve_app_language(dir.path()).is_err());
}

#[cfg(feature = "app")]
#[test]
fn app_build_infers_groovy_gradle_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("build.gradle"), "plugins {}\n").unwrap();
    assert_eq!(super::resolve_app_language(dir.path()).unwrap(), "kotlin");
}

#[cfg(feature = "app")]
#[test]
fn app_build_infers_swift_manifest() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Package.swift"),
        "// swift-tools-version: 5.9\n",
    )
    .unwrap();
    assert_eq!(super::resolve_app_language(dir.path()).unwrap(), "swift");
}

#[cfg(feature = "app")]
#[test]
fn app_build_rejects_ambiguous_runtime_markers() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Package.swift"),
        "// swift-tools-version: 5.9\n",
    )
    .unwrap();
    fs::write(dir.path().join("build.gradle"), "plugins {}\n").unwrap();
    assert!(super::resolve_app_language(dir.path()).is_err());
}

#[cfg(feature = "app")]
#[test]
fn app_build_rejects_non_string_language() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mgc.toml"), "[app]\nlanguage = 42\n").unwrap();
    assert!(super::resolve_app_language(dir.path()).is_err());
}

#[cfg(all(unix, feature = "app"))]
#[test]
fn app_build_rejects_symlinked_config_and_framework_manifests() {
    let project = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();

    fs::write(
        external.path().join("mgc.toml"),
        "[app]\nlanguage = 'flutter'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("mgc.toml"),
        project.path().join("mgc.toml"),
    )
    .unwrap();
    assert!(super::resolve_app_language(project.path()).is_err());

    fs::remove_file(project.path().join("mgc.toml")).unwrap();
    fs::write(
        external.path().join("Package.swift"),
        "// swift-tools-version: 5.9\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        external.path().join("Package.swift"),
        project.path().join("Package.swift"),
    )
    .unwrap();
    assert!(super::resolve_app_language(project.path()).is_err());
}

#[cfg(feature = "clo")]
#[test]
fn cdk_build_recognizes_windows_cmd_shim_as_installed() {
    let root = tempfile::tempdir().unwrap();
    let bin_dir = root.path().join("node_modules").join(".bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("cdk.cmd"), "@echo off\r\n").unwrap();

    assert!(super::cloud_local_executable_exists(
        root.path(),
        "cdk",
        true
    ));
    assert!(!super::cloud_local_executable_exists(
        root.path(),
        "cdk",
        false
    ));
}

#[cfg(feature = "clo")]
#[test]
fn cdk_synth_disables_background_telemetry() {
    let local_bin = Path::new("/lane/project/node_modules/.bin");
    let env = super::cloud_cdk_synth_env(local_bin).unwrap();

    assert_eq!(
        env.iter()
            .find(|(key, _)| key == "CDK_DISABLE_CLI_TELEMETRY")
            .map(|(_, value)| value.as_str()),
        Some("true")
    );
    assert_eq!(
        env.iter()
            .find(|(key, _)| key == "PATH")
            .and_then(|(_, value)| std::env::split_paths(value).next()),
        Some(local_bin.to_path_buf())
    );
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
fn iot_build_refuses_platformio_without_native_backend() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("platformio.ini"),
        "[env:esp32dev]\nplatform = espressif32\n",
    )
    .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let error = rt
        .block_on(super::build_iot(project.path()))
        .expect_err("PlatformIO must not be invoked as a build backend");
    assert!(
        error
            .to_string()
            .contains("native PlatformIO build backend")
    );
}

#[cfg(feature = "hardware")]
#[test]
fn hardware_build_refuses_platformio_without_native_backend() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("platformio.ini"),
        "[env:board]\nplatform = native\n",
    )
    .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let error = rt
        .block_on(super::build_hardware(project.path()))
        .expect_err("PlatformIO must not be invoked as a build backend");
    assert!(
        error
            .to_string()
            .contains("native PlatformIO build backend")
    );
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

#[cfg(feature = "lib")]
#[test]
fn lib_java_maven_build_does_not_fall_through_to_typescript() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("pom.xml"),
        "<project><modelVersion>4.0.0</modelVersion></project>",
    )
    .unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let error = rt
        .block_on(super::build_lib(tmp.path()))
        .expect_err("Java Maven build must not be mistaken for TypeScript");
    let message = error.to_string();
    assert!(
        message.contains("Java build descriptor"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("tsc"),
        "unexpected TypeScript route: {message}"
    );
    assert!(
        message.contains("refusing to route") && message.contains("Maven/Gradle"),
        "the error must state the fail-closed boundary: {message}"
    );
}

/// ng must run WITHOUT --preserve-symlinks: the flags break beasties
/// (critical-CSS inliner) DOM-module identity on mgc's store-backed tree
/// (`document.documentElement?.setAttribute is not a function`), while the
/// plain node invocation builds fine. RED-first for the ng opt-out.
#[test]
fn framework_build_script_maps_ng_without_preserve_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let bin_dir = dir.path().join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("ng"), "").unwrap();

    let ng = map_framework_build_script(dir.path(), &["ng", "build"])
        .unwrap()
        .unwrap();
    assert_eq!(ng.0, Path::new("node"));
    let args =
        ng.1.iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
    assert!(
        !args
            .iter()
            .any(|a| a == "--preserve-symlinks" || a == "--preserve-symlinks-main"),
        "ng must not carry preserve-symlinks flags, got: {args:?}"
    );
    assert!(args.iter().any(|a| a.contains("node_modules")));
    assert_eq!(args.last().unwrap(), "build");
}

#[test]
fn framework_build_chain_keeps_typecheck_separate_and_rejects_shell_controls() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("vite"), "").unwrap();
    fs::write(bin.join("tsc"), "").unwrap();
    fs::write(bin.join("vue-tsc"), "").unwrap();
    let compat = crate::commands::compat::CompatMode::Native;
    let chain = map_framework_build_chain(dir.path(), "vite build && tsc --noEmit", &compat)
        .unwrap()
        .unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].1.last().unwrap(), "build");
    assert_eq!(chain[1].1.last().unwrap(), "--noEmit");
    let vue = map_framework_build_chain(dir.path(), "vue-tsc --noEmit && vite build", &compat)
        .unwrap()
        .unwrap();
    assert_eq!(vue.len(), 2);
    for script in [
        r#"vite build --base "dist&&preview""#,
        r"vite build --base dist\&\&preview",
    ] {
        let quoted = map_framework_build_chain(dir.path(), script, &compat)
            .unwrap()
            .unwrap();
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].1.last().unwrap(), "dist&&preview");
    }

    for script in [
        "vite build &&",
        "vite build || tsc",
        "vite build; tsc",
        "vite build && unknown",
        "vite build && npm install",
    ] {
        assert!(
            map_framework_build_chain(dir.path(), script, &compat).is_err(),
            "unsafe or unsupported build chain accepted: {script}"
        );
    }
}
