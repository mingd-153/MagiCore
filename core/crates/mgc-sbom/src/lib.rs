//! MagiCore SBOM (Software Bill of Materials) Generator
//! Tạo SBOM (Danh sách vật liệu phần mềm) theo chuẩn CycloneDX

use anyhow::Result;

pub mod cyclonedx;
pub mod generator;

pub use cyclonedx::{Bom, Component, ComponentType, Dependency};
pub use generator::SbomGenerator;

/// SBOM format — Định dạng SBOM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbomFormat {
    /// CycloneDX JSON format — Định dạng CycloneDX JSON
    CycloneDx,
    /// SPDX format (future) — Định dạng SPDX (tương lai)
    Spdx,
}

/// SBOM generation options — Tùy chọn tạo SBOM
#[derive(Debug, Clone)]
pub struct SbomOptions {
    /// Include dev dependencies — Bao gồm dev dependencies
    pub include_dev: bool,
    /// Include licenses — Bao gồm giấy phép
    pub include_licenses: bool,
    /// Include hashes — Bao gồm hash
    pub include_hashes: bool,
    /// SBOM format — Định dạng SBOM
    pub format: SbomFormat,
}

impl Default for SbomOptions {
    fn default() -> Self {
        Self {
            include_dev: false,
            include_licenses: true,
            include_hashes: true,
            format: SbomFormat::CycloneDx,
        }
    }
}

/// SBOM error — Lỗi SBOM
#[derive(Debug, thiserror::Error)]
pub enum SbomError {
    #[error("Invalid lockfile: {0}")]
    InvalidLockfile(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type SbomResult<T> = Result<T, SbomError>;
