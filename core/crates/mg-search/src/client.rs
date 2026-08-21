//! SearchClient trait and implementations for different registries
//! Trait SearchClient và các implementation cho các registry khác nhau

use crate::types::{Registry, SearchResult};
use anyhow::Result;
use async_trait::async_trait;

/// Search client trait - implemented by each registry adapter
/// Trait search client - implement bởi từng adapter registry
#[async_trait]
pub trait SearchClient: Send + Sync {
    /// Search packages in this registry
    /// Tìm kiếm packages trong registry này
    ///
    /// # Arguments
    /// * `query` - Search term (e.g., "gin", "cors")
    ///
    /// # Returns
    /// * `Ok(Vec<SearchResult>)` - List of matching packages (max 10)
    /// * `Err` - Network error, API error, or timeout
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>>;
    
    /// Get the registry this client searches
    /// Lấy registry mà client này tìm kiếm
    fn registry(&self) -> Registry;
    
    /// Optional: check if API is reachable
    /// Tùy chọn: kiểm tra API có thể truy cập không
    async fn health_check(&self) -> Result<bool> {
        Ok(true)  // Default: assume healthy
    }
}
