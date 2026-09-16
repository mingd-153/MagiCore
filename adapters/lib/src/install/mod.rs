//! `install/mod.rs` — Lib adapter install orchestrator.
//! Orchestrates install across TypeScript (delegate web), Rust (cargo), Python (pip/uv).

pub mod fetch;
// pub for integration tests (cross-project reuse proof); semver —
// internal surface, do not use outside this crate's tests.
// pub cho integration test (chứng minh tái sử dụng chéo project);
// bề mặt nội bộ — không dùng ngoài test của crate này.
pub mod shared_store;
pub mod verify;

use mgc_resolver::protocols::{CratesProtocol, PypiProtocol, RegistryProtocol, ResolvedEntry};
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
        // Go: `go mod download` fetches the pinned module set — the go
        // toolchain owns module caching (Q9-style delegation, no shim).
        // Go: `go mod download` tải tập module đã ghim — go toolchain giữ
        // module cache (ủy quyền kiểu Q9, không lệnh bọc).
        LibLanguage::Go => install_go(project_root, opts).await,
        // Java/.NET install is not wired yet (P2 audit parity scope):
        // gradle/dotnet own dependency fetching; mgc audits their
        // lockfiles. Honest failure, never a silent no-op summary.
        // Install Java/.NET chưa nối (scope parity audit P2): gradle/
        // dotnet giữ việc tải dependency; mgc audit lockfile của chúng.
        // Fail trung thực, không trả summary no-op âm thầm.
        LibLanguage::Java => Err(MgError::Other(
            "java install is delegated to gradle (mgc reads gradle/verification-metadata.xml for audits); the native java install lane lands with P2".to_string(),
        )),
        LibLanguage::DotNet => Err(MgError::Other(
            ".NET install is delegated to dotnet restore (mgc reads packages.lock.json for audits); the native .NET install lane lands with P2".to_string(),
        )),
    }
}

/// Fetch Go modules per go.mod — delegated to the go toolchain.
/// Tải module Go theo go.mod — ủy quyền cho go toolchain.
///
/// DELEGATED: `go mod download` runs for real — mgc does not own this
/// dependency lifecycle.
/// (DELEGATED: `go mod download` chạy thật — mgc không sở hữu lifecycle
/// dependency này.)
async fn install_go(project_root: &Path, _opts: InstallOptions) -> MgResult<InstallSummary> {
    // P0-6: delegated install — the go module cache owns the bytes, so
    // the summary says so instead of a silent zero byte-count.
    // P0-6: install ủy quyền — module cache của go giữ byte, summary
    // nói rõ điều đó thay vì byte-count 0 âm thầm.
    let started = Instant::now();
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };
    mgc_exec::run::run(
        "go",
        &["mod".to_string(), "download".to_string()],
        &exec_opts,
    )
    .map_err(|e| MgError::Other(format!("go mod download failed: {e}")))?;
    Ok(InstallSummary {
        added: vec![],
        bytes_from_cache: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        cache_mode: InstallCacheMode::Delegated,
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
