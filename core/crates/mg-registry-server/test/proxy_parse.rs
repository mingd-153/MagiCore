#![allow(clippy::unwrap_used)]
//! ITEM 4 proxy test — parse metadata npmjs → Package
use mg_registry_server::model::Package;

#[tokio::test]
async fn proxy_parse_npmjs_metadata() {
    let response = reqwest::Client::new()
        .get("https://registry.npmjs.org/react")
        .send()
        .await;
    let Ok(response) = response else {
        eprintln!("skipping npm metadata proxy parse test: network unavailable");
        return;
    };
    let json = response.text().await.unwrap();
    let pkg: Package = serde_json::from_str(&json).unwrap();
    assert!(pkg.name == "react");
    assert!(!pkg.versions.is_empty());
}
