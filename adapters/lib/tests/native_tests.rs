#![cfg(test)]
#![allow(clippy::unwrap_used)]

//! Native registry client tests — HERMETIC qua mockito (không mạng thật).
//! Các dead-test trong block comment cũ đã được hồi sinh thành test offline thật.

use mgc_lib_adapter::native::cargo_client::CargoClient;
use mgc_lib_adapter::native::pypi_client::PyPiClient;
use mgc_lib_adapter::native::RegistryClient;
use mgc_types::{PackageId, PackageName, Version};

#[test]
fn cargo_client_new_uses_default_registry() {
    let client = CargoClient::new();
    drop(client);
}

#[test]
fn cargo_client_with_custom_registry() {
    let client = CargoClient::with_registry("https://custom.io".to_string());
    drop(client);
}

#[test]
fn pypi_client_new_uses_default_registry() {
    let client = PyPiClient::new();
    drop(client);
}

#[test]
fn pypi_client_with_custom_registry() {
    let client = PyPiClient::with_registry("https://pypi.example.com".to_string());
    drop(client);
}

// ---------------------------------------------------------- cargo (sparse index NDJSON)

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn cargo_fetch_metadata_parses_ndjson_skips_yanked() {
    let mut server = mockito::Server::new_async().await;
    // sparse index path cho "serde": /se/rd/serde — mỗi dòng 1 version entry
    let mock = server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body(
            "{\"vers\":\"1.0.0\",\"yanked\":false}\n\
             {\"vers\":\"2.0.0\",\"yanked\":true}\n\
             {\"vers\":\"1.5.0\",\"yanked\":false}\n",
        )
        .create_async()
        .await;

    let client = CargoClient::with_registry(server.url());
    let name = PackageName::new("serde").unwrap();
    let metadata = client.fetch_metadata(&name).await.unwrap();

    mock.assert_async().await;
    assert_eq!(metadata.name.as_str(), "serde");
    // yanked 2.0.0 bị bỏ; versions giữ thứ tự parse
    assert_eq!(metadata.versions.len(), 2);
    assert!(!metadata
        .versions
        .contains(&Version::parse("2.0.0").unwrap()));
    assert_eq!(metadata.latest, Version::parse("1.5.0").unwrap());
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn cargo_list_versions_matches_metadata() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/to/ki/tokio")
        .with_status(200)
        .with_body("{\"vers\":\"1.28.0\",\"yanked\":false}\n")
        .create_async()
        .await;

    let client = CargoClient::with_registry(server.url());
    let versions = client
        .list_versions(&PackageName::new("tokio").unwrap())
        .await
        .unwrap();
    assert_eq!(versions, vec![Version::parse("1.28.0").unwrap()]);
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn cargo_metadata_404_fails_closed_with_clear_error() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/no/pe/nope-does-not-exist")
        .with_status(404)
        .create_async()
        .await;

    let client = CargoClient::with_registry(server.url());
    let err = client
        .fetch_metadata(&PackageName::new("nope-does-not-exist").unwrap())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn cargo_all_versions_yanked_is_an_error_not_empty() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/se/rd/serde")
        .with_status(200)
        .with_body("{\"vers\":\"1.0.0\",\"yanked\":true}\n")
        .create_async()
        .await;

    let client = CargoClient::with_registry(server.url());
    let err = client
        .fetch_metadata(&PackageName::new("serde").unwrap())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no valid versions"), "{err}");
}

// ---------------------------------------------------------- pypi (JSON API)

fn pypi_json(info_version: &str) -> String {
    format!(
        r#"{{
            "info": {{
                "version": "{info_version}",
                "summary": "demo summary",
                "home_page": "https://example.dev",
                "project_url": "https://example.dev/repo"
            }},
            "releases": {{
                "1.0.0": [{{"url": "__MOCK__/files/pkg-1.0.0-py3-none-any.whl", "packagetype": "bdist_wheel"}}],
                "0.9.0": [{{"url": "__MOCK__/files/pkg-0.9.0.tar.gz", "packagetype": "sdist"}}]
            }}
        }}"#
    )
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn pypi_fetch_metadata_parses_json_api() {
    let mut server = mockito::Server::new_async().await;
    let body = pypi_json("1.0.0").replace("__MOCK__", &server.url());
    server
        .mock("GET", "/pypi/demo-pkg/json")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;

    let client = PyPiClient::with_registry(server.url());
    let metadata = client
        .fetch_metadata(&PackageName::new("demo-pkg").unwrap())
        .await
        .unwrap();

    assert_eq!(metadata.name.as_str(), "demo-pkg");
    assert_eq!(metadata.latest, Version::parse("1.0.0").unwrap());
    assert_eq!(metadata.description.as_deref(), Some("demo summary"));
    // releases sort tăng dần
    assert_eq!(
        metadata.versions,
        vec![
            Version::parse("0.9.0").unwrap(),
            Version::parse("1.0.0").unwrap()
        ]
    );
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn pypi_download_prefers_wheel_over_sdist() {
    let mut server = mockito::Server::new_async().await;
    let body = pypi_json("1.0.0").replace("__MOCK__", &server.url());

    let meta_mock = server
        .mock("GET", "/pypi/demo-pkg/json")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;
    // wheel bytes đặc trưng để phân biệt với sdist
    let wheel_mock = server
        .mock("GET", "/files/pkg-1.0.0-py3-none-any.whl")
        .with_status(200)
        .with_body(b"WHEELBYTES")
        .create_async()
        .await;

    let client = PyPiClient::with_registry(server.url());
    let package_id = PackageId::new(
        PackageName::new("demo-pkg").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );
    let data = client.download_package(&package_id).await.unwrap();

    meta_mock.assert_async().await;
    wheel_mock.assert_async().await;
    assert_eq!(
        data,
        b"WHEELBYTES".to_vec(),
        "phải ưu tiên wheel, không phải sdist"
    );
}

#[tokio::test]
#[ignore = "network: mockito binds localhost socket"]
async fn pypi_download_missing_version_errors() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/pypi/demo-pkg/json")
        .with_status(200)
        .with_body(pypi_json("1.0.0").replace("__MOCK__", &server.url()))
        .create_async()
        .await;

    let client = PyPiClient::with_registry(server.url());
    let package_id = PackageId::new(
        PackageName::new("demo-pkg").unwrap(),
        Version::parse("7.7.7").unwrap(),
    );
    let err = client.download_package(&package_id).await.unwrap_err();
    assert!(err.to_string().contains("7.7.7"), "{err}");
}
