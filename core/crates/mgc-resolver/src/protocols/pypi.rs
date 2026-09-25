//! PyPI native engine (Python).
//! Engine native PyPI (Python).
//!
//! JSON API spec: `{index}/pypi/{name}/json` lists `info` + `releases`
//! (version → files). Transitive deps come from the version-specific
//! `{index}/pypi/{name}/{version}/json` → `info.requires_dist` (PEP 508).
//! Wheel selection prefers a universal `py3-none-any` wheel, then a
//! host-platform wheel, then an sdist. sha256 is verified from `digests`.
//! Spec JSON API: `{index}/pypi/{name}/json` liệt kê `info` + `releases`
//! (version → files). Dep bắc cầu lấy từ `{index}/pypi/{name}/{version}/json`
//! → `info.requires_dist` (PEP 508). Chọn wheel ưu tiên wheel phổ quát
//! `py3-none-any`, rồi wheel khớp platform host, rồi sdist. sha256 xác minh
//! từ `digests`.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use mgc_types::{MgError, MgResult, Version};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_INDEX_URL: &str = "https://pypi.org";

/// Native PyPI engine.
/// Engine native PyPI.
#[derive(Debug, Clone)]
pub struct PypiProtocol {
    index_url: String,
    client: mgc_http::HttpClient,
}

impl PypiProtocol {
    /// Build with an explicit index URL.
    /// Dựng với index URL tường minh.
    pub fn new(index_url: &str) -> Self {
        Self {
            index_url: index_url.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
        }
    }

    /// Build from environment: `MGC_PYPI_INDEX_URL`.
    /// Dựng từ môi trường: `MGC_PYPI_INDEX_URL`.
    pub fn from_env() -> Self {
        let index_url = std::env::var("MGC_PYPI_INDEX_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_INDEX_URL.to_string());
        Self::new(&index_url)
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

    /// Materialize a wheel/sdist into `{wheels_dir}/<filename>`.
    /// Materialize wheel/sdist vào `{wheels_dir}/<filename>`.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        wheels_dir: &Path,
    ) -> MgResult<PathBuf> {
        let filename = Self::artifact_filename(&entry.artifact_url)?;
        std::fs::create_dir_all(wheels_dir)?;
        let dest = wheels_dir.join(filename);
        std::fs::write(&dest, bytes)?;
        Ok(dest)
    }

    /// Validate registry-controlled Python identity fields before deriving
    /// a cache path. Package names are one segment; versions and filenames
    /// accept only characters valid in wheel/sdist basenames.
    /// (Kiểm tra trường identity từ registry trước khi tạo cache path.)
    pub fn importable_site_dirname(name: &str, version: &str) -> MgResult<String> {
        mgc_types::PackageName::new(name)?;
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return Err(MgError::Other(
                "invalid Python package name for materialization".to_string(),
            ));
        }
        mgc_types::Version::parse(version)?;
        if version.is_empty()
            || !version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-_+".contains(&byte))
        {
            return Err(MgError::Other(
                "invalid Python package version for materialization".to_string(),
            ));
        }
        Ok(format!("{name}-{version}"))
    }

