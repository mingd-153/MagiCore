//! `install/mod.rs` — Lib adapter install orchestrator.
//! Installs TypeScript through the web engine and supported registry ecosystems natively.
//! Cài TypeScript qua web engine và các ecosystem registry được hỗ trợ bằng engine native.

pub mod fetch;
// pub for integration tests (cross-project reuse proof); semver —
// internal surface, do not use outside this crate's tests.
// pub cho integration test (chứng minh tái sử dụng chéo project);
// bề mặt nội bộ — không dùng ngoài test của crate này.
pub mod shared_store;
pub mod verify;

use mgc_lockfile::EcosystemTag;
use mgc_resolver::protocols::{
    CratesProtocol, GoModProtocol, MavenProtocol, NuGetProtocol, PypiProtocol, RegistryProtocol,
    ResolvedEntry,
};
use mgc_store::ContentStore;
use mgc_types::adapter::{InstallCacheMode, InstallOptions, InstallSummary};
use mgc_types::capabilities::ContentStoreProvider;
use mgc_types::{MgError, MgResult, ResolvedGraph, ResolvedPackage};
use std::path::Path;
use std::time::Instant;

use crate::language::LibLanguage;

/// Install orchestrator for lib adapter.
/// Điều phối install cho TypeScript/Rust/Python library projects.
pub(crate) async fn run_install(
    language: LibLanguage,
    web: Option<&mgc_web_adapter::WebAdapter>,
    graph: &ResolvedGraph,
    project_root: &Path,
    opts: InstallOptions,
    _store: Option<&ContentStore>,
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<InstallSummary> {
    // Python supports verified cache-only replay; other native registry
    // lanes still download during install and must refuse offline mode.
    // (Python replay cache đã verify được; lane native khác phải từ chối.)
    if opts.offline && !matches!(language, LibLanguage::Ts | LibLanguage::Python) {
        return Err(MgError::Unsupported {
            core: "lib",
            capability: "offline install",
            guidance: "offline install is unsupported for this native ecosystem until cache-only frozen reinstall is implemented; no files were changed".to_string(),
        });
    }

    if language != LibLanguage::Ts {
        validate_lock_coverage(graph, ecosystem_for_language(language), &lock_packages)?;
        // Reject a malformed or hostile lock before downloads/CAS writes.
        // (Từ chối lock hỏng trước khi tải hoặc ghi CAS.)
        read_existing_lock(project_root)?;
    }
    match language {
        LibLanguage::Ts => {
            let web = web.ok_or_else(|| {
                MgError::Other("TypeScript lib requires web adapter delegate".to_string())
            })?;
            web.install(graph, project_root, opts).await
        }
        LibLanguage::Rust => {
            let summary = install_rust_native(graph).await?;
            write_canonical_lock_with_roots(
                project_root,
                EcosystemTag::Rust,
                lock_packages,
                direct_package_ids(graph),
            )?;
            Ok(summary)
        }
        LibLanguage::Python => {
            let summary = install_python_native(graph, &lock_packages, opts.offline).await?;
            if !opts.offline {
                write_canonical_lock_with_roots(
                    project_root,
                    EcosystemTag::Python,
                    lock_packages,
                    direct_package_ids(graph),
                )?;
            }
            Ok(summary)
        }
        // Go: native module proxy engine (Phase 2) — zip download →
        // ziphash/sumdb verify → CAS → download-cache materialization.
        // (Go: engine module proxy native (Phase 2) — tải zip → verify
        // ziphash/sumdb → CAS → materialize download cache.)
        LibLanguage::Go => {
            let summary = install_go_native(graph, &lock_packages).await?;
            write_canonical_lock_with_roots(
                project_root,
                EcosystemTag::Go,
                lock_packages,
                direct_package_ids(graph),
            )?;
            Ok(summary)
        }
        // Java: native Maven engine (Phase 2) — jar+pom download →
        // sha256/sha1 verify → CAS → local-repo materialization
        // (`mvn -o` readable). Gradle-owned installs stay unsupported
        // (build scripts are programs — honest failure, never a silent
        // no-op summary).
        // (Java: engine Maven native (Phase 2) — tải jar+pom → verify
        // sha256/sha1 → CAS → materialize local repo (đọc được bởi
        // `mvn -o`). Install do gradle giữ vẫn unsupported (build script là
        // chương trình — fail trung thực, không trả summary no-op âm thầm).)
        LibLanguage::Java => {
            let summary = install_maven_native(graph, &lock_packages).await?;
            write_canonical_lock_with_roots(
                project_root,
                EcosystemTag::Maven,
                lock_packages,
                direct_package_ids(graph),
            )?;
            Ok(summary)
        }
        // .NET: native NuGet v3 engine (Phase 2) — nupkg download →
        // registration SHA-512 verify → CAS → global-packages
        // materialization (`dotnet restore --source`-readable layout).
        // (.NET: engine NuGet v3 native (Phase 2) — tải nupkg → verify
        // SHA-512 theo registration → CAS → materialize global-packages
        // (layout mà `dotnet restore --source` đọc được).)
        LibLanguage::DotNet => {
            let summary = install_nuget_native(graph, &lock_packages).await?;
            write_canonical_lock_with_roots(
                project_root,
                EcosystemTag::NuGet,
                lock_packages,
                direct_package_ids(graph),
            )?;
            Ok(summary)
        }
    }
}

/// Native Go install: download the module set (zip + go.mod + .info) →
/// verify sha256 (proxy .ziphash) or the sumdb `h1:` directory hash →
/// import to the mgc CAS (blake3) → materialize the go download cache
/// (`{gomodcache}/cache/download/{module}/@v/…`, `GOPROXY=off` readable).
/// No `go mod download` spawn — mgc owns resolve/fetch/install (Phase 2).
/// Install Go native: tải trọn bộ module (zip + go.mod + .info) → xác minh
/// sha256 (proxy .ziphash) hoặc directory hash `h1:` của sumdb → import vào
/// CAS mgc (blake3) → materialize go download cache
/// (`{gomodcache}/cache/download/{module}/@v/…`, đọc được với `GOPROXY=off`).
/// Không spawn `go mod download` — mgc giữ resolve/fetch/install (Phase 2).
/// Integrity markers for one graph package, reattached from its lock
/// entry by identity. Graph packages carry sha256/integrity only —
/// protocol-specific markers (go sumdb-ziphash, nuget sha512) travel in
/// the lock; without reattaching, the verifier sees neither source.
/// (Gắn lại marker toàn vẹn từ entry lock theo danh tính.)
fn markers_for(
    pkg: &mgc_types::ResolvedPackage,
    lock_packages: &[mgc_lockfile::Package],
) -> Vec<String> {
    lock_entry_for(pkg, lock_packages)
        .and_then(|p| p.markers.clone())
        .unwrap_or_default()
}

/// Lock entry for one graph package: exact name+version first, then
/// parse-equal fallback — Maven coordinates are NOT semver-normalizable
/// (`1.3` ≠ `1.3.0` on disk) while graph versions are normalized, so an
/// exact-only match orphans real entries.
/// (Tìm entry lock: khớp chính xác trước, rồi khớp nới lỏng.)
fn lock_entry_for<'a>(
    pkg: &mgc_types::ResolvedPackage,
    lock_packages: &'a [mgc_lockfile::Package],
) -> Option<&'a mgc_lockfile::Package> {
    let name = pkg.id.name_str();
    let version = pkg.id.version().to_string();
    if let Some(found) = lock_packages
        .iter()
        .find(|p| p.name == name && p.version == version)
    {
        return Some(found);
    }
    lock_packages.iter().find(|p| {
        p.name == name
            && mgc_types::Version::parse(&p.version).ok().as_ref() == Some(pkg.id.version())
    })
}

