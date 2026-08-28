#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for AI model OCI operations

use super::{cas_import, cas_pull, remove_local, save_manifest_in, ModelManifest};
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
    assert!(store.contains(&mgc_store::cas::IntegrityHash::from_hash_str(&hash, false)));
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
fn remove_local_missing_bails() {
    let (store_root, base) = tmp_store("missing");
    std::env::set_var("MAGICORE_STORE_ROOT", &store_root);
    std::fs::create_dir_all(&store_root).unwrap();
    let _ = &base;
    assert!(remove_local("not-there").is_err());
}

#[test]
fn unsupported_source_bails() {
    let (store_root, base) = tmp_store("unsupported");
    std::env::set_var("MAGICORE_STORE_ROOT", &store_root);
    std::fs::create_dir_all(&store_root).unwrap();
    let _ = &base;
    let rt = tokio::runtime::Runtime::new().unwrap();
    assert!(rt.block_on(cas_pull("file:///tmp/x")).is_err());
}
