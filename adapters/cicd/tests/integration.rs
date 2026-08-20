#![allow(clippy::unwrap_used)]
//! Integration tests for mg-cicd-adapter — sát với src/lib.rs
//! Kiểm thử: detect_provider (7 providers × 2 paths), adapter_for, PackageAdapter trait.

use mg_cicd_adapter::{adapter_for, detect_provider, CicdAdapter, CicdProvider};
use mg_types::adapter::{AddOptions, PackageAdapter};
use mg_types::PackageName;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mg-cicd-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── detect_provider — file-based markers ───────────────────────────────────

#[test]
fn detect_cloudflare_via_wrangler_toml() {
    let dir = tmp("cf");
    std::fs::write(dir.join("wrangler.toml"), "name = \"worker\"\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Cloudflare));
}

#[test]
fn detect_github_actions_via_workflows_dir() {
    let dir = tmp("gha");
    std::fs::create_dir_all(dir.join(".github").join("workflows")).unwrap();
    std::fs::write(
        dir.join(".github").join("workflows").join("ci.yml"),
        "name: CI\non: [push]\n",
    )
    .unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::GithubActions));
}

#[test]
fn detect_gitlab_via_gitlab_ci_yml() {
    let dir = tmp("gl");
    std::fs::write(dir.join(".gitlab-ci.yml"), "stages: [test]\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Gitlab));
}

#[test]
fn detect_circleci_via_config_yml() {
    let dir = tmp("cc");
    std::fs::create_dir_all(dir.join(".circleci")).unwrap();
    std::fs::write(dir.join(".circleci").join("config.yml"), "version: 2.1\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::CircleCi));
}

#[test]
fn detect_argocd_via_application_yaml() {
    let dir = tmp("argo");
    std::fs::create_dir_all(dir.join("argocd")).unwrap();
    std::fs::write(
        dir.join("argocd").join("application.yaml"),
        "kind: Application\n",
    )
    .unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Argocd));
}

#[test]
fn detect_aws_via_main_tf() {
    let dir = tmp("aws-tf");
    std::fs::write(dir.join("main.tf"), "provider \"aws\" {}\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Aws));
}

// ── detect_provider — mg.toml override (priority) ──────────────────────────

#[test]
fn detect_aws_via_mg_toml_overrides_file_marker() {
    let dir = tmp("mg-aws");
    // wrangler.toml → Cloudflare, mg.toml → Aws (mg.toml thắng)
    std::fs::write(dir.join("wrangler.toml"), "name = \"worker\"\n").unwrap();
    std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"aws\"\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Aws));
}

#[test]
fn detect_gcp_via_mg_toml() {
    let dir = tmp("mg-gcp");
    std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"gcp\"\n").unwrap();
    assert_eq!(detect_provider(&dir), Some(CicdProvider::Gcp));
}

#[test]
fn detect_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(detect_provider(&dir).is_none());
}

#[test]
fn detect_returns_none_for_unknown_mg_toml_provider() {
    let dir = tmp("unknown");
    std::fs::write(dir.join("mg.toml"), "[cicd]\nprovider = \"jenkins\"\n").unwrap();
    // "jenkins" chưa được hỗ trợ → None
    assert!(detect_provider(&dir).is_none());
}

// ── CicdProvider helpers ───────────────────────────────────────────────────

#[test]
fn cicd_provider_as_str_values() {
    assert_eq!(CicdProvider::GithubActions.as_str(), "github-actions");
    assert_eq!(CicdProvider::Gitlab.as_str(), "gitlab");
    assert_eq!(CicdProvider::CircleCi.as_str(), "circleci");
    assert_eq!(CicdProvider::Cloudflare.as_str(), "cloudflare");
    assert_eq!(CicdProvider::Aws.as_str(), "aws");
    assert_eq!(CicdProvider::Gcp.as_str(), "gcp");
    assert_eq!(CicdProvider::Argocd.as_str(), "argocd");
}

// ── adapter_for ────────────────────────────────────────────────────────────

#[test]
fn adapter_for_returns_some_with_wrangler() {
    let dir = tmp("af-cf");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    assert!(adapter_for(&dir).is_some());
}

#[test]
fn adapter_for_returns_none_without_markers() {
    let dir = tmp("af-none");
    assert!(adapter_for(&dir).is_none());
}

#[test]
fn adapter_provider_method_returns_correct_str() {
    let dir = tmp("af-gha");
    std::fs::create_dir_all(dir.join(".github").join("workflows")).unwrap();
    std::fs::write(
        dir.join(".github").join("workflows").join("ci.yml"),
        "name: CI\n",
    )
    .unwrap();
    let a = adapter_for(&dir).unwrap();
    assert_eq!(a.provider(), "github-actions");
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let a = CicdAdapter {
        provider: CicdProvider::GithubActions,
    };
    assert_eq!(a.name(), "cicd");
    assert_eq!(format!("{:?}", a.ecosystem()), "Cicd");
}

#[test]
fn can_handle_returns_true_for_known_marker() {
    let dir = tmp("ch-true");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert!(a.can_handle(&dir));
}

#[tokio::test]
async fn install_returns_ok_delegating_to_provider_tooling() {
    let dir = tmp("install-ok");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    let graph = a.resolve(&manifest).await.unwrap();
    assert!(a.install(&graph, &dir, Default::default()).await.is_ok());
}

#[tokio::test]
async fn add_fails_closed_with_error_mentioning_deploy() {
    let dir = tmp("add-fail");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("my-service").unwrap();
    let err = a
        .add(&dir, &name, None, AddOptions::default())
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("deploy") || msg.contains("cicd"),
        "error must mention deploy: {msg}"
    );
}

#[tokio::test]
async fn remove_fails_closed() {
    let dir = tmp("remove-fail");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let name = PackageName::new("svc").unwrap();
    assert!(a.remove(&dir, &name).await.is_err());
}

#[tokio::test]
async fn update_fails_closed() {
    let dir = tmp("update-fail");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    assert!(a.update(&dir, None).await.is_err());
}

#[tokio::test]
async fn audit_returns_clean_for_empty_cicd_project() {
    let dir = tmp("audit");
    std::fs::write(dir.join("wrangler.toml"), "name = \"w\"\n").unwrap();
    let a = adapter_for(&dir).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}
