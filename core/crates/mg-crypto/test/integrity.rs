//! Tests for integrity module
//! Tests cho module integrity

use mg_crypto::integrity::{IntegrityVerifier, SriHash};
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_sri_parse() {
    let sri = SriHash::parse("blake3-YWJjMTIz").unwrap();
    assert_eq!(sri.algorithm, "blake3");
    assert_eq!(sri.hash, "YWJjMTIz");
}

#[test]
fn test_sri_to_string() {
    let sri = SriHash {
        algorithm: "blake3".to_string(),
        hash: "abc123".to_string(),
    };
    // A9 FIX: Use Display trait instead of inherent method
    assert_eq!(sri.to_string(), "blake3-abc123");
    assert_eq!(format!("{}", sri), "blake3-abc123");
}

#[test]
fn test_compute_and_verify() {
    let data = b"hello world";
    let sri = IntegrityVerifier::compute(data);

    assert_eq!(sri.algorithm, "blake3");
    IntegrityVerifier::verify(data, &sri).unwrap();
}

#[test]
fn test_verify_mismatch() {
    let data = b"hello world";
    let sri = IntegrityVerifier::compute(data);

    let wrong_data = b"wrong data";
    assert!(IntegrityVerifier::verify(wrong_data, &sri).is_err());
}

#[test]
fn test_compute_file() {
    let mut tmpfile = NamedTempFile::new().unwrap();
    tmpfile.write_all(b"test content").unwrap();
    tmpfile.flush().unwrap();

    let sri = IntegrityVerifier::compute_file(tmpfile.path()).unwrap();
    assert_eq!(sri.algorithm, "blake3");

    IntegrityVerifier::verify_file(tmpfile.path(), &sri).unwrap();
}

#[test]
fn test_unsupported_algorithm() {
    let sri = SriHash {
        algorithm: "sha256".to_string(),
        hash: "abc".to_string(),
    };
    assert!(IntegrityVerifier::verify(b"data", &sri).is_err());
}
