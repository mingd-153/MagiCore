//! `install/mod.rs` — App adapter install orchestrator.
//! Installs only ecosystems whose dependency lifecycle is implemented natively.

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
    _opts: InstallOptions,
    _store: Option<&ContentStore>,
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<InstallSummary> {
    if !matches!(
        language,
        AppLanguage::Flutter | AppLanguage::Swift | AppLanguage::ReactNative
    ) {
        return Err(MgError::Unsupported {
            core: "app",
            capability: "install",
            guidance: "MagiCore has no complete native dependency engine for this app ecosystem"
                .to_string(),
        });
    }
    match language {
        // Flutter: native pub.dev engine (Phase 2) — no `flutter pub get`
        // spawn for resolve/fetch/install.
        AppLanguage::Flutter => {
            let summary = install_flutter_native(graph, project_root).await?;
            write_canonical_lock(
                project_root,
                &[mgc_lockfile::EcosystemTag::Dart],
                lock_packages,
            )?;
            Ok(summary)
        }

        AppLanguage::Kotlin => Err(MgError::Unsupported {
            core: "app",
            capability: "native Kotlin dependency install",
            guidance: "MagiCore does not yet implement native Kotlin dependency resolution and materialization".to_string(),
        }),

        // Swift registry archives are verified and materialized by MGC;
        // Git-source dependencies are rejected by the resolver.
        // (MGC xác minh và materialize archive Swift registry; dependency Git
        // bị resolver từ chối.)
        AppLanguage::Swift => {
            let summary = install_swift_native(graph, &lock_packages, project_root).await?;
            write_canonical_lock(
                project_root,
                &[mgc_lockfile::EcosystemTag::Swift],
                lock_packages,
            )?;
            Ok(summary)
        }

        // RN spans JS, Android, and iOS package graphs. Until all three
        // graphs are owned and committed atomically, fail instead of
        // reporting a partially installed project as successful.
        AppLanguage::ReactNative => Err(MgError::Unsupported {
            core: "app",
            capability: "complete native React Native dependency install",
            guidance: "MagiCore requires native JS, Maven, and CocoaPods engines plus one atomic cross-ecosystem transaction".to_string(),
        }),

        AppLanguage::ObjC | AppLanguage::Multi => Err(MgError::Unsupported {
            core: "app",
            capability: "native dependency install",
            guidance: "MagiCore does not yet implement a native dependency engine for this app ecosystem".to_string(),
        }),
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
async fn install_flutter_native(
    graph: &ResolvedGraph,
    project_root: &Path,
) -> MgResult<InstallSummary> {
    let started = std::time::Instant::now();
    let protocol = PubProtocol::from_env();
    let pub_cache = pub_cache_root()?;
    // An empty resolved graph has no artifacts to fetch or persist. Avoid
    // opening/creating the global CAS in that no-op case (which can fail on
    // read-only or sandboxed home directories despite there being no work).
    // (Graph rỗng không có artifact; không mở CAS toàn cục vô ích.)
    let store = if graph.packages.is_empty() {
        None
    } else {
        Some(content_store()?)
    };
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
        let bytes = protocol.download(&entry).await?;
        protocol.verify(&entry, &bytes)?;
        let store = store.as_ref().ok_or_else(|| {
            MgError::Store("non-empty Flutter graph has no initialized content store".to_string())
        })?;
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &bytes, &pub_cache)?;
        added.push(pkg.id.clone());
    }

    // Flutter commands are invoked with --no-pub, so MGC must provide the
    // package configuration normally generated by `pub get`. Without it,
    // install could report success while build/test/dev cannot resolve
    // `package:` imports.
    write_flutter_package_config(graph, project_root, &pub_cache)?;

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FlutterPackageConfig {
    config_version: u8,
    generator: &'static str,
    generator_version: &'static str,
    packages: Vec<FlutterPackageConfigEntry>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FlutterPackageConfigEntry {
    name: String,
    root_uri: String,
    package_uri: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_version: Option<String>,
}

fn write_flutter_package_config(
    graph: &ResolvedGraph,
    project_root: &Path,
    pub_cache: &Path,
) -> MgResult<()> {
    let config_dir = project_root.join(".dart_tool");
    match std::fs::symlink_metadata(&config_dir) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(MgError::Integrity(
                "refusing to write Flutter package config through a linked or non-directory .dart_tool".to_string(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&config_dir)
                .map_err(|e| MgError::Other(format!("create .dart_tool directory: {e}")))?;
        }
        Err(error) => {
            return Err(MgError::Other(format!(
                "inspect .dart_tool directory: {error}"
            )));
        }
    }

    let mut entries = Vec::with_capacity(graph.packages.len());
    let mut seen = std::collections::BTreeSet::new();
    for package in &graph.packages {
        let name = package.id.name_str();
        if !seen.insert(name.to_string()) {
            return Err(MgError::Integrity(format!(
                "Flutter graph contains duplicate package name '{name}'"
            )));
        }
        let package_root = pub_cache.join("hosted").join("pub.dev").join(format!(
            "{}-{}",
            name,
            package.id.version()
        ));
        let manifest_path = package_root.join("pubspec.yaml");
        let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|error| {
            MgError::Other(format!(
                "read materialized Flutter package manifest '{}': {error}",
                manifest_path.display()
            ))
        })?;
        let manifest: serde_yaml::Value =
            serde_yaml::from_str(&manifest_text).map_err(|error| {
                MgError::Other(format!(
                    "parse materialized Flutter package manifest '{}': {error}",
                    manifest_path.display()
                ))
            })?;
        let actual_name = manifest
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .ok_or_else(|| {
                MgError::Integrity(format!(
                    "materialized Flutter package '{}' has no valid pubspec name",
                    package_root.display()
                ))
            })?;
        if actual_name != name {
            return Err(MgError::Integrity(format!(
                "Flutter archive identity mismatch: graph says '{name}', archive says '{actual_name}'"
            )));
        }
        let language_version = manifest
            .get("environment")
            .and_then(|value| value.get("sdk"))
            .and_then(serde_yaml::Value::as_str)
            .and_then(dart_language_version_floor);
        let canonical_root = package_root.canonicalize().map_err(|error| {
            MgError::Other(format!("canonicalize Flutter package root: {error}"))
        })?;
        let root_uri = url::Url::from_directory_path(&canonical_root)
            .map_err(|_| MgError::Other("cannot encode Flutter package root URI".to_string()))?
            .to_string();
        entries.push(FlutterPackageConfigEntry {
            name: name.to_string(),
            root_uri,
            package_uri: "lib/",
            language_version,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    let config = FlutterPackageConfig {
        config_version: 2,
        generator: "MagiCore",
        generator_version: env!("CARGO_PKG_VERSION"),
        packages: entries,
    };
    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|error| MgError::Other(format!("serialize Flutter package config: {error}")))?;
    atomic_write_project_file(
        &config_dir.join("package_config.json"),
        &bytes,
        "Flutter package config",
    )
}

/// Extract the Dart language version floor from constraints with an explicit
/// lower bound. Constraints without a safely understood floor leave this
/// optional field absent rather than inventing a language version.
fn dart_language_version_floor(constraint: &str) -> Option<String> {
    for token in constraint.split_whitespace() {
        let candidate = token
            .strip_prefix(">=")
            .or_else(|| token.strip_prefix('^'))
            .or_else(|| token.strip_prefix('='));
        let Some(candidate) = candidate else { continue };
        let mut parts = candidate.trim().split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next()?.parse::<u64>().ok()?;
        if major > 0 || minor > 0 {
            return Some(format!("{major}.{minor}"));
        }
    }
    None
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

/// Replace the lock entries owned by this app install lane. Empty results
/// clear stale entries for the lane; entries from every other ecosystem
/// remain untouched (RN JS is written by the web adapter).
/// Thay các entry thuộc lane app đang cài. Kết quả rỗng xóa pin cũ của
/// lane; ecosystem khác được giữ nguyên (RN JS do web adapter ghi).
fn write_canonical_lock(
    project_root: &Path,
    replace_ecosystems: &[mgc_lockfile::EcosystemTag],
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<()> {
    let lock_path = project_root.join("mgc.lock");
    if replace_ecosystems.is_empty() {
        return Err(MgError::Other(
            "refusing to write mgc.lock without an owned ecosystem scope".to_string(),
        ));
    }
    if lock_packages
        .iter()
        .any(|package| !replace_ecosystems.contains(&package.ecosystem))
    {
        return Err(MgError::Other(
            "refusing to write an app lock entry outside the active ecosystem scope".to_string(),
        ));
    }

    let owner_core = app_lock_owner_core(project_root)?;
    let mut lockfile = match std::fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let content = std::fs::read_to_string(&lock_path)
                .map_err(|e| MgError::Other(format!("failed to read existing mgc.lock: {e}")))?;
            mgc_lockfile::parser::parse_lockfile(&content).map_err(|e| {
                MgError::Other(format!(
                    "refusing to replace invalid existing mgc.lock: {e}"
                ))
            })?
        }
        Ok(_) => {
            return Err(MgError::Other(
                "refusing to replace non-regular mgc.lock".to_string(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if lock_packages.is_empty() {
                return Ok(());
            }
            mgc_lockfile::Lockfile::new()
        }
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot inspect existing mgc.lock: {error}"
            )));
        }
    };
    let original_packages = lockfile.packages.clone();
    let original_version = lockfile.version.clone();

    // One ecosystem can be used by multiple cores in a polyglot project
    // (for example Dart by App and Python by AI after a future schema map).
    // Replace only entries owned by this project core; never erase a sibling
    // core's pins merely because the ecosystem tag overlaps.
    // (Chỉ thay entry của core hiện tại; không xóa pin của core khác do trùng ecosystem.)
    for ecosystem in replace_ecosystems {
        let competing_owner_exists = lockfile.packages.iter().any(|package| {
            package.ecosystem == *ecosystem
                && package
                    .owner_core
                    .as_deref()
                    .is_some_and(|owner| owner != owner_core)
        });
        if !competing_owner_exists {
            for package in &mut lockfile.packages {
                if package.ecosystem == *ecosystem && package.owner_core.is_none() {
                    package.owner_core = Some(owner_core.clone());
                }
            }
        }
    }
    let mut owned_packages = lock_packages;
    for package in &mut owned_packages {
        package.owner_core = Some(owner_core.clone());
    }
    lockfile.packages.retain(|package| {
        !replace_ecosystems.contains(&package.ecosystem)
            || package.owner_core.as_deref() != Some(owner_core.as_str())
    });
    lockfile.packages.extend(owned_packages);
    if lockfile.packages == original_packages
        && original_version == mgc_lockfile::LOCKFILE_SCHEMA_VERSION
    {
        return Ok(());
    }
    mgc_lockfile::ensure_lockfile_mutation_allowed(&lock_path)
        .map_err(|error| MgError::Other(error.to_string()))?;
    lockfile.metadata.generated_at = chrono::Utc::now().to_rfc3339();
    lockfile.metadata.generator = format!("mgc/{}", env!("CARGO_PKG_VERSION"));
    let toml = mgc_lockfile::writer::serialize_lockfile(&lockfile)
        .map_err(|e| MgError::Other(format!("lockfile serialization failed: {e}")))?;
    atomic_write_canonical_lock(&lock_path, toml.as_bytes())
}

/// Resolve the app install lane's core identity from the project marker/config.
/// Missing identity is only tolerated for legacy direct adapter callers and
/// defaults to `app`; malformed or unknown identity fails closed.
/// (Đọc core từ marker/config; chỉ caller adapter legacy thiếu metadata mới mặc định app.)
fn app_lock_owner_core(project_root: &Path) -> MgResult<String> {
    let marker = mgc_config::project::ProjectConfig::read_core_marker(project_root)
        .map_err(|error| MgError::Other(format!("cannot determine lock owner core: {error}")))?;
    let owner = match marker {
        Some(core) => core,
        None => match mgc_config::project::ProjectConfig::load(project_root)
            .map_err(|error| MgError::Other(format!("cannot read project core owner: {error}")))?
        {
            Some(config) => canonical_core_name(&config.ecosystem),
            None => "app".to_string(),
        },
    };
    if !mgc_config::project::ProjectConfig::KNOWN_CORES.contains(&owner.as_str()) {
        return Err(MgError::Other(format!(
            "refusing to write app lock entries for unknown core owner '{owner}'"
        )));
    }
    Ok(owner)
}

fn canonical_core_name(value: &str) -> String {
    let core = value.trim().to_ascii_lowercase();
    if core == "cloud" {
        "clo".to_string()
    } else {
        core
    }
}

/// Publish mgc.lock using a unique same-directory temp and an atomic
/// platform-aware replace, so a crash cannot leave a truncated lock.
/// Ghi mgc.lock bằng temp duy nhất cùng thư mục và thay thế atomic.
fn atomic_write_canonical_lock(path: &Path, bytes: &[u8]) -> MgResult<()> {
    mgc_lockfile::ensure_lockfile_mutation_allowed(path)
        .map_err(|error| MgError::Other(error.to_string()))?;
    atomic_write_project_file(path, bytes, "mgc.lock")
}

fn atomic_write_project_file(path: &Path, bytes: &[u8], label: &str) -> MgResult<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NONCE: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| MgError::Other(format!("{label} path has no valid filename")))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| MgError::Other(format!("system clock is invalid: {e}")))?
        .as_nanos();
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{filename}.{}.{}.{nonce}.tmp",
        std::process::id(),
        timestamp
    ));

    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| MgError::Other(format!("cannot create {label} staging file: {e}")))?;
        file.write_all(bytes)
            .map_err(|e| MgError::Other(format!("cannot write {label} staging file: {e}")))?;
        file.sync_all()
            .map_err(|e| MgError::Other(format!("cannot sync {label} staging file: {e}")))?;
        mgc_lockfile::atomic::atomic_replace_file(&temp, path)
            .map_err(|e| MgError::Other(format!("cannot atomically replace {label}: {e}")))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| MgError::Other(format!("cannot sync {label} parent directory: {e}")))?;
        Ok(())
    })();
    if let Err(write_error) = result {
        match std::fs::remove_file(&temp) {
            Ok(()) => Err(write_error),
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                Err(write_error)
            }
            Err(cleanup_error) => Err(MgError::Other(format!(
                "{write_error}; additionally failed to remove staging file '{}': {cleanup_error}",
                temp.display()
            ))),
        }
    } else {
        result
    }
}

#[cfg(test)]
#[path = "test/mod.rs"]
mod tests;
