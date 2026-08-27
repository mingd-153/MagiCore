use super::*;
use tempfile::TempDir;

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn test_verify_valid_checksum() {
    let tmp = tmp();
    let model = tmp.path().join("model.bin");
    std::fs::write(&model, b"test data").unwrap();

    let hash = mgc_crypto::Blake3Hasher::hash_bytes(b"test data");
    let checksum = hash.to_hex();

    let result = verify_model_checksum(&model, &checksum);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_verify_invalid_checksum() {
    let tmp = tmp();
    let model = tmp.path().join("model.bin");
    std::fs::write(&model, b"test data").unwrap();

    let wrong = "deadbeef";
    let result = verify_model_checksum(&model, wrong);
    assert!(result.is_err());
}

#[test]
fn test_verify_missing_file() {
    let tmp = tmp();
    let missing = tmp.path().join("missing.bin");

    let result = verify_model_checksum(&missing, "abc");
    assert!(result.is_err());
}

#[test]
fn test_compute_hash() {
    let hash = compute_hash(b"hello");
    assert_eq!(hash.len(), 64); // BLAKE3 = 32 bytes = 64 hex chars
}
