//! v4 install-readiness tests: SRI gate, SRI carry-over, round-trip.
//! Test sẵn sàng install v4: cổng SRI, mang SRI sang, round-trip.

use mgc_lockfile::canonical::{parse_v4_document, write_v4_document};
use mgc_lockfile::migrate::migrate_v3_to_v4;
use mgc_lockfile::policy::check_install_sri;
use mgc_lockfile::schema::{ArtifactRef, Lockfile, Package};

const SHA512_SRI: &str = "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";
const SHA256_SRI: &str = "sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";

#[test]
fn install_sri_gate_accepts_supported_algorithms() {
    check_install_sri(SHA512_SRI).unwrap();
    check_install_sri(SHA256_SRI).unwrap();
}

#[test]
fn install_sri_gate_rejects_weak_unknown_and_malformed() {
    for bad in [
        "",
        "blake3-abc123",
        "md5-d41d8cd98f00b204e9800998ecf8427e",
        "sha1-2jmj7l5rSw0yVb/vlWAYkK/YBwk=",
        "sha512-",
        "-AAAA",
        "no-dash-at-all",
        "sha999-AAAA",
    ] {
        assert!(
            check_install_sri(bad).is_err(),
            "SRI value must be refused: {bad}"
        );
    }
}

fn v3_package(name: &str, version: &str, integrity: &str) -> Package {
    let mut package = Package::new(
        name.to_string(),
        version.to_string(),
        format!("https://registry.npmjs.org/{name}/-/{name}-{version}.tgz"),
        integrity.to_string(),
    );
    package.registry = Some("https://registry.npmjs.org".to_string());
    package
}

#[test]
fn migrate_carries_supported_sri_into_v4_artifact() {
    let mut lock = Lockfile::new();
    lock.packages
        .push(v3_package("leftpad", "1.3.0", SHA512_SRI));
    let (v4, warnings) = migrate_v3_to_v4(lock).unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let pin = v4
        .packages
        .iter()
        .find(|package| package.key.name == "leftpad")
        .expect("migrated pin");
    assert_eq!(
        pin.artifact
            .as_ref()
            .and_then(|artifact| artifact.integrity_sri.as_deref()),
        Some(SHA512_SRI)
    );
}

#[test]
fn migrate_drops_unsupported_sri_with_warning_instead_of_fabricating() {
    let mut lock = Lockfile::new();
    lock.packages
        .push(v3_package("leftpad", "1.3.0", "blake3-abc123"));
    let (v4, warnings) = migrate_v3_to_v4(lock).unwrap();
    assert!(
        warnings.iter().any(|warning| warning.contains("leftpad")),
        "expected an SRI warning, got: {warnings:?}"
    );
    let pin = v4
        .packages
        .iter()
        .find(|package| package.key.name == "leftpad")
        .expect("migrated pin");
    assert_eq!(
        pin.artifact
            .as_ref()
            .and_then(|artifact| artifact.integrity_sri.as_deref()),
        None
    );
}

#[test]
fn v4_document_round_trips_with_integrity_sri() {
    let mut lock = Lockfile::new();
    lock.packages
        .push(v3_package("leftpad", "1.3.0", SHA512_SRI));
    let (v4, warnings) = migrate_v3_to_v4(lock).unwrap();
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let text = write_v4_document(&v4).unwrap();
    assert!(text.contains("integrity_sri"), "SRI must hit disk:\n{text}");
    let back = parse_v4_document(&text).unwrap();
    assert_eq!(back, v4);
}

#[test]
fn artifact_without_sri_still_deserializes_as_none() {
    let text = r#"
version = "4"
[metadata]
generated_at = "2026-10-01T00:00:00Z"
generator = "mgc/test"
lockfile_hash = ""
[[package]]
[package.key]
ecosystem = "web"
name = "leftpad"
version = "1.3.0"
source_id = "npm"
[package.artifact]
url = "https://registry.npmjs.org/leftpad/-/leftpad-1.3.0.tgz"
content_hash = ""
downloaded_from = "https://registry.npmjs.org"
"#;
    let doc = parse_v4_document(text).unwrap();
    let artifact: &ArtifactRef = doc.packages[0].artifact.as_ref().expect("artifact");
    assert_eq!(artifact.integrity_sri, None);
}
