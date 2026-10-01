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

    /// Materialize a wheel/sdist into `{wheels_dir}/sha256/<digest>`.
    /// Materialize wheel/sdist vào `{wheels_dir}/sha256/<digest>`.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        wheels_dir: &Path,
    ) -> MgResult<PathBuf> {
        let digest = Self::verified_materialization_digest(entry, bytes)?;
        let relative = Self::artifact_cache_relpath(&entry.artifact_url, &digest)?;
        std::fs::create_dir_all(wheels_dir)?;
        let dest = wheels_dir.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, bytes)?;
        Ok(dest)
    }

    /// Return a cache path isolated by the authenticated artifact digest.
    /// Trả path cache tách biệt theo digest artifact đã xác minh.
    pub fn artifact_cache_relpath(artifact_url: &str, sha256: &str) -> MgResult<PathBuf> {
        let digest = Self::canonical_sha256(sha256)?;
        Self::artifact_filename(artifact_url)?;
        Ok(PathBuf::from("sha256").join(digest))
    }

    /// Build a site directory identity that cannot alias another wheel with
    /// the same distribution/version but different bytes.
    /// (Tạo định danh site không đè wheel khác cùng tên/version nhưng khác byte.)
    pub fn importable_site_dirname_for_digest(
        name: &str,
        version: &str,
        sha256: &str,
    ) -> MgResult<String> {
        let base = Self::importable_site_dirname(name, version)?;
        let digest = Self::canonical_sha256(sha256)?;
        Ok(format!("{base}-{digest}"))
    }

    fn canonical_sha256(sha256: &str) -> MgResult<String> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MgError::Integrity(
                "Python cache identity requires a valid 64-character SHA-256 digest".to_string(),
            ));
        }
        Ok(sha256.to_ascii_lowercase())
    }

    fn verified_materialization_digest(entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<String> {
        let expected = Self::canonical_sha256(&entry.sha256)?;
        let actual = super::sha256_hex(bytes);
        if actual != expected {
            return Err(MgError::Integrity(format!(
                "refusing to materialize Python artifact {}@{} with mismatched SHA-256",
                entry.name, entry.version
            )));
        }
        Ok(actual)
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

    /// Return the validated basename used for an artifact URL.
    /// Trả basename đã kiểm tra dùng cho URL artifact.
    pub fn artifact_filename_for_url(artifact_url: &str) -> MgResult<String> {
        Self::artifact_filename(artifact_url)
    }

    /// Re-verify a runtime site tree against the RECORD contained in its
    /// digest-authenticated wheel. The RECORD inside `site` is deliberately
    /// ignored because both it and the imported modules can be modified after
    /// installation.
    /// (Xác minh lại cây runtime bằng RECORD trong wheel đã xác thực digest;
    /// không tin RECORD nằm trong site vì có thể bị sửa cùng module.)
    pub fn verify_runtime_materialization(
        wheel_bytes: &[u8],
        expected_name: &str,
        expected_version: &str,
        expected_sha256: &str,
        site: &Path,
    ) -> MgResult<()> {
        if expected_sha256.len() != 64
            || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(MgError::Integrity(
                "Python runtime lock entry has no valid SHA-256 digest".to_string(),
            ));
        }
        let actual_sha256 = super::sha256_hex(wheel_bytes);
        if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
            return Err(MgError::Integrity(
                "cached Python wheel does not match the mgc.lock SHA-256 digest".to_string(),
            ));
        }

        Self::verify_runtime_materialization_contents(
            wheel_bytes,
            expected_name,
            expected_version,
            site,
        )
    }

    fn verify_runtime_materialization_contents(
        wheel_bytes: &[u8],
        expected_name: &str,
        expected_version: &str,
        site: &Path,
    ) -> MgResult<()> {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        use std::collections::{BTreeMap, BTreeSet};

        let site_metadata = std::fs::symlink_metadata(site)?;
        if !site_metadata.file_type().is_dir() {
            return Err(MgError::Integrity(
                "Python runtime site is not a real directory".to_string(),
            ));
        }

        let archive_entries = super::zip_reader::read_zip_entries(wheel_bytes)?;
        // Keep slices into the single decoded entry set. Cloning each
        // payload into the map doubled peak memory for large wheels.
        // (Map mượn slice từ entries; clone payload làm RAM đỉnh tăng gấp đôi.)
        let mut archive_files: BTreeMap<String, &[u8]> = BTreeMap::new();
        let mut record_bytes = None;
        let mut metadata_bytes = None;
        for entry in &archive_entries {
            validate_record_path(&entry.name)?;
            if archive_files
                .insert(entry.name.clone(), entry.data.as_slice())
                .is_some()
            {
                return Err(MgError::Integrity(format!(
                    "Python wheel contains duplicate member '{}'",
                    entry.name
                )));
            }
            if entry.name.ends_with(".dist-info/RECORD")
                && record_bytes.replace(entry.data.as_slice()).is_some()
            {
                return Err(MgError::Integrity(
                    "Python wheel contains multiple dist-info/RECORD files".to_string(),
                ));
            }
            if entry.name.ends_with(".dist-info/METADATA")
                && metadata_bytes.replace(entry.data.as_slice()).is_some()
            {
                return Err(MgError::Integrity(
                    "Python wheel contains multiple dist-info/METADATA files".to_string(),
                ));
            }
        }
        let record = record_bytes.ok_or_else(|| {
            MgError::Integrity("authenticated Python wheel has no dist-info/RECORD".to_string())
        })?;
        let record = std::str::from_utf8(record)
            .map_err(|_| MgError::Integrity("Python wheel RECORD is not UTF-8".to_string()))?;
        let metadata = metadata_bytes.ok_or_else(|| {
            MgError::Integrity("authenticated Python wheel has no dist-info/METADATA".to_string())
        })?;
        verify_wheel_identity(metadata, expected_name, expected_version)?;
        let urlsafe = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let mut expected_files = BTreeSet::new();
        for (line_index, line) in record.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let fields = parse_record_row(line).map_err(|reason| {
                MgError::Integrity(format!(
                    "malformed authenticated wheel RECORD line {}: {reason}",
                    line_index + 1
                ))
            })?;
            let [relative, hash, size] = fields.as_slice() else {
                return Err(MgError::Integrity(format!(
                    "malformed authenticated wheel RECORD line {}",
                    line_index + 1
                )));
            };
            validate_record_path(relative)?;
            let Some(archive_member) = archive_files.get(relative.as_str()) else {
                return Err(MgError::Integrity(format!(
                    "wheel RECORD references absent member '{relative}'"
                )));
            };
            if !expected_files.insert(relative.clone()) {
                return Err(MgError::Integrity(format!(
                    "wheel RECORD repeats member '{relative}'"
                )));
            }
            let target = verified_site_file_path(site, relative)?;
            let data = std::fs::read(&target)?;
            if data.as_slice() != *archive_member {
                return Err(MgError::Integrity(format!(
                    "Python runtime materialization differs from authenticated wheel member '{relative}'"
                )));
            }
            let is_record = relative.ends_with(".dist-info/RECORD");
            if is_record {
                if !hash.is_empty() || !size.is_empty() {
                    return Err(MgError::Integrity(
                        "wheel RECORD must not self-hash its RECORD row".to_string(),
                    ));
                }
                continue;
            }
            let digest = hash.strip_prefix("sha256=").ok_or_else(|| {
                MgError::Integrity(format!(
                    "unsupported or missing wheel RECORD hash for '{relative}'"
                ))
            })?;
            let expected_size: u64 = size.parse().map_err(|_| {
                MgError::Integrity(format!("invalid wheel RECORD size for '{relative}'"))
            })?;
            if data.len() as u64 != expected_size {
                return Err(MgError::Integrity(format!(
                    "Python runtime materialization size mismatch for '{relative}'"
                )));
            }
            let expected_digest = urlsafe.decode(digest).map_err(|_| {
                MgError::Integrity(format!("invalid wheel RECORD digest for '{relative}'"))
            })?;
            let actual_digest: [u8; 32] = Sha256::digest(&data).into();
            if actual_digest.as_slice() != expected_digest.as_slice() {
                return Err(MgError::Integrity(format!(
                    "Python runtime materialization digest mismatch for '{relative}'"
                )));
            }
        }
        if expected_files.len() != archive_files.len()
            || archive_files
                .keys()
                .any(|name| !expected_files.contains(name))
        {
            return Err(MgError::Integrity(
                "authenticated wheel members and RECORD entries do not match".to_string(),
            ));
        }

        let mut actual_files = BTreeSet::new();
        collect_regular_files(site, site, &mut actual_files)?;
        if actual_files != expected_files {
            return Err(MgError::Integrity(
                "Python runtime site contains unrecorded or missing files".to_string(),
            ));
        }
        Ok(())
    }

    /// Whether an artifact can be materialized by the current native
    /// Python runtime (pure-Python wheel only; no build backend or ABI
    /// loader is implemented yet).
    /// (Artifact có thể materialize bởi runtime Python native hiện tại.)
    pub fn is_importable_pure_wheel(artifact_url: &str) -> MgResult<bool> {
        let filename = Self::artifact_filename(artifact_url)?;
        Ok(Self::is_pure_wheel_filename(&filename))
    }

    /// Unpack a PURE-PYTHON wheel into a digest-specific site directory.
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
        let digest = if entry.sha256.is_empty() {
            super::sha256_hex(bytes)
        } else {
            Self::verified_materialization_digest(entry, bytes)?
        };
        let site = wheels_dir
            .join("site")
            .join(Self::importable_site_dirname_for_digest(
                &entry.name,
                &entry.version,
                &digest,
            )?);
        super::zip_reader::extract_zip(bytes, &site)?;
        // Verify archive identity before extraction, then validate the
        // resulting tree against RECORD without hashing the wheel a second time.
        // (Xác minh digest trước khi bung, rồi đối chiếu cây với RECORD không hash lại.)
        Self::verify_runtime_materialization_contents(bytes, &entry.name, &entry.version, &site)?;
        Ok(Some(site))
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

