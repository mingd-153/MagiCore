#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Native client tests for app adapter.

use mgc_app_adapter::native::cocoapods_client::CocoaPodsClient;
use mgc_app_adapter::native::maven_client::MavenClient;
use mgc_app_adapter::native::pub_client::PubClient;

#[test]
fn pub_client_constructs() {
    let client = PubClient::new();
    drop(client);
}

#[test]
fn pub_client_default_trait() {
    let client = PubClient::default();
    drop(client);
}

#[test]
fn maven_client_constructs() {
    let client = MavenClient::new();
    drop(client);
}

#[test]
fn maven_client_default_trait() {
    let client = MavenClient::default();
    drop(client);
}

#[test]
fn cocoapods_client_constructs() {
    let client = CocoaPodsClient::new();
    drop(client);
}

#[test]
fn cocoapods_client_default_trait() {
    let client = CocoaPodsClient::default();
    drop(client);
}
