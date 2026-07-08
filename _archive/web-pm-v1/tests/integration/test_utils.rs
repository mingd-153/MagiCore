//! Shared test utilities for integration tests

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use mg_core::{PackageId, PackageName, Version};
use mg_lockfile::{Lockfile, LockfilePackage, PackageResolution};
use mg_store::ContentStore;

/// A mock project fixture with temp directory, package.json, and mg.yaml.
pub struct TestFixture {
    pub root: PathBuf,
    pub _dir: tempfile::TempDir,
}

impl TestFixture {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn package_json_path(&self) -> PathBuf {
        self.root.join("package.json")
    }

    pub fn mg_yaml_path(&self) -> PathBuf {
        self.root.join("mg.yaml")
    }

    pub fn node_modules_path(&self) -> PathBuf {
        self.root.join("node_modules")
    }
}

/// Creates a temp directory with a mock project (package.json, mg.yaml).
pub fn create_mock_project() -> TestFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();

    let pkg_json = serde_json::json!({
        "name": "test-project",
        "version": "1.0.0",
        "dependencies": {
            "react": "^18.0.0",
            "lodash": "^4.17.0"
        }
    });
    fs::write(
        root.join("package.json"),
        serde_json::to_string_pretty(&pkg_json).unwrap(),
    )
    .expect("write package.json");

    let mg_yaml = r#"
catalogs:
  default:
    packages:
      react: "18.2.0"
"#;
    fs::write(root.join("mg.yaml"), mg_yaml).expect("write mg.yaml");

    TestFixture { root, _dir: dir }
}

/// Creates a mock content store in a temp directory.
pub fn create_mock_store() -> ContentStore {
    let dir = tempfile::tempdir().expect("tempdir");
    ContentStore::new(dir.path().to_path_buf()).expect("ContentStore::new")
}

/// Creates a gzipped tarball with the given files.
pub fn create_mock_tarball(_name: &str, _version: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let mut tar_data = Vec::new();
    {
        let encoder = GzEncoder::new(&mut tar_data, Compression::default());
        let mut tar_builder = tar::Builder::new(encoder);

        // Strip leading dir or put files under "package/"
        for (file_path, content) in files {
            let archive_path = format!("package/{}", file_path);
            let mut header = tar::Header::new_gnu();
            header.set_path(&archive_path).expect("set_path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append(&header, content.as_bytes())
                .expect("append");
        }
        tar_builder.finish().expect("finish");
    }
    tar_data
}

/// Helper to create a simple lockfile from a list of package specs.
pub fn create_mock_lockfile(packages: &[(&str, &str)]) -> Lockfile {
    let mut lock = Lockfile::new(1, "https://registry.npmjs.org");
    for (name, version) in packages {
        lock.add_package(LockfilePackage {
            id: format!("{}@{}", name, version),
            name: name.to_string(),
            version: version.to_string(),
            resolution: PackageResolution {
                r#type: "registry".to_string(),
                url: format!(
                    "https://registry.npmjs.org/{}/-/{}-{}.tgz",
                    name, name, version
                ),
                registry: Some("npm".to_string()),
            },
            integrity: Some(format!("sha512-{}", name)),
            dependencies: vec![],
        dep_specs: vec![],
            resolved: false,
            resolved_at: None,
        });
    }
    lock.sort_packages();
    lock.compute_content_hash();
    lock
}

/// Mock registry server simulating an npm-compatible registry.
pub struct MockRegistryServer {
    pub url: String,
    packages: HashMap<String, Vec<(String, Vec<u8>)>>,
    fail_paths: Vec<String>,
    port: u16,
}

/// A mock dependency provider for testing the resolver.
pub struct MockDependencyProvider {
    packages: HashMap<String, HashMap<String, Vec<mg_resolver::ResolvedDependency>>>,
}

impl MockDependencyProvider {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn add_package(
        &mut self,
        name: &str,
        version: &str,
        deps: Vec<mg_resolver::ResolvedDependency>,
    ) {
        self.packages
            .entry(name.to_string())
            .or_default()
            .insert(version.to_string(), deps);
    }
}

use async_trait::async_trait;

#[async_trait]
impl mg_resolver::DependencyProvider for MockDependencyProvider {
    async fn get_versions(&self, package: &PackageName) -> Vec<Version> {
        self.packages
            .get(package.as_str())
            .map(|versions| {
                let mut v: Vec<Version> = versions
                    .keys()
                    .filter_map(|s| Version::parse(s).ok())
                    .collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    async fn get_dependencies(&self, package_id: &PackageId) -> Vec<mg_resolver::ResolvedDependency> {
        self.packages
            .get(package_id.name().as_str())
            .and_then(|versions| versions.get(&package_id.version().to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for MockDependencyProvider {
    fn default() -> Self {
        Self::new()
    }
}
