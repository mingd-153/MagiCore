//! Tests for ed25519_signer module
//! Tests cho module ed25519_signer

use mg_crypto::ed25519_signer::{Ed25519PublicKey, Ed25519Signature, Ed25519Signer, verify_signature, verify_string_signature};
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;

#[test]
fn test_sign_verify_roundtrip() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let message = b"test message";
    let signature = signer.sign(message);
    let public_key = signer.public_key();

    assert!(verify_signature(&public_key, message, &signature).is_ok());
}

#[test]
fn test_verify_wrong_message_fails() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let message = b"test message";
    let signature = signer.sign(message);
    let public_key = signer.public_key();

    let wrong_message = b"wrong message";
    assert!(verify_signature(&public_key, wrong_message, &signature).is_err());
}

#[test]
fn test_sign_string() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let message = "hello world";
    let signature = signer.sign_string(message);
    let public_key = signer.public_key();

    assert!(verify_string_signature(&public_key, message, &signature).is_ok());
}

#[test]
fn test_signature_base64_roundtrip() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let signature = signer.sign(b"test");

    let b64 = signature.to_base64();
    let parsed = Ed25519Signature::from_base64(&b64).unwrap();
    assert_eq!(signature, parsed);
}

#[test]
fn test_public_key_base64_roundtrip() {
    let rng = SystemRandom::new();
    let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signer = Ed25519Signer::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();

    let public_key = signer.public_key();

    let b64 = public_key.to_base64();
    let parsed = Ed25519PublicKey::from_base64(&b64).unwrap();
    assert_eq!(public_key, parsed);
}
