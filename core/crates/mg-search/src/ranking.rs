//! Ranking algorithm for search results
//! Thuật toán xếp hạng cho kết quả tìm kiếm

use crate::types::{ProjectContext, Registry, SearchResult};

/// Rank search results based on multiple factors
/// Xếp hạng kết quả tìm kiếm dựa trên nhiều yếu tố
///
/// Factors (Các yếu tố):
/// - Name match quality (0-100): exact > partial > fuzzy
/// - Project context boost (0-50): matching registry gets boost
/// - Popularity (0-30): stars/downloads (log scale)
/// - Freshness (0-10): recently updated packages
/// - Quality (0-10): npm quality score
pub fn rank_results(results: &mut [SearchResult], context: &ProjectContext, query: &str) {
    let query_lower = query.to_lowercase();
    
    for result in results.iter_mut() {
        let mut score = 0.0;
        
        // 1. Name Match (0-100)
        // 1. Khớp tên (0-100)
        let name_lower = result.name.to_lowercase();
        if name_lower == query_lower {
            score += 100.0; // Exact match — khớp chính xác
        } else if name_lower.contains(&query_lower) {
            score += 50.0; // Partial match — khớp một phần
        } else {
            score += 20.0; // Fuzzy match — khớp mờ
        }
        
        // 2. Project Context Boost (0-50)
        // 2. Tăng điểm theo context dự án (0-50)
        if context_matches(&result.registry, &context.core) {
            score += 50.0;
        }
        
        // 3. Popularity (0-30)
        // 3. Độ phổ biến (0-30)
        if let Some(stars) = result.metadata.stars {
            // Log scale: 10k stars → 12 points, 100k → 15 points
            // Thang log: 10k stars → 12 điểm, 100k → 15 điểm
            score += (stars as f64).log10() * 3.0;
        }
        if let Some(downloads) = result.metadata.downloads {
            // Log scale: 1M downloads → 12 points
            // Thang log: 1M lượt tải → 12 điểm
            score += (downloads as f64).log10() * 2.0;
        }
        
        // 4. Freshness (0-10)
        // 4. Độ mới (0-10)
        let freshness_score = parse_updated_score(&result.metadata.updated);
        score += freshness_score;
        
        // 5. Quality (0-10, npm only)
        // 5. Chất lượng (0-10, chỉ npm)
        if let Some(quality) = result.metadata.quality {
            score += (quality as f64) / 10.0; // Convert 0-100 to 0-10
        }
        
        result.score = score;
    }
    
    // Sort by score descending
    // Sắp xếp theo điểm giảm dần
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
}

/// Check if registry matches project core
/// Kiểm tra registry có khớp với core dự án không
fn context_matches(registry: &Registry, core: &str) -> bool {
    matches!(
        (registry, core),
        (Registry::Npm, "web") |
        (Registry::Crates, "lib") |
        (Registry::Crates, "game") |
        (Registry::Crates, "iot") |
        (Registry::Go, "cloud") |
        (Registry::PyPI, "ai")
    )
}

/// Parse "X days/weeks/months ago" to freshness score (0-10)
/// Parse "X ngày/tuần/tháng trước" thành điểm độ mới (0-10)
fn parse_updated_score(updated_str: &str) -> f64 {
    if updated_str.contains("days ago") || updated_str.contains("day ago") {
        let days: u32 = updated_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        
        if days < 7 {
            10.0
        } else if days < 30 {
            5.0
        } else {
            2.0
        }
    } else if updated_str.contains("weeks ago") || updated_str.contains("week ago") {
        let weeks: u32 = updated_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4);
        
        if weeks < 2 {
            8.0
        } else if weeks < 4 {
            5.0
        } else {
            2.0
        }
    } else if updated_str.contains("months ago") || updated_str.contains("month ago") {
        let months: u32 = updated_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(6);
        
        if months < 3 {
            5.0
        } else if months < 6 {
            3.0
        } else {
            1.0
        }
    } else {
        0.0 // Years ago or unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ResultMetadata;

    #[test]
    fn test_context_matches() {
        assert!(context_matches(&Registry::Npm, "web"));
        assert!(context_matches(&Registry::Crates, "lib"));
        assert!(context_matches(&Registry::Crates, "game"));
        assert!(context_matches(&Registry::Go, "cloud"));
        assert!(context_matches(&Registry::PyPI, "ai"));
        assert!(!context_matches(&Registry::Npm, "game"));
    }

    #[test]
    fn test_parse_updated_score() {
        assert_eq!(parse_updated_score("3 days ago"), 10.0);
        assert_eq!(parse_updated_score("2 weeks ago"), 5.0);
        assert_eq!(parse_updated_score("1 month ago"), 5.0);
        assert_eq!(parse_updated_score("1 year ago"), 0.0);
    }

    #[test]
    fn test_rank_results_exact_match() {
        let context = ProjectContext {
            core: "web".to_string(),
            signatures: vec!["package.json".to_string()],
        };
        
        let mut results = vec![
            SearchResult {
                name: "zod".to_string(),
                registry: Registry::Npm,
                full_path: "zod".to_string(),
                version: "3.22.4".to_string(),
                description: "TypeScript-first schema validation".to_string(),
                metadata: ResultMetadata {
                    downloads: Some(10_000_000),
                    stars: None,
                    updated: "1 week ago".to_string(),
                    quality: Some(95.0),
                },
                score: 0.0,
            },
        ];
        
        rank_results(&mut results, &context, "zod");
        
        // Exact match (100) + context (50) + downloads (14) + freshness (8) + quality (9.5) = 181.5
        assert!(results[0].score > 170.0);
    }
}
