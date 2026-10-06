#![cfg(test)]
#![allow(clippy::unwrap_used)]
// Tests mutate env vars single-threaded per test process (edition 2024 unsafe rule).
// Test đổi env var đơn luồng theo từng process test (luật unsafe edition 2024).
#![allow(unsafe_code)]
//! Tests for AI model OCI operations

use super::{
    ModelManifest, cas_import, cas_pull, model_manifest_relative_path, next_hf_download_total,
    parse_hf_locator, quantize, remove_local, save_manifest_in, validate_model_name,
};
use std::path::PathBuf;

fn tmp_store(tag: &str) -> (PathBuf, PathBuf) {
    let mut base = std::env::temp_dir();
    base.push(format!("mgc-model-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let store = base.join("store").join("v3");
    (store, base)
}

#[test]
fn cas_import_roundtrip() {
    let (store_root, base) = tmp_store("roundtrip");
    std::fs::create_dir_all(&store_root).unwrap();
    let store = mgc_store::cas::ContentStore::new(store_root.clone()).unwrap();

    let src = base.join("model.bin");
    std::fs::write(&src, b"model-bytes-1234").unwrap();
    let (hash, len) = cas_import(&store, &src).unwrap();
    assert_eq!(len, 16);
    let typed = mgc_store::cas::IntegrityHash::from_hash_str(&hash, false)
        .expect("cas_import must return a valid blake3 hex digest");
    assert!(store.contains(&typed));
}

#[test]
fn manifest_save_and_list() {
    let (store_root, base) = tmp_store("manifest");
    let dest = store_root.join("models");
    let _ = &base;

    save_manifest_in(
        dest.clone(),
        &ModelManifest {
            name: "org/model/file.bin".to_string(),
            source: "hf://org/model/file.bin".to_string(),
            blobs: vec!["abc".to_string()],
            total_bytes: 10,
            pulled_at: "100".to_string(),
        },
    )
    .unwrap();

    let list = super::read_manifests_in(dest);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "org/model/file.bin");
    assert_eq!(list[0].source, "hf://org/model/file.bin");
}

#[test]
fn model_manifest_names_reject_path_traversal_and_absolute_paths() {
    for name in [
        "../escape",
        "org/../../escape",
        "/absolute",
        "org\\..\\escape",
    ] {
        assert!(
            validate_model_name(name).is_err(),
            "accepted unsafe name: {name}"
        );
    }
    assert!(validate_model_name("org/model/file.bin").is_ok());
    assert_eq!(
        model_manifest_relative_path("ai/model:stable").unwrap(),
        PathBuf::from("ai/model%3Astable.json")
    );
    let (_store, base) = tmp_store("manifest-traversal");
    let outside = base.join("escape.json");
    let error = save_manifest_in(
        base.join("store").join("models"),
        &ModelManifest {
            name: "../escape".into(),
            source: "test".into(),
            blobs: Vec::new(),
            total_bytes: 0,
            pulled_at: "0".into(),
        },
    )
    .expect_err("unsafe model names must not create files outside the model manifest root");
    assert!(error.to_string().contains("safe relative path"));
    assert!(!outside.exists());
}

#[cfg(unix)]
#[test]
fn model_manifest_write_replaces_link_without_writing_through_it() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let models = root.path().join("models");
    let nested = models.join("org").join("model");
    std::fs::create_dir_all(&nested).unwrap();
    let outside = root.path().join("outside.json");
    std::fs::write(&outside, b"outside-data").unwrap();
    symlink(&outside, nested.join("file.bin.json")).unwrap();

    save_manifest_in(
        models,
        &ModelManifest {
            name: "org/model/file.bin".into(),
            source: "fixture".into(),
            blobs: Vec::new(),
            total_bytes: 0,
            pulled_at: "0".into(),
        },
    )
    .unwrap();

    assert_eq!(std::fs::read(&outside).unwrap(), b"outside-data");
}

#[cfg(unix)]
#[test]
fn model_manifest_listing_does_not_follow_symlink_directories() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let models = root.path().join("models");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("hidden.json"),
        serde_json::to_vec(&ModelManifest {
            name: "hidden".into(),
            source: "fixture".into(),
            blobs: Vec::new(),
            total_bytes: 0,
            pulled_at: "0".into(),
        })
        .unwrap(),
    )
    .unwrap();
    symlink(&outside, models.join("external")).unwrap();

    assert!(super::read_manifests_in(models).is_empty());
}

#[cfg(unix)]
#[test]
fn model_manifest_update_preserves_legacy_colon_path() {
    let root = tempfile::tempdir().unwrap();
    let models = root.path().join("models");
    let legacy = models.join("ai").join("model:stable.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, b"legacy").unwrap();

    save_manifest_in(
        models.clone(),
        &ModelManifest {
            name: "ai/model:stable".into(),
            source: "fixture".into(),
            blobs: Vec::new(),
            total_bytes: 0,
            pulled_at: "1".into(),
        },
    )
    .unwrap();

    assert!(legacy.exists());
    assert!(!models.join("ai/model%3Astable.json").exists());
}

#[test]
fn hf_model_locator_requires_commit_revision_and_sha256_pin() {
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let digest = "a".repeat(64);
    assert!(parse_hf_locator("org/model/file.gguf", None, None).is_err());
    assert!(parse_hf_locator("org/model/file.gguf", Some("main"), Some(&digest)).is_err());
    assert!(parse_hf_locator("org/model/file.gguf", Some(revision), Some("bad")).is_err());
    assert!(parse_hf_locator("org/../model.gguf", Some(revision), Some(&digest)).is_err());
    let (name, url, parsed_revision, parsed_digest) = parse_hf_locator(
        "org/model/weights/model.gguf",
        Some(revision),
        Some(&digest),
    )
    .expect("pinned nested artifact locator should parse");
    assert_eq!(name, "org/model/weights/model.gguf");
    assert_eq!(parsed_revision, revision);
    assert_eq!(parsed_digest, digest);
    assert_eq!(
        url.as_str(),
        format!("https://huggingface.co/org/model/resolve/{revision}/weights/model.gguf")
    );
}

#[test]
fn hf_download_quota_checks_each_chunk_before_write() {
    assert_eq!(next_hf_download_total(5, 5, 10).unwrap(), 10);
    assert!(
        next_hf_download_total(5, 6, 10)
            .unwrap_err()
            .to_string()
            .contains("--max-bytes")
    );
    assert!(next_hf_download_total(u64::MAX, 1, u64::MAX).is_err());
}

#[test]
fn remove_local_missing_bails() {
    let (store_root, base) = tmp_store("missing");
    unsafe { std::env::set_var("MAGICORE_STORE_ROOT", &store_root) };
    std::fs::create_dir_all(&store_root).unwrap();
    let _ = &base;
    assert!(remove_local("not-there").is_err());
}

#[test]
fn unsupported_source_bails() {
    let (store_root, base) = tmp_store("unsupported");
    unsafe { std::env::set_var("MAGICORE_STORE_ROOT", &store_root) };
    std::fs::create_dir_all(&store_root).unwrap();
    let _ = &base;
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(
        rt.block_on(cas_pull("file:///tmp/x", None, None, None))
            .is_err()
    );
}

#[test]
fn quantize_fails_closed_without_invoking_external_python() {
    let error = quantize("model.gguf", "q4_k_m", None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("native GGUF quantization is not implemented")
    );
    assert!(error.to_string().contains("will not invoke Python"));
}
