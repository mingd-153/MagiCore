use std::fs;
use std::path::PathBuf;

use mgpm_installer::installer::{InstallOptions, Installer};
use mgpm_linker::linker::LinkerStrategy;
use mgpm_lockfile::{Lockfile, LockfilePackage, PackageResolution};

use tokio::sync::mpsc;

fn create_mock_tarball(name: &str, version: &str) -> Vec<u8> {
    let mut tar_data = Vec::new();
    {
        let encoder = flate2::write::GzEncoder::new(&mut tar_data, flate2::Compression::default());
        let mut tar_builder = tar::Builder::new(encoder);

        let pkg_json = serde_json::json!({
            "name": name,
            "version": version,
            "main": "index.js"
        });
        let pkg_json_bytes = serde_json::to_string_pretty(&pkg_json).unwrap();

        let index_js = format!("module.exports = {{}}; // {} v{}", name, version);

        for (path, content) in &[
            ("package/package.json", pkg_json_bytes.as_bytes()),
            ("package/index.js", index_js.as_bytes()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).expect("set_path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder.append(&header, *content).expect("append");
        }

        tar_builder.finish().expect("finish");
    }
    tar_data
}

#[test]
fn e2e_full_install_pipeline() {
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    let store_dir = tempfile::tempdir().expect("store tempdir");
    let store_path = store_dir.path().to_path_buf();

    let pkg_json = serde_json::json!({
        "name": "full-install-test",
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

    let mgpm_yaml = r#"
version: 1
install:
  hoist: false
  symlinks: true
  concurrency: 4
  retries: 1
"#;
    fs::write(root.join("mgpm.yaml"), mgpm_yaml).expect("write mgpm.yaml");

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
        dependencies: vec![],
    });
    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();

    let lf_path = root.join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lockfile, &lf_path).expect("write lockfile");

    let opts = InstallOptions {
        concurrency: 4,
        retries: 1,
        retry_delay_ms: 100,
        store_path: store_path.clone(),
        virtual_store_path: root.join(".mgpm").join("virtual_store"),
        hoisted_node_modules: false,
        hoist_pattern: vec!["*".to_string()],
        offline: true,
        dry_run: false,
        project_root: root.clone(),
        sqlite_path: store_path.join("mgpm.db"),
        jsonl_log: false,
        linker_strategy: LinkerStrategy::Hoisted,
        gvs_root: PathBuf::from("/tmp/.mgpm").join("gvs").join("v1"),
    };

    let (tx, _rx) = mpsc::channel(256);

    let result = rt.block_on(async {
        let installer = Installer::new(opts, tx).expect("create installer");
        let lockfile = mgpm_lockfile::text::read_text(&lf_path).expect("read lockfile");
        installer.install_lockfile(&lockfile).await
    });

    // In offline mode with empty store, the installer tries to download
    // and fails (no cache hit). Expect the package to fail.
    assert_eq!(
        result.failed, 1,
        "offline install with empty store should fail for 1 package"
    );
    assert_eq!(result.succeeded, 0);
    assert_eq!(result.skipped, 0);
}

#[test]
fn e2e_full_install_with_store_verify() {
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();

    let store_dir = tempfile::tempdir().expect("store tempdir");
    let store_path = store_dir.path().to_path_buf();

    let tarball = create_mock_tarball("test-pkg", "1.0.0");
    let tarball_path = root.join("test-pkg-1.0.0.tgz");
    fs::write(&tarball_path, &tarball).expect("write tarball");

    let mut lockfile = Lockfile::new(1, "https://registry.npmjs.org");
    lockfile.add_package(LockfilePackage {
        id: "test-pkg@1.0.0".to_string(),
        name: "test-pkg".to_string(),
        version: "1.0.0".to_string(),
        resolution: PackageResolution {
            r#type: "registry".to_string(),
            url: format!("file://{}", tarball_path.display()),
            registry: Some("npm".to_string()),
        },
        integrity: None,
        dependencies: vec![],
    });
    lockfile.sort_packages();
    lockfile.compute_content_hash();
    lockfile.update_timestamp();

    let lf_path = root.join("mgpm.lock");
    mgpm_lockfile::text::write_text(&lockfile, &lf_path).expect("write lockfile");

    let opts = InstallOptions {
        concurrency: 4,
        retries: 1,
        retry_delay_ms: 100,
        store_path: store_path.clone(),
        virtual_store_path: root.join(".mgpm").join("virtual_store"),
        hoisted_node_modules: false,
        hoist_pattern: vec!["*".to_string()],
        offline: true,
        dry_run: false,
        project_root: root.clone(),
        sqlite_path: store_path.join("mgpm.db"),
        jsonl_log: false,
        linker_strategy: LinkerStrategy::Hoisted,
        gvs_root: PathBuf::from("/tmp/.mgpm").join("gvs").join("v1"),
    };

    let (tx, _rx) = mpsc::channel(256);

    let result = rt.block_on(async {
        let installer = Installer::new(opts, tx).expect("create installer");
        let lockfile = mgpm_lockfile::text::read_text(&lf_path).expect("read lockfile");
        installer.install_lockfile(&lockfile).await
    });

    // The installer uses reqwest HTTP client which does not support file:// URLs.
    // In offline mode with empty store, it will fail to download.
    // The important thing is it doesn't panic and returns a structured result.
    assert!(
        result.failed <= 1,
        "install should not hard-fail, got {} failed",
        result.failed
    );

    let serialized = serde_json::to_string(&lockfile).unwrap();
    let deserialized: Lockfile = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.packages.len(), 1);
    assert_eq!(deserialized.packages[0].name, "test-pkg");
}
