use super::*;

#[tokio::test]
async fn test_import_detects_and_converts_package_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create package.json
    let pkg_json = r#"{
        "name": "demo-import",
        "version": "1.0.0",
        "dependencies": {
            "left-pad": "^1.3.0"
        }
    }"#;
    std::fs::write(root.join("package.json"), pkg_json).unwrap();

    // Create package-lock.json v3
    let npm_lock = r#"{
        "name": "demo-import",
        "version": "1.0.0",
        "lockfileVersion": 3,
        "packages": {
            "": { "name": "demo-import", "version": "1.0.0" },
            "node_modules/left-pad": {
                "version": "1.3.0",
                "integrity": "sha512-abc"
            }
        }
    }"#;
    std::fs::write(root.join("package-lock.json"), npm_lock).unwrap();

    // Run import
    let res = run(Some(root.to_path_buf())).await;
    assert!(res.is_ok(), "import should succeed: {:?}", res.err());

    // Verify mgc.lock generated
    let mgc_lock = root.join("mgc.lock");
    assert!(mgc_lock.exists(), "mgc.lock must be created");

    let checksum = root.join("mgc.lock.sha256");
    assert!(checksum.exists(), "mgc.lock.sha256 must be created");

    let marker = root.join(mgc_config::project::ProjectConfig::CORE_MARKER_FILE);
    assert!(marker.exists(), ".mgc.core signature marker must be created");

    // Read back lockfile
    let lockfile = mgc_lockfile::read_lockfile_checked(root).unwrap().unwrap();
    assert_eq!(lockfile.core, "web");
    assert_eq!(lockfile.packages.len(), 1);
    assert_eq!(lockfile.packages[0].name, "left-pad");
    assert_eq!(lockfile.packages[0].version, "1.3.0");
}
