//! pub.dev protocol engine tests — hermetic via mockito (no real network).
//! Test engine pub.dev — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{PubProtocol, RegistryProtocol};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) => panic!(
            "pub.dev mock tests require localhost; refusing to report skipped tests as passing: {error}"
        ),
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
async fn pub_missing_archive_digest_fails_resolution() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let mut version = pub_version(
        "1.0.0",
        &format!("{base}/archives/http-1.0.0.tar.gz"),
        "",
        json!({}),
    );
    version
        .as_object_mut()
        .expect("pub version object")
        .remove("archive_sha256");
    server
        .mock("GET", "/api/packages/http")
        .with_status(200)
        .with_body(pub_json(vec![version]))
        .create_async()
        .await;

    let error = PubProtocol::new(&base)
        .resolve("http", "^1.0.0")
        .await
        .expect_err("pub.dev package without an archive digest must not resolve");
    assert!(error.to_string().contains("archive_sha256"), "{error}");
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
async fn pub_multi_root_resolution_intersects_transitive_constraints() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let sha = sha256_hex(b"ARCHIVE");
    let root_a = vec![pub_version(
        "1.0.0",
        &format!("{base}/archives/root_a-1.0.0.tar.gz"),
        &sha,
        json!({ "shared": "^1.0.0" }),
    )];
    let root_b = vec![pub_version(
        "1.0.0",
        &format!("{base}/archives/root_b-1.0.0.tar.gz"),
        &sha,
        json!({ "shared": "<1.3.0" }),
    )];
    let shared = vec!["1.2.0", "1.3.0", "1.4.0"]
        .into_iter()
        .map(|version| {
            pub_version(
                version,
                &format!("{base}/archives/shared-{version}.tar.gz"),
                &sha,
                json!({}),
            )
        })
        .collect();
    for (name, versions) in [("root_a", root_a), ("root_b", root_b), ("shared", shared)] {
        server
            .mock("GET", format!("/api/packages/{name}").as_str())
            .with_status(200)
            .with_body(pub_json(versions))
            .create_async()
            .await;
    }

    let protocol = PubProtocol::new(&base);
    let graph = protocol
        .resolve_graph_roots(&[
            ("root_a".to_string(), "any".to_string()),
            ("root_b".to_string(), "any".to_string()),
        ])
        .await
        .unwrap();
    let shared = graph
        .iter()
        .find(|entry| entry.name == "shared")
        .expect("shared transitive package must be in the graph");
    assert_eq!(
        shared.version, "1.2.0",
        "both root constraints are satisfiable by 1.2.0; the resolver must intersect them instead of choosing per-root versions"
    );

    let error = protocol
        .resolve_graph_roots(&[("shared".to_string(), "^2.0.0".to_string())])
        .await
        .expect_err(
            "an empty intersection must fail closed instead of selecting an incompatible version",
        );
    assert!(matches!(error, mgc_types::MgError::DependencyConflict(_)));
}

#[tokio::test]
async fn pub_multi_root_metadata_fetch_is_bounded_and_concurrent() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) => panic!("failed to bind pub concurrency test server: {error}"),
    };
    let address = listener.local_addr().unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let server_active = Arc::clone(&active);
    let server_max_active = Arc::clone(&max_active);
    let archive_sha256 = sha256_hex(b"ARCHIVE");
    let request_count = 32usize;
    let server = tokio::spawn(async move {
        let mut handlers = Vec::with_capacity(request_count);
        for _ in 0..request_count {
            let (mut socket, _) = listener.accept().await.unwrap();
            let active = Arc::clone(&server_active);
            let max_active = Arc::clone(&server_max_active);
            let archive_sha256 = archive_sha256.clone();
            handlers.push(tokio::spawn(async move {
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = socket.read(&mut chunk).await.unwrap();
                    assert_ne!(read, 0, "client closed before sending HTTP headers");
                    request.extend_from_slice(&chunk[..read]);
                }
                let request_line = String::from_utf8(request).unwrap();
                let package = request_line
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|path| path.strip_prefix("/api/packages/"))
                    .expect("package route");

                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(40)).await;
                active.fetch_sub(1, Ordering::SeqCst);

                let body = serde_json::to_vec(&json!({
                    "name": package,
                    "versions": [{
                        "version": "1.0.0",
                        "pubspec": {
                            "version": "1.0.0",
                            "environment": { "sdk": "^3.0.0" },
                            "dependencies": {}
                        },
                        "archive_url": "http://127.0.0.1/archive.tar.gz",
                        "archive_sha256": archive_sha256
                    }]
                }))
                .unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
            }));
        }
        for handler in handlers {
            handler.await.unwrap();
        }
    });

    let protocol = PubProtocol::new(&format!("http://{address}"));
    let roots = (0..request_count)
        .map(|index| (format!("root{index}"), "any".to_string()))
        .collect::<Vec<_>>();
    let entries = protocol.resolve_graph_roots(&roots).await.unwrap();
    server.await.unwrap();

    assert_eq!(entries.len(), request_count);
    assert!(
        max_active.load(Ordering::SeqCst) > 1,
        "pub.dev multi-root metadata requests must overlap"
    );
    assert!(
        max_active.load(Ordering::SeqCst) <= 16,
        "pub.dev metadata concurrency must honor the shared resolver cap"
    );
}

