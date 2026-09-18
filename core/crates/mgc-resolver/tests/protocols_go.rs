//! Go module proxy engine tests — hermetic via mockito (no real network).
//! Test engine Go module proxy — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use mgc_resolver::protocols::sha256_hex;
use mgc_resolver::protocols::{GoModProtocol, RegistryProtocol};

/// Build a minimal module zip whose layout matches dirhash.HashZip input
/// (entries carry the `{module}@{version}/` prefix). Uses the shared
/// hand-rolled zip shape (store method) — no zip-writer crate.
/// Dựng zip module tối giản với layout đúng input của dirhash.HashZip
/// (entry mang prefix `{module}@{version}/`). Dùng shape zip tự ghép dùng
/// chung (method store) — không crate zip-writer.
fn build_module_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut centrals: Vec<Vec<u8>> = Vec::new();
    for (name, data) in entries {
        let local_offset = out.len() as u32;
        let crc = mgc_resolver::protocols::zip_reader::crc32(data);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method 0 = store
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        let mut cd = Vec::new();
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes()); // method
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&local_offset.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        centrals.push(cd);
    }
    let cd_offset = out.len() as u32;
    for cd in &centrals {
        out.extend_from_slice(cd);
    }
    let cd_size = out.len() as u32 - cd_offset;
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
    out.extend_from_slice(&(centrals.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping go mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

#[tokio::test]
async fn go_resolves_graph_with_ziphash_and_materializes_cache() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let zip_bytes = build_module_zip(&[
        (
            "example.com/pkg@v1.2.3/go.mod",
            b"module example.com/pkg\n".as_slice(),
        ),
        ("example.com/pkg@v1.2.3/pkg.go", b"package pkg\n".as_slice()),
    ]);
    let zip_hash = sha256_hex(&zip_bytes);
    let gomod_body = "module example.com/pkg\n\ngo 1.22\n\nrequire (\n\tgithub.com/dep/one v1.0.0\n\tgithub.com/dep/two v0.2.0 // indirect\n)\n\nreplace github.com/dep/one => github.com/dep/one-fork v1.0.0\n\nexclude github.com/dep/two v0.2.0\n";
    let info_body = r#"{"Version":"v1.2.3","Time":"2024-01-02T03:04:05Z"}"#;

    // @v/list with a pseudo-version and a lower version that must lose.
    server
        .mock("GET", "/example.com/pkg/@v/list")
        .with_status(200)
        .with_body("v1.2.3\nv1.2.2\nv0.0.0-20230101000000-abcdef123456\n")
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/pkg/@v/v1.2.3.info")
        .with_status(200)
        .with_body(info_body)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/pkg/@v/v1.2.3.mod")
        .with_status(200)
        .with_body(gomod_body)
        .create_async()
        .await;
    let ziphash_mock = server
        .mock("GET", "/example.com/pkg/@v/v1.2.3.ziphash")
        .with_status(200)
        .with_body(zip_hash.as_str())
        .create_async()
        .await;
    let zip_mock = server
        .mock("GET", "/example.com/pkg/@v/v1.2.3.zip")
        .with_status(200)
        .with_body(zip_bytes.as_slice())
        .create_async()
        .await;

    let protocol = GoModProtocol::with_sum_base(&server.url(), &server.url());
    let entry = protocol.resolve("example.com/pkg", "1.2.3").await.unwrap();
    assert_eq!(entry.version, "1.2.3");
    // Pinned exact version won over 1.2.2 and the pseudo-version.
    // (Version ghim chính xác thắng 1.2.2 và pseudo-version.)
    assert_eq!(
        entry.deps,
        vec![
            // replace rewrote the requirement; the excluded indirect dep
            // (dep/two @0.2.0) must be dropped from the graph.
            // (replace viết lại requirement; dep indirect bị exclude
            // (dep/two @0.2.0) phải bị loại khỏi graph.)
            ("github.com/dep/one-fork".to_string(), "1.0.0".to_string()),
        ]
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "indirect:github.com/dep/two"),
        "indirect dep must be recorded: {:?}",
        entry.extra_markers
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "excluded:github.com/dep/two@0.2.0"),
        "excluded module must be recorded: {:?}",
        entry.extra_markers
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "replace:github.com/dep/one=>github.com/dep/one-fork"),
        "replace must be recorded: {:?}",
        entry.extra_markers
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "time:2024-01-02T03:04:05Z"),
        "release time marker expected: {:?}",
        entry.extra_markers
    );

    // Download + verify against the proxy ziphash (sha256).
    let files = protocol.download_module(&entry).await.unwrap();
    protocol.verify(&entry, &files.zip).unwrap();
    ziphash_mock.assert_async().await;
    zip_mock.assert_async().await;

    // Materialize into a GOMODCACHE layout — GOPROXY=off readable.
    let cache = tempfile::tempdir().unwrap();
    let zip_path = protocol.materialize(&entry, &files, cache.path()).unwrap();
    assert_eq!(
        zip_path,
        cache
            .path()
            .join("cache/download/example.com/pkg/@v/v1.2.3.zip")
    );
    for f in ["v1.2.3.zip", "v1.2.3.mod", "v1.2.3.info", "v1.2.3.ziphash"] {
        assert!(
            cache
                .path()
                .join("cache/download/example.com/pkg/@v")
                .join(f)
                .is_file(),
            "{f} must be materialized"
        );
    }
    let stored_hash = std::fs::read_to_string(
        cache
            .path()
            .join("cache/download/example.com/pkg/@v/v1.2.3.ziphash"),
    )
    .unwrap();
    assert_eq!(stored_hash.trim(), sha256_hex(&zip_bytes));
}

