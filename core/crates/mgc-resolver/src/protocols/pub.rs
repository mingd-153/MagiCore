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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

const DEFAULT_API_URL: &str = "https://pub.dev";
const MAX_PUB_GRAPH_SOLVE_ROUNDS: usize = 128;

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

    async fn package_doc(&self, name: &str) -> MgResult<PubPackage> {
        validate_pub_package_name(name)?;
        let url = format!("{}/api/packages/{name}", self.api_url);
        let body = self.get_text(&url).await?;
        serde_json::from_str(&body)
            .map_err(|error| MgError::Other(format!("parse pub.dev json failed: {error}")))
    }

    fn select_entry(
        name: &str,
        doc: &PubPackage,
        constraints: &[String],
    ) -> MgResult<ResolvedEntry> {
        let mut best: Option<(Version, &PubVersion)> = None;
        for candidate in &doc.versions {
            let Ok(version) = Version::parse(&candidate.version) else {
                continue;
            };
            if !constraints
                .iter()
                .all(|constraint| dart_matches(constraint, &version))
            {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(selected, _)| version > *selected)
            {
                best = Some((version, candidate));
            }
        }

        let (version, candidate) = best.ok_or_else(|| {
            MgError::DependencyConflict(format!(
                "no pub.dev version of '{name}' satisfies all constraints: {}",
                constraints.join(" AND ")
            ))
        })?;
        if candidate.archive_url.is_empty() {
            return Err(MgError::Other(format!(
                "package {name} {version} has no archive_url"
            )));
        }

        let mut markers = Vec::new();
        let pubspec = candidate.pubspec.as_ref().ok_or_else(|| {
            MgError::Other(format!(
                "pub.dev version {name} {version} is missing pubspec metadata"
            ))
        })?;
        if let Some(environment) = &pubspec.environment
            && !environment.sdk.is_empty()
        {
            markers.push(format!("sdk:{}", environment.sdk));
        }
        let deps = match &pubspec.dependencies {
            // Pubspecs without a dependency section have no registry edges.
            // A null field in pub.dev's JSON representation has the same
            // meaning as the omitted section; a null whole-pubspec is rejected
            // above because its graph cannot be proven.
            None => Vec::new(),
            Some(dependencies) => parse_pub_dependencies(dependencies, &mut markers)?,
        };
        Ok(ResolvedEntry {
            name: name.to_string(),
            version: version.to_string(),
            deps,
            artifact_url: candidate.archive_url.clone(),
            sha256: candidate.archive_sha256.clone().unwrap_or_default(),
            extra_markers: markers,
        })
    }

    /// Resolve all project roots together. Each pass derives constraints from
    /// the previous pass's selected package versions, then selects the highest
    /// version satisfying every currently known incoming edge. Rebuilding the
    /// constraint map each pass removes edges from superseded versions. Cycles
    /// or a graph that does not converge fail closed; this solver does not
    /// claim PubGrub-style backtracking for cases requiring a lower parent.
    /// (Resolve chung mọi root. Mỗi lượt gom constraint từ graph đã chọn ở
    /// lượt trước, chọn bản cao nhất thỏa tất cả cạnh; graph dựng lại để bỏ
    /// cạnh của version cũ. Chu trình/không hội tụ fail-closed; chưa claim
    /// backtracking kiểu PubGrub khi cần hạ version package cha.)
    async fn resolve_joint_roots(
        &self,
        roots: &[(String, String)],
    ) -> MgResult<Vec<ResolvedEntry>> {
        let mut docs = HashMap::<String, PubPackage>::new();
        let mut selected = HashMap::<String, ResolvedEntry>::new();
        let mut seen_states = HashSet::<Vec<(String, String)>>::new();

        for _ in 0..MAX_PUB_GRAPH_SOLVE_ROUNDS {
            let mut constraints = BTreeMap::<String, BTreeSet<String>>::new();
            for (name, range) in roots {
                constraints
                    .entry(name.clone())
                    .or_default()
                    .insert(range.clone());
            }
            for entry in selected.values() {
                for (dependency, range) in &entry.deps {
                    constraints
                        .entry(dependency.clone())
                        .or_default()
                        .insert(range.clone());
                }
            }

            let mut next = HashMap::<String, ResolvedEntry>::new();
            for (name, ranges) in constraints {
                if !docs.contains_key(&name) {
                    docs.insert(name.clone(), self.package_doc(&name).await?);
                }
                let constraints = ranges.into_iter().collect::<Vec<_>>();
                let doc = docs.get(&name).ok_or_else(|| {
                    MgError::Integrity(format!("missing cached pub.dev metadata for '{name}'"))
                })?;
                let entry = Self::select_entry(&name, doc, &constraints)?;
                next.insert(name, entry);
            }

            let signature = selection_signature(&next);
            let previous_signature = selection_signature(&selected);
            if signature == previous_signature {
                let ordered = next
                    .into_iter()
                    .collect::<BTreeMap<_, _>>()
                    .into_values()
                    .collect();
                return Ok(ordered);
            }
            if !seen_states.insert(signature) {
                return Err(MgError::DependencyConflict(
                    "pub.dev dependency selection oscillates across graph passes; refusing to emit an unstable lock graph".to_string(),
                ));
            }
            selected = next;
        }

        Err(MgError::DependencyConflict(format!(
            "pub.dev dependency graph did not converge within {MAX_PUB_GRAPH_SOLVE_ROUNDS} passes"
        )))
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
        let doc = self.package_doc(name).await?;
        Self::select_entry(name, &doc, &[range.to_string()])
    }

    async fn resolve_graph_roots(
        &self,
        roots: &[(String, String)],
    ) -> MgResult<Vec<ResolvedEntry>> {
        self.resolve_joint_roots(roots).await
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

fn selection_signature(selected: &HashMap<String, ResolvedEntry>) -> Vec<(String, String)> {
    let mut signature = selected
        .iter()
        .map(|(name, entry)| (name.clone(), entry.version.clone()))
        .collect::<Vec<_>>();
    signature.sort();
    signature
}

fn parse_pub_dependencies(
    dependencies: &HashMap<String, serde_json::Value>,
    markers: &mut Vec<String>,
) -> MgResult<Vec<(String, String)>> {
    let mut parsed = Vec::with_capacity(dependencies.len());
    for (name, value) in dependencies {
        validate_pub_package_name(name)?;
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

fn validate_pub_package_name(name: &str) -> MgResult<()> {
    let mut bytes = name.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(MgError::InvalidPackageName(name.to_string()));
    }
    Ok(())
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
    pubspec: Option<PubSpec>,
    #[serde(default)]
    archive_url: String,
    #[serde(default)]
    archive_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PubSpec {
    #[serde(default)]
    environment: Option<PubEnvironment>,
    #[serde(default)]
    dependencies: Option<HashMap<String, serde_json::Value>>,
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

    #[test]
    fn pub_package_names_cannot_escape_registry_path_segments() {
        for name in ["../escape", "@scope/pkg", "UpperCase", "foo/bar"] {
            assert!(validate_pub_package_name(name).is_err(), "accepted {name}");
        }
        assert!(validate_pub_package_name("valid_pkg2").is_ok());
    }
}
