//! Phase 2 native registry protocol slots — typed placeholders only.
//! Chỗ cắm registry protocol native Phase 2 — chỉ là placeholder có type-check.
//!
//! These stubs give the resolver a stable, type-checked seam for future
//! per-ecosystem native engines. They are NOT wired into the web install
//! path (web already resolves/fetches natively) and every call fails closed
//! with `MgError::Unsupported` until Phase 2 ships real engines.
//! Các stub này giữ một điểm ghép ổn định, có type-check cho engine native
//! theo từng ecosystem trong tương lai. KHÔNG wire vào đường install web
//! (web đã resolve/fetch native) và mọi lời gọi fail-closed với
//! `MgError::Unsupported` cho tới khi Phase 2 có engine thật.

use async_trait::async_trait;
use mgc_types::{MgError, MgResult};

/// Minimal resolution payload the future engines return — Payload resolve
/// tối thiểu mà engine tương lai trả về.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackageSpec {
    /// Package name at the registry — Tên package trên registry.
    pub name: String,
    /// Exact resolved version — Version chính xác đã resolve.
    pub version: String,
}

/// Contract for a native registry protocol engine (Phase 2 slot).
/// Hợp đồng cho engine registry protocol native (chỗ cắm Phase 2).
#[async_trait]
pub trait RegistryProtocol: Send + Sync {
    /// Resolve `name` within `range` against this protocol's registry.
    /// Resolve `name` trong khoảng `range` trên registry của protocol này.
    async fn resolve_package(&self, name: &str, range: &str) -> MgResult<ResolvedPackageSpec>;
}

/// Phase 2 native engine slot — npm registry. The web adapter keeps its own
/// native resolve/fetch/CAS path; this slot only pins the future seam.
/// Chỗ cắm engine native Phase 2 — registry npm. Adapter web giữ đường
/// resolve/fetch/CAS native riêng; slot này chỉ ghim điểm ghép tương lai.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpmProtocol;

/// Phase 2 native engine slot — crates.io registry.
/// Chỗ cắm engine native Phase 2 — registry crates.io.
#[derive(Debug, Clone, Copy, Default)]
pub struct CratesProtocol;

/// Phase 2 native engine slot — PyPI registry.
/// Chỗ cắm engine native Phase 2 — registry PyPI.
#[derive(Debug, Clone, Copy, Default)]
pub struct PypiProtocol;

/// Phase 2 native engine slot — pub.dev registry (Dart/Flutter).
/// Chỗ cắm engine native Phase 2 — registry pub.dev (Dart/Flutter).
#[derive(Debug, Clone, Copy, Default)]
pub struct PubProtocol;

#[async_trait]
impl RegistryProtocol for NpmProtocol {
    async fn resolve_package(&self, _name: &str, _range: &str) -> MgResult<ResolvedPackageSpec> {
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
}

#[async_trait]
impl RegistryProtocol for CratesProtocol {
    async fn resolve_package(&self, _name: &str, _range: &str) -> MgResult<ResolvedPackageSpec> {
        // Fail closed — delegated cargo toolchain remains the only path.
        // Fail-closed — toolchain cargo được ủy thác vẫn là đường duy nhất.
        Err(MgError::Unsupported {
            core: "rust",
            capability: "resolve",
            guidance: "crates.io native resolver is a Phase 2 slot; delegate to the cargo toolchain until then"
                .to_string(),
        })
    }
}

#[async_trait]
impl RegistryProtocol for PypiProtocol {
    async fn resolve_package(&self, _name: &str, _range: &str) -> MgResult<ResolvedPackageSpec> {
        // Fail closed — delegated uv/pip toolchain remains the only path.
        // Fail-closed — toolchain uv/pip được ủy thác vẫn là đường duy nhất.
        Err(MgError::Unsupported {
            core: "python",
            capability: "resolve",
            guidance: "PyPI native resolver is a Phase 2 slot; delegate to the uv/pip toolchain until then"
                .to_string(),
        })
    }
}

#[async_trait]
impl RegistryProtocol for PubProtocol {
    async fn resolve_package(&self, _name: &str, _range: &str) -> MgResult<ResolvedPackageSpec> {
        // Fail closed — delegated dart/flutter toolchain remains the only path.
        // Fail-closed — toolchain dart/flutter được ủy thác vẫn là đường duy nhất.
        Err(MgError::Unsupported {
            core: "app",
            capability: "resolve",
            guidance: "pub.dev native resolver is a Phase 2 slot; delegate to the dart/flutter toolchain until then"
                .to_string(),
        })
    }
}
