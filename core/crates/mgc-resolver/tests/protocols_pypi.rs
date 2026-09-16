//! PyPI protocol engine tests — hermetic via mockito (no real network).
//! Test engine PyPI — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{PypiProtocol, RegistryProtocol};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping pypi mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

fn file_entry(filename: &str, url: &str, sha: &str, ptype: &str) -> String {
    format!(
        r#"{{"filename":"{filename}","url":"{url}","digests":{{"sha256":"{sha}"}},"packagetype":"{ptype}","requires_python":">=3.9"}}"#
    )
}

fn index_json(releases: &str) -> String {
    format!(r#"{{"info":{{"requires_python":">=3.9"}},"releases":{{{releases}}}}}"#)
}

fn version_json(requires_dist: &str) -> String {
    format!(r#"{{"info":{{"requires_dist":[{requires_dist}]}},"releases":{{}}}}"#)
}

#[tokio::test]
async fn pypi_selects_universal_wheel_over_sdist_and_resolves_deps() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let wheel_sha = sha256_hex(b"WHEEL_BYTES");
    let sdist_sha = sha256_hex(b"SDIST_BYTES");
    let base = server.url();

    let releases = format!(
        r#""1.0.0": [{wheel}, {sdist}], "0.9.0": [{sdist0}]"#,
        wheel = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/demo-1.0.0-py3-none-any.whl"),
            &wheel_sha,
            "bdist_wheel"
        ),
        sdist = file_entry(
            "demo-1.0.0.tar.gz",
            &format!("{base}/files/demo-1.0.0.tar.gz"),
            &sdist_sha,
            "sdist"
        ),
        sdist0 = file_entry(
            "demo-0.9.0.tar.gz",
            &format!("{base}/files/demo-0.9.0.tar.gz"),
            &sdist_sha,
            "sdist"
        ),
    );

    let index_mock = server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(index_json(&releases))
        .create_async()
        .await;
    let version_mock = server
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(
            r#""dep-a>=1.0", "extras-only ; extra == \"security\"""#,
        ))
        .create_async()
        .await;

    let protocol = PypiProtocol::new(&base);
    let entry = protocol.resolve("demo", ">=0.9").await.unwrap();

    assert_eq!(entry.version, "1.0.0");
    assert_eq!(
        entry.artifact_url,
        format!("{base}/files/demo-1.0.0-py3-none-any.whl")
    );
    assert_eq!(entry.sha256, wheel_sha);
    assert_eq!(entry.deps, vec![("dep-a".to_string(), ">=1.0".to_string())]);
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "wheel:py3-none-any"),
        "universal wheel tag must be recorded: {:?}",
        entry.extra_markers
    );
    // extra dep marker recorded, dep skipped
    assert!(
        entry.extra_markers.iter().any(|m| m.contains("extra")),
        "extra dep marker must be recorded"
    );
    assert!(
        !entry.deps.iter().any(|(n, _)| n == "extras-only"),
        "extra dep must be dropped from graph"
    );

    index_mock.assert_async().await;
    version_mock.assert_async().await;
}

#[tokio::test]
async fn pypi_compatible_release_range() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let sha = sha256_hex(b"WHEEL");
    let base = server.url();
    let releases = format!(
        r#""1.4.9": [{w}], "1.5.0": [{w}], "1.4.4": [{w}]"#,
        w = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/w.whl"),
            &sha,
            "bdist_wheel"
        ),
    );
    server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(index_json(&releases))
        .create_async()
        .await;
    server
        .mock("GET", "/pypi/demo/1.4.9/json")
        .with_status(200)
        .with_body(version_json(""))
        .create_async()
        .await;

    let protocol = PypiProtocol::new(&base);
    // ~=1.4.5 → >=1.4.5,<1.5 → 1.4.9 (1.5.0 excluded, 1.4.4 excluded)
    assert_eq!(
        protocol.resolve("demo", "~=1.4.5").await.unwrap().version,
        "1.4.9"
    );
}

#[tokio::test]
async fn pypi_sdist_fallback_when_no_wheel() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let sha = sha256_hex(b"SDIST");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{s}]"#,
        s = file_entry(
            "demo-1.0.0.tar.gz",
            &format!("{base}/files/demo-1.0.0.tar.gz"),
            &sha,
            "sdist"
        ),
    );
    server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(index_json(&releases))
        .create_async()
        .await;
    server
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(""))
        .create_async()
        .await;

    let protocol = PypiProtocol::new(&base);
    let entry = protocol.resolve("demo", "==1.0.0").await.unwrap();
    assert_eq!(
        entry.artifact_url,
        format!("{base}/files/demo-1.0.0.tar.gz")
    );
    assert!(entry.extra_markers.iter().any(|m| m.starts_with("sdist:")));
}

#[tokio::test]
async fn pypi_resolve_download_verify_store_ref_full_path() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let bytes = b"WHEEL_ARCHIVE";
    let sha = sha256_hex(bytes);
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{w}]"#,
        w = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/demo-1.0.0-py3-none-any.whl"),
            &sha,
            "bdist_wheel"
        ),
    );

    let index_mock = server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(index_json(&releases))
        .create_async()
        .await;
    let version_mock = server
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(""))
        .create_async()
        .await;
    let file_mock = server
        .mock("GET", "/files/demo-1.0.0-py3-none-any.whl")
        .with_status(200)
        .with_body(bytes.as_slice())
        .create_async()
        .await;

    let protocol = PypiProtocol::new(&base);
    let entry = protocol.resolve("demo", "==1.0.0").await.unwrap();
    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();
    let expected_hex = blake3::hash(bytes).to_hex().to_string();
    assert_eq!(
        protocol.store_ref(&downloaded),
        format!("files/blake3/{}/{}", &expected_hex[..2], expected_hex)
    );

    index_mock.assert_async().await;
    version_mock.assert_async().await;
    file_mock.assert_async().await;
}
