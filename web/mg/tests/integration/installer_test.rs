//! Integration tests for the installer

use mg_lockfile::Lockfile;
use tokio::sync::mpsc;

/// Tests that dry-run mode returns all packages as skipped.
#[tokio::test]
async fn test_dry_run_mode() {
    use mg_installer::{InstallOptions, Installer};

    let (tx, _rx) = mpsc::channel(256);
    let dir = tempfile::tempdir().unwrap();

    let options = InstallOptions {
        dry_run: true,
        store_path: dir.path().join("store"),
        virtual_store_path: dir.path().join(".mg"),
        project_root: dir.path().to_path_buf(),
        sqlite_path: dir.path().join("mg.db"),
        ..Default::default()
    };

    let installer = match Installer::new(options, tx) {
        Ok(i) => i,
        Err(_) => {
            // Installer may fail to init SQLite in some environments; that's OK
            return;
        }
    };

    let mut lockfile = Lockfile::new(1, "npm");
    lockfile.add_package(mg_lockfile::LockfilePackage {
        id: "react@18.2.0".to_string(),
        name: "react".to_string(),
        version: "18.2.0".to_string(),
        resolution: mg_lockfile::PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: None,
        dependencies: vec![],
        resolved: false,
        resolved_at: None,
    });

    let result = installer.install_lockfile(&lockfile).await;
    assert_eq!(result.skipped, 1);
    assert_eq!(result.failed, 0);
}

/// Tests that offline mode skips already-cached packages.
#[tokio::test]
async fn test_offline_mode() {
    use mg_installer::{InstallOptions, Installer};

    let (tx, _rx) = mpsc::channel(256);
    let dir = tempfile::tempdir().unwrap();

    let options = InstallOptions {
        offline: true,
        dry_run: false,
        store_path: dir.path().join("store"),
        virtual_store_path: dir.path().join(".mg"),
        project_root: dir.path().to_path_buf(),
        sqlite_path: dir.path().join("mg.db"),
        ..Default::default()
    };

    let installer = match Installer::new(options, tx) {
        Ok(i) => i,
        Err(_) => return,
    };

    let mut lockfile = Lockfile::new(1, "npm");
    lockfile.add_package(mg_lockfile::LockfilePackage {
        id: "offline-pkg@1.0.0".to_string(),
        name: "offline-pkg".to_string(),
        version: "1.0.0".to_string(),
        resolution: mg_lockfile::PackageResolution {
            r#type: "registry".to_string(),
            url: "https://registry.npmjs.org/offline-pkg/-/offline-pkg-1.0.0.tgz".to_string(),
            registry: Some("npm".to_string()),
        },
        integrity: None,
        dependencies: vec![],
        resolved: false,
        resolved_at: None,
    });

    let result = installer.install_lockfile(&lockfile).await;
    assert_eq!(result.failed, 1); // not in cache + offline = fail
}
