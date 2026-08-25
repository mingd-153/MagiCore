//! Tests for types module
//! Tests cho module types

use mgc_search::types::*;
use std::str::FromStr;

#[test]
fn test_registry_as_str() {
    assert_eq!(Registry::Npm.as_str(), "npm");
    assert_eq!(Registry::Crates.as_str(), "crates");
    assert_eq!(Registry::Go.as_str(), "go");
    assert_eq!(Registry::PyPI.as_str(), "pypi");
}

#[test]
fn test_registry_from_str() {
    assert_eq!(Registry::from_str("npm"), Ok(Registry::Npm));
    assert_eq!(Registry::from_str("NPM"), Ok(Registry::Npm));
    assert_eq!(Registry::from_str("crates"), Ok(Registry::Crates));
    assert_eq!(Registry::from_str("crates.io"), Ok(Registry::Crates));
    assert_eq!(Registry::from_str("go"), Ok(Registry::Go));
    assert_eq!(Registry::from_str("pkg.go.dev"), Ok(Registry::Go));
    assert_eq!(Registry::from_str("pypi"), Ok(Registry::PyPI));
    assert_eq!(Registry::from_str("pip"), Ok(Registry::PyPI));
    assert!(Registry::from_str("unknown").is_err());
}

#[test]
fn test_registry_display() {
    assert_eq!(format!("{}", Registry::Npm), "npm");
    assert_eq!(format!("{}", Registry::Crates), "crates");
    assert_eq!(format!("{}", Registry::Go), "go");
    assert_eq!(format!("{}", Registry::PyPI), "pypi");
}

#[test]
fn test_search_query_serialization() {
    let query = SearchQuery {
        query: "test".to_string(),
        context: ProjectContext {
            core: "web".to_string(),
            signatures: vec!["package.json".to_string()],
        },
    };

    let json = serde_json::to_string(&query).unwrap();
    let deserialized: SearchQuery = serde_json::from_str(&json).unwrap();

    assert_eq!(query.query, deserialized.query);
    assert_eq!(query.context.core, deserialized.context.core);
}

#[test]
fn test_search_result_serialization() {
    let result = SearchResult {
        name: "test-package".to_string(),
        registry: Registry::Npm,
        full_path: "test-package".to_string(),
        version: "1.0.0".to_string(),
        description: "Test package".to_string(),
        metadata: ResultMetadata {
            downloads: Some(10000),
            stars: Some(500),
            updated: "1 week ago".to_string(),
            quality: Some(90.0),
        },
        score: 95.5,
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: SearchResult = serde_json::from_str(&json).unwrap();

    assert_eq!(result.name, deserialized.name);
    assert_eq!(result.registry, deserialized.registry);
    assert_eq!(result.score, deserialized.score);
}

#[test]
fn test_result_metadata_default() {
    let metadata = ResultMetadata::default();

    assert!(metadata.downloads.is_none());
    assert!(metadata.stars.is_none());
    assert_eq!(metadata.updated, "");
    assert!(metadata.quality.is_none());
}
