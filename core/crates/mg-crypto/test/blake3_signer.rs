//! Tests for blake3_signer module
//! Tests cho module blake3_signer

use mg_crypto::blake3_signer::{Blake3Hash, Blake3Hasher};

#[test]
fn test_hash_empty() {
    let hash = Blake3Hasher::hash_bytes(b"");
    assert_eq!(hash.0.len(), 32);
}

#[test]
fn test_hash_hello_world() {
    let hash = Blake3Hasher::hash_string("hello world");
    let expected = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
    assert_eq!(hash.to_hex(), expected);
}

#[test]
fn test_hash_verify() {
    let data = b"test data";
    let hash = Blake3Hasher::hash_bytes(data);
    assert!(Blake3Hasher::verify(data, &hash));
    assert!(!Blake3Hasher::verify(b"wrong data", &hash));
}

#[test]
fn test_hash_hex_roundtrip() {
    let hash = Blake3Hasher::hash_string("test");
    let hex = hash.to_hex();
    let parsed = Blake3Hash::from_hex(&hex).unwrap();
    assert_eq!(hash, parsed);
}

#[test]
fn test_hash_base64_roundtrip() {
    let hash = Blake3Hasher::hash_string("test");
    let b64 = hash.to_base64();
    let parsed = Blake3Hash::from_base64(&b64).unwrap();
    assert_eq!(hash, parsed);
}

#[test]
fn test_hex_dos_protection() {
    // A1 TEST: Oversized hex string should error (not panic)
    let evil = "a".repeat(200);
    let result = Blake3Hash::from_hex(&evil);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too long"));
}

#[test]
fn test_mmap_empty_file() {
    // A6 TEST: Empty file should not crash
    use tempfile::NamedTempFile;
    let tmpfile = NamedTempFile::new().unwrap();
    
    #[cfg(not(target_env = "msvc"))]
    {
        let hash = Blake3Hasher::hash_file_mmap(tmpfile.path()).unwrap();
        let expected = Blake3Hasher::hash_bytes(&[]);
        assert_eq!(hash, expected);
    }
}
