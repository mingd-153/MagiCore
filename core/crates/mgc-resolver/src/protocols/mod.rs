//! Phase 2 native registry protocol engines.
//! Engine registry protocol native Phase 2.
//!
//! Real HTTP engines for crates.io (Rust), PyPI (Python), pub.dev (Dart) and
//! the Go module proxy, plus the still-stubbed npm slot (the web core keeps
//! its own native path). Each engine resolves a `name` within a `range`
//! against its registry, selects the highest matching version, filters deps,
//! verifies the artifact hash and materializes the toolchain layout. The
//! engines are a LIBRARY in core — adapters wire into them, they never touch
//! adapters directly.
//! Engine HTTP thật cho crates.io (Rust), PyPI (Python), pub.dev (Dart) và
//! Go module proxy, cùng slot npm vẫn là stub (core web giữ đường native
//! riêng). Mỗi engine resolve `name` trong `range` trên registry của nó,
//! chọn version cao nhất thoả, lọc deps, xác minh hash artifact và
//! materialize layout toolchain. Engine là LIBRARY trong core — adapter wire
//! vào, engine không đụng adapter.

pub mod archive;
pub mod crates;
pub mod go;
pub mod hfhub;
pub mod maven;
pub mod nuget;
#[path = "pub.rs"]
pub mod pubdev;
pub mod pypi;
pub mod reactnative;
pub mod swift;
pub mod zip_reader;

pub use crates::CratesProtocol;
pub use go::GoModProtocol;
pub use hfhub::{HfHubProtocol, ModelFile, ModelResolution, git_blob_sha1};
pub use maven::MavenProtocol;
pub use nuget::NuGetProtocol;
pub use pubdev::PubProtocol;
pub use pypi::PypiProtocol;
pub use reactnative::{
    CocoaPodsProtocol, GradleLockEntry, PODSPEC_SHA1_MARKER_PREFIX, PodfileLock,
    RN_TIER_MARKER_PREFIX,
};
pub use swift::SwiftRegistryProtocol;
pub use swift::bump_swift_requirement;
pub use swift::remove_swift_requirement;

use async_trait::async_trait;
use futures_util::stream::{self, StreamExt, TryStreamExt};
use mgc_types::{MgError, MgResult};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(super) const GRAPH_RESOLVE_CONCURRENCY: usize = 16;
const GRAPH_ROOT_CONCURRENCY: usize = 4;

/// Resolution payload a native engine returns for one package.
/// Payload resolve mà engine native trả về cho một package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEntry {
    /// Package name at the registry — Tên package trên registry.
    pub name: String,
    /// Exact resolved version — Version chính xác đã resolve.
    pub version: String,
    /// Direct (already filtered) dependencies as `(name, range)`.
    /// Dependencies trực tiếp (đã lọc) dạng `(name, range)`.
    pub deps: Vec<(String, String)>,
    /// Artifact download URL — URL tải artifact.
    pub artifact_url: String,
    /// Declared sha256 (lowercase hex; empty when the registry gave none).
    /// Sha256 khai báo (hex thường; rỗng khi registry không cung cấp).
    pub sha256: String,
    /// Extra markers (cfg tags, wheel tags, optional-dep names, PEP 508 markers).
    /// Marker thêm (tag cfg, tag wheel, tên optional-dep, marker PEP 508).
    pub extra_markers: Vec<String>,
}

/// Contract for a native registry protocol engine (Phase 2).
/// Hợp đồng cho engine registry protocol native (Phase 2).
#[async_trait]
pub trait RegistryProtocol: Send + Sync {
    /// Resolve `name` within `range` against this protocol's registry.
    /// Resolve `name` trong khoảng `range` trên registry của protocol này.
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry>;

    /// Download the artifact bytes for a resolved entry.
    /// Tải byte artifact cho entry đã resolve.
    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>>;

