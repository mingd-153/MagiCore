use std::fs;

fn v3_fixture() -> String {
    r#"version = "3"

[metadata]
generated_at = "2026-01-01T00:00:00Z"
generator = "mgc/1.1.0"
lockfile_hash = ""

[[package]]
name = "lodash"
version = "4.17.21"
resolved = "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
integrity = "sha512-v2kDEe57lecTulaDIuNTPy3Ry4gLGJAR/9kM4xCoYpMDZ/LaWM/2nha+WamLbg=="
dependencies = []

[package.provenance]
source_kind = "registry-import"
"#
    .to_string()
}

fn write_fixture(dir: &std::path::Path) {
    fs::write(dir.join("mgc.lock"), v3_fixture()).unwrap();
    fs::write(
        dir.join("mgc.toml"),
        "name = \"mig\"\necosystem = \"web\"\n",
    )
    .unwrap();
}

#[tokio::test]
async fn migrate_lock_v3_to_v4_writes_valid_v4() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await
    .unwrap();
    let text = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    let v4 = mgc_lockfile::canonical::parse_v4_document(&text).unwrap();
    assert_eq!(v4.version, "4");
    assert!(!v4.metadata.lockfile_hash.is_empty());
    assert_eq!(
        mgc_lockfile::canonical::payload_digest(&v4.payload()),
        v4.metadata.lockfile_hash
    );
    // No temp files leak from the atomic write.
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("mgc.lock.tmp."))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn migrate_lock_already_v4_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let run = super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    };
    super::super::migrate::run(run.clone()).await.unwrap();
    let after_first = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    super::super::migrate::run(run).await.unwrap();
    let after_second = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    // Idempotent content (timestamps may differ, graph must not).
    let first = mgc_lockfile::canonical::parse_v4_document(&after_first).unwrap();
    let second = mgc_lockfile::canonical::parse_v4_document(&after_second).unwrap();
    assert_eq!(first.packages, second.packages);
}

#[tokio::test]
async fn migrate_lock_rejects_unknown_target() {
    let dir = tempfile::tempdir().unwrap();
    let err = super::super::migrate::MigrateCmd::Lock {
        to: "5".to_string(),
        dir: Some(dir.path().to_path_buf()),
    };
    let result = super::super::migrate::run(err).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn migrate_lock_missing_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let result = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;
    assert!(result.is_err());
}
