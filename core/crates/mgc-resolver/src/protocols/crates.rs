//! Crates.io sparse-index native engine (Rust).
//! Engine native sparse-index crates.io (Rust).
//!
//! Sparse index HTTP spec (RFC 2789): index entries are newline-delimited
//! JSON at `{index}/{prefix}/{name}` where the prefix rule is len 1→"1",
//! len 2→"2", len 3→"3/{first}", len≥4→"{first2}/{first3}". The artifact is
//! downloaded from `{download_base}/{name}/{version}/download` (a `.crate`
//! gzipped tarball) and its `sha256` verified against the index `cksum`.
//! Spec sparse-index (RFC 2789): entry index là JSON newline-delimited tại
//! `{index}/{prefix}/{name}` với luật prefix len 1→"1", len 2→"2",
//! len 3→"3/{first}", len≥4→"{first2}/{first3}". Artifact tải từ
//! `{download_base}/{name}/{version}/download` (tarball `.crate` gzip) và
//! `sha256` được xác minh theo `cksum` trong index.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use mgc_types::{MgError, MgResult, Version};
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_INDEX_URL: &str = "https://index.crates.io";
const DEFAULT_DOWNLOAD_BASE: &str = "https://crates.io/api/v1/crates";

/// Native crates.io sparse-index engine.
/// Engine native sparse-index crates.io.
#[derive(Debug, Clone)]
pub struct CratesProtocol {
    index_url: String,
    download_base: String,
    client: mgc_http::HttpClient,
}

impl CratesProtocol {
    /// Build with an explicit index URL (default download base).
    /// Dựng với index URL tường minh (download base mặc định).
    pub fn new(index_url: &str) -> Self {
        Self::with_download_base(index_url, DEFAULT_DOWNLOAD_BASE)
    }

    /// Build with explicit index + download base (testability).
    /// Dựng với index + download base tường minh (cho test).
    pub fn with_download_base(index_url: &str, download_base: &str) -> Self {
        Self {
            index_url: index_url.trim_end_matches('/').to_string(),
            download_base: download_base.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
        }
    }

    /// Build from environment: `MGC_CRATES_INDEX_URL` / `MGC_CRATES_DOWNLOAD_URL`.
    /// Dựng từ môi trường: `MGC_CRATES_INDEX_URL` / `MGC_CRATES_DOWNLOAD_URL`.
    pub fn from_env() -> Self {
        let index_url = std::env::var("MGC_CRATES_INDEX_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_INDEX_URL.to_string());
        let download_base = std::env::var("MGC_CRATES_DOWNLOAD_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_DOWNLOAD_BASE.to_string());
        Self::with_download_base(&index_url, &download_base)
    }

    fn index_url_for(&self, name: &str) -> String {
        let name = name.to_ascii_lowercase();
        let prefix = sparse_prefix(&name);
        format!("{}/{prefix}/{name}", self.index_url)
    }

    fn download_url_for(&self, name: &str, version: &str) -> String {
        format!("{}/{name}/{version}/download", self.download_base)
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

    async fn get_bytes(&self, url: &str) -> MgResult<Vec<u8>> {
        let resp = self
            .client
            .get(url)
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        if !status.is_success() {
            return Err(MgError::Network(format!("GET {url} returned {status}")));
        }
        Ok(bytes.to_vec())
    }

    /// Materialize a crate into the cargo layout: `.crate` in
    /// `{cargo_home}/registry/cache/` and extracted sources in
    /// `{cargo_home}/registry/src/{name}-{version}/`. Returns the `.crate` path.
    /// Materialize crate vào layout cargo: `.crate` trong
    /// `{cargo_home}/registry/cache/` và source đã giải nén trong
    /// `{cargo_home}/registry/src/{name}-{version}/`. Trả path `.crate`.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        cargo_home: &std::path::Path,
    ) -> MgResult<PathBuf> {
        let cache_dir = cargo_home.join("registry").join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let crate_path = cache_dir.join(format!("{}-{}.crate", entry.name, entry.version));
        std::fs::write(&crate_path, bytes)?;

        let src_dir = cargo_home
            .join("registry")
            .join("src")
            .join(format!("{}-{}", entry.name, entry.version));
        super::archive::extract_tar_gz(bytes, &src_dir)?;
        Ok(crate_path)
    }
}

impl Default for CratesProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl RegistryProtocol for CratesProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let index_url = self.index_url_for(name);
        let body = self.get_text(&index_url).await?;

        let mut best: Option<(Version, CrateIndexEntry)> = None;
        let mut best_yanked: Option<(Version, CrateIndexEntry)> = None;

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: CrateIndexEntry = serde_json::from_str(line)
                .map_err(|e| MgError::Other(format!("parse crate index entry failed: {e}")))?;
            if !entry.name.eq_ignore_ascii_case(name) {
                continue;
            }
            let Ok(version) = Version::parse(&entry.vers) else {
                continue;
            };
            if !cargo_matches(range, &version) {
                continue;
            }
            if entry.yanked {
                if best_yanked.as_ref().is_none_or(|(v, _)| version > *v) {
                    best_yanked = Some((version, entry));
                }
                continue;
            }
            if best.as_ref().is_none_or(|(v, _)| version > *v) {
                best = Some((version, entry));
            }
        }

