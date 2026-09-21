//! `install/mod.rs` — App adapter install orchestrator.
//! Orchestrates install across Flutter (pub get), Kotlin (gradle), Swift (spm), React Native (npm).

pub mod fetch;
pub mod verify;

use mgc_resolver::protocols::{
    PubProtocol, RegistryProtocol, ResolvedEntry, SwiftRegistryProtocol,
};
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
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<InstallSummary> {
    match language {
        // Flutter: native pub.dev engine (Phase 2) — no `flutter pub get`
        // spawn for resolve/fetch/install.
        AppLanguage::Flutter => {
            let summary = install_flutter_native(graph).await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }

        // Kotlin/Android: gradle sync
        AppLanguage::Kotlin => install_kotlin(project_root, opts).await,

        // Swift: native SwiftPM engine (Phase 2) — registry zips verified
        // against the registry checksum, git-only deps cloned at the pinned
        // SHA, checkouts materialized under the mgc Swift root and a
        // SwiftPM-compatible Package.resolved exported into the project.
        // (Swift: engine SwiftPM native (Phase 2) — zip registry xác minh
        // theo checksum registry, dep chỉ-git clone tại SHA đã ghim,
        // checkouts materialize dưới gốc Swift của mgc và Package.resolved
        // tương thích SwiftPM được xuất vào project.)
        AppLanguage::Swift => {
            let summary = install_swift_native(graph, &lock_packages, project_root).await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }

        // React Native: layered install — Android pins through the native
        // Maven lane, the JS tier delegates to the web adapter's npm
        // pipeline, iOS pods were checksum-verified at resolve time.
        // (React Native: install phân tầng — pin Android qua lane Maven
        // native, tier JS ủy quyền cho pipeline npm của adapter web, pod
        // iOS đã xác minh checksum lúc resolve.)
        AppLanguage::ReactNative => {
            let summary = crate::native::rn_layers::install_rn_layers(
                graph,
                &lock_packages,
                project_root,
                opts,
            )
            .await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }

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

/// Native Swift install: registry entries download → verify the registry
/// checksum → import to the mgc CAS (blake3) → extract into
/// `{swift_root}/checkouts/{identity}-{version}/`; GIT entries clone at the
/// pinned tag with commit-SHA verification (a moved tag fails closed) —
/// checkouts are directories, so no CAS archive import applies to them.
/// A SwiftPM-compatible Package.resolved (v2) is exported into the project.
/// No `swift package resolve` spawn — mgc owns resolve/fetch/install
/// (Phase 2).
/// Install Swift native: entry registry tải → verify checksum registry →
/// import vào CAS mgc (blake3) → giải nén vào
/// `{swift_root}/checkouts/{identity}-{version}/`; entry GIT clone tại tag
/// đã ghim với xác minh SHA commit (tag bị dịch fail-closed) — checkout là
/// thư mục nên không áp import CAS archive. Package.resolved (v2) tương
/// thích SwiftPM được xuất vào project. Không spawn `swift package resolve`
/// — mgc giữ resolve/fetch/install (Phase 2).
async fn install_swift_native(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
    project_root: &Path,
) -> MgResult<InstallSummary> {
    let started = std::time::Instant::now();
    let protocol = SwiftRegistryProtocol::from_env();
    let swift_root = swift_store_root()?;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
        let is_git = !pkg.tarball_url.ends_with(".zip");
        if is_git {
            // Resolve-time provenance SHA rides the lock markers — an
            // absent lock (fresh adapter instance) degrades to an UNVERIFIED
            // checkout with a loud warning (no silent trust).
            // (SHA provenance lúc resolve nằm trong marker lock — thiếu
            // lock (instance adapter mới) hạ xuống checkout CHƯA XÁC MINH
            // kèm cảnh báo ồn ào (không tin tưởng âm thầm).)
            let expected = lock_packages
                .iter()
                .find(|p| p.name == pkg.id.name_str())
                .and_then(|p| p.markers.as_ref())
                .and_then(|ms| {
                    ms.iter()
                        .find_map(|m| m.strip_prefix("git-commit:").map(str::to_string))
                });
            protocol.materialize_git(&entry, expected.as_deref(), &swift_root)?;
        } else {
            let bytes = protocol.download(&entry).await?;
            protocol.verify(&entry, &bytes)?;
            store
                .import_bytes(&bytes)
                .map_err(|e| MgError::Store(e.to_string()))?;
            protocol.materialize(&entry, &bytes, &swift_root)?;
        }
        added.push(pkg.id.clone());
    }

    // Export the SwiftPM-compatible Package.resolved from the resolution.
    // (Xuất Package.resolved tương thích SwiftPM từ kết quả resolve.)
    let pins = build_swift_pins(graph, lock_packages);
    protocol.export_package_resolved(&pins, project_root)?;

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Package.resolved pins from the resolved graph: registry entries key the
/// identity as `scope.name`; git entries use the repo name with the commit
/// SHA as revision provenance (from the lock markers when present).
/// Pin Package.resolved từ graph đã resolve: entry registry khóa identity
/// dạng `scope.name`; entry git dùng tên repo với SHA commit làm provenance
/// revision (từ marker lock khi có).
fn build_swift_pins(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
) -> Vec<mgc_resolver::protocols::swift::SwiftResolvedPin> {
    graph
        .packages
        .iter()
        .map(|pkg| {
            let name = pkg.id.name_str();
            let is_git = !pkg.tarball_url.ends_with(".zip");
            if is_git {
                let revision = lock_packages
                    .iter()
                    .find(|p| p.name == name)
                    .and_then(|p| p.markers.as_ref())
                    .and_then(|ms| {
                        ms.iter()
                            .find_map(|m| m.strip_prefix("git-commit:").map(str::to_string))
                    });
                let version = pkg.id.version();
                // A tag-shaped version pins `state.version`; branch/SHA
                // pins carry the revision only.
                // (Version dạng tag ghim `state.version`; pin branch/SHA
                // chỉ mang revision.)
                mgc_resolver::protocols::swift::SwiftResolvedPin {
                    identity: name.rsplit('/').next().unwrap_or(name).to_lowercase(),
                    version: mgc_types::Version::parse(&version.to_string())
                        .ok()
                        .map(|_| version.to_string()),
                    revision,
                    branch: None,
                    location: Some(format!("https://{}.git", name)),
                }
            } else {
                mgc_resolver::protocols::swift::SwiftResolvedPin {
                    identity: name.replace('/', ".").to_lowercase(),
                    version: Some(pkg.id.version().to_string()),
                    revision: None,
                    branch: None,
                    location: None,
                }
            }
        })
        .collect()
}

/// Resolve (and create) the mgc-managed Swift store root
/// (`~/.magicore/store/swift`) — checkouts live under `<root>/checkouts`.
/// `MGC_SWIFT_STORE_ROOT` overrides the root (testability + shared-cache
/// placement), mirroring the other engines' env seams.
/// Resolve (và tạo) gốc store Swift do mgc quản (`~/.magicore/store/swift`)
/// — checkouts nằm dưới `<root>/checkouts`. `MGC_SWIFT_STORE_ROOT` ghi đè
/// gốc (khả nghiệm + vị trí cache dùng chung), giống các seam env của
/// engine khác.
fn swift_store_root() -> MgResult<PathBuf> {
    let root = match std::env::var("MGC_SWIFT_STORE_ROOT")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        Some(over) => PathBuf::from(over),
        None => {
            let home =
                dirs::home_dir().ok_or_else(|| MgError::Other("no home directory".to_string()))?;
            home.join(".magicore").join("store").join("swift")
        }
    };
    std::fs::create_dir_all(&root)
        .map_err(|e| MgError::Other(format!("create swift store root: {e}")))?;
    Ok(root)
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

    // Try Swift (iOS) — native SwiftPM engine (Phase 2).
    // (Thử Swift (iOS) — engine SwiftPM native (Phase 2).)
    if project_root.join("Package.swift").exists() {
        return install_swift_native(graph, &[], project_root).await;
    }

    Err(MgError::Other(
        "multi-platform project: no recognized manifest found".to_string(),
    ))
}

/// Flush the native-resolve entries into the canonical v3 mgc.lock
/// (merge-by-identity — same contract as the lib adapter lane). Nothing
/// resolved natively → the existing lock stays untouched.
/// Ghi các entry resolve-native vào mgc.lock v3 chuẩn (merge theo danh
/// tính — cùng hợp đồng với lane lib adapter). Không resolve gì native →
/// lock có sẵn giữ nguyên.
fn write_canonical_lock(
    project_root: &Path,
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<()> {
    if lock_packages.is_empty() {
        return Ok(());
    }
    let lock_path = project_root.join("mgc.lock");
    let mut lockfile = if lock_path.exists() {
        mgc_lockfile::parser::parse_lockfile(
            &std::fs::read_to_string(&lock_path)
                .map_err(|e| MgError::Other(format!("failed to read existing mgc.lock: {e}")))?,
        )
        .unwrap_or_else(|_| mgc_lockfile::Lockfile::new())
    } else {
        mgc_lockfile::Lockfile::new()
    };
    for pkg in lock_packages {
        // Replace same name+version entry, keep others — merge-by-identity.
        // (Thay entry cùng name+version, giữ entry khác — merge theo danh
        // tính.)
        lockfile
            .packages
            .retain(|p| !(p.name == pkg.name && p.version == pkg.version));
        lockfile.packages.push(pkg);
    }
    lockfile.metadata.generated_at = {
        // ISO-8601 without external deps — seconds precision is enough for
        // the metadata field. (ISO-8601 không cần crate ngoài — độ chính
        // xác giây đủ cho trường metadata.)
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{secs}")
    };
    lockfile.metadata.generator = format!("mgc/{}", env!("CARGO_PKG_VERSION"));
    let toml = mgc_lockfile::writer::serialize_lockfile(&lockfile)
        .map_err(|e| MgError::Other(format!("lockfile serialization failed: {e}")))?;
    std::fs::write(&lock_path, toml.as_bytes())
        .map_err(|e| MgError::Other(format!("failed to write mgc.lock: {e}")))?;
    Ok(())
}
