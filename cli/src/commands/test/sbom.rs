use std::path::PathBuf;

use mgc_crypto::keyring::KeyPair;
use mgc_lockfile::{
    EcosystemTag,
    canonical::{LockfileV4, PackageV4, payload_digest, write_v4_document},
    policy::sign_payload_digest,
    v4::{PackageKey, VariantKey},
};

fn create_signed_v4_lock(root: &std::path::Path, signer: &KeyPair) {
    let mut lock = LockfileV4::new("mgc-test");
    lock.packages.push(PackageV4 {
        key: PackageKey {
            ecosystem: EcosystemTag::Rust,
            name: "serde".into(),
            version: "1.0.0".into(),
            source_id: "crates-io".into(),
            variant: VariantKey::default(),
        },
        edges: vec![],
        artifact: None,
        provenance: None,
        toolchain: None,
        scripts_policy: None,
        store_ref: None,
    });
    lock.metadata.lockfile_hash = payload_digest(&lock.payload());
    lock.metadata.signature =
        Some(sign_payload_digest(&lock.metadata.lockfile_hash, signer).unwrap());
    std::fs::write(root.join("mgc.lock"), write_v4_document(&lock).unwrap()).unwrap();
    std::fs::write(
        root.join("mgc.toml"),
        format!(
            "name = \"sbom-fixture\"\necosystem = \"lib\"\n\n[trust]\nkeys = [\"{}\"]\n",
            signer.key_id
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn sbom_command_reads_trusted_v4_and_exports_ecosystem_purl() {
    let root = tempfile::tempdir().unwrap();
    let signer = KeyPair::generate().unwrap();
    create_signed_v4_lock(root.path(), &signer);
    let output = root.path().join("sbom.json");

    super::run(
        Some("cyclonedx-json".into()),
        Some(output.clone()),
        None,
        None,
        Some(PathBuf::from(root.path())),
    )
    .await
    .unwrap();

    let bom: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(bom["components"][0]["purl"], "pkg:cargo/serde@1.0.0");
    assert!(
        bom["components"][0]["bom-ref"]
            .as_str()
            .unwrap()
            .starts_with("mgc:lock-v4:")
    );
}

#[tokio::test]
async fn sbom_command_rejects_v4_with_stale_digest_before_writing_output() {
    let root = tempfile::tempdir().unwrap();
    let signer = KeyPair::generate().unwrap();
    create_signed_v4_lock(root.path(), &signer);
    let lock_path = root.path().join("mgc.lock");
    let content = std::fs::read_to_string(&lock_path).unwrap();
    let tampered = content.replace("name = \"serde\"", "name = \"attacker\"");
    std::fs::write(&lock_path, tampered).unwrap();
    let output = root.path().join("sbom.json");

    let result = super::run(
        Some("cyclonedx-json".into()),
        Some(output.clone()),
        None,
        None,
        Some(PathBuf::from(root.path())),
    )
    .await;

    assert!(result.is_err());
    assert!(!output.exists());
}

#[tokio::test]
async fn sbom_command_does_not_advertise_unimplemented_spdx_output() {
    let root = tempfile::tempdir().unwrap();
    let result = super::run(
        Some("spdx".into()),
        None,
        None,
        None,
        Some(PathBuf::from(root.path())),
    )
    .await;

    assert!(result.unwrap_err().to_string().contains("not implemented"));
}
