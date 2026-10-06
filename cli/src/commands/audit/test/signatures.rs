//! Signature-audit tests for signed payloads and tamper detection.
//! Kiểm thử lệnh audit chữ ký và phát hiện dữ liệu bị sửa.
#![allow(clippy::unwrap_used)]

use super::*;
use base64::Engine as _;
use mgc_oidc::{OidcClaims, claims::Audience};
use mgc_registry_server::trusted::{AttestationPayload, BuilderIdentity, sign_attestation};

#[test]
fn signature_audit_reads_registry_from_mgc_registry_env() {
    use clap::CommandFactory as _;

    let command = crate::Cli::command();
    let audit = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "audit")
        .expect("audit subcommand");
    let signatures = audit
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "signatures")
        .expect("signatures subcommand");
    let registry = signatures
        .get_arguments()
        .find(|argument| argument.get_id() == "registry")
        .expect("registry argument");
    assert_eq!(
        registry.get_env().and_then(|value| value.to_str()),
        Some("MGC_REGISTRY")
    );
}

fn bundle_with_key() -> (AttestationBundle, mgc_crypto::KeyPair) {
    let key = mgc_crypto::KeyPair::generate().unwrap();
    let entry = sign_attestation(
        AttestationPayload {
            schema_version: 1,
            package: "pkg".into(),
            version: "1.0.0".into(),
            digest: format!(
                "sha512-{}",
                base64::engine::general_purpose::STANDARD.encode([3u8; 64])
            ),
            artifact_sha256: None,
            sequence: 1,
            builder: BuilderIdentity::from(&OidcClaims {
                iss: "https://token.actions.githubusercontent.com".into(),
                sub: "repo:acme/pkg:ref:refs/heads/main".into(),
                aud: Audience::Single("registry".into()),
                exp: 2,
                iat: 1,
                repository: Some("acme/pkg".into()),
                job_workflow_ref: None,
                workflow_ref: None,
                event_name: None,
            }),
            created_at_unix: 1,
            previous_hash: None,
        },
        &key,
    )
    .unwrap();
    (
        AttestationBundle {
            package: "pkg".into(),
            public_key: key.public_key.to_base64(),
            key_id: key.key_id.clone(),
            head_sequence: 1,
            entries: vec![entry],
        },
        key,
    )
}

fn bundle() -> AttestationBundle {
    bundle_with_key().0
}

#[test]
fn signature_audit_core_scopes_oci_repository() {
    assert_eq!(
        effective_signature_package(Some("ai"), "oci", "models/weights").unwrap(),
        "ai/models/weights"
    );
    assert_eq!(
        effective_signature_package(Some("clo"), "oci", "artifacts/app").unwrap(),
        "cloud/artifacts/app"
    );
    assert_eq!(
        effective_signature_package(Some("ai"), "npm", "@acme/widgets").unwrap(),
        "@acme/widgets"
    );
    assert!(effective_signature_package(Some("unknown"), "oci", "models/weights").is_err());
}

#[test]
fn verifies_signed_bundle_and_rejects_mutated_payload() {
    let mut record = bundle();
    assert_eq!(verify_bundle("pkg", &record).unwrap(), 1);
    record.entries[0].payload.version = "9.9.9".into();
    assert!(verify_bundle("pkg", &record).is_err());
}

#[test]
fn rejects_empty_bundle_and_wrong_package() {
    let mut record = bundle();
    assert!(verify_bundle("other", &record).is_err());
    record.entries.clear();
    assert!(verify_bundle("pkg", &record).is_err());
}

#[test]
fn verifies_hash_chain_across_page_boundaries() {
    let (mut record, key) = bundle_with_key();
    let first = record.entries[0].clone();
    let second = sign_attestation(
        AttestationPayload {
            schema_version: 1,
            package: "pkg".into(),
            version: "1.1.0".into(),
            digest: format!(
                "sha512-{}",
                base64::engine::general_purpose::STANDARD.encode([4u8; 64])
            ),
            artifact_sha256: None,
            sequence: 2,
            builder: first.payload.builder.clone(),
            created_at_unix: 2,
            previous_hash: Some(first.entry_hash.clone()),
        },
        &key,
    )
    .unwrap();
    record.head_sequence = 2;
    record.entries.push(second.clone());

    let public_key = verified_bundle_key(&record).unwrap();
    let previous = verify_page(
        "pkg",
        "npm",
        &record.entries[..1],
        &record.key_id,
        &public_key,
        1,
        None,
    )
    .unwrap();
    assert_eq!(previous.as_deref(), Some(first.entry_hash.as_str()));
    let final_hash = verify_page(
        "pkg",
        "npm",
        &record.entries[1..],
        &record.key_id,
        &public_key,
        2,
        previous,
    )
    .unwrap();
    assert_eq!(final_hash.as_deref(), Some(second.entry_hash.as_str()));
    assert_eq!(verify_bundle("pkg", &record).unwrap(), 2);
}
