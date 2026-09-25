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
