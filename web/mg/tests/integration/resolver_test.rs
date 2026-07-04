//! Integration tests for the dependency resolver

use mg_core::{Catalog, PackageName};
use mg_resolver::Resolver;

use crate::test_utils::MockDependencyProvider;

#[test]
fn test_basic_resolution() {
    let mut provider = MockDependencyProvider::new();
    provider.add_package("react", "18.2.0", vec![]);
    provider.add_package("react", "18.1.0", vec![]);
    provider.add_package("react", "17.0.0", vec![]);

    let resolver = Resolver::new(std::sync::Arc::new(provider));
    let wanted = vec![(PackageName::new("react").unwrap(), "^18.0.0".to_string())];
    let result = resolver.solve(&wanted).unwrap();
    assert_eq!(result.resolutions.len(), 1);
    assert_eq!(result.resolutions[0].version.to_string(), "18.2.0");
}

#[test]
fn test_catalog_pinning() {
    let mut provider = MockDependencyProvider::new();
    provider.add_package("react", "19.0.0", vec![]);
    provider.add_package("react", "18.2.0", vec![]);

    let mut resolver = Resolver::new(std::sync::Arc::new(provider));
    let mut catalog = Catalog::default();
    catalog.set("react", "18.2.0");
    let mut catalogs = std::collections::HashMap::new();
    catalogs.insert("default".to_string(), catalog);
    resolver.set_catalogs(catalogs);

    let version = resolver.resolve_catalog("react", "default").unwrap();
    assert_eq!(version.to_string(), "18.2.0");
}

#[test]
fn test_workspace_resolution() {
    use mg_resolver::solver::{WorkspaceInfo, WorkspaceMemberInfo};
    use std::path::PathBuf;

    let provider = MockDependencyProvider::new();
    let resolver = Resolver::new(std::sync::Arc::new(provider));

    let workspace = WorkspaceInfo::new(vec![WorkspaceMemberInfo {
        name: "my-pkg".to_string(),
        path: PathBuf::from("/workspace/packages/my-pkg"),
        version: "1.0.0".to_string(),
    }]);

    let wanted = vec![
        (
            PackageName::new("my-pkg").unwrap(),
            "workspace:*".to_string(),
        ),
        (PackageName::new("lodash").unwrap(), "^4.0.0".to_string()),
    ];

    let result = resolver
        .resolve_with_workspace(&wanted, &workspace)
        .unwrap();
    assert!(result
        .resolutions
        .iter()
        .any(|r| r.package_id.name().as_str() == "my-pkg"));
}

#[test]
fn test_override_injection() {
    let mut provider = MockDependencyProvider::new();
    provider.add_package("react", "18.2.0", vec![]);
    provider.add_package("react", "17.0.0", vec![]);

    let mut resolver = Resolver::new(std::sync::Arc::new(provider));
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("react".to_string(), "17.0.0".to_string());
    resolver.set_overrides(overrides);
    resolver.set_catalogs(std::collections::HashMap::new());

    let wanted = vec![(PackageName::new("react").unwrap(), "^18.0.0".to_string())];
    let result = resolver.solve(&wanted).unwrap();
    assert_eq!(result.resolutions.len(), 1);
}

#[test]
fn test_resolve_nonexistent_returns_empty() {
    let provider = MockDependencyProvider::new();
    let resolver = Resolver::new(std::sync::Arc::new(provider));

    let wanted = vec![(
        PackageName::new("nonexistent").unwrap(),
        "^1.0.0".to_string(),
    )];
    let result = resolver.solve(&wanted).unwrap();
    assert!(result.resolutions.is_empty());
}

#[test]
fn test_catalog_not_found() {
    let provider = MockDependencyProvider::new();
    let resolver = Resolver::new(std::sync::Arc::new(provider));
    let result = resolver.resolve_catalog("react", "missing-catalog");
    assert!(result.is_err());
}
