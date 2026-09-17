//! NuGet v3 engine tests — hermetic via mockito (no real network).
//! Test engine NuGet v3 — hermetic qua mockito (không mạng thật).

#![allow(clippy::unwrap_used)]

use base64::Engine as _;
use mgc_resolver::protocols::{NuGetProtocol, RegistryProtocol};
use sha2::{Digest, Sha512};

async fn mock_server() -> Option<mockito::ServerGuard> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("warning: skipping nuget mock test because localhost bind is blocked");
            return None;
        }
        Err(error) => panic!("failed to probe localhost bind: {error}"),
    }
    Some(mockito::Server::new_async().await)
}

/// Hand-assembled store-method zip standing in for a nupkg (no zip crate).
/// Zip method store tự ghép thay nupkg (không crate zip).
fn build_nupkg(id: &str, version: &str) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = vec![
        (format!("{id}.nuspec"), nuspec_body(id).into_bytes()),
        (
            format!("lib/net6.0/{id}.dll"),
            format!("{id} {version} payload").into_bytes(),
        ),
    ];
    let mut out: Vec<u8> = Vec::new();
    let mut centrals: Vec<Vec<u8>> = Vec::new();
    for (name, data) in &entries {
        let local_offset = out.len() as u32;
        let crc = mgc_resolver::protocols::zip_reader::crc32(data);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
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
        cd.extend_from_slice(&0u16.to_le_bytes());
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

fn nuspec_body(id: &str) -> String {
    format!(
        r#"<package><metadata>
  <id>{id}</id>
  <dependencies>
    <group>
      <dependency id="base.lib" version="[2.1.0]" />
    </group>
    <group targetFramework="net6.0">
      <dependency id="net6.only" version="3.0.0" />
      <dependency id="no.version" />
    </group>
  </dependencies>
</metadata></package>"#
    )
}

fn registration_body(id: &str, version: &str, nupkg_hash_b64: &str) -> String {
    format!(
        r#"{{"items":[{{"items":[{{"catalogEntry":{{"id":"{id}","version":"{version}","listed":true,"packageHashAlgorithm":"SHA512","packageHash":"{nupkg_hash_b64}"}}}}]}}]}}"#
    )
}

#[tokio::test]
async fn nuget_service_index_probe_and_full_resolve_verify_materialize() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let nupkg = build_nupkg("Demo.Lib", "13.0.3");
    let sha512_b64 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&nupkg));

    // Service index advertising both resources (any order).
    // (Service index công bố cả hai resource (thứ tự bất kỳ).)
    server
        .mock("GET", "/v3/index.json")
        .with_status(200)
        .with_body(format!(
            r#"{{"version":"3.0.0","resources":[{{"@id":"{base}/flat/","@type":"PackageBaseAddress/3.0.0"}},{{"@id":"{base}/reg/","@type":"RegistrationsBaseUrl/3.6.0"}}]}}"#
        ))
        .create_async()
        .await;

    server
        .mock("GET", "/flat/demo.lib/index.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.0","13.0.3","2.0.0"]}"#)
        .create_async()
        .await;
    // Minimum-version selection: bare "13.0.3" = exact pin → 13.0.3.
    // (Resolve minimum-version: "13.0.3" trần = ghim chính xác → 13.0.3.)
    server
        .mock("GET", "/reg/demo.lib/index.json")
        .with_status(200)
        .with_body(registration_body("Demo.Lib", "13.0.3", &sha512_b64))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/demo.lib/13.0.3/demo.lib.nuspec")
        .with_status(200)
        .with_body(nuspec_body("Demo.Lib"))
        .create_async()
        .await;
    let nupkg_mock = server
        .mock("GET", "/flat/demo.lib/13.0.3/Demo.Lib.13.0.3.nupkg")
        .with_status(200)
        .with_body(nupkg.as_slice())
        .create_async()
        .await;

    let protocol = NuGetProtocol::from_service_index(&format!("{base}/v3/index.json"))
        .await
        .unwrap();
    let entry = protocol.resolve("Demo.Lib", "13.0.3").await.unwrap();
    assert_eq!(entry.version, "13.0.3");
    assert!(
        entry.sha256.is_empty(),
        "sha256 stays empty — the authoritative digest is SHA-512 via marker"
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == &format!("sha512:{sha512_b64}")),
        "{:?}",
        entry.extra_markers
    );
    assert!(
        entry.extra_markers.iter().any(|m| m == "group-tfm:net6.0"),
        "framework group must be marked: {:?}",
        entry.extra_markers
    );
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m == "unresolved-version:no.version"),
        "{:?}",
        entry.extra_markers
    );
    assert_eq!(
        entry.deps,
        vec![
            ("base.lib".to_string(), "[2.1.0]".to_string()),
            ("net6.only".to_string(), "3.0.0".to_string()),
        ]
    );

    let downloaded = protocol.download(&entry).await.unwrap();
    protocol.verify(&entry, &downloaded).unwrap();
    nupkg_mock.assert_async().await;

    // Global-packages materialization: nupkg + extracted content + sha512
    // sidecar.
    // (Materialize global-packages: nupkg + nội dung giải nén + sidecar
    // sha512.)
    let root = tempfile::tempdir().unwrap();
    let nupkg_path = protocol
        .materialize(&entry, &downloaded, root.path())
        .unwrap();
    assert_eq!(
        nupkg_path,
        root.path().join("demo.lib/13.0.3/Demo.Lib.13.0.3.nupkg")
    );
    assert!(
        root.path()
            .join("demo.lib/13.0.3/demo.lib.nuspec")
            .is_file(),
        "nuspec extracted from the nupkg"
    );
    assert!(
        root.path()
            .join("demo.lib/13.0.3/lib/net6.0/Demo.Lib.dll")
            .is_file(),
        "package content extracted"
    );
    let sidecar = std::fs::read_to_string(
        root.path()
            .join("demo.lib/13.0.3/Demo.Lib.13.0.3.nupkg.sha512"),
    )
    .unwrap();
    assert_eq!(sidecar, sha512_b64);
}

