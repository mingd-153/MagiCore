//! pub.dev protocol engine tests — hermetic via mockito (no real network).
//! Test engine pub.dev — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{PubProtocol, RegistryProtocol};
use serde_json::{Value, json};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping pub mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

fn pub_version(version: &str, archive_url: &str, sha: &str, deps: Value) -> Value {
    json!({
        "version": version,
        "pubspec": {
            "version": version,
            "environment": { "sdk": "^3.0.0" },
            "dependencies": deps,
        },
        "archive_url": archive_url,
        "archive_sha256": sha,
    })
}

fn pub_json(versions: Vec<Value>) -> String {
    serde_json::to_string(&json!({ "name": "http", "versions": versions })).unwrap()
}

#[tokio::test]
async fn pub_constraint_selection_caret_any_exact() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let sha = sha256_hex(b"ARCHIVE");
    let versions = vec![
        pub_version(
            "1.2.0",
            &format!("{base}/archives/http-1.2.0.tar.gz"),
            &sha,
            json!({}),
        ),
        pub_version(
            "0.13.0",
            &format!("{base}/archives/http-0.13.0.tar.gz"),
            &sha,
            json!({}),
        ),
        pub_version(
            "2.0.0",
            &format!("{base}/archives/http-2.0.0.tar.gz"),
            &sha,
            json!({}),
        ),
    ];
    server
        .mock("GET", "/api/packages/http")
        .with_status(200)
        .with_body(pub_json(versions))
        .create_async()
        .await;

    let protocol = PubProtocol::new(&base);
    // ^1.0.0 → >=1.0.0 <2.0.0 → 1.2.0
    assert_eq!(
        protocol.resolve("http", "^1.0.0").await.unwrap().version,
        "1.2.0"
    );
    // any → highest → 2.0.0
    assert_eq!(
        protocol.resolve("http", "any").await.unwrap().version,
        "2.0.0"
    );
    // bare version → exact
    assert_eq!(
        protocol.resolve("http", "0.13.0").await.unwrap().version,
        "0.13.0"
    );
}

#[tokio::test]
async fn pub_resolves_dependencies_and_full_path() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bytes = b"PUB_ARCHIVE_BYTES";
    let sha = sha256_hex(bytes);
    let base = server.url();
    let versions = vec![pub_version(
        "1.2.0",
        &format!("{base}/archives/http-1.2.0.tar.gz"),
        &sha,
        json!({ "http_parser": "^0.4.0", "flutter": "^3.0.0" }),
    )];
    let api_mock = server
        .mock("GET", "/api/packages/http")
        .with_status(200)
        .with_body(pub_json(versions))
        .create_async()
        .await;
    let archive_mock = server
        .mock("GET", "/archives/http-1.2.0.tar.gz")
        .with_status(200)
        .with_body(bytes.as_slice())
        .create_async()
        .await;

    let protocol = PubProtocol::new(&base);
    let entry = protocol.resolve("http", "^1.0.0").await.unwrap();
    // flutter SDK dep dropped, http_parser kept
    assert_eq!(
        entry.deps,
        vec![("http_parser".to_string(), "^0.4.0".to_string())]
    );
    assert!(entry.extra_markers.iter().any(|m| m == "sdk:^3.0.0"));

    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();
    let expected_hex = blake3::hash(bytes).to_hex().to_string();
    assert_eq!(
        protocol.store_ref(&downloaded),
        format!("files/blake3/{}/{}", &expected_hex[..2], expected_hex)
    );

    api_mock.assert_async().await;
    archive_mock.assert_async().await;
}

#[tokio::test]
async fn pub_unresolvable_deps_record_markers_not_silence() {
    // V1.2 (D0): SDK-owned, path/git/hosted-object, and empty-constraint
    // deps are recorded as markers — the old silent `continue` hid graph
    // holes.
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let sha = sha256_hex(b"ARCHIVE2");
    let versions = vec![pub_version(
        "3.0.0",
        &format!("{base}/archives/mix-3.0.0.tar.gz"),
        &sha,
        json!({
            "flutter": "sdk",
            "gitdep": {"git": "https://example.invalid/repo.git"},
            "emptydep": "",
            "realdep": "^1.0.0",
        }),
    )];
    server
        .mock("GET", "/api/packages/mix")
        .with_status(200)
        .with_body(pub_json(versions))
        .create_async()
        .await;

    let protocol = PubProtocol::new(&base);
    let entry = protocol.resolve("mix", "any").await.unwrap();
    assert_eq!(
        entry.deps,
        vec![("realdep".to_string(), "^1.0.0".to_string())]
    );
    for marker in [
        "sdk-owned:flutter",
        "non-registry-dep:gitdep",
        "empty-constraint:emptydep",
    ] {
        assert!(
            entry.extra_markers.iter().any(|m| m == marker),
            "missing marker {marker}: {:?}",
            entry.extra_markers
        );
    }
}
