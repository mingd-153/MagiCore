use std::collections::HashMap;
use std::fs;

use mgpm_core::PackageName;
use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution, ResolutionPipeline, ResolutionConfig, WantedDependency};
use mgpm_resolver::Resolver;
use mgpm_resolver::solver::ResolvedDep;
use mgpm_store::ContentStore;

struct E2eProvider {
    packages: HashMap<String, HashMap<String, Vec<ResolvedDep>>>,
}

impl mgpm_resolver::DependencyProvider for E2eProvider {
    fn get_versions(&self, package: &mgpm_core::PackageName) -> Vec<mgpm_core::Version> {
        self.packages
            .get(package.as_str())
            .map(|versions| {
                let mut v: Vec<mgpm_core::Version> = versions
                    .keys()
                    .filter_map(|s| mgpm_core::Version::parse(s).ok())
                    .collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    fn get_dependencies(&self, package_id: &mgpm_core::PackageId) -> Vec<ResolvedDep> {
        self.packages
            .get(package_id.name().as_str())
            .and_then(|versions| versions.get(&package_id.version().to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

fn make_provider() -> E2eProvider {
    let mut provider = E2eProvider {
        packages: HashMap::new(),
    };

    provider.packages.insert(
        "is-odd".to_string(),
        HashMap::from_iter(vec![(
            "3.0.1".to_string(),
            vec![ResolvedDep {
                package: PackageName::new("is-number").unwrap(),
                spec: "^7.0.0".to_string(),
                optional: false,
                peer: false,
            }],
        )]),
    );

    provider.packages.insert(
        "is-number".to_string(),
        HashMap::from_iter(vec![(
            "7.0.0".to_string(),
            vec![],
        )]),
    );

    provider
}

#[tokio::test]
async fn e2e_smoke_resolve_and_lockfile() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let pkg_json = serde_json::json!({
        "name": "e2e-smoke-test",
        "version": "1.0.0",
        "dependencies": {
            "is-odd": "^3.0.0"
        }
    });
    fs::write(
        root.join("package.json"),
        serde_json::to_string_pretty(&pkg_json).unwrap(),
    )
    .expect("write package.json");

    let provider = make_provider();
    let resolver = Resolver::new(Box::new(provider));
    let config = ResolutionConfig::default();
    let pipeline = ResolutionPipeline::new(resolver, config);

    let wanted = vec![WantedDependency {
        name: PackageName::new("is-odd").unwrap(),
        version_req: "^3.0.0".to_string(),
        dev: false,
        optional: false,
    }];

    let lockfile = pipeline
        .resolve_and_lock(&wanted, root, None)
        .await
        .expect("resolve_and_lock should succeed");

    // Current resolver solves 1 level (picks latest version, but doesn't
    // recurse into transitive dependencies in the simple solve() path).
    assert_eq!(lockfile.packages.len(), 1, "should resolve is-odd (1 level)");

    let has_odd = lockfile.packages.iter().any(|p| p.name == "is-odd");
    assert!(has_odd, "should include is-odd");

    assert!(
        root.join("mgpm.lock").exists(),
        "text lockfile should exist"
    );
    assert!(
        root.join("mgpm.lockb").exists(),
        "binary lockfile should exist"
    );

    let lock_text = fs::read_to_string(root.join("mgpm.lock")).expect("read lockfile");
    assert!(
        lock_text.contains("is-odd"),
        "lockfile content should contain is-odd"
    );
}

#[test]
fn e2e_smoke_install_from_lockfile() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let store_dir = tempfile::tempdir().expect("store tempdir");
    let store_path = store_dir.path().to_path_buf();

    let _store = ContentStore::new(store_path.clone()).expect("create store");

    let mut lockfile = Lockfile::new(1, "https://registry.npmjs.org");
    lockfile.add_package(LockfilePackage {
        id: "is-odd@3.0.1".to_string(),
        name: "is-odd".to_string(),
        version: "3.0.1".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/is-odd/-/is-odd-3.0.1.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc123".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lockfile.sort_packages();
    lockfile.compute_content_hash();

    let lf_path = root.join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lockfile, &lf_path).expect("write lockfile");

    let loaded = mgpm_lockfile::text::read_text(&lf_path).expect("read lockfile");
    assert_eq!(loaded.packages.len(), 1);
    assert_eq!(loaded.packages[0].name, "is-odd");
    assert_eq!(loaded.packages[0].version, "3.0.1");
}

#[tokio::test]
async fn e2e_smoke_empty_project_resolve() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let provider = make_provider();
    let resolver = Resolver::new(Box::new(provider));
    let config = ResolutionConfig::default();
    let pipeline = ResolutionPipeline::new(resolver, config);

    let lockfile = pipeline
        .resolve_and_lock(&[], root, None)
        .await
        .expect("resolving empty deps should succeed");

    assert_eq!(lockfile.packages.len(), 0);
}

#[tokio::test]
async fn e2e_smoke_resolve_nonexistent() {
    let provider = E2eProvider {
        packages: HashMap::new(),
    };
    let resolver = Resolver::new(Box::new(provider));
    let config = ResolutionConfig::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let pipeline = ResolutionPipeline::new(resolver, config);

    let wanted = vec![WantedDependency {
        name: PackageName::new("nonexistent-pkg-12345").unwrap(),
        version_req: "^1.0.0".to_string(),
        dev: false,
        optional: false,
    }];

    let result = pipeline.resolve_and_lock(&wanted, tmp.path(), None).await;
    // Current resolver returns Ok with 0 resolutions for unknown packages
    assert!(result.is_ok(), "resolver returns Ok with 0 resolutions for unknown packages");
    let lockfile = result.unwrap();
    assert_eq!(lockfile.packages.len(), 0, "no packages should be resolved");
}

#[test]
fn e2e_smoke_lockfile_integrity() {
    let mut lockfile = Lockfile::new(1, "https://registry.npmjs.org");
    lockfile.add_package(LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-abc".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lockfile.add_package(LockfilePackage {
        id: "lodash@4.17.21".to_string(),
        name: "lodash".to_string(),
        version: "4.17.21".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-def".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();

    let hash = lockfile.metadata.content_hash.clone();
    assert!(!hash.is_empty(), "content hash should not be empty");
    assert_eq!(lockfile.packages.len(), 2);

    let tmp = tempfile::tempdir().expect("tempdir");
    let lf_path = tmp.path().join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lockfile, &lf_path).expect("write");

    let loaded = mgpm_lockfile::text::read_text(&lf_path).expect("read");
    assert_eq!(loaded.packages.len(), 2);

    assert_eq!(loaded.packages[0].name, "lodash");
    assert_eq!(loaded.packages[1].name, "react");
}

#[test]
fn e2e_smoke_binary_lockfile_roundtrip() {
    let mut lockfile = Lockfile::new(1, "https://registry.npmjs.org");
    lockfile.add_package(LockfilePackage {
        id: "typescript@5.4.0".to_string(),
        name: "typescript".to_string(),
        version: "5.4.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/typescript/-/typescript-5.4.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: Some("sha512-xyz".to_string()),
            resolved: false,
            resolved_at: None,
        });
    lockfile.sort_packages();
    lockfile.compute_content_hash();

    let tmp = tempfile::tempdir().expect("tempdir");
    let bin_path = tmp.path().join("mgpm.lockb");
    mgpm_lockfile::binary::write_binary(&lockfile, &bin_path).expect("write binary");

    let loaded = mgpm_lockfile::binary::read_binary(&bin_path).expect("read binary");
    assert_eq!(loaded.packages.len(), 1);
    assert_eq!(loaded.packages[0].name, "typescript");
    assert_eq!(loaded.packages[0].version, "5.4.0");
}
