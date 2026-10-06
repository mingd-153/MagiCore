//! Registry trust binding contract tests.
//! Kiểm thử hợp đồng binding trusted của registry.
#![allow(clippy::unwrap_used)]

use super::effective_trusted_binding_package;

#[test]
fn registry_trust_uses_core_namespace_only_for_oci() {
    for core in [
        "web", "ai", "app", "lib", "game", "iot", "cloud", "cicd", "hardware",
    ] {
        assert_eq!(
            effective_trusted_binding_package("oci", Some(core), "models/weights").unwrap(),
            format!("{core}/models/weights")
        );
    }
    assert_eq!(
        effective_trusted_binding_package("oci", Some("clo"), "models/weights").unwrap(),
        "cloud/models/weights"
    );
    assert_eq!(
        effective_trusted_binding_package("oci", None, "acme/image").unwrap(),
        "acme/image"
    );
    assert_eq!(
        effective_trusted_binding_package("npm", Some("web"), "@acme/widgets").unwrap(),
        "@acme/widgets"
    );
    assert!(effective_trusted_binding_package("oci", Some("unknown"), "models/weights").is_err());
}