fn ecosystem_for_language(language: LibLanguage) -> EcosystemTag {
    match language {
        LibLanguage::Ts => EcosystemTag::Web,
        LibLanguage::Rust => EcosystemTag::Rust,
        LibLanguage::Python => EcosystemTag::Python,
        LibLanguage::Go => EcosystemTag::Go,
        LibLanguage::Java => EcosystemTag::Maven,
        LibLanguage::DotNet => EcosystemTag::NuGet,
    }
}

fn validate_lock_coverage(
    graph: &ResolvedGraph,
    ecosystem: EcosystemTag,
    lock_packages: &[mgc_lockfile::Package],
) -> MgResult<()> {
    if lock_packages
        .iter()
        .any(|package| package.ecosystem != ecosystem)
    {
        return Err(MgError::Other(format!(
            "resolver returned a lock entry outside active ecosystem '{ecosystem}'"
        )));
    }
    for resolved in &graph.packages {
        if lock_entry_for(resolved, lock_packages).is_none() {
            return Err(MgError::Other(format!(
                "refusing install: resolved package '{}'@{} has no matching integrity/lock entry",
                resolved.id.name_str(),
                resolved.id.version()
            )));
        }
    }
    Ok(())
}

pub(crate) fn read_existing_lock(project_root: &Path) -> MgResult<mgc_lockfile::Lockfile> {
    let path = project_root.join("mgc.lock");
    match mgc_lockfile::parser::load_lockfile(&path) {
        Ok(lockfile) => Ok(lockfile),
        Err(mgc_lockfile::LockfileError::IoError(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(mgc_lockfile::Lockfile::new())
        }
        Err(error) => Err(MgError::Other(format!(
            "refusing to use invalid mgc.lock '{}': {error}",
            path.display()
        ))),
    }
}

pub(crate) fn complete_lock_packages_from_existing(
    graph: &ResolvedGraph,
    ecosystem: EcosystemTag,
    lock_packages: &mut Vec<mgc_lockfile::Package>,
    existing: &mgc_lockfile::Lockfile,
) {
    for resolved in &graph.packages {
        if lock_entry_for(resolved, lock_packages).is_some() {
            continue;
        }
        if let Some(package) = existing
            .packages
            .iter()
            .filter(|package| package.ecosystem == ecosystem)
            .find(|package| lock_entry_for(resolved, std::slice::from_ref(*package)).is_some())
        {
            lock_packages.push(package.clone());
        }
    }
}

async fn install_go_native(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = GoModProtocol::from_env();
    let gomodcache = shared_store::SharedStoreRun::go()?.install_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let mut entry = entry_from_package(pkg);
        entry.extra_markers = markers_for(pkg, lock_packages);
        let files = protocol.download_module(&entry).await?;
        protocol.verify(&entry, &files.zip)?;
        store
            .import_bytes(&files.zip)
            .map_err(|e| MgError::Store(e.to_string()))?;
        store
            .import_bytes(&files.gomod)
            .map_err(|e| MgError::Store(e.to_string()))?;
        store
            .import_bytes(&files.info)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &files, &gomodcache)?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Importable site dirs for natively installed pure-python wheels, read
/// from the project's mgc.lock (no re-resolve). Runners (pytest/python)
/// prepend these to PYTHONPATH so `mgc test` sees mgc-owned packages —
/// the run side of "run tại MGC". A missing lock is a non-native project;
/// a malformed lock or missing materialization for a locked Python package
/// is an error, never a silently incomplete runtime.
/// (Site dir import được cho wheel pure-python đã cài native.)
pub fn native_python_path_entries(
    project_root: &std::path::Path,
) -> MgResult<Vec<std::path::PathBuf>> {
    let store = shared_store::SharedStoreRun::pypi()?;
    native_python_path_entries_from_store(project_root, &store.install_root)
}

/// Resolve and re-verify native Python runtime paths from an explicit MGC
/// PyPI store root. This explicit-root variant also makes store-integrity
/// tests hermetic instead of depending on the current user's home directory.
/// (Resolve và xác minh lại runtime Python từ gốc store PyPI tường minh.)
pub fn native_python_path_entries_from_store(
    project_root: &std::path::Path,
    pypi_store_root: &std::path::Path,
) -> MgResult<Vec<std::path::PathBuf>> {
    let manifest_path = project_root.join("pyproject.toml");
    let python_manifest = match std::fs::symlink_metadata(&manifest_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if !crate::manifest::supports_native_python_project(project_root) {
                return Err(MgError::Other(
                    "pyproject.toml uses unsupported or externally-owned Python dependency metadata; refusing to run with ambient Python packages".to_string(),
                ));
            }
            Some(crate::manifest::parse_pyproject_manifest(project_root)?)
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(MgError::Other(
                "refusing symlinked pyproject.toml for Python runtime".to_string(),
            ));
        }
        Ok(_) => {
            return Err(MgError::Other(
                "pyproject.toml for Python runtime is not a regular file".to_string(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot inspect pyproject.toml for Python runtime: {error}"
            )));
        }
    };
    if python_manifest.is_none()
        && let Some(manifest_name) = external_python_manifest_name(project_root)?
    {
        return Err(MgError::Other(format!(
            "Python dependency manifest '{manifest_name}' is not owned by the native MagiCore resolver; refusing to run with ambient Python packages"
        )));
    }
    let lock_path = project_root.join("mgc.lock");
    let lockfile = match mgc_lockfile::load_lockfile(&lock_path) {
        Ok(lockfile) => lockfile,
        Err(mgc_lockfile::LockfileError::IoError(e))
            if e.kind() == std::io::ErrorKind::NotFound =>
        {
            if let Some(manifest) = python_manifest
                && !manifest.dependencies.is_empty()
            {
                return Err(MgError::Other(format!(
                    "Python project declares dependencies in pyproject.toml but has no mgc.lock; run `mgc install --core {}` before launching Python",
                    python_project_owner(project_root)?
                )));
            }
            return Ok(Vec::new());
        }
        Err(e) => {
            return Err(MgError::Other(format!(
                "cannot safely read mgc.lock for Python runtime: {e}"
            )));
        }
    };
    let owner_core = python_project_owner(project_root)?;
    let python_packages = select_python_packages_for_owner(&lockfile.packages, &owner_core)?;
    if let Some(manifest) = python_manifest {
        for dependency in &manifest.dependencies {
            let normalized_name = normalize_python_distribution_name(dependency.name.as_str());
            let locked = python_packages.iter().find(|package| {
                normalize_python_distribution_name(&package.name) == normalized_name
            });
            let Some(locked) = locked else {
                return Err(MgError::Other(format!(
                    "Python dependency '{}' declared in pyproject.toml is absent from the {}-owned mgc.lock; run `mgc install --core {}`",
                    dependency.name, owner_core, owner_core
                )));
            };
            let locked_version = mgc_types::Version::parse(&locked.version).map_err(|error| {
                MgError::Other(format!(
                    "Python dependency '{}' has an uncheckable locked version '{}': {error}",
                    dependency.name, locked.version
                ))
            })?;
            if !dependency.range.matches(&locked_version) {
                return Err(MgError::Other(format!(
                    "Python dependency '{}' requires '{}' but mgc.lock pins '{}'; run `mgc install --core {}`",
                    dependency.name, dependency.range, locked.version, owner_core
                )));
            }
        }
    }
    if python_packages.is_empty() {
        return Ok(Vec::new());
    }
    let wheels_root = pypi_store_root.join("wheels");
    let site = wheels_root.join("site");
    python_packages
        .into_iter()
        .map(|package| {
            let integrity = package
                .integrity
                .strip_prefix("sha256-")
                .ok_or_else(|| {
                    MgError::Integrity(format!(
                        "Python dependency '{}'@{} has no SHA-256 integrity in mgc.lock",
                        package.name, package.version
                    ))
                })?;
            let artifact_name = PypiProtocol::artifact_filename_for_url(&package.resolved)?;
            let artifact_relative =
                PypiProtocol::artifact_cache_relpath(&package.resolved, integrity)?;
            let digest_artifact_path = wheels_root.join(&artifact_relative);
            // Read pre-digest caches only as a verified migration fallback.
            // (Chỉ đọc cache cũ như lối chuyển đổi có xác minh digest.)
            let artifact_path = match std::fs::symlink_metadata(&digest_artifact_path) {
                Ok(_) => digest_artifact_path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    wheels_root.join(artifact_name)
                }
                Err(error) => return Err(error.into()),
            };
            let artifact_metadata = std::fs::symlink_metadata(&artifact_path).map_err(|error| {
                MgError::Integrity(format!(
                    "Python dependency '{}'@{} has no cached wheel for runtime verification: {error}",
                    package.name, package.version
                ))
            })?;
            if !artifact_metadata.file_type().is_file() {
                return Err(MgError::Integrity(format!(
                    "Python wheel cache entry '{}' is not a regular file",
                    artifact_path.display()
                )));
            }
            let wheel_bytes = std::fs::read(&artifact_path)?;
            let digest_site = site.join(PypiProtocol::importable_site_dirname_for_digest(
                &package.name,
                &package.version,
                integrity,
            )?);
            let legacy_site = site.join(python_site_dirname(&package.name, &package.version)?);
            let path = match std::fs::symlink_metadata(&digest_site) {
                Ok(_) => digest_site,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => legacy_site,
                Err(error) => return Err(error.into()),
            };
            PypiProtocol::verify_runtime_materialization(
                &wheel_bytes,
                &package.name,
                &package.version,
                integrity,
                &path,
            )?;
            if !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_dir()) {
                return Err(MgError::Other(format!(
                    "Python dependency '{}'@{} is locked but has no importable MagiCore materialization; compiled wheels/sdists are not yet supported by the native Python runtime",
                    package.name, package.version
                )));
            }
            Ok(path)
        })
        .collect()
}

