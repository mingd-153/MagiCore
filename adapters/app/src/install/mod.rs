//! `install/mod.rs` — App adapter install orchestrator.
//! Orchestrates install across Flutter (pub get), Kotlin (gradle), Swift (spm), React Native (npm).

pub mod fetch;
pub mod verify;

use mgc_store::ContentStore;
use mgc_types::adapter::{InstallOptions, InstallSummary};
use mgc_types::{MgError, MgResult, ResolvedGraph};
use std::path::Path;

use crate::language::AppLanguage;

/// Install orchestrator for app adapter.
/// Điều phối install cho Flutter/Kotlin/Swift/ReactNative app projects.
pub async fn run_install(
    language: AppLanguage,
    graph: &ResolvedGraph,
    project_root: &Path,
    opts: InstallOptions,
    _store: Option<&ContentStore>,
) -> MgResult<InstallSummary> {
    match language {
        // Flutter: pub get/upgrade
        AppLanguage::Flutter => install_flutter(project_root, opts).await,

        // Kotlin/Android: gradle sync
        AppLanguage::Kotlin => install_kotlin(project_root, opts).await,

        // Swift/iOS: swift package resolve
        AppLanguage::Swift => install_swift(project_root, opts).await,

        // React Native: delegate to web adapter (npm)
        AppLanguage::ReactNative => Err(MgError::Other(
            "React Native should delegate to web adapter for npm dependencies".to_string(),
        )),

        // ObjC: CocoaPods pod install
        AppLanguage::ObjC => install_objc(project_root, opts).await,

        // Multi-platform: detect primary and install
        AppLanguage::Multi => install_multi(project_root, opts, graph).await,
    }
}

/// Install Flutter dependencies via `flutter pub get`.
async fn install_flutter(project_root: &Path, opts: InstallOptions) -> MgResult<InstallSummary> {
    let mut args = vec!["pub".to_string(), "get".to_string()];

    if opts.frozen {
        args.push("--offline".to_string());
    }

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("flutter", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("flutter pub get failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "flutter pub get exited with code {}",
            result.exit_code
        )));
    }

    // TODO: parse pubspec.lock to build InstallSummary
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: result.duration_ms,
    })
}

/// Install Kotlin/Android dependencies via `gradle`.
async fn install_kotlin(project_root: &Path, opts: InstallOptions) -> MgResult<InstallSummary> {
    // Use gradlew if available, fallback to gradle
    let tool = if project_root.join("gradlew").exists() {
        "./gradlew"
    } else {
        "gradle"
    };

    let mut args = vec!["dependencies".to_string()];

    if opts.frozen {
        args.push("--offline".to_string());
    }

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run(tool, &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("gradle dependencies failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "gradle exited with code {}",
            result.exit_code
        )));
    }

    // TODO: parse gradle.lockfile / build.gradle
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: result.duration_ms,
    })
}

/// Install Swift dependencies via `swift package resolve`.
async fn install_swift(project_root: &Path, _opts: InstallOptions) -> MgResult<InstallSummary> {
    let args = vec!["package".to_string(), "resolve".to_string()];

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("swift", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("swift package resolve failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "swift package resolve exited with code {}",
            result.exit_code
        )));
    }

    // TODO: parse Package.resolved
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: result.duration_ms,
    })
}

/// Install ObjC dependencies via `pod install`.
async fn install_objc(project_root: &Path, opts: InstallOptions) -> MgResult<InstallSummary> {
    let mut args = vec!["install".to_string()];

    if opts.frozen {
        // CocoaPods uses Podfile.lock
        args.push("--deployment".to_string());
    }

    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("pod", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("pod install failed: {}", e)))?;

    if result.exit_code != 0 {
        return Err(MgError::Other(format!(
            "pod install exited with code {}",
            result.exit_code
        )));
    }

    // TODO: parse Podfile.lock
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: result.duration_ms,
    })
}

/// Install multi-platform dependencies (detect primary platform).
async fn install_multi(
    project_root: &Path,
    opts: InstallOptions,
    _graph: &ResolvedGraph,
) -> MgResult<InstallSummary> {
    // Try Flutter first (common multi-platform framework)
    if project_root.join("pubspec.yaml").exists() {
        return install_flutter(project_root, opts).await;
    }

    // Try Kotlin (Android)
    if project_root.join("build.gradle").exists() || project_root.join("build.gradle.kts").exists()
    {
        return install_kotlin(project_root, opts).await;
    }

    // Try Swift (iOS)
    if project_root.join("Package.swift").exists() {
        return install_swift(project_root, opts).await;
    }

    Err(MgError::Other(
        "multi-platform project: no recognized manifest found".to_string(),
    ))
}
