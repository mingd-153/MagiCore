#![cfg(test)]
#![allow(clippy::unwrap_used)]

// Auto-migrated from core/crates/mgc-crypto/src/checksum.rs
use mgc_crypto::checksum::*;


#[test]
fn test_checksum_adler32_basic() {
    // Known adler32: b"hello" → 0x062c0215
    assert_eq!(checksum_adler32(b"hello"), 0x062c0215);
}

#[test]
fn test_checksum_adler32_empty() {
    // adler32 of empty data: a=1, b=0 → (0 << 16) | 1 = 1
    assert_eq!(checksum_adler32(b""), 1);
}

#[test]
fn test_checksum_adler32_different_inputs_differ() {
    assert_ne!(checksum_adler32(b"abc"), checksum_adler32(b"xyz"));
}

#[test]
fn test_verify_adler32_match() {
    assert!(verify_adler32(b"hello", 0x062c0215).unwrap());
}

#[test]
fn test_verify_adler32_mismatch() {
    assert!(!verify_adler32(b"hello", 0).unwrap());
}
