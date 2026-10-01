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
async fn migrate_lock_v3_to_v4_refuses_until_runtime_can_read_it() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let lock_path = dir.path().join("mgc.lock");
    let before = fs::read(&lock_path).unwrap();
    let result = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;

    let error = result.expect_err("migration must not create an unreadable lockfile");
    assert!(
        error
            .to_string()
            .contains("schema v4 migration is disabled"),
        "{error}"
    );
    assert_eq!(fs::read(&lock_path).unwrap(), before);
}

#[tokio::test]
async fn migrate_lock_already_v4_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let v3 = mgc_lockfile::parser::parse_lockfile(&v3_fixture()).unwrap();
    let (mut v4, _) = mgc_lockfile::migrate_v3_to_v4(v3).unwrap();
    v4.metadata.generated_at = "2026-09-26T00:00:00Z".to_string();
    v4.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&v4.payload());
    fs::write(
        dir.path().join("mgc.lock"),
        mgc_lockfile::canonical::write_v4_document(&v4).unwrap(),
    )
    .unwrap();
    let before = fs::read(dir.path().join("mgc.lock")).unwrap();
    let run = super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    };
    super::super::migrate::run(run.clone()).await.unwrap();
    let after_first = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    super::super::migrate::run(run).await.unwrap();
    let after_second = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    assert_eq!(after_first.as_bytes(), before);
    assert_eq!(after_second, after_first);
}

#[tokio::test]
async fn migrate_lock_rejects_v4_with_tampered_payload_digest() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let v3 = mgc_lockfile::parser::parse_lockfile(&v3_fixture()).unwrap();
    let (mut v4, _) = mgc_lockfile::migrate_v3_to_v4(v3).unwrap();
    v4.metadata.generated_at = "2026-09-26T00:00:00Z".to_string();
    v4.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&v4.payload());
    let mut text = mgc_lockfile::canonical::write_v4_document(&v4).unwrap();
    text = text.replace("name = \"lodash\"", "name = \"attacker-replaced\"");
    let lock_path = dir.path().join("mgc.lock");
    fs::write(&lock_path, &text).unwrap();

    let result = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;

    assert!(
        result.is_err(),
        "tampered v4 document must not pass as a no-op"
    );
    assert_eq!(fs::read(&lock_path).unwrap(), text.as_bytes());
}

#[tokio::test]
async fn migrate_lock_rejects_v4_with_invalid_signature_math() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let v3 = mgc_lockfile::parser::parse_lockfile(&v3_fixture()).unwrap();
    let (mut v4, _) = mgc_lockfile::migrate_v3_to_v4(v3).unwrap();
    v4.metadata.generated_at = "2026-09-26T00:00:00Z".to_string();
    v4.metadata.lockfile_hash = mgc_lockfile::canonical::payload_digest(&v4.payload());
    v4.metadata.signature = Some(mgc_lockfile::SignatureBlock {
        algorithm: "ed25519".to_string(),
        key_id: "0000000000000000".to_string(),
        public_key: "not-a-public-key".to_string(),
        digest: v4.metadata.lockfile_hash.clone(),
        signed_at: "2026-09-26T00:00:00Z".to_string(),
        signature: "ed25519-not-a-signature".to_string(),
    });
    let text = mgc_lockfile::canonical::write_v4_document(&v4).unwrap();
    let lock_path = dir.path().join("mgc.lock");
    fs::write(&lock_path, &text).unwrap();

    let result = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;

    assert!(
        result.is_err(),
        "invalid signature must not pass as a no-op"
    );
    assert_eq!(fs::read(&lock_path).unwrap(), text.as_bytes());
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