/// Normalize a Python distribution name according to PEP 503 for manifest ↔
/// lock coverage comparisons (not for filesystem paths).
/// Chuẩn hóa tên distribution Python theo PEP 503 khi đối chiếu manifest/lock.
fn normalize_python_distribution_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut separator = false;
    for ch in name.chars() {
        if matches!(ch, '-' | '_' | '.') {
            separator = true;
        } else {
            if separator && !normalized.is_empty() {
                normalized.push('-');
            }
            separator = false;
            normalized.push(ch.to_ascii_lowercase());
        }
    }
    normalized
}

/// Find dependency metadata that the current native PEP 621 lane does not
/// own. Presence alone is sufficient to block an ambient-runtime fallback;
/// this deliberately does not parse or execute those files.
/// Tìm manifest Python chưa thuộc lane native; chỉ cần tồn tại là chặn fallback.
fn external_python_manifest_name(project_root: &Path) -> MgResult<Option<String>> {
    let entries = std::fs::read_dir(project_root).map_err(|error| {
        MgError::Other(format!(
            "cannot inspect Python dependency manifests: {error}"
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            MgError::Other(format!(
                "cannot inspect Python dependency manifest entry: {error}"
            ))
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        if matches!(
            name.as_str(),
            "uv.lock"
                | "poetry.lock"
                | "pdm.lock"
                | "pipfile"
                | "pipfile.lock"
                | "pylock.toml"
                | "setup.py"
                | "setup.cfg"
                | "environment.yml"
                | "environment.yaml"
                | "conda-lock.yml"
                | "conda-lock.yaml"
                | "requirements.in"
                | "constraints.txt"
        ) || (name.starts_with("requirements") && name.ends_with(".txt"))
            || (name.starts_with("pylock.") && name.ends_with(".toml"))
            || (name.starts_with("constraints-") && name.ends_with(".txt"))
        {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

/// Resolve the core that owns this Python runtime without guessing across a
/// mixed-core lockfile. The legacy default matches lock writing for projects
/// with neither marker nor config (`lib`).
/// Xác định core sở hữu runtime Python; mặc định cũ chỉ dùng khi không mơ hồ.
fn python_project_owner(project_root: &Path) -> MgResult<String> {
    if let Some(core) =
        mgc_config::project::ProjectConfig::read_core_marker(project_root).map_err(|error| {
            MgError::Other(format!("cannot read Python project core marker: {error}"))
        })?
    {
        return Ok(canonical_core_name(&core));
    }
    if let Some(config) =
        mgc_config::project::ProjectConfig::load(project_root).map_err(|error| {
            MgError::Other(format!("cannot read Python project core config: {error}"))
        })?
    {
        return Ok(canonical_core_name(&config.ecosystem));
    }
    Ok("lib".to_string())
}

/// Select Python lock entries owned by the active core. Legacy entries are
/// accepted only if the lock has no competing core owner.
/// Chỉ chọn entry Python thuộc core hiện tại; lock cũ mơ hồ bị chặn.
fn select_python_packages_for_owner<'a>(
    packages: &'a [mgc_lockfile::Package],
    owner_core: &str,
) -> MgResult<Vec<&'a mgc_lockfile::Package>> {
    let all_python_packages = packages
        .iter()
        .filter(|package| package.ecosystem == mgc_lockfile::EcosystemTag::Python)
        .collect::<Vec<_>>();
    let has_unowned_packages = all_python_packages
        .iter()
        .any(|package| package.owner_core.is_none());
    if owner_core != "lib" && has_unowned_packages {
        return Err(MgError::Other(
            "Python entries in mgc.lock have ambiguous core ownership; run `mgc install` to migrate the lock before launching Python".to_string(),
        ));
    }
    Ok(all_python_packages
        .into_iter()
        .filter(|package| {
            package.owner_core.as_deref() == Some(owner_core)
                || (owner_core == "lib" && package.owner_core.is_none())
        })
        .collect())
}

/// Validate untrusted lock fields before they become a filesystem path.
/// Python project names are single-segment names and PEP 440 versions must
/// not contain path separators or platform-special characters.
/// (Validate field lock không tin cậy trước khi tạo path filesystem.)
fn python_site_dirname(name: &str, version: &str) -> MgResult<String> {
    PypiProtocol::importable_site_dirname(name, version)
}

/// MGC-owned Python installed set. Unlike a toolchain venv, native wheels
/// live in the shared managed site tree; report only exact Python pins in
/// `mgc.lock` whose importable materialization was verified above.
/// (Danh sách package Python MGC sở hữu: chỉ pin mgc.lock có site đã kiểm.)
pub(crate) fn native_python_installed_packages(
    project_root: &Path,
    manifest: &mgc_types::Manifest,
) -> MgResult<Vec<mgc_types::adapter::InstalledPackage>> {
    // Listing reports only packages MGC has actually locked/materialized. A
    // Python manifest without mgc.lock is not a runtime launch request, so it
    // must not turn the honest empty installed set into an error.
    // (List chỉ báo package do MGC quản lý; project chưa có lock thì tập đã
    // cài của MGC là rỗng, không phải lỗi runtime.)
    match std::fs::symlink_metadata(project_root.join("mgc.lock")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot inspect mgc.lock for Python package listing: {error}"
            )));
        }
        Ok(_) => {}
    }
    let lockfile = read_existing_lock(project_root)?;
    let owner_core = python_project_owner(project_root)?;
    let python_packages = select_python_packages_for_owner(&lockfile.packages, &owner_core)?;
    let paths = native_python_path_entries(project_root)?;
    if python_packages.len() != paths.len() {
        return Err(MgError::Other(
            "Python lock/materialization count changed while listing; retry the operation"
                .to_string(),
        ));
    }
    python_packages
        .into_iter()
        .zip(paths)
        .map(|(package, path)| {
            let name = mgc_types::PackageName::new(&package.name)?;
            let version = mgc_types::Version::parse(&package.version)?;
            let is_direct = manifest
                .all_dependencies()
                .any(|dependency| dependency.name == name);
            Ok(mgc_types::adapter::InstalledPackage {
                id: mgc_types::PackageId::new(name, version),
                path,
                integrity: (!package.integrity.is_empty()).then(|| package.integrity.clone()),
                is_direct,
                is_dev: false,
            })
        })
        .collect()
}

