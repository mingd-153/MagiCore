//! CycloneDX SBOM schema implementation
//! Implementation schema CycloneDX SBOM

use serde::{Deserialize, Serialize};

/// CycloneDX BOM (Bill of Materials) — BOM CycloneDX
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bom {
    /// BOM format (always "CycloneDX") — Định dạng BOM
    pub bom_format: String,

    /// Spec version (e.g., "1.5") — Phiên bản spec
    pub spec_version: String,

    /// Serial number (UUID) — Số serial (UUID)
    pub serial_number: String,

    /// Version (increment for updates) — Phiên bản (tăng khi cập nhật)
    pub version: u32,

    /// Metadata — Metadata
    pub metadata: Metadata,

    /// Components list — Danh sách components
    pub components: Vec<Component>,

    /// Dependencies graph — Đồ thị dependencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<Dependency>>,
}

/// Metadata — Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// Timestamp (ISO 8601) — Timestamp (ISO 8601)
    pub timestamp: String,

    /// Tools used to generate BOM — Công cụ tạo BOM
    pub tools: Vec<Tool>,

    /// Component being described — Component được mô tả
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<Component>,
}

/// Tool — Công cụ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Vendor — Nhà cung cấp
    pub vendor: String,

    /// Name — Tên
    pub name: String,

    /// Version — Phiên bản
    pub version: String,
}

/// Component — Component
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Type (library, application, framework, etc.) — Loại
    #[serde(rename = "type")]
    pub component_type: ComponentType,

    /// BOM-Ref (unique ID) — BOM-Ref (ID duy nhất)
    pub bom_ref: String,

    /// Name — Tên
    pub name: String,

    /// Version — Phiên bản
    pub version: String,

    /// Package URL (purl) — URL package (purl)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,

    /// Hashes — Hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<Vec<Hash>>,

    /// Licenses — Giấy phép
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licenses: Option<Vec<License>>,
}

/// Component type — Loại component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentType {
    Application,
    Framework,
    Library,
    Container,
    Device,
    File,
}

/// Hash — Hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hash {
    /// Algorithm (SHA-256, BLAKE3, etc.) — Thuật toán
    pub alg: String,

    /// Content (hex) — Nội dung (hex)
    pub content: String,
}

/// License — Giấy phép
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// License ID (SPDX) — ID giấy phép (SPDX)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// License name — Tên giấy phép
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Dependency — Dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    /// Reference to component — Tham chiếu đến component
    #[serde(rename = "ref")]
    pub dependency_ref: String,

    /// List of dependencies — Danh sách dependencies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}

impl Bom {
    /// Create new BOM — Tạo BOM mới
    pub fn new() -> Self {
        Self {
            bom_format: "CycloneDX".to_string(),
            spec_version: "1.5".to_string(),
            serial_number: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
            version: 1,
            metadata: Metadata {
                timestamp: chrono::Utc::now().to_rfc3339(),
                tools: vec![Tool {
                    vendor: "MagiCore".to_string(),
                    name: "mgc".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                }],
                component: None,
            },
            components: Vec::new(),
            dependencies: None,
        }
    }
}

impl Default for Bom {
    fn default() -> Self {
        Self::new()
    }
}
