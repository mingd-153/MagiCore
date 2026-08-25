//! CI/CD provider detection for mgc-cicd-adapter.
//! Tách nhận diện provider khỏi adapter chính để dễ maintain.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CicdProvider {
    GithubActions,
    Gitlab,
    CircleCi,
    Cloudflare,
    Aws,
    Gcp,
    Argocd,
}

impl CicdProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            CicdProvider::GithubActions => "github-actions",
            CicdProvider::Gitlab => "gitlab",
            CicdProvider::CircleCi => "circleci",
            CicdProvider::Cloudflare => "cloudflare",
            CicdProvider::Aws => "aws",
            CicdProvider::Gcp => "gcp",
            CicdProvider::Argocd => "argocd",
        }
    }
}

pub fn detect_provider(root: &Path) -> Option<CicdProvider> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("cicd")
                .and_then(|c| c.get("provider"))
                .and_then(|p| p.as_str())
            {
                return match p {
                    "github-actions" => Some(CicdProvider::GithubActions),
                    "gitlab" => Some(CicdProvider::Gitlab),
                    "circleci" => Some(CicdProvider::CircleCi),
                    "cloudflare" => Some(CicdProvider::Cloudflare),
                    "aws" => Some(CicdProvider::Aws),
                    "gcp" => Some(CicdProvider::Gcp),
                    "argocd" => Some(CicdProvider::Argocd),
                    _ => None,
                };
            }
        }
    }
    if root.join("wrangler.toml").exists() {
        return Some(CicdProvider::Cloudflare);
    }
    if root.join("argocd").join("application.yaml").exists() {
        return Some(CicdProvider::Argocd);
    }
    if root.join(".github").join("workflows").exists() {
        return Some(CicdProvider::GithubActions);
    }
    if root.join(".gitlab-ci.yml").exists() {
        return Some(CicdProvider::Gitlab);
    }
    if root.join(".circleci").join("config.yml").exists() {
        return Some(CicdProvider::CircleCi);
    }
    if root.join("main.tf").exists() {
        return Some(CicdProvider::Aws);
    }
    None
}

pub(crate) fn manifest_is_cicd(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "cicd" {
                    return true;
                }
            }
            if v.get("cicd").is_some() {
                return true;
            }
        }
    }
    detect_provider(root).is_some()
}