#[tokio::test]
async fn pub_null_optional_maps_are_empty_but_null_pubspec_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let sha = sha256_hex(b"ARCHIVE");

    let mut dependency_free = pub_version(
        "1.0.0",
        &format!("{base}/archives/example-1.0.0.tar.gz"),
        &sha,
        json!({}),
    );
    dependency_free["pubspec"]["environment"] = Value::Null;
    dependency_free["pubspec"]["dependencies"] = Value::Null;
    let nullable_mock = server
        .mock("GET", "/api/packages/example")
        .with_status(200)
        .with_body(pub_json(vec![dependency_free]))
        .create_async()
        .await;

    let missing_pubspec = json!({
        "version": "1.0.0",
        "pubspec": null,
        "archive_url": format!("{base}/archives/missing-1.0.0.tar.gz"),
        "archive_sha256": sha,
    });
    let missing_mock = server
        .mock("GET", "/api/packages/missing")
        .with_status(200)
        .with_body(pub_json(vec![missing_pubspec]))
        .create_async()
        .await;

    let protocol = PubProtocol::new(&base);
    let entry = protocol.resolve("example", "^1.0.0").await.unwrap();
    assert!(entry.deps.is_empty());
    assert!(entry.extra_markers.is_empty());

    let error = protocol
        .resolve("missing", "^1.0.0")
        .await
        .expect_err("a selected version with no Pubspec must fail closed");
    assert!(error.to_string().contains("pubspec metadata"));
    nullable_mock.assert_async().await;
    missing_mock.assert_async().await;
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
async fn pub_sdk_owned_dependencies_are_recorded_not_silenced() {
    // SDK-owned dependencies are recorded explicitly; they are never
    // mistaken for resolved registry packages.
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
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|marker| marker == "sdk-owned:flutter"),
        "missing SDK marker: {:?}",
        entry.extra_markers
    );
}

#[tokio::test]
async fn pub_empty_dependency_constraint_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let versions = vec![pub_version(
        "3.0.0",
        &format!("{base}/archives/mix-3.0.0.tar.gz"),
        &sha256_hex(b"ARCHIVE2"),
        json!({"emptydep": ""}),
    )];
    server
        .mock("GET", "/api/packages/mix")
        .with_status(200)
        .with_body(pub_json(versions))
        .create_async()
        .await;

    let error = PubProtocol::new(&base)
        .resolve("mix", "any")
        .await
        .unwrap_err();
    assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
}

#[tokio::test]
async fn pub_git_dependency_fails_closed_instead_of_omitting_graph_edge() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let versions = vec![pub_version(
        "3.0.0",
        &format!("{base}/archives/mix-3.0.0.tar.gz"),
        &sha256_hex(b"ARCHIVE2"),
        json!({
            "gitdep": {"git": "https://example.invalid/repo.git"},
            "realdep": "^1.0.0",
        }),
    )];
    server
        .mock("GET", "/api/packages/mix")
        .with_status(200)
        .with_body(pub_json(versions))
        .create_async()
        .await;

    let error = PubProtocol::new(&base)
        .resolve("mix", "any")
        .await
        .unwrap_err();
    assert!(matches!(error, mgc_types::MgError::Unsupported { .. }));
}
