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

fn v3_lossless_fixture() -> String {
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
registry = "https://registry.npmjs.org"

[package.provenance]
source_kind = "registry-import"
"#
    .to_string()
}

#[tokio::test]
async fn migrate_lock_v3_to_v4_refuses_lossy_locks_without_touching_disk() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let lock_path = dir.path().join("mgc.lock");
    let before = fs::read(&lock_path).unwrap();
    let result = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;

    // The shared fixture has no registry: the "unknown" source warning
    // makes the migration lossy, so the old lock must survive untouched.
    let error = result.expect_err("lossy migration must not write v4");
    assert!(error.to_string().contains("lossy"), "{error}");
    assert_eq!(fs::read(&lock_path).unwrap(), before);
}

#[tokio::test]
async fn migrate_lock_v3_to_v4_emits_lossless_v4_with_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mgc.lock"), v3_lossless_fixture()).unwrap();
    fs::write(
        dir.path().join("mgc.toml"),
        "name = \"mig\"\necosystem = \"web\"\n",
    )
    .unwrap();
    super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await
    .expect("lossless migration must emit v4");
    let text = fs::read_to_string(dir.path().join("mgc.lock")).unwrap();
    let doc = mgc_lockfile::canonical::parse_v4_document(&text).unwrap();
    assert_eq!(doc.version, "4");
    let pin = doc
        .packages
        .iter()
        .find(|package| package.key.name == "lodash")
        .expect("migrated pin");
    assert!(
        pin.artifact
            .as_ref()
            .and_then(|artifact| artifact.integrity_sri.as_deref())
            .is_some_and(|sri| sri.starts_with("sha512-")),
        "migrated v4 pin must carry installer SRI"
    );
    let report = mgc_lockfile::policy::verify_v4_math(&doc).unwrap();
    assert!(report.digest_ok, "emitted digest must verify");
    // Re-migrating is no longer possible (source is v4 now) and the
    // file is a no-op for the migrator.
    let rerun = super::super::migrate::run(super::super::migrate::MigrateCmd::Lock {
        to: "4".to_string(),
        dir: Some(dir.path().to_path_buf()),
    })
    .await;
    assert!(rerun.is_ok(), "v4 source must be a no-op: {rerun:?}");
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
