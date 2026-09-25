//! pub.dev native engine (Dart/Flutter).
//! Engine native pub.dev (Dart/Flutter).
//!
//! JSON API spec: `{api}/api/packages/{name}` returns the version list with
//! each version's `pubspec` (dependencies map) + `archive_url` +
//! `archive_sha256`. Dart constraints: `any`, `^x.y.z`, bare = exact,
//! `>=x.y.z <z` (space AND), `x.y.*` wildcard. The archive (tar.gz) is
//! verified against `archive_sha256` and extracted to the pub cache layout.
//! Spec JSON API: `{api}/api/packages/{name}` trả danh sách version với
//! `pubspec` (map dependencies) + `archive_url` + `archive_sha256`. Ràng
//! buộc Dart: `any`, `^x.y.z`, trần = exact, `>=x.y.z <z` (space AND),
//! `x.y.*` wildcard. Archive (tar.gz) xác minh theo `archive_sha256` và giải
//! nén vào layout pub cache.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use mgc_types::{MgError, MgResult, Version};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_API_URL: &str = "https://pub.dev";

/// Native pub.dev engine.
/// Engine native pub.dev.
#[derive(Debug, Clone)]
pub struct PubProtocol {
    api_url: String,
    client: mgc_http::HttpClient,
}

impl PubProtocol {
    /// Build with an explicit API URL.
    /// Dựng với API URL tường minh.
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
        }
    }

    /// Build from environment: `MGC_PUB_INDEX_URL`.
    /// Dựng từ môi trường: `MGC_PUB_INDEX_URL`.
    pub fn from_env() -> Self {
        let api_url = std::env::var("MGC_PUB_INDEX_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_API_URL.to_string());
        Self::new(&api_url)
    }

    async fn get_text(&self, url: &str) -> MgResult<String> {
        let resp = self
            .client
            .get(url)
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        if !status.is_success() {
            return Err(MgError::Network(format!(
                "GET {url} returned {status}: {body}"
            )));
        }
        Ok(body)
    }

    /// Materialize a package into the pub cache layout:
    /// `{pub_cache}/hosted/pub.dev/{name}-{version}/`. Usable by
    /// `dart pub get --offline`.
    /// Materialize package vào layout pub cache:
    /// `{pub_cache}/hosted/pub.dev/{name}-{version}/`. Dùng được bởi
    /// `dart pub get --offline`.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        pub_cache: &Path,
    ) -> MgResult<PathBuf> {
        let dest = pub_cache
            .join("hosted")
            .join("pub.dev")
            .join(format!("{}-{}", entry.name, entry.version));
        super::archive::extract_tar_gz(bytes, &dest)?;
        Ok(dest)
    }
}

impl Default for PubProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl RegistryProtocol for PubProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let url = format!("{}/api/packages/{name}", self.api_url);
        let body = self.get_text(&url).await?;
        let doc: PubPackage = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse pub.dev json failed: {e}")))?;

        let mut best: Option<(Version, &PubVersion)> = None;
        for pv in &doc.versions {
            let Ok(version) = Version::parse(&pv.version) else {
                continue;
            };
            if !dart_matches(range, &version) {
                continue;
            }
            if best.as_ref().is_none_or(|(v, _)| version > *v) {
                best = Some((version, pv));
            }
        }

        let (version, pv) =
            best.ok_or_else(|| MgError::Other(format!("no version of {name} matches '{range}'")))?;
        if pv.archive_url.is_empty() {
            return Err(MgError::Other(format!(
                "package {name} {version} has no archive_url"
            )));
        }

        let mut markers = Vec::new();
        if !pv.pubspec.environment.sdk.is_empty() {
            markers.push(format!("sdk:{}", pv.pubspec.environment.sdk));
        }
        let deps = parse_pub_dependencies(&pv.pubspec.dependencies, &mut markers)?;

        Ok(ResolvedEntry {
            name: name.to_string(),
            version: version.to_string(),
            deps,
            artifact_url: pv.archive_url.clone(),
            sha256: pv.archive_sha256.clone().unwrap_or_default(),
            extra_markers: markers,
        })
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        let resp = self
            .client
            .get(&entry.artifact_url)
            .await
            .map_err(|e| MgError::Network(format!("GET {} failed: {e}", entry.artifact_url)))?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| {
            MgError::Network(format!("read {} body failed: {e}", entry.artifact_url))
        })?;
        if !status.is_success() {
            return Err(MgError::Network(format!(
                "GET {} returned {status}",
                entry.artifact_url
            )));
        }
        Ok(bytes.to_vec())
    }
}

fn parse_pub_dependencies(
    dependencies: &HashMap<String, serde_json::Value>,
    markers: &mut Vec<String>,
) -> MgResult<Vec<(String, String)>> {
    let mut parsed = Vec::with_capacity(dependencies.len());
    for (name, value) in dependencies {
        // SDK-owned packages are part of the selected SDK, not pub.dev.
        if matches!(
            name.as_str(),
            "flutter" | "flutter_test" | "integration_test" | "dart"
        ) {
            markers.push(format!("sdk-owned:{name}"));
            continue;
        }
        let Some(range) = value.as_str() else {
            let source = value
                .as_object()
                .and_then(|object| {
                    ["path", "git", "hosted"]
                        .into_iter()
                        .find(|key| object.contains_key(*key))
                })
                .unwrap_or("non-registry");
            return Err(MgError::Unsupported {
                core: "app",
                capability: "pub.dev dependency source",
                guidance: format!(
                    "dependency '{name}' uses {source}; the native pub.dev resolver cannot claim or substitute it"
                ),
            });
        };
        if range.trim().is_empty() {
            return Err(MgError::Unsupported {
                core: "app",
                capability: "pub.dev dependency constraint",
                guidance: format!("dependency '{name}' has an empty version constraint"),
            });
        }
        parsed.push((name.clone(), range.to_string()));
    }
    parsed.sort();
    Ok(parsed)
}

