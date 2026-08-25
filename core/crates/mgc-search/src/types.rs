//! Core data types for multi-registry search
//! Các kiểu dữ liệu cốt lõi cho tìm kiếm đa registry

use serde::{Deserialize, Serialize};
use std::fmt;

/// Search query with project context
/// Query tìm kiếm kèm context dự án
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Search term (e.g., "gin", "cors", "axum")
    /// Từ khóa tìm kiếm
    pub query: String,

    /// Project context for smart ranking
    /// Context dự án để xếp hạng thông minh
    pub context: ProjectContext,
}

/// Project context extracted from current directory
/// Context dự án lấy từ thư mục hiện tại
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Core type (web, game, ai, cloud, etc.)
    /// Loại core (web, game, ai, cloud, v.v.)
    pub core: String,

    /// Signature files found (package.json, Cargo.toml, go.mod, etc.)
    /// Các file signature tìm thấy
    pub signatures: Vec<String>,
}

/// Registry enum - supported package registries
/// Enum Registry - các registry package được hỗ trợ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Registry {
    /// npm registry (Node.js/TypeScript packages)
    /// Registry npm (packages Node.js/TypeScript)
    Npm,

    /// crates.io (Rust packages)
    /// crates.io (packages Rust)
    Crates,

    /// pkg.go.dev (Go modules)
    /// pkg.go.dev (modules Go)
    Go,

    /// PyPI (Python packages)
    /// PyPI (packages Python)
    PyPI,
}

impl Registry {
    /// Get registry name as string
    /// Lấy tên registry dạng chuỗi
    pub fn as_str(&self) -> &'static str {
        match self {
            Registry::Npm => "npm",
            Registry::Crates => "crates",
            Registry::Go => "go",
            Registry::PyPI => "pypi",
        }
    }

    /// Parse registry from string
    /// Parse registry từ chuỗi
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "npm" => Some(Registry::Npm),
            "crates" | "crates.io" => Some(Registry::Crates),
            "go" | "pkg.go.dev" => Some(Registry::Go),
            "pypi" | "pip" => Some(Registry::PyPI),
            _ => None,
        }
    }
}

impl fmt::Display for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Search result from a registry
/// Kết quả tìm kiếm từ một registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Package name
    /// Tên package
    pub name: String,

    /// Registry source
    /// Registry nguồn
    pub registry: Registry,

    /// Full package path (e.g., "github.com/gin-gonic/gin" for Go)
    /// Đường dẫn package đầy đủ
    pub full_path: String,

    /// Latest version
    /// Phiên bản mới nhất
    pub version: String,

    /// Package description
    /// Mô tả package
    pub description: String,

    /// Metadata (downloads, stars, updated date)
    /// Metadata (lượt tải, stars, ngày cập nhật)
    pub metadata: ResultMetadata,

    /// Ranking score (0-100+)
    /// Điểm xếp hạng (0-100+)
    pub score: f64,
}

/// Result metadata - popularity and quality indicators
/// Metadata kết quả - chỉ số phổ biến và chất lượng
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultMetadata {
    /// Monthly downloads (npm, PyPI)
    /// Lượt tải hàng tháng (npm, PyPI)
    pub downloads: Option<u64>,

    /// GitHub stars (Go, crates.io)
    /// Stars GitHub (Go, crates.io)
    pub stars: Option<u64>,

    /// Last updated date (ISO 8601 or relative "2 weeks ago")
    /// Ngày cập nhật cuối (ISO 8601 hoặc tương đối "2 tuần trước")
    pub updated: String,

    /// Quality score (0.0-1.0, npm only)
    /// Điểm chất lượng (0.0-1.0, chỉ npm)
    pub quality: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_as_str() {
        assert_eq!(Registry::Npm.as_str(), "npm");
        assert_eq!(Registry::Crates.as_str(), "crates");
        assert_eq!(Registry::Go.as_str(), "go");
        assert_eq!(Registry::PyPI.as_str(), "pypi");
    }

    #[test]
    fn test_registry_from_str() {
        assert_eq!(Registry::from_str("npm"), Some(Registry::Npm));
        assert_eq!(Registry::from_str("NPM"), Some(Registry::Npm));
        assert_eq!(Registry::from_str("crates.io"), Some(Registry::Crates));
        assert_eq!(Registry::from_str("go"), Some(Registry::Go));
        assert_eq!(Registry::from_str("pkg.go.dev"), Some(Registry::Go));
        assert_eq!(Registry::from_str("pypi"), Some(Registry::PyPI));
        assert_eq!(Registry::from_str("pip"), Some(Registry::PyPI));
        assert_eq!(Registry::from_str("unknown"), None);
    }

    #[test]
    fn test_registry_display() {
        assert_eq!(format!("{}", Registry::Npm), "npm");
        assert_eq!(format!("{}", Registry::Crates), "crates");
        assert_eq!(format!("{}", Registry::Go), "go");
        assert_eq!(format!("{}", Registry::PyPI), "pypi");
    }
}