/// Native Maven install: download each jar → verify sha256 (or the recorded
/// sha1) → import to the mgc CAS (blake3) → download the POM → materialize
/// into `{m2_root}/repository/{gpath}/{artifact}/{version}/` (`mvn -o`
/// readable). No `mvn dependency:go-offline` spawn — mgc owns
/// resolve/fetch/install for pom.xml projects (Phase 2).
/// Install Maven native: tải từng jar → verify sha256 (hoặc sha1 đã ghi) →
/// import vào CAS mgc (blake3) → tải POM → materialize vào
/// `{m2_root}/repository/{gpath}/{artifact}/{version}/` (đọc được bởi
/// `mvn -o`). Không spawn `mvn dependency:go-offline` — mgc giữ
/// resolve/fetch/install cho project pom.xml (Phase 2).
async fn install_maven_native(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = MavenProtocol::from_env();
    let m2_root = shared_store::SharedStoreRun::maven()?.install_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let mut entry = entry_from_package(pkg);
        entry.extra_markers = markers_for(pkg, lock_packages);
        // Maven coordinates keep their RAW version string (`1.3`, never
        // normalized `1.3.0`): filenames on Central are literal. Prefer
        // the lock entry's raw version when present.
        // (Tọa độ Maven giữ chuỗi version THÔ.)
        if let Some(raw) = lock_entry_for(pkg, lock_packages).map(|p| p.version.clone()) {
            entry.version = raw;
        }
        let jar = protocol.download(&entry).await?;
        protocol.verify(&entry, &jar)?;
        let (group, artifact) = MavenProtocol::split_coordinate(&entry.name)?;
        let pom = protocol
            .download_pom(&group, &artifact, &entry.version)
            .await?;
        store
            .import_bytes(&jar)
            .map_err(|e| MgError::Store(e.to_string()))?;
        store
            .import_bytes(&pom)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &jar, &pom, &m2_root)?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Native NuGet install: download each nupkg → verify the registration's
