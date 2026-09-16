//! `install/mod.rs` — Lib adapter install orchestrator.
//! Orchestrates install across TypeScript (delegate web), Rust (cargo), Python (pip/uv).

pub mod fetch;
// pub for integration tests (cross-project reuse proof); semver —
// internal surface, do not use outside this crate's tests.
// pub cho integration test (chứng minh tái sử dụng chéo project);
// bề mặt nội bộ — không dùng ngoài test của crate này.
pub mod shared_store;
pub mod verify;

use mgc_resolver::protocols::{
    CratesProtocol, GoModProtocol, MavenProtocol, PypiProtocol, RegistryProtocol, ResolvedEntry,
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
    match language {
        LibLanguage::Ts => {
            let web = web.ok_or_else(|| {
                MgError::Other("TypeScript lib requires web adapter delegate".to_string())
            })?;
            web.install(graph, project_root, opts).await
        }
        LibLanguage::Rust => {
            let summary = install_rust_native(graph).await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }
        LibLanguage::Python => {
            let summary = install_python_native(graph).await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }
        // Go: native module proxy engine (Phase 2) — zip download →
        // ziphash/sumdb verify → CAS → download-cache materialization.
        // (Go: engine module proxy native (Phase 2) — tải zip → verify
        // ziphash/sumdb → CAS → materialize download cache.)
        LibLanguage::Go => {
            let summary = install_go_native(graph).await?;
            write_canonical_lock(project_root, lock_packages)?;
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
            let summary = install_maven_native(graph).await?;
            write_canonical_lock(project_root, lock_packages)?;
            Ok(summary)
        }
        LibLanguage::DotNet => Err(MgError::Other(
            ".NET install is delegated to dotnet restore (mgc reads packages.lock.json for audits); the native .NET install lane lands with P2".to_string(),
        )),
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
async fn install_go_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = GoModProtocol::from_env();
    let gomodcache = shared_store::SharedStoreRun::go()?.cache_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
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
async fn install_maven_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = MavenProtocol::from_env();
    let m2_root = shared_store::SharedStoreRun::maven()?.cache_root;
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
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

/// Native rust install: download each `.crate` → verify sha256 → import to
/// the mgc CAS (blake3) → materialize the cargo layout. No `cargo fetch`
/// spawn — mgc owns resolve/fetch/install (Phase 2).
/// Install rust native: tải từng `.crate` → verify sha256 → import vào CAS
/// mgc (blake3) → materialize layout cargo. Không spawn `cargo fetch` — mgc
/// giữ resolve/fetch/install (Phase 2).
async fn install_rust_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = CratesProtocol::from_env();
    let cargo_home = shared_store::SharedStoreRun::cargo()?.cache_root;
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
async fn install_python_native(graph: &ResolvedGraph) -> MgResult<InstallSummary> {
    let started = Instant::now();
    let protocol = PypiProtocol::from_env();
    let wheels_dir = shared_store::SharedStoreRun::pypi()?
        .cache_root
        .join("wheels");
    let store = content_store()?;
    let mut added = Vec::with_capacity(graph.packages.len());

    for pkg in &graph.packages {
        let entry = entry_from_package(pkg);
        let bytes = protocol.download(&entry).await?;
        protocol.verify(&entry, &bytes)?;
        store
            .import_bytes(&bytes)
            .map_err(|e| MgError::Store(e.to_string()))?;
        protocol.materialize(&entry, &bytes, &wheels_dir)?;
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

/// Build (and create) the mgc CAS content store used by native install.
/// Dựng (và tạo) content store CAS mgc cho install native.
fn content_store() -> MgResult<ContentStore> {
    ContentStore::new(mgc_store::default_store_root()).map_err(|e| MgError::Store(e.to_string()))
}

/// Flush the native-resolution lock entries to the canonical mgc.lock v3.
/// Ghi entry lock từ resolve native xuống mgc.lock v3 canonical.
fn write_canonical_lock(
    project_root: &Path,
    lock_packages: Vec<mgc_lockfile::Package>,
) -> MgResult<()> {
    if lock_packages.is_empty() {
        // Nothing resolved natively (e.g. empty dependency set) — leave any
        // existing lock untouched.
        // (Không resolve gì native — giữ nguyên lock có sẵn.)
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
        // ISO-8601 without external deps — seconds precision is enough
        // for the metadata field. (ISO-8601 không cần crate ngoài — độ
        // chính xác giây đủ cho trường metadata.)
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
