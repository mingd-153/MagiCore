//! Tests for search orchestrator
//! Tests cho search orchestrator

use mg_search::{SearchClient, SearchOrchestrator};
use mg_search::types::*;
use async_trait::async_trait;
use std::sync::Arc;

struct MockClient {
    registry: Registry,
    results: Vec<SearchResult>,
}

#[async_trait]
impl SearchClient for MockClient {
    async fn search(&self, _query: &str) -> anyhow::Result<Vec<SearchResult>> {
        Ok(self.results.clone())
    }
    
    fn registry(&self) -> Registry {
        self.registry
    }
}

#[tokio::test]
async fn test_orchestrator_search_all() {
    let mock_result = SearchResult {
        name: "test-package".to_string(),
        registry: Registry::Npm,
        full_path: "test-package".to_string(),
        version: "1.0.0".to_string(),
        description: "Test package".to_string(),
        metadata: ResultMetadata::default(),
        score: 90.0,
    };
    
    let client = Arc::new(MockClient {
        registry: Registry::Npm,
        results: vec![mock_result],
    }) as Arc<dyn SearchClient>;
    
    let orchestrator = SearchOrchestrator::new(vec![client]);
    
    let query = SearchQuery {
        query: "test".to_string(),
        context: ProjectContext {
            core: "web".to_string(),
            signatures: vec!["package.json".to_string()],
        },
    };
    
    let results = orchestrator.search_all(&query).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "test-package");
}

#[tokio::test]
async fn test_orchestrator_multiple_clients() {
    let npm_result = SearchResult {
        name: "npm-package".to_string(),
        registry: Registry::Npm,
        full_path: "npm-package".to_string(),
        version: "1.0.0".to_string(),
        description: "NPM package".to_string(),
        metadata: ResultMetadata::default(),
        score: 80.0,
    };
    
    let go_result = SearchResult {
        name: "go-package".to_string(),
        registry: Registry::Go,
        full_path: "github.com/user/go-package".to_string(),
        version: "v1.0.0".to_string(),
        description: "Go package".to_string(),
        metadata: ResultMetadata::default(),
        score: 85.0,
    };
    
    let npm_client = Arc::new(MockClient {
        registry: Registry::Npm,
        results: vec![npm_result],
    }) as Arc<dyn SearchClient>;
    
    let go_client = Arc::new(MockClient {
        registry: Registry::Go,
        results: vec![go_result],
    }) as Arc<dyn SearchClient>;
    
    let orchestrator = SearchOrchestrator::new(vec![npm_client, go_client]);
    
    let query = SearchQuery {
        query: "package".to_string(),
        context: ProjectContext {
            core: "web".to_string(),
            signatures: vec!["package.json".to_string()],
        },
    };
    
    let results = orchestrator.search_all(&query).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_orchestrator_empty_results() {
    let client = Arc::new(MockClient {
        registry: Registry::Npm,
        results: vec![],
    }) as Arc<dyn SearchClient>;
    
    let orchestrator = SearchOrchestrator::new(vec![client]);
    
    let query = SearchQuery {
        query: "nonexistent".to_string(),
        context: ProjectContext {
            core: "web".to_string(),
            signatures: vec!["package.json".to_string()],
        },
    };
    
    let result = orchestrator.search_all(&query).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No packages found"));
}