        let chosen = best.or_else(|| {
            // Never select a yanked version; if only yanked candidates
            // match, warn and fail closed (the dependency is skipped by the
            // caller, never silently pinned to a yanked artifact).
            // Không bao giờ chọn version yanked; nếu chỉ còn ứng viên yanked
            // khớp, cảnh báo và fail-closed (caller bỏ dependency, không bao
            // giờ âm thầm ghim vào artifact yanked).
            if best_yanked.is_some() {
                eprintln!(
                    "WARNING: {name}@{range} only matches yanked versions — skipping (fail-closed)"
                );
            }
            None
        });

        let entry = chosen.map(|(_, e)| e).ok_or_else(|| {
            MgError::Other(format!(
                "no non-yanked version of {name} matches range '{range}'"
            ))
        })?;

        let artifact_url = self.download_url_for(name, &entry.vers);
        // The REAL sparse index carries a BARE 64-hex `cksum` (no
        // scheme prefix) — an earlier revision only accepted a
        // `sha256:`-prefixed value and dropped every genuine checksum.
        // (Index thật mang `cksum` hex trần, không prefix.)
        let sha256 = entry
            .cksum
            .as_deref()
            .map(str::trim)
            .filter(|c| c.len() == 64 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
            .or_else(|| {
                entry
                    .cksum
                    .as_deref()
                    .and_then(|c| c.strip_prefix("sha256:"))
                    .filter(|c| c.len() == 64 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
            })
            .map(|c| c.to_ascii_lowercase())
            .unwrap_or_default();

        let mut resolved = ResolvedEntry {
            name: name.to_string(),
            version: entry.vers.clone(),
            deps: Vec::new(),
            artifact_url,
            sha256,
            extra_markers: Vec::new(),
        };

        for dep in &entry.deps {
            let kind = dep.kind.as_deref().unwrap_or("normal");
            // dev deps are excluded from the graph — only normal + build.
            // (dev deps bị loại khỏi graph — chỉ normal + build.)
            if kind == "dev" {
                continue;
            }
            let actual_name = dep.package.clone().unwrap_or_else(|| dep.name.clone());
            // optional deps are dropped from the graph and recorded in extras.
            // (optional dep bị bỏ khỏi graph và ghi vào extras.)
            if dep.optional {
                resolved
                    .extra_markers
                    .push(format!("optional:{actual_name}"));
                continue;
            }
            // target != null: include only when the cfg whitelist matches
            // the host; otherwise record the marker and skip.
            // (target != null: chỉ include khi whitelist cfg khớp host;
            // ngược lại ghi marker và bỏ qua.)
            if let Some(target) = &dep.target
                && !cfg_matches_host(target)
            {
                resolved.extra_markers.push(format!("target:{target}"));
                continue;
            }
            let req = if dep.req.trim().is_empty() {
                "*".to_string()
            } else {
                dep.req.clone()
            };
            resolved.deps.push((actual_name, req));
        }

        Ok(resolved)
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        self.get_bytes(&entry.artifact_url).await
    }
}

/// Sparse-index prefix rule for a crate name (the path component BEFORE the
/// final `/{name}` segment, which `index_url_for` appends).
/// Luật prefix sparse-index cho tên crate (thành phần path TRƯỚC đoạn
/// `/{name}` cuối, do `index_url_for` tự nối thêm).
fn sparse_prefix(name: &str) -> String {
    match name.len() {
        0 => "0".to_string(),
        1 => "1".to_string(),
        2 => "2".to_string(),
        3 => format!("3/{}", &name[..1]),
        _ => format!("{}/{}", &name[..2], &name[2..4]),
    }
}

/// Cargo semver subset matcher: `*`, `^x.y.z`, `~x.y.z`, `=x.y.z`,
/// `>=x`, `<=x`, `>x`, `<x`, comma = AND, bare version = caret.
/// Matcher semver subset Cargo: `*`, `^`, `~`, `=`, `>=`, `<=`, `>`, `<`,
/// phẩy = AND, version trần = caret.
fn cargo_matches(range: &str, version: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    range
        .split(',')
        .map(str::trim)
        .all(|part| cargo_part(part, version))
}

fn cargo_part(part: &str, version: &Version) -> bool {
    if part == "*" {
        return true;
    }
    if let Some(p) = part.strip_prefix('^') {
        return caret(p, version);
    }
    if let Some(p) = part.strip_prefix('~') {
        return tilde(p, version);
    }
    if let Some(p) = part.strip_prefix('=') {
        return exact(p, version);
    }
    if let Some(p) = part.strip_prefix(">=") {
        return ge(p, version);
    }
    if let Some(p) = part.strip_prefix("<=") {
        return le(p, version);
    }
    if let Some(p) = part.strip_prefix('>') {
        return gt(p, version);
    }
    if let Some(p) = part.strip_prefix('<') {
        return lt(p, version);
    }
    // bare version → caret semantics (cargo default).
    // (version trần → ngữ nghĩa caret (mặc định cargo).)
    caret(part, version)
}

fn caret(raw: &str, version: &Version) -> bool {
    let Some(target) = Version::parse(raw.trim()).ok() else {
        return false;
    };
    if version < &target {
        return false;
    }
    // ^1.2.3 → <2.0.0; ^0.2.3 → <0.3.0; ^0.0.3 → <0.0.4.
    if target.major > 0 {
        version.major == target.major
    } else if target.minor > 0 {
        version.major == 0 && version.minor == target.minor
    } else {
        version.major == 0 && version.minor == 0 && version.patch == target.patch
    }
}

fn tilde(raw: &str, version: &Version) -> bool {
    let Some(target) = Version::parse(raw.trim()).ok() else {
        return false;
    };
    version >= &target && version.major == target.major && version.minor == target.minor
}

fn exact(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|target| version == &target)
        .unwrap_or(false)
}

