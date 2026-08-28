//! Tests for ranking algorithm
//! Tests cho thuật toán xếp hạng

use mgc_search::ranking::rank_results;
use mgc_search::types::*;

#[test]
fn test_rank_results_exact_match() {
    let context = ProjectContext {
        core: "web".to_string(),
        signatures: vec!["package.json".to_string()],
    };

    let mut results = vec![SearchResult {
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
    }];

    rank_results(&mut results, &context, "zod");

    // Exact match (100) + context (50) + downloads (~14) + freshness (8) + quality (9.5) = ~181
    assert!(results[0].score > 170.0);
    assert!(results[0].score < 190.0);
}

#[test]
fn test_rank_results_partial_match() {
    let context = ProjectContext {
        core: "web".to_string(),
        signatures: vec!["package.json".to_string()],
    };

    let mut results = vec![SearchResult {
        name: "axum-core".to_string(),
        registry: Registry::Crates,
        full_path: "axum-core".to_string(),
        version: "0.4.0".to_string(),
        description: "Core types for axum".to_string(),
        metadata: ResultMetadata {
            downloads: None,
            stars: Some(15000),
            updated: "2 weeks ago".to_string(),
            quality: None,
        },
        score: 0.0,
    }];

    rank_results(&mut results, &context, "axum");

    // Partial match (50) + no context (0) + stars (~12) + freshness (5) = ~67
    assert!(results[0].score > 60.0);
    assert!(results[0].score < 75.0);
}

#[test]
fn test_rank_results_context_boost() {
    let context = ProjectContext {
        core: "cloud".to_string(),
        signatures: vec!["go.mod".to_string()],
    };

    let mut results = vec![
        SearchResult {
            name: "gin".to_string(),
            registry: Registry::Go,
            full_path: "github.com/gin-gonic/gin".to_string(),
            version: "v1.9.0".to_string(),
            description: "Gin web framework".to_string(),
            metadata: ResultMetadata {
                downloads: None,
                stars: Some(75000),
                updated: "1 day ago".to_string(),
                quality: None,
            },
            score: 0.0,
        },
        SearchResult {
            name: "gin".to_string(),
            registry: Registry::Npm,
            full_path: "gin".to_string(),
            version: "0.1.0".to_string(),
            description: "Git-based package manager".to_string(),
            metadata: ResultMetadata {
                downloads: Some(5000),
                stars: None,
                updated: "2 years ago".to_string(),
                quality: Some(50.0),
            },
            score: 0.0,
        },
    ];

    rank_results(&mut results, &context, "gin");

    // Go package should rank higher due to context boost
    // Package Go nên xếp hạng cao hơn do context boost
    assert_eq!(results[0].registry, Registry::Go);
    assert!(results[0].score > results[1].score);
}

#[test]
fn test_rank_results_sorting() {
    let context = ProjectContext {
        core: "web".to_string(),
        signatures: vec!["package.json".to_string()],
    };

    let mut results = vec![
        SearchResult {
            name: "low-score".to_string(),
            registry: Registry::Npm,
            full_path: "low-score".to_string(),
            version: "1.0.0".to_string(),
            description: "Low score package".to_string(),
            metadata: ResultMetadata::default(),
            score: 0.0,
        },
        SearchResult {
            name: "high-score".to_string(),
            registry: Registry::Npm,
            full_path: "high-score".to_string(),
            version: "1.0.0".to_string(),
            description: "High score package".to_string(),
            metadata: ResultMetadata {
                downloads: Some(100_000_000),
                stars: None,
                updated: "1 day ago".to_string(),
                quality: Some(100.0),
            },
            score: 0.0,
        },
    ];

    rank_results(&mut results, &context, "test");

    // Results should be sorted by score descending
    // Kết quả nên được sắp xếp theo điểm giảm dần
    assert!(results[0].score > results[1].score);
    assert_eq!(results[0].name, "high-score");
}