/// base64 SHA-512 → import to the mgc CAS (blake3) → materialize the
/// global-packages layout (`{nuget_root}/{id-lower}/{version}/` with the
/// nupkg, extracted contents and `.nupkg.sha512`). No `dotnet restore`
/// spawn — mgc owns resolve/fetch/install (Phase 2).
/// Install NuGet native: tải từng nupkg → verify SHA-512 base64 theo
/// registration → import vào CAS mgc (blake3) → materialize layout
/// global-packages (`{nuget_root}/{id-lower}/{version}/` với nupkg, nội
/// dung giải nén và `.nupkg.sha512`). Không spawn `dotnet restore` — mgc
/// giữ resolve/fetch/install (Phase 2).
async fn install_nuget_native(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = NuGetProtocol::from_env().await;
    let nuget_root = shared_store::SharedStoreRun::nuget()?.install_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let mut entry = entry_from_package(pkg);
        entry.extra_markers = markers_for(pkg, lock_packages);
        let bytes = protocol.download(&entry).await?;
        protocol.verify(&entry, &bytes)?;
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &bytes, &nuget_root)?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Native rust install: download each `.crate` → verify sha256 → import to
/// the mgc CAS (blake3) → materialize the cargo layout. No `cargo fetch`
/// spawn — mgc owns resolve/fetch/install (Phase 2).
/// Install rust native: tải từng `.crate` → verify sha256 → import vào CAS
/// mgc (blake3) → materialize layout cargo. Không spawn `cargo fetch` — mgc
/// giữ resolve/fetch/install (Phase 2).
async fn install_rust_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = CratesProtocol::from_env();
    let cargo_home = shared_store::SharedStoreRun::cargo()?.install_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
        let bytes = protocol.download(&entry).await?;
        protocol.verify(&entry, &bytes)?;
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &bytes, &cargo_home)?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Native python install: download each wheel/sdist → verify sha256 → import
/// to the mgc CAS → materialize into `{store}/pypi/wheels/`. No `uv sync`/
/// `pip install` spawn — mgc owns resolve/fetch/install (Phase 2).
/// Install python native: tải từng wheel/sdist → verify sha256 → import vào
/// CAS mgc → materialize vào `{store}/pypi/wheels/`. Không spawn `uv sync`/
/// `pip install` — mgc giữ resolve/fetch/install (Phase 2).
async fn install_python_native(
    graph: &ResolvedGraph,
    lock_packages: &[mgc_lockfile::Package],
    offline: bool,
) -> MgResult<InstallSummary> {
    let started = Instant::now();
    // Do not report success for artifacts this runtime cannot import. Check
    // the whole graph before network or store mutation so a compiled wheel
    // or sdist fails honestly instead of yielding a broken Python project.
    // (Kiểm tra toàn graph trước download/store để không báo cài thành công
    // cho wheel biên dịch hoặc sdist mà runtime hiện tại không import được.)
    validate_python_importable_graph(graph)?;
    let protocol = PypiProtocol::from_env();
    let store_root = if offline {
        shared_store::SharedStoreRun::pypi_path()?.install_root
    } else {
        shared_store::SharedStoreRun::pypi()?.install_root
    };
    let wheels_dir = store_root.join("wheels");

    // Offline replay is all-or-nothing with respect to cache availability:
    // authenticate every pinned artifact before creating the CAS or
    // materializing any package. This avoids turning a late cache miss into
    // a misleading partial successful install.
    // (Offline phải preflight toàn bộ cache trước khi tạo CAS/materialize.)
    if offline {
        for pkg in &graph.packages {
            let mut entry = entry_from_package(pkg);
            entry.extra_markers = markers_for(pkg, lock_packages);
            let relative =
                PypiProtocol::artifact_cache_relpath(&entry.artifact_url, &entry.sha256)?;
            let artifact_path = wheels_dir.join(relative);
            if read_cached_python_artifact(&artifact_path, &entry.sha256)?.is_none() {
                return Err(MgError::Other(format!(
                    "offline Python install cache miss for '{}'@{}; run `mgc install` online before using `--offline`",
                    entry.name, entry.version
                )));
            }
        }
    }

    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());
    let mut bytes_from_cache = 0_u64;

    for pkg in &graph.packages {
        let mut entry = entry_from_package(pkg);
        // Reattach lock markers (wheel tags for the ABI warning, sdist
        // flags) — the graph carries integrity only.
        // (Gắn lại marker từ lock.)
        entry.extra_markers = markers_for(pkg, lock_packages);
        let artifact_relpath =
            PypiProtocol::artifact_cache_relpath(&entry.artifact_url, &entry.sha256)?;
        let artifact_path = wheels_dir.join(artifact_relpath);
        let (bytes, cache_hit) = match read_cached_python_artifact(&artifact_path, &entry.sha256)? {
            Some(bytes) => (bytes, true),
            None => {
                if offline {
                    return Err(MgError::Other(format!(
                        "offline Python install cache entry disappeared for '{}'@{}",
                        entry.name, entry.version
                    )));
                }
                let bytes = protocol.download(&entry).await?;
                // The materializer verifies the registry digest before
                // publishing a newly fetched artifact.
                // (Materializer xác minh digest trước khi lưu artifact mới.)
                protocol.materialize(&entry, &bytes, &wheels_dir)?;
                (bytes, false)
            }
        };
        if cache_hit {
            bytes_from_cache = bytes_from_cache.saturating_add(bytes.len() as u64);
        }
        // Pure-python wheels additionally unpack into an importable site
        // dir (compiled wheels honestly skip — the ABI warning above).
        // Preflight above guarantees this is importable. Keep a runtime
        // assertion at the materializer boundary in case resolver output
        // changes between validation and installation.
        // (Preflight bảo đảm import được; kiểm lại tại materializer.)
        if protocol
            .materialize_importable(&entry, &bytes, &wheels_dir)?
            .is_none()
        {
            return Err(MgError::Other(format!(
                "Python artifact for '{}'@{} became non-importable after preflight",
                entry.name, entry.version
            )));
        }
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        added.push(pkg.id.clone());
    }

    Ok(InstallSummary {
        added,
        bytes_from_cache,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::MgCStore,
    })
}