    /// Verify the downloaded artifact's sha256 against the declared digest.
    /// Fail-closed on mismatch AND on a missing digest: an artifact no
    /// registry hash vouches for is never installed (V1.2 zero-trust —
    /// engines with alternative integrity (sha1 markers, sumdb dirhash,
    /// git-commit pins) override this method; see go/maven/nuget/swift).
    /// Xác minh sha256 của artifact đã tải so với digest khai báo.
    /// Fail-closed khi lệch VÀ khi thiếu digest: artifact không có hash
    /// registry bảo lãnh không bao giờ được cài.
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        if entry.sha256.is_empty() {
            return Err(MgError::Integrity(format!(
                "refusing artifact without digest for {}@{} (registry provided no sha256 — re-resolve after the registry publishes checksums)",
                entry.name, entry.version
            )));
        }
        let actual = sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(MgError::Integrity(format!(
                "sha256 mismatch for {}@{}: expected {}, got {}",
                entry.name, entry.version, entry.sha256, actual
            )));
        }
        Ok(())
    }

    /// Compute the mgc-store CAS reference (blake3) for artifact bytes.
    /// The ref matches `IntegrityHash::cas_path` under the store root, so a
    /// downstream `ContentStore::import_bytes` yields the same address.
    /// Tính CAS ref (blake3) của mgc-store cho byte artifact. Ref khớp
    /// `IntegrityHash::cas_path` dưới gốc store, nên `ContentStore::import_bytes`
    /// phía sau sinh cùng địa chỉ.
    fn store_ref(&self, bytes: &[u8]) -> String {
        store_ref_for_blake3(blake3::hash(bytes).to_hex().as_ref())
    }

    /// Full transitive resolution: resolve `name` and every reachable
    /// (already-filtered) dependency once, dedup by package name (the first
    /// chosen version wins, later edges reuse it), in BFS order. Any resolve
    /// error propagates (fail-closed — no silent drop of a subtree).
    /// Resolve bắc cầu đầy đủ: resolve `name` và mọi dependency khả chạm
    /// (đã lọc) đúng một lần, khử trùng theo tên (version chọn đầu thắng,
    /// cạnh sau tái sử dụng), theo thứ tự BFS. Mọi lỗi resolve lan lên
    /// (fail-closed — không âm thầm bỏ cây con).
    async fn resolve_graph(&self, name: &str, range: &str) -> MgResult<Vec<ResolvedEntry>> {
        resolve_graph_with_semaphore(
            self,
            name,
            range,
            Arc::new(Semaphore::new(GRAPH_RESOLVE_CONCURRENCY)),
        )
        .await
    }

    /// Resolve several project roots as one graph. Protocols with a native
    /// multi-constraint solver should override this; the default keeps the
    /// existing fail-closed behavior when independently resolved roots pick
    /// different versions of one transitive package.
    /// Resolve nhiều root của project thành một graph. Protocol có solver
    /// native đa-ràng-buộc nên override; mặc định giữ hành vi fail-closed
    /// khi các root resolve riêng chọn phiên bản transitive khác nhau.
    async fn resolve_graph_roots(
        &self,
        roots: &[(String, String)],
    ) -> MgResult<Vec<ResolvedEntry>> {
        let mut entries = Vec::new();
        let mut chosen = std::collections::HashMap::<String, ResolvedEntry>::new();
        let semaphore = Arc::new(Semaphore::new(GRAPH_RESOLVE_CONCURRENCY));
        let owned_roots = roots.to_vec();
        let mut root_graphs = stream::iter(owned_roots.into_iter().map(|(name, range)| {
            let semaphore = Arc::clone(&semaphore);
            async move { resolve_graph_with_semaphore(self, &name, &range, semaphore).await }
        }))
        .buffered(GRAPH_ROOT_CONCURRENCY);

        while let Some(root_entries) = root_graphs.try_next().await? {
            for entry in root_entries {
                if let Some(existing) = chosen.get(&entry.name) {
                    if existing != &entry {
                        return Err(MgError::DependencyConflict(format!(
                            "registry resolution for {} is inconsistent across root dependencies ({} vs {}); refusing to write an ambiguous lock graph",
                            entry.name, existing.version, entry.version
                        )));
                    }
                } else {
                    chosen.insert(entry.name.clone(), entry.clone());
                    entries.push(entry);
                }
            }
        }
        Ok(entries)
    }
}

