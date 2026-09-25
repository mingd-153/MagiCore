//! HuggingFace Hub protocol tests — hermetic via mockito (no network).
//! Test protocol HuggingFace Hub — hermetic qua mockito.

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{HfHubProtocol, git_blob_sha1};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping hfhub mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn tree_body(weights_sha: &str) -> String {
    format!(
        r#"[
  {{"type":"file","path":"config.json","oid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1024}},
  {{"type":"file","path":"model.safetensors","oid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","size":4200,"lfs":{{"oid":"{weights_sha}","size":4200}}}},
  {{"type":"directory","path":"sub","oid":"cccccccccccccccccccccccccccccccccccccccc"}}
]"#
    )
}

#[tokio::test]
async fn hfhub_resolve_lists_lfs_and_blob_files_sorted() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let weights = b"WEIGHTS_BYTES";
    let weights_sha = sha256_hex(weights);
    server
        .mock(
            "GET",
            format!("/api/models/org/demo/tree/{COMMIT}?recursive=true").as_str(),
        )
        .with_status(200)
        .with_body(tree_body(&weights_sha))
        .create_async()
        .await;
    server
        .mock(
            "GET",
            format!("/org/demo/resolve/{COMMIT}/model.safetensors").as_str(),
        )
        .with_status(200)
        .with_body(weights.as_slice())
        .create_async()
        .await;

    let protocol = HfHubProtocol::new(&base);
    let resolution = protocol.resolve_model("org/demo", COMMIT).await.unwrap();
    assert_eq!(resolution.commit, COMMIT);
    // Directories skipped; files sorted by path.
    assert_eq!(resolution.files.len(), 2);
    assert_eq!(resolution.files[0].path, "config.json");
    assert_eq!(resolution.files[1].path, "model.safetensors");
    assert_eq!(
        resolution.files[1].sha256.as_deref(),
        Some(weights_sha.as_str())
    );
    assert_eq!(resolution.files[1].size, Some(4200));

    // Download + verify roundtrip on the LFS file; tampered bytes fail.
    let file = &resolution.files[1];
    let downloaded = protocol.download_file(file).await.unwrap();
    protocol.verify_file(file, &downloaded).unwrap();
    let err = protocol.verify_file(file, b"TAMPERED").unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[tokio::test]
async fn hfhub_blob_fallback_verifies_without_lfs() {
    // Non-LFS files verify via recomputed git blob id (no network).
    let protocol = HfHubProtocol::new("http://127.0.0.1:9");
    let bytes = b"{\"hello\": 1}";
    let file = mgc_resolver::protocols::ModelFile {
        path: "config.json".to_string(),
        size: Some(bytes.len() as u64),
        sha256: None,
        git_blob_sha1: git_blob_sha1(bytes),
        url: "http://127.0.0.1:9/x".to_string(),
    };
    protocol.verify_file(&file, bytes).unwrap();
    let err = protocol.verify_file(&file, b"{}").unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[tokio::test]
async fn hfhub_traversal_path_in_tree_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    server
        .mock(
            "GET",
            format!("/api/models/org/demo/tree/{COMMIT}?recursive=true").as_str(),
        )
        .with_status(200)
        .with_body(r#"[{"type":"file","path":"../evil","oid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]"#)
        .create_async()
        .await;

    let protocol = HfHubProtocol::new(&base);
    let err = protocol
        .resolve_model("org/demo", COMMIT)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("escapes the repo"), "{err}");
}

#[tokio::test]
async fn hfhub_revision_must_be_exact_commit() {
    let protocol = HfHubProtocol::new("http://127.0.0.1:9");
    for bad in ["main", "v1.0", "abc123", ""] {
        let err = protocol.resolve_model("org/demo", bad).await.unwrap_err();
        assert!(
            err.to_string().contains("exact 40-hex commit"),
            "branch/tag revision must fail closed: {err}"
        );
    }
}

#[tokio::test]
async fn hfhub_model_id_rejects_traversal_and_urls() {
    let protocol = HfHubProtocol::new("http://127.0.0.1:9");
    for bad in [
        "",
        "nodivider",
        "a/b/c",
        "../evil",
        "https://x/y",
        "org/demo ",
    ] {
        let err = protocol.resolve_model(bad, COMMIT).await.unwrap_err();
        assert!(
            err.to_string().contains("invalid model id"),
            "{bad:?}: {err}"
        );
    }
}

#[test]
fn git_blob_sha1_matches_git_hash_object() {
    // Verified against real git + python hashlib (never from memory):
    // `printf 'hello' | git hash-object --stdin`.
    assert_eq!(
        git_blob_sha1(b"hello"),
        "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0"
    );
    assert_eq!(git_blob_sha1(b"").len(), 40);
}
