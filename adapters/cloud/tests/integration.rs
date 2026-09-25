#![allow(clippy::unwrap_used)]
//! Integration tests for mgc-cloud-adapter — sát với src/lib.rs
//! Kiểm thử: detect_type (4 types × 2 paths), adapter_for, cloud_type helper,
//! CDK/Pulumi sử dụng WebAdapter delegate, Terraform fail-closed add/remove.

use mgc_cloud_adapter::{CloudType, adapter_for, detect_type, generate_sbom};
use mgc_types::PackageName;
use mgc_types::adapter::{AddOptions, PackageAdapter};
use mgc_types::capabilities::{
    AuditProvider, ContentStoreProvider, CoreIdent, DependencyResolver, ProjectDetector,
};
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-cloud-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_type — file-based markers ───────────────────────────────────────

#[test]
fn detect_cloudflare_via_wrangler_toml() {
    let dir = tmp("cf");
    std::fs::write(dir.join("wrangler.toml"), "name = \"worker\"\n").unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Cloudflare));
}

#[test]
fn detect_pulumi_via_pulumi_yaml() {
    let dir = tmp("pulumi");
    std::fs::write(dir.join("Pulumi.yaml"), "name: infra\nruntime: nodejs\n").unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Pulumi));
}

#[test]
fn detect_terraform_via_dot_tf_file() {
    let dir = tmp("tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Terraform));
}

#[test]
fn detect_terraform_via_any_tf_extension() {
    let dir = tmp("tf2");
    std::fs::write(
        dir.join("networking.tf"),
        "resource \"aws_vpc\" \"main\" {}\n",
    )
    .unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Terraform));
}

#[test]
fn detect_cdk_via_package_json_aws_cdk_lib() {
    let dir = tmp("cdk");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"infra","dependencies":{"aws-cdk-lib":"^2.0.0"}}"#,
    )
    .unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Cdk));
}

#[test]
fn detect_cdk_via_package_json_cdk_key() {
    let dir = tmp("cdk2");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"infra","dependencies":{"cdk":"^2.0.0"}}"#,
    )
    .unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Cdk));
}

// ── detect_type — mgc.toml override (priority) ──────────────────────────────

#[test]
fn mgc_toml_overrides_file_marker_for_cloud_type() {
    let dir = tmp("override");
    // Pulumi.yaml → Pulumi, mgc.toml → Terraform (mgc.toml wins)
    std::fs::write(dir.join("Pulumi.yaml"), "name: x\nruntime: nodejs\n").unwrap();
    std::fs::write(dir.join("mgc.toml"), "[cloud]\ntype = \"terraform\"\n").unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Terraform));
}

#[test]
fn detect_cloudflare_via_mgc_toml() {
    let dir = tmp("mgc-cf");
    std::fs::write(dir.join("mgc.toml"), "[cloud]\ntype = \"cloudflare\"\n").unwrap();
    assert_eq!(detect_type(&dir), Some(CloudType::Cloudflare));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_type(&dir).is_none());
}

#[test]
fn detect_returns_none_for_unknown_type_in_mgc_toml() {
    let dir = tmp("unknown");
    std::fs::write(dir.join("mgc.toml"), "[cloud]\ntype = \"ansible\"\n").unwrap();
    assert!(detect_type(&dir).is_none());
}

// ── package.json without cdk deps → not CDK ────────────────────────────────

#[test]
fn package_json_without_cdk_deps_not_detected_as_cdk() {
    let dir = tmp("non-cdk");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"web","dependencies":{"react":"^18.0.0"}}"#,
    )
    .unwrap();
    assert!(detect_type(&dir).is_none());
}

// ── CloudType helpers ──────────────────────────────────────────────────────

#[test]
fn cloud_type_as_str_values() {
    assert_eq!(CloudType::Cdk.as_str(), "cdk");
    assert_eq!(CloudType::Pulumi.as_str(), "pulumi");
    assert_eq!(CloudType::Terraform.as_str(), "terraform");
    assert_eq!(CloudType::Cloudflare.as_str(), "cloudflare");
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_for_terraform() {
    let dir = tmp("af-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    assert!(adapter_for(&dir).unwrap().is_some());
}

#[test]
fn adapter_for_returns_none_without_any_marker() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).unwrap().is_none());
}

#[test]
fn adapter_cloud_type_method_returns_correct_str() {
    let dir = tmp("ct-str");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    assert_eq!(a.cloud_type(), "terraform");
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let dir = tmp("name-eco");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    assert_eq!(a.name(), "cloud");
    assert_eq!(format!("{:?}", a.ecosystem()), "Cloud");
}

#[test]
fn can_handle_returns_true_for_known_marker() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    assert!(a.can_handle(&dir));
}

#[tokio::test]
async fn parse_manifest_uses_dir_name_for_terraform() {
    let dir = tmp("my-infra-project");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert!(manifest.name.contains("my-infra-project"));
}

#[tokio::test]
async fn terraform_package_adapter_install_fails_closed_without_spawning_terraform() {
    let dir = tmp("install-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    let _manifest = a.parse_manifest(&dir).await.unwrap();
    let graph = mgc_types::ResolvedGraph::default();
    let result = a.install(&graph, &dir, Default::default()).await;
    let error = result.expect_err("PackageAdapter must not spawn Terraform");
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "install",
            ..
        }
    ));
}

#[tokio::test]
async fn add_fails_closed_for_terraform() {
    let dir = tmp("add-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    let name = PackageName::new("aws_s3_bucket").unwrap();
    let err = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("direct adapter mutation is disabled"),
        "adapter mutation must fail closed and require the CLI gateway: {msg}"
    );
}

#[tokio::test]
async fn remove_fails_closed_for_terraform() {
    let dir = tmp("remove-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    let name = PackageName::new("aws_s3_bucket").unwrap();
    assert!(a.remove(&dir, &name).await.is_err());
}

#[tokio::test]
async fn update_fails_closed_for_terraform() {
    let dir = tmp("update-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    assert!(a.update(&dir, None).await.is_err());
}

#[tokio::test]
async fn terraform_list_fails_closed_instead_of_returning_an_empty_native_list() {
    let dir = tmp("list-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let adapter = adapter_for(&dir).unwrap().unwrap();
    let error = adapter.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "list",
            ..
        }
    ));
    assert!(
        !adapter
            .capabilities()
            .contains(&mgc_types::capabilities::Capability::ContentStoreProvider)
    );
    assert!(adapter.probe_content_store().is_err());
}

#[tokio::test]
async fn pulumi_without_package_json_list_fails_closed() {
    let dir = tmp("list-pulumi-no-package");
    std::fs::write(dir.join("Pulumi.yaml"), "name: infra\nruntime: python\n").unwrap();
    let adapter = adapter_for(&dir).unwrap().unwrap();
    let error = adapter.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            core: "cloud",
            capability: "list",
            ..
        }
    ));
}

#[tokio::test]
async fn audit_returns_clean_for_terraform_project() {
    let dir = tmp("audit-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    let a = adapter_for(&dir).unwrap().unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}

#[test]
fn generate_sbom_uses_lockfile_v2_fixture() {
    let mut lockfile = mgc_lockfile::Lockfile::new();
    lockfile.add_package(mgc_lockfile::Package::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        "https://example.com/test.tgz".to_string(),
        "blake3:test123".to_string(),
    ));
    let json = generate_sbom(&lockfile, mgc_sbom::SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("test-pkg"));
    assert!(json.contains("1.0.0"));
}