#[tokio::test]
async fn nuget_tampered_nupkg_fails_closed_and_unlisted_never_selected() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let nupkg = build_nupkg("Bad.Pkg", "1.0.0");
    let sha512_b64 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(b"TAMPERED"));

    server
        .mock("GET", "/flat/bad.pkg/index.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.0"]}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/reg/bad.pkg/index.json")
        .with_status(200)
        .with_body(registration_body("Bad.Pkg", "1.0.0", &sha512_b64))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/bad.pkg/1.0.0/bad.pkg.nuspec")
        .with_status(200)
        .with_body(nuspec_body("Bad.Pkg"))
        .create_async()
        .await;

    let protocol = NuGetProtocol::with_bases(&format!("{base}/reg"), &format!("{base}/flat"));
    let entry = protocol.resolve("Bad.Pkg", "1.0.0").await.unwrap();
    let err = protocol.verify(&entry, &nupkg).unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");

    // An unlisted-only version set fails closed on resolve.
    // (Tập version chỉ toàn unlisted fail-closed khi resolve.)
    let mut unlisted = registration_body("Bad.Pkg", "1.0.0", &sha512_b64);
    unlisted = unlisted.replace("\"listed\":true", "\"listed\":false");
    let unlisted_mock = server
        .mock("GET", "/reg/bad.pkg/index.json")
        .with_status(200)
        .with_body(unlisted)
        .create_async()
        .await;
    let err = protocol.resolve("Bad.Pkg", "1.0.0").await.unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Integrity(_)), "{err:?}");
    unlisted_mock.assert_async().await;
}

