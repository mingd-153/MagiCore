//! Git Registry Client

use crate::registry::RegistryError;
use std::path::PathBuf;
use tokio::process::Command;

pub struct GitRegistry {
    cache_dir: PathBuf,
}

impl GitRegistry {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    pub async fn fetch(&self, url: &str, rev: Option<&str>) -> Result<PathBuf, RegistryError> {
        let repo_name = url
            .split('/')
            .next_back()
            .unwrap_or("repo")
            .replace(".git", "");
        let repo_path = self.cache_dir.join(&repo_name);

        if repo_path.exists() {
            // Update existing
            Command::new("git")
                .args(["fetch", "origin"])
                .current_dir(&repo_path)
                .output()
                .await?;
        } else {
            // Clone
            Command::new("git")
                .args(["clone", "--depth", "1", url, &repo_name])
                .current_dir(&self.cache_dir)
                .output()
                .await?;
        }

        if let Some(rev) = rev {
            Command::new("git")
                .args(["checkout", rev])
                .current_dir(&repo_path)
                .output()
                .await?;
        }

        Ok(repo_path)
    }
}