/// Read a digest-addressed wheel cache entry without following a final
/// symlink, then re-verify its SHA-256 before allowing it to satisfy an
/// install. A missing entry is a cache miss; malformed or modified entries
/// fail closed rather than falling through to unverified bytes.
/// Đọc cache wheel theo digest, không theo symlink và xác minh SHA-256 lại.
fn read_cached_python_artifact(path: &Path, expected_sha256: &str) -> MgResult<Option<Vec<u8>>> {
    use sha2::Digest;
    use std::io::Read;

    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => {
            return Err(MgError::Integrity(format!(
                "Python artifact cache entry '{}' is not a regular file",
                path.display()
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(MgError::Other(format!(
                "cannot inspect Python artifact cache '{}': {error}",
                path.display()
            )));
        }
    }

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path).map_err(|error| {
        MgError::Integrity(format!(
            "cannot open Python artifact cache '{}' without following links: {error}",
            path.display()
        ))
    })?;
    let metadata = file.metadata().map_err(|error| {
        MgError::Integrity(format!(
            "cannot inspect opened Python artifact cache '{}': {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(MgError::Integrity(format!(
            "Python artifact cache entry '{}' is not a regular file",
            path.display()
        )));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(MgError::Integrity(format!(
                "Python artifact cache entry '{}' is a reparse point",
                path.display()
            )));
        }
    }

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| {
        MgError::Other(format!(
            "cannot read Python artifact cache '{}': {error}",
            path.display()
        ))
    })?;
    let actual = hex::encode(sha2::Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(MgError::Integrity(format!(
            "Python artifact cache integrity mismatch for '{}': expected SHA-256 {}, got {}",
            path.display(),
            expected_sha256,
            actual
        )));
    }
    Ok(Some(bytes))
}

