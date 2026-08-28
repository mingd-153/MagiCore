//! Search orchestrator - coordinates parallel searches across registries
//! Orchestrator tìm kiếm - điều phối tìm kiếm song song qua các registry

use crate::client::SearchClient;
use crate::types::{SearchQuery, SearchResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Search orchestrator - manages multiple search clients
/// Orchestrator tìm kiếm - quản lý nhiều search client
pub struct SearchOrchestrator {
    clients: Vec<Arc<dyn SearchClient>>,
}

impl SearchOrchestrator {
    /// Create new orchestrator with search clients
    /// Tạo orchestrator mới với các search client
    pub fn new(clients: Vec<Arc<dyn SearchClient>>) -> Self {
        Self { clients }
    }

    /// Search all registries in parallel
    /// Tìm kiếm tất cả registry song song
    ///
    /// # Arguments
    /// * `query` - Search query with project context
    ///
    /// # Returns
    /// * `Ok(Vec<SearchResult>)` - All results from all registries (unsorted)
    /// * `Err` - If all searches fail
    pub async fn search_all(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let query_str = query.query.clone();

        // Spawn parallel search tasks with timeout
        // Spawn các task tìm kiếm song song với timeout
        let tasks: Vec<_> = self
            .clients
            .iter()
            .map(|client| {
                let client = Arc::clone(client);
                let query = query_str.clone();

                tokio::spawn(async move {
                    // Timeout 2s per registry
                    // Timeout 2s cho mỗi registry
                    match timeout(Duration::from_secs(2), client.search(&query)).await {
                        Ok(Ok(results)) => results,
                        Ok(Err(e)) => {
                            eprintln!("Search failed for {}: {}", client.registry(), e);
                            vec![]
                        }
                        Err(_) => {
                            eprintln!("Search timeout for {}", client.registry());
                            vec![]
                        }
                    }
                })
            })
            .collect();

        // Collect all results
        // Thu thập tất cả kết quả
        let mut all_results = Vec::new();
        for task in tasks {
            if let Ok(results) = task.await {
                all_results.extend(results);
            }
        }

        if all_results.is_empty() {
            anyhow::bail!("No packages found for '{}'", query_str);
        }

        // Apply ranking algorithm
        // Áp dụng thuật toán xếp hạng
        crate::ranking::rank_results(&mut all_results, &query.context, &query.query);

        // Sort by score descending
        // Sắp xếp theo điểm giảm dần
        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results to top 20
        // Giới hạn kết quả top 20
        all_results.truncate(20);

        Ok(all_results)
    }
}

