//! Tests for schema module
//! Tests cho module schema

use mg_lockfile::schema::{Lockfile, Package, SignatureFile};

#[test]
fn test_lockfile_new() {
    let lockfile = Lockfile::new();
    assert_eq!(lockfile.version, "2");
    assert_eq!(lockfile.packages.len(), 0);
    assert!(!lockfile.is_signed());
}

#[test]
fn test_lockfile_add_package() {
    let mut lockfile = Lockfile::new();
    let pkg = Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    );
    
    lockfile.add_package(pkg);
    assert_eq!(lockfile.packages.len(), 1);
    assert_eq!(lockfile.get_package("react").unwrap().version, "18.2.0");
}

#[test]
fn test_package_new() {
    let pkg = Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    );
    
    assert_eq!(pkg.name, "react");
    assert_eq!(pkg.version, "18.2.0");
    assert_eq!(pkg.dependencies.len(), 0);
}

#[test]
fn test_package_add_dependency() {
    let mut pkg = Package::new(
        "react".to_string(),
        "18.2.0".to_string(),
        "https://registry.npmjs.org/react/-/react-18.2.0.tgz".to_string(),
        "blake3-abc123".to_string(),
    );
    
    pkg.add_dependency("loose-envify".to_string());
    assert_eq!(pkg.dependencies.len(), 1);
    assert_eq!(pkg.dependencies[0], "loose-envify");
}

#[test]
fn test_signature_file_new() {
    let sig = SignatureFile::new(
        "blake3-abc123".to_string(),
        "ed25519-xyz789".to_string(),
        "a1b2c3d4".to_string(),
    );
    
    assert_eq!(sig.lockfile_hash, "blake3-abc123");
    assert_eq!(sig.signature, "ed25519-xyz789");
    assert_eq!(sig.key_id, "a1b2c3d4");
}

#[test]
fn test_signature_file_to_string() {
    let sig = SignatureFile {
        lockfile_hash: "blake3-abc123".to_string(),
        signature: "ed25519-xyz789".to_string(),
        key_id: "a1b2c3d4".to_string(),
        signed_at: "2026-08-21T18:30:00+07:00".to_string(),
    };
    
    let output = sig.to_string();
    assert!(output.contains("lockfile_hash = \"blake3-abc123\""));
    assert!(output.contains("signature = \"ed25519-xyz789\""));
    assert!(output.contains("key_id = \"a1b2c3d4\""));
}

#[test]
fn test_signature_file_from_str() {
    let input = r#"
# MegaGate Lockfile Signature v2
lockfile_hash = "blake3-abc123"
signature = "ed25519-xyz789"
key_id = "a1b2c3d4"
signed_at = "2026-08-21T18:30:00+07:00"
"#;
    
    let sig = SignatureFile::from_str(input).unwrap();
    assert_eq!(sig.lockfile_hash, "blake3-abc123");
    assert_eq!(sig.signature, "ed25519-xyz789");
    assert_eq!(sig.key_id, "a1b2c3d4");
    assert_eq!(sig.signed_at, "2026-08-21T18:30:00+07:00");
}

#[test]
fn test_signature_file_roundtrip() {
    let sig = SignatureFile {
        lockfile_hash: "blake3-test".to_string(),
        signature: "ed25519-test".to_string(),
        key_id: "testkey".to_string(),
        signed_at: "2026-08-21T18:30:00+07:00".to_string(),
    };
    
    let output = sig.to_string();
    let parsed = SignatureFile::from_str(&output).unwrap();
    
    assert_eq!(sig, parsed);
}