/// Confirm wheel metadata identity before its files enter Python's import path.
/// Xác nhận METADATA khớp tên/version trong lock trước khi thêm PYTHONPATH.
fn verify_wheel_identity(
    metadata: &[u8],
    expected_name: &str,
    expected_version: &str,
) -> MgResult<()> {
    let metadata = std::str::from_utf8(metadata)
        .map_err(|_| MgError::Integrity("Python wheel METADATA is not UTF-8".to_string()))?;
    let mut name = None;
    let mut version = None;
    for line in metadata.lines().take_while(|line| !line.is_empty()) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "name" => {
                if name.replace(value.trim()).is_some() {
                    return Err(MgError::Integrity(
                        "Python wheel METADATA repeats Name".to_string(),
                    ));
                }
            }
            "version" if version.replace(value.trim()).is_some() => {
                return Err(MgError::Integrity(
                    "Python wheel METADATA repeats Version".to_string(),
                ));
            }
            "version" => version = Some(value.trim()),
            _ => {}
        }
    }
    let normalize = |value: &str| {
        let mut normalized = String::with_capacity(value.len());
        let mut separator = false;
        for ch in value.chars() {
            if matches!(ch, '-' | '_' | '.') {
                separator = true;
            } else {
                if separator && !normalized.is_empty() {
                    normalized.push('-');
                }
                separator = false;
                normalized.push(ch.to_ascii_lowercase());
            }
        }
        normalized
    };
    if name.is_none_or(|actual| normalize(actual) != normalize(expected_name))
        || version.is_none_or(|actual| !pep440_release_identity_matches(actual, expected_version))
    {
        return Err(MgError::Integrity(format!(
            "Python wheel METADATA identity does not match locked {}@{}",
            expected_name, expected_version
        )));
    }
    Ok(())
}

