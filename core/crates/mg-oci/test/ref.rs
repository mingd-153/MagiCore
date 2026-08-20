#![allow(clippy::unwrap_used)]
//! OciRef parse tests — test riêng tại test/ (RULE §5)
//! (Kiểm parse repo:tag, repo@digest, mặc định latest, input rỗng)

use mg_oci::r#ref::{OciRef, OciReference};

#[test]
fn parses_tag_reference() {
    let r = OciRef::parse("registry.local/models/llama:q4").unwrap();
    assert_eq!(r.repo, "registry.local/models/llama");
    assert_eq!(r.reference, OciReference::Tag("q4".to_string()));
    assert_eq!(r.reference_str(), "q4");
    assert_eq!(r.to_string_full(), "registry.local/models/llama:q4");
}

#[test]
fn parses_digest_reference() {
    let r = OciRef::parse("models/llama@sha256:abc123").unwrap();
    assert_eq!(r.repo, "models/llama");
    assert_eq!(
        r.reference,
        OciReference::Digest("sha256:abc123".to_string())
    );
    assert_eq!(r.reference_str(), "sha256:abc123");
    assert_eq!(r.to_string_full(), "models/llama@sha256:abc123");
}

#[test]
fn defaults_to_latest_without_reference() {
    let r = OciRef::parse("models/llama").unwrap();
    assert_eq!(r.repo, "models/llama");
    assert_eq!(r.reference, OciReference::Tag("latest".to_string()));
}

#[test]
fn rejects_empty_and_partial() {
    assert!(OciRef::parse("").is_err());
    assert!(OciRef::parse(":tag").is_err());
    assert!(OciRef::parse("@sha256:x").is_err());
    assert!(OciRef::parse("repo:").is_err());
    assert!(OciRef::parse("repo@").is_err());
}
