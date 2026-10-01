//! PyPI protocol engine tests — hermetic via mockito (no real network).
//! Test engine PyPI — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{PypiProtocol, RegistryProtocol};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) => panic!(
            "PyPI mock tests require localhost; refusing to report skipped tests as passing: {error}"
        ),
    }
    Some(mockito::Server::new_async().await)
}

fn wheel_with_record(record: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in [
        ("six.py", b"VALUE = 1\n".as_slice()),
        (
            "six-1.0.0.dist-info/METADATA",
            b"Name: six\nVersion: 1.0.0\n".as_slice(),
        ),
        ("six-1.0.0.dist-info/RECORD", record.as_bytes()),
    ] {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn wheel_with_module(module_bytes: &[u8]) -> Vec<u8> {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let metadata = b"Name: demo\nVersion: 1.0.0\n";
    let module_hash =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(module_bytes));
    let metadata_hash =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(metadata));
    let record = format!(
        "demo.py,sha256={module_hash},{}\ndemo-1.0.0.dist-info/METADATA,sha256={metadata_hash},{}\ndemo-1.0.0.dist-info/RECORD,,\n",
        module_bytes.len(),
        metadata.len()
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in [
        ("demo.py", module_bytes),
        ("demo-1.0.0.dist-info/METADATA", metadata.as_slice()),
        ("demo-1.0.0.dist-info/RECORD", record.as_bytes()),
    ] {
        writer.start_file(path, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
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
async fn pypi_malformed_requires_dist_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let sha = sha256_hex(b"WHEEL");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{wheel}]"#,
        wheel = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/demo.whl"),
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
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(r#"">=1""#))
        .create_async()
        .await;

    let error = PypiProtocol::new(&base)
        .resolve("demo", "==1.0.0")
        .await
        .expect_err("malformed Requires-Dist must not disappear from the graph");
    assert!(error.to_string().contains("Requires-Dist"), "{error}");
}

#[tokio::test]
async fn pypi_requested_dependency_extras_fail_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let sha = sha256_hex(b"WHEEL");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{wheel}]"#,
        wheel = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/demo.whl"),
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
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(r#""idna[socks]>=3""#))
        .create_async()
        .await;

    let error = PypiProtocol::new(&base)
        .resolve("demo", "==1.0.0")
        .await
        .expect_err("requested Python extras must not be stripped from the graph");
    assert!(error.to_string().contains("extras"), "{error}");
}

#[test]
fn pypi_materialization_isolated_by_integrity_when_url_basename_collides() {
    let temp = tempfile::tempdir().unwrap();
    let protocol = PypiProtocol::new("https://pypi.example.test");
    let artifact_url = "https://files.example.test/packages/demo-1.0.0-py3-none-any.whl";
    let first_bytes = b"wheel bytes from source one";
    let second_bytes = b"wheel bytes from source two";
    let first = mgc_resolver::protocols::ResolvedEntry {
        name: "demo".to_string(),
        version: "1.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: artifact_url.to_string(),
        sha256: sha256_hex(first_bytes),
        extra_markers: Vec::new(),
    };
    let second = mgc_resolver::protocols::ResolvedEntry {
        sha256: sha256_hex(second_bytes),
        ..first.clone()
    };

    let first_path = protocol
        .materialize(&first, first_bytes, temp.path())
        .unwrap();
    let second_path = protocol
        .materialize(&second, second_bytes, temp.path())
        .unwrap();

    assert_ne!(
        first_path, second_path,
        "same URL basename with different integrity must not alias in a shared store"
    );
    assert_eq!(std::fs::read(first_path).unwrap(), first_bytes);
    assert_eq!(std::fs::read(second_path).unwrap(), second_bytes);
}

#[test]
fn pypi_materialization_rejects_bytes_that_disagree_with_lock_digest() {
    let temp = tempfile::tempdir().unwrap();
    let protocol = PypiProtocol::new("https://pypi.example.test");
    let bytes = b"artifact bytes";
    let entry = mgc_resolver::protocols::ResolvedEntry {
        name: "demo".to_string(),
        version: "1.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: "https://files.example.test/demo-1.0.0.whl".to_string(),
        sha256: sha256_hex(b"different bytes"),
        extra_markers: Vec::new(),
    };

    let error = protocol
        .materialize(&entry, bytes, temp.path())
        .expect_err("must refuse bytes not authenticated by the resolved digest");

    assert!(error.to_string().contains("mismatched SHA-256"));
    assert!(std::fs::read_dir(temp.path()).unwrap().next().is_none());
}

#[test]
fn pypi_importable_sites_are_isolated_by_integrity_for_same_package_version() {
    let temp = tempfile::tempdir().unwrap();
    let protocol = PypiProtocol::new("https://pypi.example.test");
    let artifact_url = "https://files.example.test/packages/demo-1.0.0-py3-none-any.whl";
    let first_bytes = wheel_with_module(b"VALUE = 'source one'\n");
    let second_bytes = wheel_with_module(b"VALUE = 'source two'\n");
    let mut entry = mgc_resolver::protocols::ResolvedEntry {
        name: "demo".to_string(),
        version: "1.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: artifact_url.to_string(),
        sha256: sha256_hex(&first_bytes),
        extra_markers: Vec::new(),
    };

    let mut mismatched_entry = entry.clone();
    mismatched_entry.sha256 = sha256_hex(b"not the wheel bytes");
    assert!(
        protocol
            .materialize_importable(&mismatched_entry, &first_bytes, temp.path())
            .is_err()
    );
    assert!(
        !temp.path().join("site").exists(),
        "digest mismatch must be rejected before unpacking"
    );

    let first_site = protocol
        .materialize_importable(&entry, &first_bytes, temp.path())
        .unwrap()
        .unwrap();
    entry.sha256 = sha256_hex(&second_bytes);
    let second_site = protocol
        .materialize_importable(&entry, &second_bytes, temp.path())
        .unwrap()
        .unwrap();

    assert_ne!(first_site, second_site);
    assert_eq!(
        std::fs::read(first_site.join("demo.py")).unwrap(),
        b"VALUE = 'source one'\n"
    );
    assert_eq!(
        std::fs::read(second_site.join("demo.py")).unwrap(),
        b"VALUE = 'source two'\n"
    );
}

#[test]
fn pypi_import_materialization_rejects_a_wheel_with_a_stale_record() {
    let wheel = wheel_with_record(
        "six.py,sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA,23\nsix-1.0.0.dist-info/METADATA,sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB,11\nsix-1.0.0.dist-info/RECORD,,\n",
    );
    let entry = mgc_resolver::protocols::ResolvedEntry {
        name: "six".to_string(),
        version: "1.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: "https://files.example.test/six-1.0.0-py3-none-any.whl".to_string(),
        sha256: sha256_hex(&wheel),
        extra_markers: Vec::new(),
    };
    let temp = tempfile::tempdir().unwrap();

    let error = PypiProtocol::new("https://pypi.example.test")
        .materialize_importable(&entry, &wheel, temp.path())
        .expect_err("tampered member must fail closed");

    assert!(error.to_string().contains("RECORD"), "{error}");
}

#[tokio::test]
async fn pypi_skips_unrequested_extra_marked_transitive_requirements() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let sha = sha256_hex(b"WHEEL");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{wheel}]"#,
        wheel = file_entry(
            "demo-1.0.0-py3-none-any.whl",
            &format!("{base}/files/demo.whl"),
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
        .mock("GET", "/pypi/demo/1.0.0/json")
        .with_status(200)
        .with_body(version_json(r#""attrs[tests]>=22; extra == \"cov\"""#))
        .create_async()
        .await;

    let entry = PypiProtocol::new(&base)
        .resolve("demo", "==1.0.0")
        .await
        .expect("an unrequested optional extra must not block base installation");

    assert!(
        entry.deps.is_empty(),
        "unrequested extra leaked into the base graph"
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

#[tokio::test]
async fn pypi_foreign_wheel_loses_to_sdist() {
    // V1.2 (D0): no "any wheel" fallback — a wheel built for another
    // platform must never install silently. `win_amd64` matches no host
    // branch on any CI OS (macos-aarch64 / linux-aarch64 / windows all
    // reject it), so the sdist must win everywhere.
    let Some(mut server) = mock_server().await else {
        return;
    };
    let wheel_sha = sha256_hex(b"FOREIGN_WHEEL");
    let sdist_sha = sha256_hex(b"SDIST_FALLBACK");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{w}, {s}]"#,
        w = file_entry(
            "demo-1.0.0-cp312-cp312-win_amd64.whl",
            &format!("{base}/files/demo-1.0.0-cp312-cp312-win_amd64.whl"),
            &wheel_sha,
            "bdist_wheel"
        ),
        s = file_entry(
            "demo-1.0.0.tar.gz",
            &format!("{base}/files/demo-1.0.0.tar.gz"),
            &sdist_sha,
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
        format!("{base}/files/demo-1.0.0.tar.gz"),
        "foreign wheel must lose to sdist"
    );
    assert_eq!(entry.sha256, sdist_sha);
}

#[tokio::test]
async fn pypi_only_foreign_wheel_fails_closed() {
    // Nothing installable and no sdist: resolve MUST fail, never hand
    // back the foreign wheel.
    let Some(mut server) = mock_server().await else {
        return;
    };
    let wheel_sha = sha256_hex(b"FOREIGN_ONLY");
    let base = server.url();
    let releases = format!(
        r#""1.0.0": [{w}]"#,
        w = file_entry(
            "demo-1.0.0-cp312-cp312-win_amd64.whl",
            &format!("{base}/files/demo-1.0.0-cp312-cp312-win_amd64.whl"),
            &wheel_sha,
            "bdist_wheel"
        ),
    );
    server
        .mock("GET", "/pypi/demo/json")
        .with_status(200)
        .with_body(index_json(&releases))
        .create_async()
        .await;

    let protocol = PypiProtocol::new(&base);
    let err = protocol.resolve("demo", "==1.0.0").await.unwrap_err();
    assert!(
        err.to_string().contains("no downloadable file"),
        "unexpected error: {err}"
    );
}

#[test]
fn pypi_install_rejects_unhashed_non_record_wheel_files() {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let module = b"VALUE = 1\n";
    let metadata = b"Name: six\nVersion: 1.0.0\n";
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let record = format!(
        "six.py,,{}\nsix-1.0.0.dist-info/METADATA,sha256={},{}\nsix-1.0.0.dist-info/RECORD,,\n",
        module.len(),
        b64.encode(Sha256::digest(metadata)),
        metadata.len(),
    );
    let wheel = wheel_with_record(&record);
    let temp = tempfile::tempdir().unwrap();
    let protocol = PypiProtocol::new("https://pypi.org");
    let entry = mgc_resolver::protocols::ResolvedEntry {
        name: "six".to_string(),
        version: "1.0.0".to_string(),
        deps: Vec::new(),
        artifact_url: "https://files.pythonhosted.org/six-1.0.0-py3-none-any.whl".to_string(),
        sha256: sha256_hex(&wheel),
        extra_markers: Vec::new(),
    };

    assert!(
        protocol
            .materialize_importable(&entry, &wheel, temp.path())
            .is_err(),
        "a regular wheel member without a SHA-256 RECORD entry must not install successfully"
    );
}