#[tokio::test]
async fn go_sumdb_dirhash_fallback_verifies_and_fails_closed() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let zip_bytes = build_module_zip(&[
        (
            "example.com/sum@v0.1.0/go.mod",
            b"module example.com/sum\n".as_slice(),
        ),
        (
            "example.com/sum@v0.1.0/main.go",
            b"package main\n".as_slice(),
        ),
    ]);
    // Compute the REAL sumdb dirhash of the zip for the mocked record.
    // (Tính dirhash sumdb THẬT của zip cho bản ghi mock.)
    let entries = mgc_resolver::protocols::zip_reader::read_zip_entries(&zip_bytes).unwrap();
    let h1 = mgc_resolver::protocols::go::dirhash_zip(&entries).unwrap();

    server
        .mock("GET", "/example.com/sum/@v/list")
        .with_status(200)
        .with_body("v0.1.0\n")
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/sum/@v/v0.1.0.info")
        .with_status(200)
        .with_body(r#"{"Version":"v0.1.0","Time":"2024-05-06T07:08:09Z"}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/sum/@v/v0.1.0.mod")
        .with_status(200)
        .with_body("module example.com/sum\n")
        .create_async()
        .await;
    // No .ziphash on this proxy → sumdb fallback.
    // (Proxy này không có .ziphash → fallback sumdb.)
    server
        .mock("GET", "/example.com/sum/@v/v0.1.0.ziphash")
        .with_status(404)
        .create_async()
        .await;
    server
        .mock("GET", "/lookup/example.com/sum@v0.1.0")
        .with_status(200)
        .with_body(format!(
            // Genuine sumdb lookup shape (space-separated tokens, one
            // record per line, sig tail) — captured from sum.golang.org.
            // (Dạng lookup sumdb thật: token cách dấu cách, mỗi bản ghi
            // một dòng.)
            "42\nexample.com/sum v0.1.0 {h1}\nexample.com/sum v0.1.0/go.mod h1:AAAA=\n\ngo.sum database tree\n99\nSIGNATURE=\n"
        ))
        .create_async()
        .await;

    let protocol = GoModProtocol::with_sum_base(&server.url(), &server.url());
    let entry = protocol.resolve("example.com/sum", "*").await.unwrap();
    assert_eq!(entry.version, "0.1.0");
    assert!(entry.sha256.is_empty(), "sumdb path must not fake a sha256");
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m.starts_with("sumdb-ziphash:h1:")),
        "sumdb dirhash must be recorded: {:?}",
        entry.extra_markers
    );

    // Genuine zip verifies through the h1 directory hash.
    // (Zip nguyên bản được xác minh qua directory hash h1.)
    protocol.verify(&entry, &zip_bytes).unwrap();

    // Tampered payload fails closed with Integrity — flip a byte inside the
    // first entry's stored data (located via the local header, not a fixed
    // offset).
    // (Payload đã can thiệp fail-closed với Integrity — lật một byte trong
    // dữ liệu entry đầu (định vị qua local header, không dùng offset cứng).)
    let mut tampered = zip_bytes.clone();
    let first_local = zip_bytes
        .windows(4)
        .position(|w| w == b"PK\x03\x04")
        .unwrap();
    let name_len =
        u16::from_le_bytes([zip_bytes[first_local + 26], zip_bytes[first_local + 27]]) as usize;
    let data_idx = first_local + 30 + name_len + 2;
    tampered[data_idx] ^= 0xFF;
    let err = protocol.verify(&entry, &tampered).unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
}

#[tokio::test]
async fn go_latest_fallback_and_range_selection() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    // list 404 → @latest fallback.
    server
        .mock("GET", "/example.com/only/@v/list")
        .with_status(404)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/only/@v/latest")
        .with_status(200)
        .with_body(r#"{"Version":"v0.3.1","Time":"2024-02-03T04:05:06Z"}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/only/@v/v0.3.1.mod")
        .with_status(200)
        .with_body("module example.com/only\n")
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/only/@v/v0.3.1.info")
        .with_status(200)
        .with_body(r#"{"Version":"v0.3.1","Time":"2024-02-03T04:05:06Z"}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/example.com/only/@v/v0.3.1.ziphash")
        .with_status(200)
        .with_body("a".repeat(64))
        .create_async()
        .await;

    let protocol = GoModProtocol::with_sum_base(&server.url(), &server.url());
    let entry = protocol.resolve("example.com/only", "*").await.unwrap();
    assert_eq!(entry.version, "0.3.1");

    // Range that does not match the latest version fails closed.
    // (Range không khớp version mới nhất fail-closed.)
    assert!(protocol.resolve("example.com/only", "1.0.0").await.is_err());
}
