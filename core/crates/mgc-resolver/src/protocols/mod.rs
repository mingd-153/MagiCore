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
#[path = "pub.rs"]
pub mod pubdev;
pub mod pypi;
pub mod zip_reader;

pub use crates::CratesProtocol;
pub use go::GoModProtocol;
pub use pubdev::PubProtocol;
pub use pypi::PypiProtocol;

use async_trait::async_trait;
use mgc_types::{MgError, MgResult};
use sha2::{Digest, Sha256};

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
    /// Fail-closed on mismatch. An empty declared digest is NOT a failure —
    /// there is simply no registry hash to check against (the CAS import in
    /// the install path still records the blake3 truth).
    /// Xác minh sha256 của artifact đã tải so với digest khai báo.
    /// Fail-closed khi lệch. Digest rỗng KHÔNG phải lỗi — chỉ là registry
    /// không cho hash để check (import CAS ở đường install vẫn ghi blake3).
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        if entry.sha256.is_empty() {
            return Ok(());
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
        let mut entries: Vec<ResolvedEntry> = Vec::new();
        let mut chosen: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut queue: std::collections::VecDeque<(String, String)> =
            std::collections::VecDeque::new();
        queue.push_back((name.to_string(), range.to_string()));

        while let Some((n, r)) = queue.pop_front() {
            if chosen.contains_key(&n) {
                continue;
            }
            let entry = self.resolve(&n, &r).await?;
            chosen.insert(entry.name.clone(), entry.version.clone());
            for (dep_name, dep_range) in &entry.deps {
                if !chosen.contains_key(dep_name) {
                    queue.push_back((dep_name.clone(), dep_range.clone()));
                }
            }
            entries.push(entry);
        }
        Ok(entries)
    }
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