/// Dart constraint matcher: `any`, `^x.y.z`, bare = exact, space-separated
/// AND (`>=x.y.z <z`), `x.y.*` wildcard.
/// Matcher ràng buộc Dart: `any`, `^x.y.z`, trần = exact, AND cách nhau bởi
/// dấu cách (`>=x.y.z <z`), `x.y.*` wildcard.
fn dart_matches(range: &str, version: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "any" {
        return true;
    }
    range
        .split_whitespace()
        .all(|part| dart_part(part, version))
}

fn dart_part(part: &str, version: &Version) -> bool {
    if part == "any" || part == "*" {
        return true;
    }
    if let Some(p) = part.strip_prefix('^') {
        return dart_caret(p, version);
    }
    if let Some(p) = part.strip_prefix(">=") {
        return dart_ge(p, version);
    }
    if let Some(p) = part.strip_prefix("<=") {
        return dart_le(p, version);
    }
    if let Some(p) = part.strip_prefix('>') {
        return dart_gt(p, version);
    }
    if let Some(p) = part.strip_prefix('<') {
        return dart_lt(p, version);
    }
    if part.contains('*') {
        return dart_wildcard(part, version);
    }
    // bare version → exact.
    Version::parse(part)
        .map(|target| version == &target)
        .unwrap_or(false)
}

fn dart_caret(raw: &str, version: &Version) -> bool {
    let Some(target) = Version::parse(raw.trim()).ok() else {
        return false;
    };
    version >= &target && version.major == target.major
}

fn dart_wildcard(part: &str, version: &Version) -> bool {
    let base = part.trim_end_matches('*').trim_end_matches('.');
    let parts: Vec<&str> = base.split('.').collect();
    if parts.is_empty() {
        return true;
    }
    let major: u64 = parts[0].parse().unwrap_or(u64::MAX);
    if version.major != major {
        return false;
    }
    if let Some(minor) = parts.get(1) {
        let minor: u64 = minor.parse().unwrap_or(u64::MAX);
        return version.minor == minor;
    }
    true
}

fn dart_ge(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version >= &t)
        .unwrap_or(false)
}
fn dart_le(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version <= &t)
        .unwrap_or(false)
}
fn dart_gt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version > &t)
        .unwrap_or(false)
}
fn dart_lt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version < &t)
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct PubPackage {
    #[serde(default)]
    versions: Vec<PubVersion>,
}

#[derive(Debug, Deserialize)]
struct PubVersion {
    version: String,
    #[serde(default)]
    pubspec: PubSpec,
    #[serde(default)]
    archive_url: String,
    #[serde(default)]
    archive_sha256: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PubSpec {
    #[serde(default)]
    environment: PubEnvironment,
    #[serde(default)]
    dependencies: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct PubEnvironment {
    #[serde(default)]
    sdk: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgc_types::Version;

    #[test]
    fn dart_caret_and_exact() {
        assert!(dart_matches("^1.2.0", &Version::parse("1.9.0").unwrap()));
        assert!(!dart_matches("^1.2.0", &Version::parse("2.0.0").unwrap()));
        assert!(dart_matches("1.2.0", &Version::parse("1.2.0").unwrap()));
        assert!(!dart_matches("1.2.0", &Version::parse("1.2.1").unwrap()));
    }

    #[test]
    fn dart_any_and_wildcard() {
        assert!(dart_matches("any", &Version::parse("9.9.9").unwrap()));
        assert!(dart_matches("1.2.*", &Version::parse("1.2.7").unwrap()));
        assert!(!dart_matches("1.2.*", &Version::parse("1.3.0").unwrap()));
        assert!(dart_matches(
            ">=1.0.0 <2.0.0",
            &Version::parse("1.5.0").unwrap()
        ));
    }

    #[test]
    fn pub_metadata_rejects_non_registry_dependencies_instead_of_dropping_them() {
        for source in ["path", "git", "hosted"] {
            let value = match source {
                "path" => serde_json::json!({"path": "../local"}),
                "git" => serde_json::json!({"git": {"url": "https://example.invalid/repo.git"}}),
                _ => serde_json::json!({"hosted": "https://packages.invalid", "version": "^1.0.0"}),
            };
            let dependencies = HashMap::from([("local_pkg".to_string(), value)]);
            let error = parse_pub_dependencies(&dependencies, &mut vec![]).unwrap_err();
            assert!(matches!(error, MgError::Unsupported { .. }));
            assert!(error.to_string().contains(source));
        }
    }

    #[test]
    fn pub_metadata_excludes_sdk_packages_but_keeps_registry_edges() {
        let dependencies = HashMap::from([
            ("flutter_test".to_string(), serde_json::json!("any")),
            ("http".to_string(), serde_json::json!("^1.0.0")),
        ]);
        let mut markers = vec![];
        let parsed = parse_pub_dependencies(&dependencies, &mut markers).unwrap();
        assert_eq!(parsed, vec![("http".to_string(), "^1.0.0".to_string())]);
        assert_eq!(markers, vec!["sdk-owned:flutter_test"]);
    }
}