fn validate_python_importable_graph(graph: &ResolvedGraph) -> MgResult<()> {
    for package in &graph.packages {
        if !PypiProtocol::is_importable_pure_wheel(&package.tarball_url)? {
            return Err(MgError::Other(format!(
                "native Python install does not yet support compiled wheels or sdists ('{}'@{}); no external package manager was invoked",
                package.id.name_str(),
                package.id.version()
            )));
        }
    }
    Ok(())
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

/// Build (and create) the mgc CAS content store used by native install.
/// Dựng (và tạo) content store CAS mgc cho install native.
fn content_store() -> MgResult<ContentStore> {
    ContentStore::new(mgc_store::default_store_root()).map_err(|e| MgError::Store(e.to_string()))
}

/// Flush the native-resolution lock entries to the canonical mgc.lock v3.
/// Ghi entry lock từ resolve native xuống mgc.lock v3 canonical.
#[cfg(test)]
fn write_canonical_lock(
    project_root: &Path,
    ecosystem: EcosystemTag,
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<()> {
    write_canonical_lock_with_roots(project_root, ecosystem, lock_packages, Vec::new())
}

fn write_canonical_lock_with_roots(
    project_root: &Path,
    ecosystem: EcosystemTag,
    mut lock_packages: Vec<mgc_lockfile::Package>,
    direct_package_ids: Vec<String>,
) -> MgResult<()> {
    let lock_path = project_root.join("mgc.lock");
    let mut lockfile = read_existing_lock(project_root)?;
    let original_packages = lockfile.packages.clone();
    let original_version = lockfile.version.clone();
    let original_roots = lockfile.root_dependencies_by_owner.clone();
    // Replace this ecosystem's resolved set as a whole. Package-only merge
    // retained removed and transitive dependencies in the unified lock.
    // (Thay trọn tập ecosystem; merge từng package để lại entry đã gỡ.)
    if lock_packages.iter().any(|pkg| pkg.ecosystem != ecosystem) {
        return Err(MgError::Other(format!(
            "resolver returned a package outside the active ecosystem '{ecosystem}'"
        )));
    }
    let marker = mgc_config::project::ProjectConfig::read_core_marker(project_root)
        .map_err(|error| MgError::Other(format!("cannot determine lock owner core: {error}")))?;
    let (owner_core, explicit_project_owner) = match marker {
        Some(core) => (core, true),
        None => match mgc_config::project::ProjectConfig::load(project_root)
            .map_err(|error| MgError::Other(format!("cannot read project core owner: {error}")))?
        {
            Some(config) => (canonical_core_name(&config.ecosystem), true),
            None => ("lib".to_string(), false),
        },
    };
    if !mgc_config::project::ProjectConfig::KNOWN_CORES.contains(&owner_core.as_str()) {
        return Err(MgError::Other(format!(
            "refusing to write lock entries for unknown core owner '{owner_core}'"
        )));
    }

    // A unified lock may contain the same language ecosystem under multiple
    // cores (for example AI/Python and Lib/Python). Replace only this core's
    // entries. A single-core project signature permits safe adoption of a
    // legacy unowned set; a mixed lock with a competing owner stays ambiguous
    // and is preserved instead of guessed or deleted.
    // (Lock hợp nhất có thể dùng chung ecosystem ở nhiều core; chỉ thay entry
    // của core hiện tại; chỉ nhận diện lock cũ khi chữ ký project không mơ hồ.)
    let conflicting_owner_exists = lockfile.packages.iter().any(|pkg| {
        pkg.ecosystem == ecosystem
            && pkg
                .owner_core
                .as_deref()
                .is_some_and(|owner| owner != owner_core)
    });
    if explicit_project_owner && !conflicting_owner_exists {
        for package in &mut lockfile.packages {
            if package.ecosystem == ecosystem && package.owner_core.is_none() {
                package.owner_core = Some(owner_core.clone());
            }
        }
    }
    for package in &mut lock_packages {
        package.owner_core = Some(owner_core.clone());
    }
    lockfile.packages.retain(|pkg| {
        pkg.ecosystem != ecosystem || pkg.owner_core.as_deref() != Some(owner_core.as_str())
    });
    lockfile.packages.extend(lock_packages);
    mgc_lockfile::update_owner_root_pins(&mut lockfile, &owner_core, ecosystem, direct_package_ids);
    if lockfile.packages == original_packages
        && lockfile.root_dependencies_by_owner == original_roots
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

fn direct_package_ids(graph: &ResolvedGraph) -> Vec<String> {
    graph
        .packages
        .iter()
        .filter(|package| package.direct)
        .map(|package| package.id.to_string())
        .collect()
}

fn canonical_core_name(value: &str) -> String {
    let core = value.trim().to_ascii_lowercase();
    if core == "cloud" {
        "clo".to_string()
    } else {
        core
    }
}

/// Atomically publish the serialized lock so a crash cannot leave a
/// truncated `mgc.lock`. The CLI mutation gateway serializes project
/// writers; this ensures durable file replacement. (Ghi lock nguyên tử.)
fn atomic_write_canonical_lock(path: &Path, bytes: &[u8]) -> MgResult<()> {
    mgc_lockfile::ensure_lockfile_mutation_allowed(path)
        .map_err(|error| MgError::Other(error.to_string()))?;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| MgError::Other("mgc.lock path has no valid filename".to_string()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| MgError::Other(format!("system clock is invalid: {e}")))?
        .as_nanos();
    let temp = parent.join(format!(".{filename}.{}.{nonce:x}.tmp", std::process::id()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temp).map_err(|e| {
            MgError::Other(format!(
                "cannot create lock staging file '{}': {e}",
                temp.display()
            ))
        })?;
        file.write_all(bytes)
            .map_err(|e| MgError::Other(format!("cannot write lock staging file: {e}")))?;
        file.sync_all()
            .map_err(|e| MgError::Other(format!("cannot sync lock staging file: {e}")))?;
        replace_lock_file(&temp, path)
            .map_err(|e| MgError::Other(format!("cannot atomically replace mgc.lock: {e}")))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| MgError::Other(format!("cannot sync lock directory: {e}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Atomically replace a lockfile on every supported OS. Rust's Windows
/// `rename` does not promise replacement when the destination exists, so
/// use the same write-through MoveFileEx semantics as mgc-lockfile.
/// (Thay lockfile nguyên tử trên mọi OS; Windows cần cờ replace tường minh.)
fn replace_lock_file(temp: &Path, destination: &Path) -> std::io::Result<()> {
    mgc_lockfile::atomic::atomic_replace_file(temp, destination)
}

#[cfg(test)]
#[path = "test/mod.rs"]
mod tests;
