//! Crates protocol engine tests — hermetic via mockito (no real network).
//! Test engine crates — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{CratesProtocol, RegistryProtocol};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping crates mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

/// Sparse-index NDJSON line for one crate version.
fn crate_line(name: &str, vers: &str, yanked: bool, cksum: &str, deps: &str) -> String {
    format!(
        r#"{{"name":"{name}","vers":"{vers}","deps":[{deps}],"cksum":"sha256:{cksum}","features":{{}},"yanked":{yanked},"links":null}}"#
    )
}

fn dep(name: &str, req: &str, kind: &str, optional: bool) -> String {
    format!(
        r#"{{"name":"{name}","req":"{req}","features":[],"optional":{optional},"default_features":true,"target":null,"kind":"{kind}","package":null}}"#
    )
}

#[tokio::test]
async fn crates_resolves_transitive_graph_skips_yanked_and_optional() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let serde_bytes = b"SERDE_CRATE_1.0.219";
    let core_bytes = b"SERDE_CORE_1.0.5";
    let serde_cksum = sha256_hex(serde_bytes);
    let core_cksum = sha256_hex(core_bytes);

    // serde 1.0.219 → normal dep serde_core (1.0) + optional serde_derive
    // 2.0.0 yanked (must be skipped) + 1.0.100 (lower, must lose to 1.0.219)
    let serde_body = [
        crate_line(
            "serde",
            "1.0.219",
            false,
            &serde_cksum,
            &format!(
                "{},{}",
                dep("serde_core", "1.0", "normal", false),
                dep("serde_derive", "=1.0.219", "normal", true)
            ),
        ),
        crate_line("serde", "2.0.0", true, &serde_cksum, ""),
        crate_line("serde", "1.0.100", false, &serde_cksum, ""),
    ]
    .join("\n");

    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(serde_body)
        .create_async()
        .await;

    // serde_core 1.0.5 → no deps
    server
        .mock("GET", "/se/rd/serde_core")
        .with_status(200)
        .with_body(crate_line("serde_core", "1.0.5", false, &core_cksum, ""))
        .create_async()
        .await;

    let protocol = CratesProtocol::with_download_base(&server.url(), &server.url());
    let entries = protocol.resolve_graph("serde", "^1.0").await.unwrap();

    assert_eq!(
        entries.len(),
        2,
        "optional serde_derive + yanked 2.0.0 must be excluded"
    );
    let serde = &entries[0];
    assert_eq!(serde.name, "serde");
    assert_eq!(serde.version, "1.0.219");
    assert_eq!(
        serde.deps,
        vec![("serde_core".to_string(), "1.0".to_string())]
    );
    assert!(
        serde
            .extra_markers
            .iter()
            .any(|m| m == "optional:serde_derive"),
        "optional dep must be recorded in extras: {:?}",
        serde.extra_markers
    );

    let core = &entries[1];
    assert_eq!(core.name, "serde_core");
    assert_eq!(core.version, "1.0.5");
}

#[tokio::test]
async fn crates_semver_selection_cases() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bytes = b"SERDE_CRATE";
    let cksum = sha256_hex(bytes);
    let body = [
        crate_line("serde", "1.0.219", false, &cksum, ""),
        crate_line("serde", "2.0.0", false, &cksum, ""),
        crate_line("serde", "1.0.100", false, &cksum, ""),
    ]
    .join("\n");
    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let protocol = CratesProtocol::with_download_base(&server.url(), &server.url());

    // ^1.0 → highest 1.x (2.0.0 excluded)
    assert_eq!(
        protocol.resolve("serde", "^1.0").await.unwrap().version,
        "1.0.219"
    );
    // =1.0.100 → exact
    assert_eq!(
        protocol.resolve("serde", "=1.0.100").await.unwrap().version,
        "1.0.100"
    );
    // >=1.0, <2.0 → 1.0.219
    assert_eq!(
        protocol
            .resolve("serde", ">=1.0, <2.0")
            .await
            .unwrap()
            .version,
        "1.0.219"
    );
    // ~1.0.100 → >=1.0.100 <1.1.0 → 1.0.219 (higher, same minor)
    assert_eq!(
        protocol.resolve("serde", "~1.0.100").await.unwrap().version,
        "1.0.219"
    );
}

#[tokio::test]
async fn crates_resolve_download_verify_store_ref_full_path() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bytes = b"SERDE_CRATE_ARCHIVE";
    let cksum = sha256_hex(bytes);

    let index_mock = server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(crate_line("serde", "1.0.219", false, &cksum, ""))
        .create_async()
        .await;
    let download_mock = server
        .mock("GET", "/serde/1.0.219/download")
        .with_status(200)
        .with_body(bytes.as_slice())
        .create_async()
        .await;

    let protocol = CratesProtocol::with_download_base(&server.url(), &server.url());
    let entry = protocol.resolve("serde", "^1.0").await.unwrap();
    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();

    let expected_hex = blake3::hash(bytes).to_hex().to_string();
    let expected_ref = format!("files/blake3/{}/{}", &expected_hex[..2], expected_hex);
    assert_eq!(protocol.store_ref(&downloaded), expected_ref);

    index_mock.assert_async().await;
    download_mock.assert_async().await;
}

#[tokio::test]
async fn crates_sha256_mismatch_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bytes = b"SERDE_CRATE_ARCHIVE";
    let wrong_cksum = sha256_hex(b"TAMPERED_BYTES");

    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(crate_line("serde", "1.0.219", false, &wrong_cksum, ""))
        .create_async()
        .await;
    server
        .mock("GET", "/serde/1.0.219/download")
        .with_status(200)
        .with_body(bytes.as_slice())
        .create_async()
        .await;

    let protocol = CratesProtocol::with_download_base(&server.url(), &server.url());
    let entry = protocol.resolve("serde", "^1.0").await.unwrap();
    let downloaded = protocol.download(&entry).await.unwrap();
    let err = protocol.verify(&entry, &downloaded).unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}