async fn resolve_graph_with_semaphore<P: RegistryProtocol + ?Sized>(
    protocol: &P,
    name: &str,
    range: &str,
    semaphore: Arc<Semaphore>,
) -> MgResult<Vec<ResolvedEntry>> {
    let mut entries = Vec::new();
    let mut chosen = std::collections::HashMap::<String, String>::new();
    let mut queue = std::collections::VecDeque::from([(name.to_string(), range.to_string())]);

    while !queue.is_empty() {
        // Resolve peers in this BFS layer concurrently, but process results in
        // queue order. Root graphs in this invocation share one semaphore so
        // their combined requests stay within the protocol-level limit.
        // (Resolve các dependency cùng tầng BFS song song nhưng xử lý theo
        // thứ tự queue. Các root anh em dùng chung semaphore để không nhân
        // giới hạn request của protocol.)
        let layer = queue.drain(..).collect::<Vec<_>>();
        let mut resolved_layer = stream::iter(layer.into_iter().map(|(name, range)| {
            let semaphore = Arc::clone(&semaphore);
            async move {
                let _permit = semaphore.acquire_owned().await.map_err(|_| {
                    MgError::Other("registry graph resolver semaphore closed".to_string())
                })?;
                let entry = protocol.resolve(&name, &range).await?;
                Ok::<_, MgError>((name, range, entry))
            }
        }))
        .buffered(GRAPH_RESOLVE_CONCURRENCY);

        while let Some((name, range, entry)) = resolved_layer.try_next().await? {
            if let Some(chosen_version) = chosen.get(&name) {
                // Re-resolve conflicting constraints and fail closed rather
                // than silently choosing a version without backtracking.
                // (Resolve lại constraint xung đột và fail-closed thay vì âm
                // thầm chọn version khi chưa có backtracking.)
                if entry.version != *chosen_version {
                    return Err(MgError::DependencyConflict(format!(
                        "incompatible constraints for {name}: selected {chosen_version}, but constraint '{range}' resolves to {} (native resolver does not backtrack yet)",
                        entry.version
                    )));
                }
                continue;
            }
            chosen.insert(entry.name.clone(), entry.version.clone());
            for (dep_name, dep_range) in &entry.deps {
                if !chosen.contains_key(dep_name) {
                    queue.push_back((dep_name.clone(), dep_range.clone()));
                }
            }
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// sha256 of bytes as lowercase hex — SỞ HỮU chung cho mọi engine.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Format a blake3 hex digest as the mgc-store CAS ref (`files/blake3/xx/hash`).
/// Định dạng digest blake3 hex thành CAS ref của mgc-store.
pub fn store_ref_for_blake3(hex_digest: &str) -> String {
    let prefix = &hex_digest[..2.min(hex_digest.len())];
    format!("files/blake3/{prefix}/{hex_digest}")
}

/// Phase 2 native engine slot — npm registry. The web adapter keeps its own
/// native resolve/fetch/CAS path; this slot only pins the future seam and
/// fails closed on every call.
/// Chỗ cắm engine native Phase 2 — registry npm. Adapter web giữ đường
/// resolve/fetch/CAS native riêng; slot này chỉ ghim điểm ghép tương lai và
/// fail-closed trên mọi lời gọi.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpmProtocol;

#[async_trait]
impl RegistryProtocol for NpmProtocol {
    async fn resolve(&self, _name: &str, _range: &str) -> MgResult<ResolvedEntry> {
        // Fail closed: a stub must never fake a successful resolution.
        // Fail-closed: stub không bao giờ giả một lần resolve thành công.
        Err(MgError::Unsupported {
            core: "web",
            capability: "resolve",
            guidance:
                "npm native resolver is a Phase 2 slot; the web adapter's native path is unaffected"
                    .to_string(),
        })
    }

    async fn download(&self, _entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        // Fail closed — a stub must never fake a successful download.
        // Fail-closed — stub không bao giờ giả một lần download thành công.
        Err(MgError::Unsupported {
            core: "web",
            capability: "fetch",
            guidance:
                "npm native fetch is a Phase 2 slot; the web adapter's native path is unaffected"
                    .to_string(),
        })
    }
}
