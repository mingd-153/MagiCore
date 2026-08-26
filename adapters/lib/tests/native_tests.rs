//! Native registry client tests.

#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::native::cargo_client::CargoClient;
use mgc_lib_adapter::native::pypi_client::PyPiClient;

#[test]
fn cargo_client_new_uses_default_registry() {
    let client = CargoClient::new();
    // Just verify it constructs without panic
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

// Note: The following tests require network access and may be slow/flaky
// They are commented out by default but can be enabled for manual testing

/*
#[tokio::test]
#[ignore]
async fn cargo_client_fetch_serde_metadata() {
    let client = CargoClient::new();
    let name = PackageName::new("serde").unwrap();
    let metadata = client.fetch_metadata(&name).await.unwrap();

    assert_eq!(metadata.name.as_str(), "serde");
    assert!(!metadata.versions.is_empty());
    assert!(metadata.latest > Version::parse("1.0.0").unwrap());
}

#[tokio::test]
#[ignore]
async fn pypi_client_fetch_requests_metadata() {
    let client = PyPiClient::new();
    let name = PackageName::new("requests").unwrap();
    let metadata = client.fetch_metadata(&name).await.unwrap();

    assert_eq!(metadata.name.as_str(), "requests");
    assert!(!metadata.versions.is_empty());
    assert!(metadata.latest > Version::parse("2.0.0").unwrap());
}

#[tokio::test]
#[ignore]
async fn cargo_client_list_versions() {
    let client = CargoClient::new();
    let name = PackageName::new("tokio").unwrap();
    let versions = client.list_versions(&name).await.unwrap();

    assert!(!versions.is_empty());
    assert!(versions.iter().any(|v| v.major >= 1));
}

#[tokio::test]
#[ignore]
async fn pypi_client_list_versions() {
    let client = PyPiClient::new();
    let name = PackageName::new("numpy").unwrap();
    let versions = client.list_versions(&name).await.unwrap();

    assert!(!versions.is_empty());
}

#[tokio::test]
#[ignore]
async fn cargo_client_download_small_crate() {
    let client = CargoClient::new();
    let package_id = PackageId::new(
        PackageName::new("anyhow").unwrap(),
        Version::parse("1.0.0").unwrap(),
    );

    let data = client.download_package(&package_id).await.unwrap();
    assert!(!data.is_empty());
    // .crate files are gzip tarballs
    assert_eq!(&data[0..2], &[0x1f, 0x8b]); // gzip magic bytes
}

#[tokio::test]
#[ignore]
async fn pypi_client_download_wheel() {
    let client = PyPiClient::new();
    let package_id = PackageId::new(
        PackageName::new("certifi").unwrap(),
        Version::parse("2024.8.30").unwrap(),
    );

    let data = client.download_package(&package_id).await.unwrap();
    assert!(!data.is_empty());
}
*/

#[test]
fn cargo_client_default_trait() {
    let client = CargoClient::default();
    drop(client);
}

#[test]
fn pypi_client_default_trait() {
    let client = PyPiClient::default();
    drop(client);
}