/// Compare stable PEP 440 release segments while preserving fail-closed
/// behavior for epochs, pre/post/dev releases, local labels, and malformed
/// strings that this resolver does not model yet. PEP 440 treats trailing
/// zero release segments as equivalent (for example, `26.3 == 26.3.0`).
/// So khớp release segment ổn định theo PEP 440; từ chối cú pháp chưa hỗ trợ.
fn pep440_release_identity_matches(actual: &str, expected: &str) -> bool {
    fn release_parts(value: &str) -> Option<Vec<u64>> {
        let value = value.trim();
        let value = value
            .strip_prefix('v')
            .or_else(|| value.strip_prefix('V'))
            .unwrap_or(value);
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return None;
        }
        let mut parts = value
            .split('.')
            .map(|part| {
                if part.is_empty() {
                    return None;
                }
                part.parse::<u64>().ok()
            })
            .collect::<Option<Vec<_>>>()?;
        while parts.last() == Some(&0) && parts.len() > 1 {
            parts.pop();
        }
        Some(parts)
    }

    match (release_parts(actual), release_parts(expected)) {
        (Some(actual), Some(expected)) => actual == expected,
        _ => false,
    }
}

impl Default for PypiProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Parse the three CSV fields in a RECORD row, honoring quoted commas and
/// escaped quotes while rejecting multiline/malformed fields.
/// Parse ba cột CSV của RECORD, hỗ trợ dấu phẩy/quote escape.
fn parse_record_row(line: &str) -> Result<Vec<String>, &'static str> {
    let mut fields = Vec::with_capacity(3);
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    let mut closed_quote = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed_quote = true;
                }
            }
            '"' if field.is_empty() && !closed_quote => quoted = true,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut field));
                closed_quote = false;
                if fields.len() >= 3 {
                    return Err("expected exactly three CSV fields");
                }
            }
            _ if quoted => field.push(ch),
            _ if closed_quote => return Err("unexpected data after quoted CSV field"),
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err("unterminated quoted CSV field");
    }
    fields.push(field);
    if fields.len() != 3 {
        return Err("expected exactly three CSV fields");
    }
    Ok(fields)
}