fn ge(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|target| version >= &target)
        .unwrap_or(false)
}

fn le(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|target| version <= &target)
        .unwrap_or(false)
}

fn gt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|target| version > &target)
        .unwrap_or(false)
}

fn lt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|target| version < &target)
        .unwrap_or(false)
}

/// Whether a crate dep `target` cfg expression matches the current host,
/// restricted to the common whitelist (unix, target_os, target_arch).
/// Xem biểu thức cfg `target` của dep crate có khớp host hiện tại không,
/// giới hạn trong whitelist phổ biến (unix, target_os, target_arch).
fn cfg_matches_host(target: &str) -> bool {
    let trimmed = target.trim();
    let expr = trimmed
        .strip_prefix("cfg(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed)
        .trim();

    if expr == "unix" {
        return std::env::consts::FAMILY == "unix";
    }
    if let Some(v) = cfg_value(expr, "target_os") {
        return v == std::env::consts::OS;
    }
    if let Some(v) = cfg_value(expr, "target_arch") {
        return v == std::env::consts::ARCH;
    }
    // Outside the whitelist → not a match (dep skipped, marker recorded).
    // (Ngoài whitelist → không khớp (bỏ dep, ghi marker).)
    false
}

fn cfg_value<'a>(expr: &'a str, key: &str) -> Option<&'a str> {
    let rest = expr.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.trim_start_matches('"');
    rest.split('"').next()
}

/// One crate version entry from the sparse index NDJSON.
/// Một entry version crate từ NDJSON sparse-index.
#[derive(Debug, Deserialize)]
struct CrateIndexEntry {
    name: String,
    vers: String,
    #[serde(default)]
    deps: Vec<CrateDep>,
    #[serde(default)]
    cksum: Option<String>,
    #[serde(default)]
    yanked: bool,
}

/// One dependency edge in a crate version entry.
/// Một cạnh dependency trong entry version crate.
#[derive(Debug, Deserialize)]
struct CrateDep {
    name: String,
    #[serde(default)]
    req: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    package: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgc_types::Version;

    #[test]
    fn sparse_prefix_rules() {
        assert_eq!(sparse_prefix("a"), "1");
        assert_eq!(sparse_prefix("ab"), "2");
        assert_eq!(sparse_prefix("abc"), "3/a");
        assert_eq!(sparse_prefix("serde"), "se/rd");
    }

    #[test]
    fn cargo_caret_semantics() {
        let v = Version::parse("1.2.5").unwrap();
        assert!(cargo_matches("^1.2.3", &v));
        assert!(!cargo_matches("^1.2.3", &Version::parse("2.0.0").unwrap()));
        assert!(cargo_matches("1.2", &v)); // bare = caret
        assert!(cargo_matches(">=1.0, <2.0", &v));
        assert!(cargo_matches("~1.2.3", &v));
        assert!(!cargo_matches("~1.2.3", &Version::parse("1.3.0").unwrap()));
        assert!(cargo_matches("=1.2.5", &v));
    }
}
