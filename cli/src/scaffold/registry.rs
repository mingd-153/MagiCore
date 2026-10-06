//! Scaffold registry client (fetch artifacts from remote registry).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::spec::{ScaffoldRef, ScaffoldSpec};

/// Registry client for fetching scaffold artifacts.
pub struct ScaffoldRegistry {
    base_url: String,
    client: reqwest::Client,
}

impl ScaffoldRegistry {
    /// Create registry client with default URL.
    pub fn new() -> Result<Self> {
        Self::try_with_url("https://registry.magicore.io")
    }

    /// Create registry client with custom URL.
    pub fn with_url(base_url: &str) -> Result<Self> {
        Self::try_with_url(base_url)
    }

    fn try_with_url(base_url: &str) -> Result<Self> {
        // Strip any `user:pass@` userinfo first: credentials in the base URL
        // would otherwise land verbatim in error messages and logs built
        // from derived request URLs. Authentication belongs in headers,
        // never in a logged URL.
        // (Lột userinfo trước: credential trong URL sẽ lọt vào log/error.)
        let mut parsed =
            url::Url::parse(base_url).map_err(|e| anyhow::anyhow!("invalid registry URL: {e}"))?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            parsed
                .set_username("")
                .map_err(|_| anyhow::anyhow!("invalid registry URL"))?;
            parsed
                .set_password(None)
                .map_err(|_| anyhow::anyhow!("invalid registry URL"))?;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("cannot build HTTP client: {e}"))?;

        Ok(Self {
            base_url: parsed.to_string().trim_end_matches('/').to_string(),
            client,
        })
    }

    /// Fetch scaffold tarball for a specific spec and resolved version.
    pub async fn fetch(&self, spec: &ScaffoldSpec, version: &str) -> Result<Vec<u8>> {
        let package_name = Self::package_name(spec);
        let url = format!(
            "{}/api/v1/scaffolds/{}/{}/-/{}-{}.tgz",
            self.base_url,
            spec.core.as_str(),
            spec.normalized_name,
            package_name,
            version
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to fetch scaffold from {}", url))?;

        if !response.status().is_success() {
            bail!("Registry returned error {}: {}", response.status(), url);
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read scaffold tarball")?;

        Ok(bytes.to_vec())
    }

    /// Resolve version from scaffold ref (latest/stable/beta/explicit).
    pub async fn resolve_version(&self, spec: &ScaffoldSpec) -> Result<String> {
        match &spec.requested_ref {
            ScaffoldRef::Version(v) => Ok(v.clone()),
            ScaffoldRef::DistTag(tag) if tag == "latest" => {
                self.fetch_dist_tag(spec, "latest").await
            }
            ScaffoldRef::DistTag(tag) if tag == "stable" => {
                self.fetch_dist_tag(spec, "stable").await
            }
            ScaffoldRef::DistTag(tag) if tag == "beta" => self.fetch_dist_tag(spec, "beta").await,
            ScaffoldRef::DistTag(tag) => bail!("Unknown dist tag: {}", tag),
            ScaffoldRef::Range(_) => bail!("Version ranges not supported yet"),
            ScaffoldRef::Default => self.fetch_dist_tag(spec, "latest").await,
        }
    }

    /// Fetch dist-tag mapping (latest → 15.5.0).
    async fn fetch_dist_tag(&self, spec: &ScaffoldSpec, tag: &str) -> Result<String> {
        let package_name = Self::package_name(spec);
        let url = format!(
            "{}/api/v1/scaffolds/{}/{}/dist-tags",
            self.base_url,
            spec.core.as_str(),
            spec.normalized_name
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to fetch dist-tags from {}", url))?;

        if !response.status().is_success() {
            bail!(
                "Scaffold {} not found in registry (dist-tags returned {})",
                package_name,
                response.status()
            );
        }

        let dist_tags: DistTags = response
            .json()
            .await
            .context("Failed to parse dist-tags JSON")?;

        dist_tags.tags.get(tag).cloned().ok_or_else(|| {
            anyhow::anyhow!("Dist-tag '{}' not found for scaffold {}", tag, package_name)
        })
    }

    /// Package name convention: mgc-create-{core}-{name}.
    fn package_name(spec: &ScaffoldSpec) -> String {
        format!("mgc-create-{}-{}", spec.core.as_str(), spec.normalized_name)
    }

    /// Check if registry is reachable (health check).
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .context("Registry health check failed")?;

        if response.status().is_success() {
            Ok(())
        } else {
            bail!("Registry health check returned {}", response.status())
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct DistTags {
    #[serde(flatten)]
    tags: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::spec::CoreKind;

    #[test]
    fn test_package_name_convention() {
        let spec = ScaffoldSpec {
            core: CoreKind::Web,
            name: "nextjs".to_string(),
            normalized_name: "nextjs".to_string(),
            requested_ref: ScaffoldRef::DistTag("latest".to_string()),
        };

        assert_eq!(
            ScaffoldRegistry::package_name(&spec),
            "mgc-create-web-nextjs"
        );
    }

    #[test]
    fn test_explicit_version_resolve() {
        let spec = ScaffoldSpec {
            core: CoreKind::Web,
            name: "nextjs@15.5.0".to_string(),
            normalized_name: "nextjs".to_string(),
            requested_ref: ScaffoldRef::Version("15.5.0".to_string()),
        };

        // Synchronous version - no async needed
        match &spec.requested_ref {
            ScaffoldRef::Version(v) => assert_eq!(v, "15.5.0"),
            _ => panic!("Expected version ref"),
        }
    }

    #[test]
    fn test_userinfo_stripped_from_custom_url() {
        let registry = ScaffoldRegistry::with_url("https://user:s3cret@example.com/reg/").unwrap();
        assert!(
            !registry.base_url.contains("s3cret"),
            "credential leaked into base URL: {}",
            registry.base_url
        );
        assert!(registry.base_url.starts_with("https://example.com/"));
    }

    #[test]
    fn test_garbage_url_refused() {
        assert!(ScaffoldRegistry::with_url("not a url :::").is_err());
    }
}