/// Wheel member paths are POSIX-relative by specification. Reject every
/// path that could escape the extraction root before joining it locally.
/// Path wheel theo POSIX phải tương đối; từ chối traversal trước khi join.
fn validate_record_path(relative: &str) -> MgResult<()> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with('\\')
        || relative.contains('\\')
        || relative.contains(':')
        || relative.contains('\0')
        || relative
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(MgError::Integrity(format!(
            "unsafe Python wheel member path '{relative}'"
        )));
    }
    Ok(())
}

/// Check every component with no-follow metadata before reading the file.
/// Kiểm tra từng thành phần không theo symlink trước khi đọc file.
fn verified_site_file_path(site: &Path, relative: &str) -> MgResult<PathBuf> {
    let components: Vec<_> = relative.split('/').collect();
    let mut current = site.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current).map_err(|error| {
            MgError::Integrity(format!(
                "Python runtime materialization is missing '{relative}': {error}"
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(MgError::Integrity(format!(
                "Python runtime materialization contains a symlink in '{relative}'"
            )));
        }
        let is_last = index + 1 == components.len();
        if (is_last && !metadata.file_type().is_file())
            || (!is_last && !metadata.file_type().is_dir())
        {
            return Err(MgError::Integrity(format!(
                "Python runtime materialization has an invalid path component in '{relative}'"
            )));
        }
    }
    Ok(current)
}

/// Enumerate regular files without following symlinks so site contents can
/// be compared exactly with the authenticated wheel RECORD.
/// Liệt kê file thường không theo symlink để đối chiếu chính xác với RECORD.
fn collect_regular_files(
    root: &Path,
    current: &Path,
    files: &mut std::collections::BTreeSet<String>,
) -> MgResult<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(MgError::Integrity(format!(
                "Python runtime site contains symlink '{}'",
                path.display()
            )));
        }
        if metadata.file_type().is_dir() {
            collect_regular_files(root, &path, files)?;
        } else if metadata.file_type().is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                MgError::Integrity("Python runtime site path escaped its root".to_string())
            })?;
            let relative = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(relative);
        } else {
            return Err(MgError::Integrity(format!(
                "Python runtime site contains special file '{}'",
                path.display()
            )));
        }
    }
    Ok(())
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
                    let version_body = self.get_text(&version_url).await.map_err(|error| {
                        MgError::Network(format!(
                            "cannot determine Python compatibility for {name} {candidate}: {error}"
                        ))
                    })?;
                    let version_doc: PypiJson = serde_json::from_str(&version_body).map_err(|error| {
                        MgError::Other(format!(
                            "parse Python compatibility metadata for {name} {candidate}: {error}"
                        ))
                    })?;
                    let required = version_doc.info.requires_python.clone().unwrap_or_default();
                    if required.trim().is_empty() || requires_python_allows(&required, major, minor)
                    {
                        if select_file(&candidate_files, consumer).is_none() {
                            last_excluded = Some(format!(
                                "{candidate} has no artifact compatible with Python {major}.{minor}"
                            ));
                            continue;
                        }
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
        let file = select_file(&files, consumer)
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
                    return Err(MgError::Other(format!(
                        "invalid PEP 508 Requires-Dist entry for {name} {version}: '{spec}'; refusing to omit it from the dependency graph"
                    )));
                };
                // A Requires-Dist entry guarded by `extra == ...` belongs
                // to an opt-in extra of the package being resolved. This
                // resolver has no requested-extra input, so it must record
                // and exclude that optional edge rather than reject a base
                // install because the optional dependency itself has extras.
                // (Requires-Dist có marker extra là cạnh tùy chọn; resolver
                // chưa nhận extra được yêu cầu nên ghi nhận và bỏ qua cạnh.)
                if marker.as_deref().is_some_and(|m| m.contains("extra")) {
                    markers.push(format!("marker:{}", marker.unwrap_or_default()));
                    continue;
                }
                if requires_requested_extras(&spec) {
                    return Err(MgError::Unsupported {
                        core: "python",
                        capability: "dependency extras",
                        guidance: format!(
                            "{name} {version} requires '{spec}', but extra propagation is not implemented; refusing to resolve an incomplete graph"
                        ),
                    });
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
fn select_file(files: &[PypiFile], consumer: Option<(u64, u64)>) -> Option<&PypiFile> {
    let compatible = |file: &&PypiFile| file_requires_python_allows(file, consumer);
    if let Some(f) = files
        .iter()
        .filter(compatible)
        .find(|f| is_universal_wheel(f))
    {
        return Some(f);
    }
    if let Some(f) = files
        .iter()
        .filter(compatible)
        .find(|f| f.packagetype == "bdist_wheel" && wheel_platform_matches_host(&f.filename))
    {
        return Some(f);
    }
    files
        .iter()
        .filter(compatible)
        .find(|f| f.packagetype == "sdist")
}

/// Ignore a file whose `Requires-Python` excludes the selected consumer.
/// Keep the historical unresolved-runtime behavior until runtime discovery is
/// wired into the project resolver rather than inventing a target version.
/// Bỏ file có `Requires-Python` loại interpreter mục tiêu; khi chưa biết runtime,
/// giữ hành vi cũ thay vì tự bịa phiên bản đích.
fn file_requires_python_allows(file: &PypiFile, consumer: Option<(u64, u64)>) -> bool {
    match (file.requires_python.as_deref(), consumer) {
        (Some(required), Some((major, minor))) => {
            required.trim().is_empty() || requires_python_allows(required, major, minor)
        }
        _ => true,
    }
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

/// Detect extras requested on a dependency name before PEP 508 parsing.
/// Dependency extras change that dependency's own transitive graph and cannot
/// be discarded without producing an incomplete resolution.
/// Phát hiện extras được yêu cầu trên tên dependency trước khi parse PEP 508.
/// Extras làm thay đổi graph bắc cầu của dependency đó nên không thể bỏ qua.
fn requires_requested_extras(spec: &str) -> bool {
    let requirement = spec
        .split_once(';')
        .map_or(spec, |(requirement, _)| requirement)
        .trim();
    let name_end = requirement
        .find(|c: char| ['=', '>', '<', '!', '~'].contains(&c))
        .unwrap_or(requirement.len());
    let name = requirement[..name_end].trim();
    name.contains('[') || name.contains(']')
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
    fn wheel_identity_accepts_pep440_equivalent_trailing_zero_release() {
        let metadata = b"Metadata-Version: 2.4\nName: packaging\nVersion: 26.3\n\n";
        assert!(verify_wheel_identity(metadata, "packaging", "26.3.0").is_ok());
    }

    #[test]
    fn wheel_identity_rejects_unsupported_or_different_versions() {
        let equivalent = b"Metadata-Version: 2.4\nName: packaging\nVersion: 26.3.0.0\n\n";
        assert!(verify_wheel_identity(equivalent, "packaging", "26.3.0").is_ok());

        for actual in ["26.3.1", "26.3rc1", "26.3.post1", "26.3+vendor", "26..3"] {
            let metadata = format!("Metadata-Version: 2.4\nName: packaging\nVersion: {actual}\n\n");
            assert!(
                verify_wheel_identity(metadata.as_bytes(), "packaging", "26.3.0").is_err(),
                "accepted {actual}"
            );
        }
    }

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
            let incompatible_file = PypiFile {
                filename: "demo-1.0.0-py3-none-any.whl".to_string(),
                url: "https://files.pythonhosted.org/demo.whl".to_string(),
                digests: PypiDigests::default(),
                packagetype: "bdist_wheel".to_string(),
                requires_python: Some(">=3.10".to_string()),
            };
            assert!(
                select_file(std::slice::from_ref(&incompatible_file), consumer_python()).is_none(),
                "a wheel-specific Requires-Python constraint must reject this artifact"
            );
            let compatible_file = PypiFile {
                requires_python: Some(">=3.9".to_string()),
                ..incompatible_file
            };
            assert_eq!(
                select_file(std::slice::from_ref(&compatible_file), consumer_python())
                    .map(|file| file.filename.as_str()),
                Some(compatible_file.filename.as_str())
            );
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
        let meta = b"Name: six\nVersion: 1.17.0\n";
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

    #[test]
    fn runtime_verification_uses_authenticated_wheel_record_not_mutable_site_record() {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let six_py = b"VALUE = 'trusted'\n";
        let meta = b"Name: six\nVersion: 1.17.0\n";
        let record = format!(
            "six.py,sha256={},{}\nsix-1.17.0.dist-info/METADATA,sha256={},{}\nsix-1.17.0.dist-info/RECORD,,\n",
            b64.encode(Sha256::digest(six_py)),
            six_py.len(),
            b64.encode(Sha256::digest(meta)),
            meta.len(),
        );
        let wheel = stored_zip(&[
            ("six.py", six_py.as_slice()),
            ("six-1.17.0.dist-info/METADATA", meta.as_slice()),
            ("six-1.17.0.dist-info/RECORD", record.as_bytes()),
        ]);
        let digest = super::super::sha256_hex(&wheel);
        let dir = tempfile::tempdir().unwrap();
        let site = PypiProtocol::new("https://pypi.org")
            .materialize_importable(
                &entry_for("https://files.pythonhosted.org/six-1.17.0-py3-none-any.whl"),
                &wheel,
                dir.path(),
            )
            .unwrap()
            .unwrap();

        PypiProtocol::verify_runtime_materialization(&wheel, "six", "1.17.0", &digest, &site)
            .unwrap();
        // The old normalization erased separators and treated `six` and
        // `s-ix` as identical; PEP 503 keeps one canonical hyphen.
        // (Chuẩn hóa cũ xóa dấu phân cách khiến `six` trùng `s-ix`.)
        assert!(
            PypiProtocol::verify_runtime_materialization(&wheel, "s-ix", "1.17.0", &digest, &site)
                .is_err()
        );
        assert!(
            PypiProtocol::verify_runtime_materialization(
                &wheel,
                "attacker-package",
                "1.17.0",
                &digest,
                &site
            )
            .is_err()
        );

        std::fs::write(site.join("six.py"), b"VALUE = 'attacker-controlled'\n").unwrap();
        // Rewriting the extracted RECORD must not bless the changed module.
        let changed = b"VALUE = 'attacker-controlled'\n";
        let forged_record = format!(
            "six.py,sha256={},{}\nsix-1.17.0.dist-info/METADATA,sha256={},{}\nsix-1.17.0.dist-info/RECORD,,\n",
            b64.encode(Sha256::digest(changed)),
            changed.len(),
            b64.encode(Sha256::digest(meta)),
            meta.len(),
        );
        std::fs::write(site.join("six-1.17.0.dist-info/RECORD"), forged_record).unwrap();
        assert!(
            PypiProtocol::verify_runtime_materialization(&wheel, "six", "1.17.0", &digest, &site)
                .is_err()
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