    fn artifact_filename(artifact_url: &str) -> MgResult<String> {
        let url = url::Url::parse(artifact_url)
            .map_err(|e| MgError::Other(format!("invalid Python artifact URL: {e}")))?;
        let filename = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| MgError::Other("Python artifact URL has no filename".to_string()))?;
        if filename == "."
            || filename == ".."
            || !filename
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        {
            return Err(MgError::Other(
                "unsafe Python artifact filename".to_string(),
            ));
        }
        Ok(filename.to_string())
    }

    /// Whether an artifact can be materialized by the current native
    /// Python runtime (pure-Python wheel only; no build backend or ABI
    /// loader is implemented yet).
    /// (Artifact có thể materialize bởi runtime Python native hiện tại.)
    pub fn is_importable_pure_wheel(artifact_url: &str) -> MgResult<bool> {
        let filename = Self::artifact_filename(artifact_url)?;
        Ok(Self::is_pure_wheel_filename(&filename))
    }

    /// Unpack a PURE-PYTHON wheel into `{wheels_dir}/site/<name>-<version>/`
    /// for importable use. Compiled wheels (versioned ABI tag) return None —
    /// unpacking them would fake an install the interpreter cannot load
    /// (the ABI warning stays the honest signal).
    /// (Bung wheel pure-python thành site dir import được.)
    pub fn materialize_importable(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        wheels_dir: &Path,
    ) -> MgResult<Option<PathBuf>> {
        let filename = Self::artifact_filename(&entry.artifact_url)?;
        if !Self::is_pure_wheel_filename(&filename) {
            return Ok(None);
        }
        let site = wheels_dir
            .join("site")
            .join(Self::importable_site_dirname(&entry.name, &entry.version)?);
        super::zip_reader::extract_zip(bytes, &site)?;
        // RECORD verification (PEP 376): every extracted file must match
        // its recorded sha256 + size. Whole-file sha256 (checked at
        // download) proves the bytes came from the registry; RECORD
        // proves the extracted tree matches the wheel's own manifest —
        // a corrupted/truncated unzip fails closed here, never imports.
        // (Xác minh RECORD: mọi file giải nén phải khớp hash + size.)
        Self::verify_wheel_record_dir(&site)?;
        Ok(Some(site))
    }

    /// Verify an unpacked wheel tree against its `*.dist-info/RECORD`
    /// (PEP 376): every listed file must exist with matching sha256
    /// (base64url, `sha256=` scheme) and byte size. The RECORD row itself
    /// carries empty hash/size (self-reference). Missing RECORD, missing
    /// files, hash or size mismatches all fail closed.
    /// (Xác minh cây wheel đã giải nén theo RECORD.)
    fn verify_wheel_record_dir(site: &Path) -> MgResult<()> {
        use sha2::{Digest, Sha256};
        let record = std::fs::read_dir(site)
            .map_err(|e| MgError::Other(format!("read unpacked wheel: {e}")))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".dist-info"))
            })
            .and_then(|d| {
                let r = d.join("RECORD");
                r.is_file().then_some(r)
            })
            .ok_or_else(|| {
                MgError::Integrity(
                    "unpacked wheel has no dist-info/RECORD (fail-closed)".to_string(),
                )
            })?;
        let body = std::fs::read_to_string(&record)
            .map_err(|e| MgError::Other(format!("read RECORD: {e}")))?;
        // base64url engine (RECORD uses url-safe alphabet, no padding).
        use base64::Engine;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        for (lineno, line) in body.lines().enumerate() {
            let line = line.trim().trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let mut cols = line.splitn(3, ',');
            let (Some(rel), hash, size) = (cols.next(), cols.next(), cols.next()) else {
                return Err(MgError::Integrity(format!(
                    "malformed RECORD line {} (fail-closed)",
                    lineno + 1
                )));
            };
            // Path traversal inside RECORD is an attack — refuse.
            if rel.contains("..") || rel.starts_with('/') || rel.starts_with('\\') {
                return Err(MgError::Integrity(format!(
                    "RECORD entry escapes the wheel: '{rel}' (fail-closed)"
                )));
            }
            let path = site.join(rel);
            let data = std::fs::read(&path).map_err(|_| {
                MgError::Integrity(format!("RECORD lists missing file '{rel}' (fail-closed)"))
            })?;
            match hash {
                // The RECORD row itself.
                None | Some("") => continue,
                Some(h) => {
                    let digest = h.strip_prefix("sha256=").ok_or_else(|| {
                        MgError::Integrity(format!(
                            "unsupported RECORD hash scheme for '{rel}' (fail-closed)"
                        ))
                    })?;
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    let actual = b64.encode(hasher.finalize());
                    // Compare unpadded both sides (registries vary).
                    if actual.trim_end_matches('=') != digest.trim_end_matches('=') {
                        return Err(MgError::Integrity(format!(
                            "RECORD hash mismatch for '{rel}' (fail-closed)"
                        )));
                    }
                    if let Some(expected_size) = size {
                        let expected_size: u64 = expected_size.trim().parse().map_err(|_| {
                            MgError::Integrity(format!(
                                "malformed RECORD size for '{rel}' (fail-closed)"
                            ))
                        })?;
                        if data.len() as u64 != expected_size {
                            return Err(MgError::Integrity(format!(
                                "RECORD size mismatch for '{rel}' (fail-closed)"
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Pure-python wheel tags: `{py}-none-any` with a py2/py3 interpreter
    /// tag (compound `py2.py3` included).
    fn is_pure_wheel_filename(filename: &str) -> bool {
        let stem = filename.strip_suffix(".whl").unwrap_or(filename);
        let mut parts = stem.rsplit('-');
        let platform = parts.next().unwrap_or("");
        let abi = parts.next().unwrap_or("");
        let py = parts.next().unwrap_or("");
        platform == "any"
            && abi == "none"
            && (py == "py" || py.starts_with("py2") || py.starts_with("py3"))
    }

    /// Export a `requirements.lock` (`--require-hashes`) body for offline
    /// `pip install -r` — pinned, hash-carrying, no re-resolve.
    /// Xuất body `requirements.lock` (`--require-hashes`) cho `pip install -r`
    /// offline — ghim, mang hash, không resolve lại.
    pub fn requirements_lock(entries: &[ResolvedEntry]) -> String {
        let mut out = String::from("# Generated by MagiCore (native PyPI resolve).\n");
        for entry in entries {
            if entry.sha256.is_empty() {
                out.push_str(&format!("{}=={}\n", entry.name, entry.version));
            } else {
                out.push_str(&format!(
                    "{}=={} --hash=sha256:{}\n",
                    entry.name, entry.version, entry.sha256
                ));
            }
        }
        out
    }
}

impl Default for PypiProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl RegistryProtocol for PypiProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let url = format!("{}/pypi/{name}/json", self.index_url);
        let body = self.get_text(&url).await?;
        let doc: PypiJson = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse pypi json failed: {e}")))?;

        // Matching versions, newest first.
        // (Các version khớp, mới nhất trước.)
        let mut candidates: Vec<(Version, Vec<PypiFile>)> = doc
            .releases
            .iter()
            .filter(|(ver_str, files)| {
                !files.is_empty()
                    && Version::parse(ver_str).is_ok_and(|v| pep440_matches(range, &v))
            })
            .filter_map(|(ver_str, files)| Version::parse(ver_str).ok().map(|v| (v, files.clone())))
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0));

        // Consumer-Python gate (`MGC_PYTHON_VERSION`): a version whose
        // `requires_python` excludes the consumer must not be selected
        // (pip would skip it too — e.g. click 8.5.0 `>=3.10` on a 3.9
        // interpreter). The first passing version wins; without the env,
        // the newest match wins (historical behavior).
        // (Cổng requires_python theo consumer.)
        let consumer = consumer_python();
        let (version, files) = match consumer {
            None => candidates
                .into_iter()
                .next()
                .ok_or_else(|| MgError::Other(format!("no version of {name} matches '{range}'")))?,
            Some((major, minor)) => {
                let mut selected: Option<(Version, Vec<PypiFile>)> = None;
                let mut last_excluded: Option<String> = None;
                for (candidate, candidate_files) in candidates {
                    let version_url = format!("{}/pypi/{name}/{candidate}/json", self.index_url);
                    let Ok(version_body) = self.get_text(&version_url).await else {
                        continue;
                    };
                    let Ok(version_doc): Result<PypiJson, _> = serde_json::from_str(&version_body)
                    else {
                        continue;
                    };
                    let required = version_doc.info.requires_python.clone().unwrap_or_default();
                    if required.trim().is_empty() || requires_python_allows(&required, major, minor)
                    {
                        selected = Some((candidate, candidate_files));
                        break;
                    }
                    last_excluded = Some(format!("{candidate} requires {required}"));
                }
                selected.ok_or_else(|| {
                    MgError::Other(format!(
                        "no version of {name} matches '{range}' on Python {major}.{minor}{}",
                        last_excluded
                            .map(|e| format!(" (newest excluded: {e})"))
                            .unwrap_or_default()
                    ))
                })?
            }
        };
        let file = select_file(&files)
            .ok_or_else(|| MgError::Other(format!("no downloadable file for {name} {version}")))?;

        let mut markers = Vec::new();
        if file.packagetype == "bdist_wheel" {
            let (py, abi, plat) = wheel_tags(&file.filename);
            markers.push(format!("wheel:{py}-{abi}-{plat}"));
        } else {
            markers.push(format!("sdist:{}", file.filename));
        }
        if let Some(rp) = &file.requires_python {
            markers.push(format!("requires-python:{rp}"));
        }

        // Transitive deps: version-specific JSON carries the real requires_dist.
        // (Dep bắc cầu: JSON theo version mang requires_dist thật.)
        let version_url = format!("{}/pypi/{name}/{version}/json", self.index_url);
        let version_body = self.get_text(&version_url).await?;
        let version_doc: PypiJson = serde_json::from_str(&version_body)
            .map_err(|e| MgError::Other(format!("parse pypi version json failed: {e}")))?;

        let mut deps = Vec::new();
        if let Some(requires) = version_doc.info.requires_dist {
            // Environment markers evaluated once per resolve (same
            // consumer for the whole graph — no per-dep env reads).
            let consumer = consumer_python();
            for spec in requires {
                let Some((dep_name, dep_range, marker)) = parse_pep508(&spec) else {
                    continue;
                };
                if marker.as_deref().is_some_and(|m| m.contains("extra")) {
                    markers.push(format!("marker:{}", marker.unwrap_or_default()));
                    continue;
                }
                if let Some(m) = marker {
                    // Certainly-excluded environments skip the dep (pip
                    // would never install it); everything else is kept
                    // with the marker recorded.
                    if marker_applies(&m, consumer) == Some(false) {
                        markers.push(format!("marker-excluded:{m}"));
                        continue;
                    }
                    markers.push(format!("marker:{m}"));
                }
                deps.push((dep_name, dep_range));
            }
        }

        Ok(ResolvedEntry {
            name: name.to_string(),
            version: version.to_string(),
            deps,
            artifact_url: file.url.clone(),
            sha256: file.digests.sha256.clone(),
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

/// Select the best installable file: universal wheel → host wheel →
/// sdist. There is deliberately NO "any wheel" fallback: installing a
/// wheel built for another platform is a silent wrong-artifact install
/// (V1.2: fail-closed — no file at all beats the wrong file).
/// Chọn file cài tốt nhất: wheel phổ quát → wheel host → sdist. Cố ý
/// KHÔNG fallback "wheel bất kỳ".
fn select_file(files: &[PypiFile]) -> Option<&PypiFile> {
    if let Some(f) = files.iter().find(|f| is_universal_wheel(f)) {
        return Some(f);
    }
    if let Some(f) = files
        .iter()
        .find(|f| f.packagetype == "bdist_wheel" && wheel_platform_matches_host(&f.filename))
    {
        return Some(f);
    }
    files.iter().find(|f| f.packagetype == "sdist")
}

fn is_universal_wheel(file: &PypiFile) -> bool {
    if file.packagetype != "bdist_wheel" {
        return false;
    }
    let (py, abi, plat) = wheel_tags(&file.filename);
    abi == "none" && plat == "any" && py.contains("py3")
}

/// Parse a wheel filename into `(python, abi, platform)` tags (last 3 parts).
/// Parse filename wheel thành tag `(python, abi, platform)` (3 phần cuối).
fn wheel_tags(filename: &str) -> (String, String, String) {
    let stem = filename.strip_suffix(".whl").unwrap_or(filename);
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() >= 3 {
        let n = parts.len();
        (
            parts[n - 3].to_string(),
            parts[n - 2].to_string(),
            parts[n - 1].to_string(),
        )
    } else {
        (String::new(), String::new(), String::new())
    }
}

fn wheel_platform_matches_host(filename: &str) -> bool {
    let (py, abi, plat) = wheel_tags(filename);
    if !platform_matches_host(&plat) {
        return false;
    }
    // Without a known consumer Python (`MGC_PYTHON_VERSION`), any
    // platform-matching wheel is accepted (historical behavior) — the
    // chosen ABI is recorded in markers + warned about at install so a
    // cp310-on-3.9 style mismatch is never silent. With the env set,
    // only ABI-compatible wheels pass (exact cpXY / abi3 floor /
    // universal); the rest are skipped, never installed.
    // (Không biết Python consumer thì nhận wheel khớp platform (cũ) +
    // cảnh báo; có env thì lọc ABI chặt.)
    let Some((major, minor)) = consumer_python() else {
        return true;
    };
    abi_compatible(&py, &abi, major, minor)
}

/// OS/arch platform check (no Python involved).
/// (Kiểm tra OS/arch.)
fn platform_matches_host(plat: &str) -> bool {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if os == "macos" && arch == "aarch64" {
        return plat.starts_with("macosx_")
            && (plat.ends_with("_arm64") || plat.ends_with("_universal2"));
    }
    if os == "linux" && arch == "aarch64" {
        return (plat.starts_with("manylinux_")
            || plat.starts_with("musllinux_")
            || plat.starts_with("linux_"))
            && plat.ends_with("_aarch64");
    }
    false
}

/// Consumer Python from `MGC_PYTHON_VERSION` (`3.9`, `3.9.6`, `39` …).
/// `None` = unknown (historical lenient behavior + warning).
/// (Python consumer từ env `MGC_PYTHON_VERSION`.)
fn consumer_python() -> Option<(u64, u64)> {
    let raw = std::env::var("MGC_PYTHON_VERSION").ok()?;
    let digits: String = raw
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parts: Vec<&str> = digits.split('.').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        // `39` (no dot) = 3.9; `310` = 3.10 (first digit = major).
        // (`39` không dấu chấm là 3.9.)
        [single] if !single.contains('.') && single.len() >= 2 => {
            let major: u64 = single[..1].parse().ok()?;
            let minor: u64 = single[1..].parse().ok()?;
            (major > 0).then_some((major, minor))
        }
        [major, minor, ..] => {
            let major: u64 = major.parse().ok()?;
            let minor: u64 = minor.parse().unwrap_or(0);
            (major > 0).then_some((major, minor))
        }
        [major] => {
            let major: u64 = major.parse().ok()?;
            (major > 0).then_some((major, 0))
        }
        _ => None,
    }
}

/// ABI compatibility of one wheel's `(py, abi)` tags against consumer
/// `(major, minor)`: universal, exact cpXY, or abi3 at/above its floor
/// (`cp37-abi3` runs on 3.7+).
/// (Tương thích ABI của wheel với Python consumer.)
fn abi_compatible(py: &str, abi: &str, major: u64, minor: u64) -> bool {
    if abi == "none" {
        // `py3-none-any` universal, or versioned pure-python (`cp39-none-any`).
        return py.split('.').any(|tag| {
            tag == "py3" || tag == format!("py{major}{minor}") || tag == format!("cp{major}{minor}")
        });
    }
    if abi == "abi3" {
        // Stable ABI: `cp37-abi3` needs consumer >= 3.7.
        // (ABI ổn định: chạy trên mọi bản >= floor.)
        for tag in py.split('.') {
            if let Some(rest) = tag.strip_prefix("cp") {
                let floor: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                let (f_major, f_minor) = match floor.len() {
                    0 => (0, 0),
                    1 => (floor.parse().unwrap_or(0), 0),
                    _ => (
                        floor[..1].parse().unwrap_or(0),
                        floor[1..].parse().unwrap_or(0),
                    ),
                };
                if major > f_major || (major == f_major && minor >= f_minor) {
                    return true;
                }
            }
        }
        return false;
    }
    // Versioned ABI (`cp39`, `pp39`): exact match only.
    // (ABI theo version: khớp chính xác.)
    let want = format!("cp{major}{minor}");
    py.split('.').any(|tag| {
        let base = tag.split('_').next().unwrap_or(tag);
        base == want
    }) && abi == want
}

/// Does a `requires_python` specifier allow consumer `(major, minor)`?
/// Comma = AND; operators `==` (with `.*`), `>=`, `<=`, `>`, `<`, `!=`,
/// `~=` (compatible release). Unknown/empty parts fail closed (deny).
/// (`requires_python` có cho consumer không? Không rõ → từ chối.)
fn requires_python_allows(spec: &str, major: u64, minor: u64) -> bool {
    fn parse_ver(text: &str) -> Option<(u64, u64, u64)> {
        let text = text.trim().trim_end_matches(".*");
        let mut parts = text.split('.');
        let maj: u64 = parts.next()?.trim().parse().ok()?;
        let min: u64 = parts.next().unwrap_or("0").trim().parse().ok()?;
        let pat: u64 = parts.next().unwrap_or("0").trim().parse().ok()?;
        Some((maj, min, pat))
    }
    let consumer = (major, minor, 0u64);
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (op, ver) = if let Some(v) = part.strip_prefix("==") {
            ("==", v)
        } else if let Some(v) = part.strip_prefix("~=") {
            ("~=", v)
        } else if let Some(v) = part.strip_prefix("!=") {
            ("!=", v)
        } else if let Some(v) = part.strip_prefix(">=") {
            (">=", v)
        } else if let Some(v) = part.strip_prefix("<=") {
            ("<=", v)
        } else if let Some(v) = part.strip_prefix('>') {
            (">", v)
        } else if let Some(v) = part.strip_prefix('<') {
            ("<", v)
        } else {
            return false;
        };
        let wildcard = ver.trim().ends_with(".*");
        let Some(target) = parse_ver(ver) else {
            return false;
        };
        let ok = match op {
            "==" => {
                if wildcard {
                    let prefix = ver.trim().trim_end_matches(".*").trim_end_matches('.');
                    let want: Vec<u64> = prefix
                        .split('.')
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    let have = [consumer.0, consumer.1, consumer.2];
                    !want.is_empty()
                        && want.len() <= 3
                        && want.iter().enumerate().all(|(i, w)| have[i] == *w)
                } else {
                    consumer == target
                }
            }
            "!=" => {
                if wildcard {
                    let prefix = ver.trim().trim_end_matches(".*").trim_end_matches('.');
                    let want: Vec<u64> = prefix
                        .split('.')
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    want.is_empty()
                        || !(want.len() <= 3
                            && want
                                .iter()
                                .enumerate()
                                .all(|(i, w)| [consumer.0, consumer.1, consumer.2][i] == *w))
                } else {
                    consumer != target
                }
            }
            ">=" => consumer >= target,
            "<=" => consumer <= target,
            ">" => consumer > target,
            "<" => consumer < target,
            "~=" => {
                // Compatible release: >= V, == V.* (same prefix length).
                // (Release tương thích.)
                let dots = ver.trim().matches('.').count();
                if dots == 0 {
                    return false;
                }
                let prefix_len = dots;
                let have = [consumer.0, consumer.1, consumer.2];
                let want = [target.0, target.1, target.2];
                consumer >= target && have[..prefix_len] == want[..prefix_len]
            }
            _ => return false,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// PEP 440 subset matcher: `==`, `==x.y.*`, `>=`, `<=`, `>`, `<`, `!=`,
/// `~=` (compatible release), comma = AND, bare version = exact.
/// Matcher PEP 440 subset: `==`, `==x.y.*`, `>=`, `<=`, `>`, `<`, `!=`,
/// `~=` (compatible release), phẩy = AND, version trần = exact.
fn pep440_matches(range: &str, version: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    range
        .split(',')
        .map(str::trim)
        .all(|part| pep440_part(part, version))
}

fn pep440_part(part: &str, version: &Version) -> bool {
    let part = part.trim();
    if part == "*" {
        return true;
    }
    if let Some(p) = part.strip_prefix("==") {
        return pep_eq(p, version);
    }
    // Single `=` is mgc's own exact-pin spelling (the orchestrator builds
    // `=version` ranges from resolved ids) — treat as exact, never as
    // no-match. (`=` đơn là cách ghim exact của mgc.)
    if let Some(p) = part.strip_prefix('=') {
        return pep_eq(p, version);
    }
    if let Some(p) = part.strip_prefix("~=") {
        return pep_compat(p, version);
    }
    if let Some(p) = part.strip_prefix("!=") {
        return !pep_eq(p, version);
    }
    if let Some(p) = part.strip_prefix(">=") {
        return pep_ge(p, version);
    }
    if let Some(p) = part.strip_prefix("<=") {
        return pep_le(p, version);
    }
    if let Some(p) = part.strip_prefix('>') {
        return pep_gt(p, version);
    }
    if let Some(p) = part.strip_prefix('<') {
        return pep_lt(p, version);
    }
    // bare version → exact.
    pep_eq(part, version)
}

fn pep_eq(raw: &str, version: &Version) -> bool {
    let raw = raw.trim();
    if let Some(base) = raw.strip_suffix(".*") {
        let base = base.trim_end_matches('.');
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
    } else {
        Version::parse(raw)
            .map(|target| version == &target)
            .unwrap_or(false)
    }
}

fn pep_compat(raw: &str, version: &Version) -> bool {
    let raw = raw.trim();
    let Some(target) = Version::parse(raw).ok() else {
        return false;
    };
    if version < &target {
        return false;
    }
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() <= 2 {
        // ~=1.4 → >=1.4,<2.0
        version.major == target.major
    } else {
        // ~=1.4.5 → >=1.4.5,<1.5
        version.major == target.major && version.minor == target.minor
    }
}

fn pep_ge(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version >= &t)
        .unwrap_or(false)
}
fn pep_le(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version <= &t)
        .unwrap_or(false)
}
fn pep_gt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version > &t)
        .unwrap_or(false)
}
fn pep_lt(raw: &str, version: &Version) -> bool {
    Version::parse(raw.trim())
        .map(|t| version < &t)
        .unwrap_or(false)
}

/// Parse a PEP 508 requirement into `(name, range, marker)`. Extra deps are
/// surfaced via the marker (caller skips them); `[extras]` are dropped.
/// Parse requirement PEP 508 thành `(name, range, marker)`. Dep extra lộ qua
/// marker (caller bỏ); `[extras]` bị loại.
fn parse_pep508(spec: &str) -> Option<(String, String, Option<String>)> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let (req, marker) = match spec.split_once(';') {
        Some((r, m)) => (r.trim(), Some(m.trim().to_string())),
        None => (spec.trim(), None),
    };
    let op_idx = req
        .find(|c: char| ['=', '>', '<', '!', '~'].contains(&c))
        .unwrap_or(req.len());
    let name_part = req[..op_idx].split('[').next().unwrap_or("").trim();
    if name_part.is_empty() {
        return None;
    }
    let range = if op_idx < req.len() {
        req[op_idx..].trim().to_string()
    } else {
        "*".to_string()
    };
    Some((name_part.to_string(), range, marker))
}

#[derive(Debug, Deserialize)]
struct PypiJson {
    #[serde(default)]
    info: PypiInfo,
    #[serde(default)]
    releases: HashMap<String, Vec<PypiFile>>,
}

#[derive(Debug, Deserialize, Default)]
struct PypiInfo {
    #[serde(default)]
    requires_dist: Option<Vec<String>>,
    #[serde(default)]
    requires_python: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PypiFile {
    #[serde(default)]
    filename: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    digests: PypiDigests,
    #[serde(default)]
    packagetype: String,
    #[serde(default)]
    requires_python: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PypiDigests {
    #[serde(default)]
    sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mgc_types::Version;

    #[test]
    fn pep440_compatible_release() {
        let v = Version::parse("1.4.2").unwrap();
        assert!(pep440_matches("~=1.4", &v));
        assert!(!pep440_matches("~=1.4", &Version::parse("2.0.0").unwrap()));
        assert!(pep440_matches("~=1.4.5", &Version::parse("1.4.9").unwrap()));
        assert!(!pep440_matches(
            "~=1.4.5",
            &Version::parse("1.5.0").unwrap()
        ));
    }

    #[test]
    fn pep440_eq_wildcard() {
        assert!(pep440_matches("==1.2.*", &Version::parse("1.2.9").unwrap()));
        assert!(!pep440_matches(
            "==1.2.*",
            &Version::parse("1.3.0").unwrap()
        ));
        assert!(pep440_matches("!=1.2", &Version::parse("1.3.0").unwrap()));
        assert!(!pep440_matches("!=1.2", &Version::parse("1.2.0").unwrap()));
    }

    #[test]
    fn pep508_parsing() {
        let (name, range, marker) = parse_pep508("requests>=2.0 ; extra == \"security\"").unwrap();
        assert_eq!(name, "requests");
        assert_eq!(range, ">=2.0");
        assert!(marker.unwrap().contains("extra"));
        let (name, range, marker) = parse_pep508("idna").unwrap();
        assert_eq!(name, "idna");
        assert_eq!(range, "*");
        assert!(marker.is_none());
    }

    #[test]
    fn wheel_tags_last_three() {
        assert_eq!(
            wheel_tags("numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl"),
            (
                "cp312".to_string(),
                "cp312".to_string(),
                "macosx_11_0_arm64".to_string()
            )
        );
        assert_eq!(
            wheel_tags("pkg-1.0-py3-none-any.whl"),
            ("py3".to_string(), "none".to_string(), "any".to_string())
        );
    }

    #[test]
    fn abi_compatible_cases() {
        // Universal + exact + abi3 floors.
        // (Universal + exact + floor abi3.)
        assert!(abi_compatible("py3", "none", 3, 9));
        assert!(abi_compatible("cp39", "cp39", 3, 9));
        assert!(!abi_compatible("cp310", "cp310", 3, 9));
        assert!(abi_compatible("cp310", "cp310", 3, 10));
        assert!(abi_compatible("cp37", "abi3", 3, 9));
        assert!(abi_compatible("cp39", "abi3", 3, 9));
        assert!(!abi_compatible("cp310", "abi3", 3, 9));
        assert!(!abi_compatible("cp39", "cp39", 3, 10));
        assert!(abi_compatible("py2.py3", "none", 3, 9));
    }

    #[test]
    fn requires_python_allows_cases() {
        assert!(requires_python_allows(">=3.9", 3, 9));
        assert!(!requires_python_allows(">=3.10", 3, 9));
        assert!(requires_python_allows(">=3.9,<4", 3, 9));
        assert!(!requires_python_allows(">=3.9,<3.10", 3, 10));
        assert!(requires_python_allows("==3.9.*", 3, 9));
        assert!(!requires_python_allows("==3.9.*", 3, 10));
        assert!(requires_python_allows("~=3.9", 3, 9));
        assert!(requires_python_allows("~=3.9", 3, 12));
        assert!(!requires_python_allows("~=3.9", 4, 0));
        assert!(!requires_python_allows("!=3.9.*", 3, 9));
        assert!(requires_python_allows("!=3.9.*", 3, 10));
        assert!(!requires_python_allows("bogus", 3, 9));
    }

    #[test]
    fn consumer_python_reads_env() {
        // Serialized: env is process-global and tests run in threads.
        // (Tuần tự hóa: env toàn process.)
        static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = SERIAL.lock().unwrap();
        let old = std::env::var("MGC_PYTHON_VERSION").ok();
        // SAFETY: test-only, held SERIAL, restored below before release.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("MGC_PYTHON_VERSION", "3.9");
            assert_eq!(consumer_python(), Some((3, 9)));
            std::env::set_var("MGC_PYTHON_VERSION", "310");
            assert_eq!(consumer_python(), Some((3, 10)));
            std::env::set_var("MGC_PYTHON_VERSION", "39");
            assert_eq!(consumer_python(), Some((3, 9)));
            std::env::remove_var("MGC_PYTHON_VERSION");
            assert_eq!(consumer_python(), None);
            if let Some(v) = old {
                std::env::set_var("MGC_PYTHON_VERSION", v);
            }
        }
    }
}

#[cfg(test)]
mod importable_tests {
    use super::*;

    /// Minimal stored-method zip builder (no compression dependency).
    fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let crc = super::super::zip_reader::crc32(data);
            let n = name.len() as u16;
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // stored
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&n.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        let cd_size = central.len() as u32;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn entry_for(url: &str) -> ResolvedEntry {
        ResolvedEntry {
            name: "six".to_string(),
            version: "1.17.0".to_string(),
            deps: Vec::new(),
            artifact_url: url.to_string(),
            sha256: String::new(),
            extra_markers: Vec::new(),
        }
    }

    #[test]
    fn registry_identity_and_artifact_name_cannot_escape_cache_paths() {
        assert_eq!(
            PypiProtocol::importable_site_dirname("six", "1.17.0").unwrap(),
            "six-1.17.0"
        );
        for (name, version) in [
            ("../../outside", "1.0.0"),
            ("six", "1.0.0-../../outside"),
            ("six", r"1.0.0-..\outside"),
        ] {
            assert!(
                PypiProtocol::importable_site_dirname(name, version).is_err(),
                "must reject {name}@{version}"
            );
        }
        assert!(
            PypiProtocol::artifact_filename("https://files.pythonhosted.org/%2e%2e%2foutside.whl")
                .is_err()
        );
    }

    #[test]
    fn native_runtime_accepts_only_pure_python_wheels() {
        assert!(
            PypiProtocol::is_importable_pure_wheel(
                "https://files.pythonhosted.org/six-1.17.0-py3-none-any.whl"
            )
            .unwrap()
        );
        assert!(
            !PypiProtocol::is_importable_pure_wheel(
                "https://files.pythonhosted.org/numpy-2.0.0-cp312-cp312-macosx_14_0_arm64.whl"
            )
            .unwrap()
        );
        assert!(
            !PypiProtocol::is_importable_pure_wheel(
                "https://files.pythonhosted.org/example-1.0.0.tar.gz"
            )
            .unwrap()
        );
    }

    #[test]
    fn materializers_reject_unsafe_registry_paths_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let protocol = PypiProtocol::new("https://pypi.org");
        assert!(
            protocol
                .materialize(
                    &entry_for("https://files.pythonhosted.org/%2e%2e%2foutside.whl"),
                    b"payload",
                    dir.path(),
                )
                .is_err()
        );
        let mut entry = entry_for("https://files.pythonhosted.org/x/six-1.17.0-py3-none-any.whl");
        entry.version = "1.0.0-../../outside".to_string();
        assert!(
            protocol
                .materialize_importable(&entry, b"not a zip", dir.path())
                .is_err()
        );
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    /// Pure-python wheels unpack into an importable site dir (RECORD
    /// verified — a tampered member fails the unpack, not the import).
    #[test]
    fn materialize_importable_unpacks_pure_wheel() {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let six_py = b"__version__ = '1.17.0'\n";
        let meta = b"Name: six\n";
        let record_body = format!(
            "six.py,sha256={},{}\nsix-1.17.0.dist-info/METADATA,sha256={},{}\nsix-1.17.0.dist-info/RECORD,,\n",
            b64.encode(Sha256::digest(six_py)),
            six_py.len(),
            b64.encode(Sha256::digest(meta)),
            meta.len(),
        );
        let wheel = stored_zip(&[
            ("six.py", six_py.as_slice()),
            ("six-1.17.0.dist-info/METADATA", meta.as_slice()),
            ("six-1.17.0.dist-info/RECORD", record_body.as_bytes()),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let site = PypiProtocol::new("https://pypi.org")
            .materialize_importable(
                &entry_for("https://files.pythonhosted.org/x/six-1.17.0-py2.py3-none-any.whl"),
                &wheel,
                dir.path(),
            )
            .unwrap()
            .expect("pure wheel must unpack");
        assert!(site.join("six.py").exists(), "six.py importable");
    }

    /// A wheel whose member was tampered after RECORD was written must
    /// fail the unpack (fail-closed, never imports).
    #[test]
    fn materialize_importable_rejects_tampered_member() {
        let wheel = stored_zip(&[
            ("six.py", b"EVIL = True\n".as_slice()),
            ("six-1.17.0.dist-info/METADATA", b"Name: six\n".as_slice()),
            // RECORD claims the ORIGINAL bytes (stale content hash).
            (
                "six-1.17.0.dist-info/RECORD",
                b"six.py,sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA,23\nsix-1.17.0.dist-info/METADATA,sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB,11\nsix-1.17.0.dist-info/RECORD,,\n".as_slice(),
            ),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let err = PypiProtocol::new("https://pypi.org")
            .materialize_importable(
                &entry_for("https://files.pythonhosted.org/x/six-1.17.0-py2.py3-none-any.whl"),
                &wheel,
                dir.path(),
            )
            .expect_err("tampered member must fail");
        assert!(
            err.to_string().contains("RECORD"),
            "failure must name RECORD: {err}"
        );
    }

    /// Compiled wheels are NOT unpacked (ABI policy) — None, honestly.
    #[test]
    fn materialize_importable_skips_compiled_wheel() {
        let wheel = stored_zip(&[("x.so", b"fake")]);
        let dir = tempfile::tempdir().unwrap();
        let site = PypiProtocol::new("https://pypi.org")
            .materialize_importable(
                &entry_for(
                    "https://files.pythonhosted.org/x/x-1.0-cp39-cp39-manylinux1_x86_64.whl",
                ),
                &wheel,
                dir.path(),
            )
            .unwrap();
        assert!(site.is_none(), "compiled wheel must not unpack");
    }
}

/// Evaluate a PEP 508 environment marker against this machine.
///
/// Returns `Some(false)` ONLY when the marker certainly excludes us (the
/// dep is skipped, like pip would); `Some(true)`/`None` keep it.
/// `extra == ...` always yields `None` (extras are handled by opt-in,
/// never by exclusion here). Unknown variables, operators, parenthesized
/// groups, and version comparisons without a known consumer all yield
/// `None` — include honestly, never exclude on a guess.
/// (Đánh giá marker môi trường PEP 508 — chỉ loại khi chắc chắn.)
fn marker_applies(marker: &str, consumer: Option<(u64, u64)>) -> Option<bool> {
    eval_marker_or(marker.trim(), consumer)
}

fn eval_marker_or(expr: &str, consumer: Option<(u64, u64)>) -> Option<bool> {
    // Top-level `or` split (no paren support — parens yield None below).
    let mut out = Some(false);
    for part in split_top_level(expr, "or") {
        match eval_marker_and(part.trim(), consumer) {
            Some(true) => return Some(true),
            None => out = None,
            Some(false) => {}
        }
    }
    out
}

fn eval_marker_and(expr: &str, consumer: Option<(u64, u64)>) -> Option<bool> {
    let mut out = Some(true);
    for part in split_top_level(expr, "and") {
        match eval_marker_atom(part.trim(), consumer) {
            Some(false) => return Some(false),
            None => out = None,
            Some(true) => {}
        }
    }
    out
}

/// Split on a top-level keyword (ignores quoted contents; bails to a
/// single chunk on any parenthesis — unhandled groups stay unknown).
fn split_top_level<'a>(expr: &'a str, keyword: &str) -> Vec<&'a str> {
    if expr.contains('(') || expr.contains(')') {
        return vec![expr];
    }
    let mut parts = Vec::new();
    let mut depth_quote: Option<char> = None;
    let mut current_start = 0;
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = depth_quote {
            if c == q {
                depth_quote = None;
            }
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' {
            depth_quote = Some(c);
            i += 1;
            continue;
        }
        // Scan bytes for this ASCII keyword, then slice only at its start
        // and end (both guaranteed UTF-8 boundaries). Testing every byte
        // with `expr[i..]` can panic when a quoted marker contains Unicode.
        // (Tìm keyword ASCII theo byte nhưng chỉ cắt ở biên UTF-8.)
        let keyword_bytes = keyword.as_bytes();
        let at_token_start = i == 0 || bytes[i - 1].is_ascii_whitespace();
        let at_token_end = i + keyword_bytes.len() == bytes.len()
            || bytes
                .get(i + keyword_bytes.len())
                .is_some_and(u8::is_ascii_whitespace);
        if at_token_start && bytes[i..].starts_with(keyword_bytes) && at_token_end {
            parts.push(expr[current_start..i].trim());
            i += keyword.len();
            current_start = i;
            continue;
        }
        i += 1;
    }
    parts.push(expr[current_start..].trim());
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('\'') && t.ends_with('\'')) || (t.starts_with('"') && t.ends_with('"')))
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn eval_marker_atom(expr: &str, consumer: Option<(u64, u64)>) -> Option<bool> {
    // (operator, full literal incl. padding). Longest-first so `===`
    // beats `==` and `not in` beats `in`; padded word-ops cannot match
    // inside identifiers.
    const OPS: &[(&str, &str)] = &[
        ("===", "==="),
        ("not in", " not in "),
        ("==", "=="),
        ("!=", "!="),
        (">=", ">="),
        ("<=", "<="),
        (">", ">"),
        ("<", "<"),
        ("in", " in "),
    ];
    let mut matched: Option<(&str, usize, usize)> = None;
    for (name, lit) in OPS {
        if let Some(pos) = expr.find(lit) {
            let cur_len = matched.map(|(_, _, l)| l).unwrap_or(0);
            if lit.len() >= cur_len {
                matched = Some((name, pos, lit.len()));
            }
        }
    }
    let (op, pos, len) = matched?;
    let var = expr[..pos].trim();
    let val = unquote(expr[pos + len..].trim());
    if op == "in" || op == "not in" {
        // Evaluated against version lists in pip; a literal right side
        // is unhandled — unknown.
        return None;
    }
    match var {
        "extra" => None,
        "python_version" | "python_full_version" => {
            let (major, minor) = consumer?;
            eval_version_cmp(op, &val, major, minor)
        }
        "sys_platform" => eval_str_cmp(op, &val, current_sys_platform()),
        "os_name" => eval_str_cmp(op, &val, if cfg!(windows) { "nt" } else { "posix" }),
        "platform_system" => eval_str_cmp(
            op,
            &val,
            if cfg!(windows) {
                "Windows"
            } else if cfg!(target_os = "macos") {
                "Darwin"
            } else {
                "Linux"
            },
        ),
        "platform_machine" => eval_str_cmp(op, &val, std::env::consts::ARCH),
        _ => None,
    }
}

fn current_sys_platform() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

fn eval_str_cmp(op: &str, expected: &str, actual: &str) -> Option<bool> {
    match op {
        "==" => Some(actual == expected),
        "!=" => Some(actual != expected),
        _ => None,
    }
}

/// Compare a (major, minor) consumer against a marker version (`3.9`,
/// `3.9.*`, `39`?). `.*` suffix = prefix match (PEP 440 wildcard).
fn eval_version_cmp(op: &str, expected: &str, major: u64, minor: u64) -> Option<bool> {
    let exp = expected.trim();
    let (prefix_match, exp) = match exp.strip_suffix(".*") {
        Some(p) => (true, p),
        None => (false, exp),
    };
    let parts: Vec<&str> = exp.split('.').collect();
    let (emajor, eminor): (u64, Option<u64>) = match parts.as_slice() {
        [maj] => (maj.parse().ok()?, None),
        [maj, min, ..] => (maj.parse().ok()?, min.parse().ok()),
        _ => return None,
    };
    if prefix_match {
        // `== 3.9.*`: major must equal; minor compared only when given.
        let major_eq = major == emajor;
        let minor_eq = eminor.is_none_or(|m| minor == m);
        return match op {
            "==" => Some(major_eq && minor_eq),
            "!=" => Some(!(major_eq && minor_eq)),
            _ => None,
        };
    }
    let eminor = eminor?;
    let ord = (major, minor).cmp(&(emajor, eminor));
    match op {
        "==" => Some(ord == std::cmp::Ordering::Equal),
        "!=" => Some(ord != std::cmp::Ordering::Equal),
        ">" => Some(ord == std::cmp::Ordering::Greater),
        ">=" => Some(ord != std::cmp::Ordering::Less),
        "<" => Some(ord == std::cmp::Ordering::Less),
        "<=" => Some(ord != std::cmp::Ordering::Greater),
        _ => None,
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn sys_platform_markers_evaluate() {
        // This machine's platform must match itself and reject others.
        #[cfg(target_os = "linux")]
        {
            assert_eq!(marker_applies("sys_platform == 'linux'", None), Some(true));
            assert_eq!(
                marker_applies("sys_platform == 'darwin'", None),
                Some(false)
            );
            assert_eq!(marker_applies("sys_platform == 'win32'", None), Some(false));
            assert_eq!(marker_applies("os_name == 'posix'", None), Some(true));
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(marker_applies("sys_platform == 'darwin'", None), Some(true));
            assert_eq!(marker_applies("sys_platform == 'linux'", None), Some(false));
        }
        #[cfg(target_os = "windows")]
        {
            assert_eq!(marker_applies("sys_platform == 'win32'", None), Some(true));
            assert_eq!(marker_applies("os_name == 'nt'", None), Some(true));
        }
        assert_eq!(marker_applies("extra == 'foo'", None), None);
        // No consumer configured: version grounds never exclude.
        assert_eq!(marker_applies("python_version < '3.0'", None), None);
        assert_eq!(marker_applies("python_version > '3.99'", None), None);
        // Consumer 3.9: version grounds decide.
        assert_eq!(
            marker_applies("python_version < '3.10'", Some((3, 9))),
            Some(true)
        );
        assert_eq!(
            marker_applies("python_version >= '3.10'", Some((3, 9))),
            Some(false)
        );
        assert_eq!(
            marker_applies("python_version == '3.9.*'", Some((3, 9))),
            Some(true)
        );
    }

    #[test]
    fn python_version_markers_need_consumer() {
        // Without MGC_PYTHON_VERSION the version is unknown — never
        // exclude on version grounds alone.
        assert_eq!(marker_applies("python_version < '3.0'", None), None);
    }

    #[test]
    fn compound_markers_compose() {
        assert_eq!(
            marker_applies("extra == 'docs' and python_version < '3.10'", Some((3, 9))),
            None,
            "unknown AND true remains unknown"
        );
        assert_eq!(
            marker_applies("extra == 'docs' and python_version > '3.99'", Some((3, 9))),
            Some(false),
            "false AND unknown is determinately false"
        );
        assert_eq!(
            marker_applies("extra == 'docs' or python_version > '3.99'", Some((3, 9))),
            None,
            "unknown OR false remains unknown"
        );
        assert_eq!(
            marker_applies(
                "platform_release == '版本' and python_version < '3.10'",
                Some((3, 9))
            ),
            None,
            "Unicode literals must not panic and unknown AND true remains unknown"
        );
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                marker_applies(
                    "sys_platform == 'darwin' and python_version > '3.99'",
                    Some((3, 9))
                ),
                Some(false)
            );
            assert_eq!(
                marker_applies(
                    "sys_platform == 'linux' and python_version > '3.99'",
                    Some((3, 9))
                ),
                Some(false)
            );
            assert_eq!(
                marker_applies("sys_platform == 'darwin' or sys_platform == 'linux'", None),
                Some(true)
            );
        }
    }
}
