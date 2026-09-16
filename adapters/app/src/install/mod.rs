//! `install/mod.rs` — App adapter install orchestrator.
//! Orchestrates install across Flutter (pub get), Kotlin (gradle), Swift (spm), React Native (npm).

pub mod fetch;
pub mod verify;

use mgc_resolver::protocols::{PubProtocol, RegistryProtocol, ResolvedEntry};
use mgc_store::ContentStore;
use mgc_types::adapter::{InstallCacheMode, InstallOptions, InstallSummary};
use mgc_types::{MgError, MgResult, ResolvedGraph, ResolvedPackage};
use std::path::{Path, PathBuf};

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
        // Flutter: native pub.dev engine (Phase 2) — no `flutter pub get`
        // spawn for resolve/fetch/install.
        AppLanguage::Flutter => install_flutter_native(graph).await,

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

/// Native Flutter install: download each package archive → verify sha256 →
/// import to the mgc CAS (blake3) → extract into the pub cache layout
/// `{store}/pub/hosted/pub.dev/{name}-{version}/` (usable by
/// `dart pub get --offline`). No `flutter pub get` spawn — mgc owns
/// resolve/fetch/install (Phase 2).
/// Install Flutter native: tải archive từng package → verify sha256 → import
/// vào CAS mgc (blake3) → giải nén vào layout pub cache
/// `{store}/pub/hosted/pub.dev/{name}-{version}/` (dùng được bởi
/// `dart pub get --offline`). Không spawn `flutter pub get` — mgc giữ
/// resolve/fetch/install (Phase 2).
async fn install_flutter_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = std::time::Instant::now();
    let protocol = PubProtocol::from_env();
    let pub_cache = pub_cache_root()?;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
        let bytes = protocol.download(&entry).await?;
        protocol.verify(&entry, &bytes)?;
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &bytes, &pub_cache)?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Reconstruct a protocol `ResolvedEntry` from a resolved graph package.
/// Dựng lại `ResolvedEntry` của protocol từ một package trong graph đã resolve.
fn entry_from_package(pkg: &ResolvedPackage) -> ResolvedEntry {
    ResolvedEntry {
        name: pkg.id.name_str().to_string(),
        version: pkg.id.version().to_string(),
        deps: Vec::new(),
        artifact_url: pkg.tarball_url.clone(),
        sha256: pkg
            .integrity
            .strip_prefix("sha256-")
            .unwrap_or("")
            .to_string(),
        extra_markers: Vec::new(),
    }
}

/// Resolve (and create) the mgc-managed pub cache root (`~/.magicore/store/pub`).
/// Resolve (và tạo) gốc pub cache do mgc quản (`~/.magicore/store/pub`).
fn pub_cache_root() -> MgResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| MgError::Other("no home directory".to_string()))?;
    let root = home.join(".magicore").join("store").join("pub");
    std::fs::create_dir_all(&root).map_err(|e| MgError::Other(format!("create pub cache: {e}")))?;
    Ok(root)
}

/// Build (and create) the mgc CAS content store used by native install.
/// Dựng (và tạo) content store CAS mgc cho install native.
fn content_store() -> MgResult<ContentStore> {
    ContentStore::new(mgc_store::default_store_root()).map_err(|e| MgError::Store(e.to_string()))
}

/// Install Kotlin/Android dependencies via `gradle`.
///
/// DELEGATED: gradle/gradlew runs for real — mgc orchestrates only and
/// does not own this dependency lifecycle.
/// (DELEGATED: gradle/gradlew chạy thật — mgc chỉ điều phối và không sở
/// hữu lifecycle dependency này.)
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

    // Issue #13: parse gradle.lockfile / build.gradle
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        // P0-6: native toolchain cache owns the bytes (delegation).
        // P0-6: cache toolchain gốc giữ byte (ủy quyền).
        cache_mode: InstallCacheMode::Delegated,
        duration_ms: result.duration_ms,
    })
}

/// Install Swift dependencies via `swift package resolve`.
///
/// DELEGATED: `swift package resolve` runs for real — mgc orchestrates
/// only and does not own this dependency lifecycle.
/// (DELEGATED: `swift package resolve` chạy thật — mgc chỉ điều phối và
/// không sở hữu lifecycle dependency này.)
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

    // Issue #13: parse Package.resolved
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        // P0-6: native toolchain cache owns the bytes (delegation).
        // P0-6: cache toolchain gốc giữ byte (ủy quyền).
        cache_mode: InstallCacheMode::Delegated,
        duration_ms: result.duration_ms,
    })
}

/// Install ObjC dependencies via `pod install`.
///
/// DELEGATED: `pod install` runs for real — mgc orchestrates only and
/// does not own this dependency lifecycle.
/// (DELEGATED: `pod install` chạy thật — mgc chỉ điều phối và không sở
/// hữu lifecycle dependency này.)
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

    // Issue #13: parse Podfile.lock
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        // P0-6: native toolchain cache owns the bytes (delegation).
        // P0-6: cache toolchain gốc giữ byte (ủy quyền).
        cache_mode: InstallCacheMode::Delegated,
        duration_ms: result.duration_ms,
    })
}

/// Install multi-platform dependencies (detect primary platform).
async fn install_multi(
    project_root: &Path,
    opts: InstallOptions,
    graph: &ResolvedGraph,
) -> MgResult<InstallSummary> {
    // Try Flutter first (common multi-platform framework)
    if project_root.join("pubspec.yaml").exists() {
        return install_flutter_native(graph).await;
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