#[tokio::test]
async fn nuget_minimum_version_selection_with_float_range() {
    let Some(mut server) = mock_server().await else {
        return;
    };
    let base = server.url();
    let nupkg = build_nupkg("Float.Pkg", "1.0.5");
    let sha512_b64 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&nupkg));

    server
        .mock("GET", "/flat/float.pkg/index.json")
        .with_status(200)
        .with_body(r#"{"versions":["1.0.5","1.9.0","2.0.0"]}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/reg/float.pkg/index.json")
        .with_status(200)
        .with_body(registration_body("Float.Pkg", "1.0.5", &sha512_b64))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/float.pkg/1.0.5/float.pkg.nuspec")
        .with_status(200)
        .with_body(r#"<package><metadata><dependencies /></metadata></package>"#)
        .create_async()
        .await;

    let protocol = NuGetProtocol::with_bases(&format!("{base}/reg"), &format!("{base}/flat"));
    // NuGet minimum-version resolution: 1.* float → the LOWEST matching
    // (1.0.5), never the newest.
    // (Resolve minimum-version của NuGet: float 1.* → bản thấp nhất khớp
    // (1.0.5), không bao giờ bản mới nhất.)
    let entry = protocol.resolve("Float.Pkg", "1.*").await.unwrap();
    assert_eq!(entry.version, "1.0.5");
    assert!(entry.deps.is_empty());
}

fn multi_tfm_nuspec(dep_a: &str, ver_a: &str, dep_b: &str, ver_b: &str) -> String {
    format!(
        r#"<package><metadata>
  <dependencies>
    <group targetFramework="net6.0">
      <dependency id="{dep_a}" version="{ver_a}" />
    </group>
    <group targetFramework="net48">
      <dependency id="{dep_b}" version="{ver_b}" />
    </group>
  </dependencies>
</metadata></package>"#
    )
}

async fn resolve_nuspec(
    server: &mut mockito::ServerGuard,
    nuspec: String,
) -> (NuGetProtocol, String) {
    let base = server.url();
    let nupkg = build_nupkg("Multi.Tfm", "9.9.9");
    let sha512_b64 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&nupkg));
    server
        .mock("GET", "/flat/multi.tfm/index.json")
        .with_status(200)
        .with_body(r#"{"versions":["9.9.9"]}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/reg/multi.tfm/index.json")
        .with_status(200)
        .with_body(registration_body("Multi.Tfm", "9.9.9", &sha512_b64))
        .create_async()
        .await;
    server
        .mock("GET", "/flat/multi.tfm/9.9.9/multi.tfm.nuspec")
        .with_status(200)
        .with_body(nuspec)
        .create_async()
        .await;
    let protocol = NuGetProtocol::with_bases(&format!("{base}/reg"), &format!("{base}/flat"));
    (protocol, sha512_b64)
}

#[tokio::test]
async fn nuget_divergent_tfm_groups_fail_closed() {
    // V1.2 (D0): two frameworks with different dependency sets and no
    // consumer target to select by — merging would silently build the
    // wrong graph. Fail naming both frameworks.
    let Some(mut server) = mock_server().await else {
        return;
    };
    let (protocol, _) = resolve_nuspec(
        &mut server,
        multi_tfm_nuspec("net6.dep", "1.0.0", "net48.dep", "2.0.0"),
    )
    .await;
    let err = protocol.resolve("Multi.Tfm", "9.9.9").await.unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("net6.0") && message.contains("net48"),
        "must name both frameworks: {message}"
    );
}

#[tokio::test]
async fn nuget_identical_tfm_groups_merge_honestly() {
    // Same dep set under two frameworks: merging is harmless and stays
    // allowed, labeled by marker (no consumer target needed).
    let Some(mut server) = mock_server().await else {
        return;
    };
    let (protocol, _) = resolve_nuspec(
        &mut server,
        multi_tfm_nuspec("shared.dep", "1.0.0", "shared.dep", "1.0.0"),
    )
    .await;
    let entry = protocol.resolve("Multi.Tfm", "9.9.9").await.unwrap();
    assert!(
        entry
            .extra_markers
            .iter()
            .any(|m| m.starts_with("multi-tfm-identical:")),
        "{:?}",
        entry.extra_markers
    );
}
