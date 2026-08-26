//! Upstream packument import — parse khoan dung field dạng string + skip version lỗi.
//! (Upstream packument import: tolerant of string-typed fields, skips bad entries.)

#![allow(clippy::unwrap_used)]
use mgc_registry_server::storage::RegistryStore;

fn packument_with_quirks(upstream_base: &str) -> String {
    format!(
        r#"{{
            "name": "demo-pkg",
            "dist-tags": {{ "latest": "1.0.0" }},
            "maintainers": [{{ "name": "a", "email": "a@x" }}],
            "time": {{}},
            "versions": {{
                "1.0.0": {{
                    "name": "demo-pkg",
                    "version": "1.0.0",
                    "description": null,
                    "dist": {{ "integrity": "sha512-x", "shasum": "abc",
                               "tarball": "{base}/demo-pkg/-/demo-pkg-1.0.0.tgz" }},
                    "main": false,
                    "license": {{"type": "MIT"}},
                    "types": "./index.d.ts",
                    "repository": "git+https://example.com/x.git",
                    "author": "Dev Person <dev@x> (https://dev.x)",
                    "bugs": "https://example.com/bugs"
                }},
                "0.9.0": {{
                    "name": "demo-pkg",
                    "version": "0.9.0",
                    "broken_shape": true,
                    "dist": {{ "integrity": 123 }}
                }}
            }}
        }}"#,
        base = upstream_base
    )
}

#[tokio::test]
async fn upstream_packument_tolerates_strings_and_skips_bad_versions() {
    let mut server = mockito::Server::new_async().await;
    let body = packument_with_quirks(&server.url());
    let m = server
        .mock("GET", "/demo-pkg")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let mut store = RegistryStore::new(tmp.path()).await.unwrap();
    store.set_upstream(Some(server.url()));

    let pkg = store
        .get_package("demo-pkg")
        .await
        .unwrap()
        .expect("packument hợp lệ phải được import dù có version dị dạng");

    m.assert_async().await;
    assert!(!pkg.private, "mirror từ npmjs không phải private");
    assert_eq!(pkg.versions.len(), 1, "version 0.9.0 dị dạng phải bị skip");
    let v1 = pkg.versions.get("1.0.0").unwrap();

    // repository/author/bugs dạng STRING vẫn vào được đúng trường
    assert_eq!(v1.repository.as_ref().unwrap().url, "git+https://example.com/x.git");
    assert_eq!(v1.author.as_ref().unwrap().name, "Dev Person");
    assert_eq!(v1.bugs.as_ref().unwrap().url, "https://example.com/bugs");
    // "main": false (bool) → None ; "types": string → giữ nguyên
    assert_eq!(v1.main, None);
    assert_eq!(v1.types.as_deref(), Some("./index.d.ts"));
    // "license" dạng map legacy → ép Null thay vì giết cả version
    assert_eq!(v1.license, None);

    // dist-tags giữ nguyên
    assert_eq!(pkg.dist_tags.get("latest").map(String::as_str), Some("1.0.0"));
}

#[tokio::test]
async fn upstream_packument_all_bad_versions_is_none() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/all-bad")
        .with_status(200)
        .with_body(r#"{ "name": "all-bad", "versions": { "1.0.0": { "oops": true } } }"#)
        .create_async()
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let mut store = RegistryStore::new(tmp.path()).await.unwrap();
    store.set_upstream(Some(server.url()));

    // Không còn version nào parse được → None (fail-closed), KHÔNG panic
    assert!(store.get_package("all-bad").await.unwrap().is_none());
}
