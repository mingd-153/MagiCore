//! Go module proxy native engine (Go modules).
//! Engine native Go module proxy (Go modules).
//!
//! GOPROXY HTTP spec: version list at `{proxy}/{module}/@v/list` (one `vX.Y.Z`
//! per line, pseudo-versions `v0.0.0-…` excluded), `@latest` fallback, and
//! per-version `@v/{version}.info` (JSON `{Version,Time}`), `@v/{version}.mod`
//! (go.mod text) and `@v/{version}.zip` (module zip). Integrity: the proxy's
//! `{version}.ziphash` (hex sha256 of the zip bytes) is the primary check;
//! when the proxy omits it, the checksum database record at
//! `{sum}/lookup/{module}@{version}` is fetched and the zip is verified via
//! the sumdb's `h1:` directory hash (sha256 over sorted per-file
//! `"<file-sha256-hex>  <name>\n"` lines) — the honest equivalent of
//! `golang.org/x/mod/sumdb/dirhash.HashZip`. Fail-closed when neither source
//! can verify.
//! Spec GOPROXY HTTP: danh sách version tại `{proxy}/{module}/@v/list` (mỗi
//! dòng `vX.Y.Z`, bỏ pseudo-version `v0.0.0-…`), fallback `@latest`, và
//! theo version có `@v/{version}.info` (JSON `{Version,Time}`),
//! `@v/{version}.mod` (nội dung go.mod) và `@v/{version}.zip` (zip module).
//! Toàn vẹn: `{version}.ziphash` của proxy (hex sha256 của byte zip) là kiểm
//! tra chính; khi proxy không cung cấp, lấy bản ghi checksum database tại
//! `{sum}/lookup/{module}@{version}` và xác minh zip bằng directory hash
//! `h1:` của sumdb (sha256 trên các dòng `"<file-sha256-hex>  <name>\n"` đã
//! sắp xếp) — tương đương trung thực của
//! `golang.org/x/mod/sumdb/dirhash.HashZip`. Fail-closed khi không nguồn nào
//! xác minh được.

use super::zip_reader;
use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use base64::Engine as _;
use mgc_types::{MgError, MgResult, Version};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const DEFAULT_PROXY_URL: &str = "https://proxy.golang.org";
const DEFAULT_SUM_URL: &str = "https://sum.golang.org";

/// Native Go module proxy engine.
/// Engine native Go module proxy.
#[derive(Debug, Clone)]
pub struct GoModProtocol {
    proxy_url: String,
    sum_url: String,
    client: mgc_http::HttpClient,
}

/// Downloaded artifacts of one module version (zip + go.mod + .info).
/// Các artifact đã tải của một version module (zip + go.mod + .info).
#[derive(Debug, Clone)]
pub struct GoModuleFiles {
    pub zip: Vec<u8>,
    pub gomod: Vec<u8>,
    pub info: Vec<u8>,
}

impl GoModProtocol {
    /// Build with an explicit proxy URL (default sumdb base).
    /// Dựng với proxy URL tường minh (sumdb base mặc định).
    pub fn new(proxy_url: &str) -> Self {
        Self::with_sum_base(proxy_url, DEFAULT_SUM_URL)
    }

    /// Build with explicit proxy + sumdb base (testability).
    /// Dựng với proxy + sumdb base tường minh (cho test).
    pub fn with_sum_base(proxy_url: &str, sum_url: &str) -> Self {
        Self {
            proxy_url: proxy_url.trim_end_matches('/').to_string(),
            sum_url: sum_url.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
        }
    }

    /// Build from environment: `MGC_GO_PROXY_URL` / `MGC_GO_SUM_URL`.
    /// Dựng từ môi trường: `MGC_GO_PROXY_URL` / `MGC_GO_SUM_URL`.
    pub fn from_env() -> Self {
        let proxy_url = std::env::var("MGC_GO_PROXY_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_PROXY_URL.to_string());
        let sum_url = std::env::var("MGC_GO_SUM_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_SUM_URL.to_string());
        Self::with_sum_base(&proxy_url, &sum_url)
    }

    async fn get_text(&self, url: &str) -> MgResult<(u16, String)> {
        let resp = self
            .client
            .get(url)
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        Ok((status, body))
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

    /// `{proxy}/{escaped-module}/@v/{file}` — module paths use the proxy
    /// escape (uppercase → `!lowercase`).
    /// `{proxy}/{module-escaped}/@v/{file}` — path module dùng escape của
    /// proxy (chữ hoa → `!chữ_thường`).
    fn proxy_url_for(&self, module: &str, file: &str) -> String {
        format!(
            "{}/{}/@v/{}",
            self.proxy_url,
            escape_module_path(module),
            file
        )
    }

    fn sum_lookup_url(&self, module: &str, version: &str) -> String {
        format!(
            "{}/lookup/{}@{}",
            self.sum_url,
            escape_module_path(module),
            version
        )
    }

    /// Fetch a `.ziphash` (hex sha256 of the zip). `Ok(None)` = the proxy
    /// does not publish one (404) — caller falls back to the sumdb.
    /// Tải `.ziphash` (hex sha256 của zip). `Ok(None)` = proxy không công bố
    /// (404) — caller fallback sang sumdb.
    async fn fetch_ziphash(&self, module: &str, version: &str) -> MgResult<Option<String>> {
        let url = self.proxy_url_for(module, &format!("{version}.ziphash"));
        let (status, body) = self.get_text(&url).await?;
        if status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!("GET {url} returned {status}")));
        }
        let hash = body.trim();
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            // A malformed proxy hash is a fail-closed integrity error, never
            // silently ignored.
            // (Hash proxy sai định dạng là lỗi integrity fail-closed, không
            // bao giờ bỏ qua âm thầm.)
            return Err(MgError::Integrity(format!(
                "malformed .ziphash for {module}@{version}: '{hash}'"
            )));
        }
        Ok(Some(hash.to_ascii_lowercase()))
    }

    /// Sumdb record for the zip: parse the lookup response and return the
    /// `h1:` directory hash of the module zip. The REAL sumdb lookup body
    /// is space-separated tokens — `<module> <version> <zip-h1>` on its
    /// own line (the `/go.mod` line carries `<version>/go.mod` as its
    /// second token and never matches). An earlier revision matched
    /// `{module}@{version}` and rejected EVERY genuine response.
    /// Bản ghi sumdb cho zip: parse response lookup và trả directory hash
    /// `h1:` của zip module. Body sumdb thật là các token cách nhau bằng
    /// dấu cách.
    async fn fetch_sumdb_ziphash(&self, module: &str, version: &str) -> MgResult<String> {
        let url = self.sum_lookup_url(module, version);
        let (status, body) = self.get_text(&url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "sumdb lookup {module}@{version} returned {status} — artifact cannot be verified (fail-closed)"
            )));
        }
        for line in body.lines() {
            let mut tokens = line.split_whitespace();
            match (tokens.next(), tokens.next(), tokens.next()) {
                (Some(record_module), Some(record_version), Some(zip_hash))
                    if record_module == module && record_version == version =>
                {
                    if !zip_hash.starts_with("h1:") {
                        return Err(MgError::Integrity(format!(
                            "sumdb zip hash for {module}@{version} is not an h1 hash: '{zip_hash}'"
                        )));
                    }
                    return Ok(zip_hash.to_string());
                }
                _ => {}
            }
        }
        Err(MgError::Integrity(format!(
            "sumdb lookup response has no record for {module}@{version} (fail-closed)"
        )))
    }

    /// Download the full module artifact set (zip + go.mod + .info) for an
    /// entry — the materializer needs all three for a GOPROXY=off cache.
    /// Tải trọn bộ artifact của một version module (zip + go.mod + .info) —
    /// materializer cần cả ba cho cache GOPROXY=off.
    pub async fn download_module(&self, entry: &ResolvedEntry) -> MgResult<GoModuleFiles> {
        let version = format!("v{}", entry.version.trim_start_matches('v'));
        let zip = self.get_bytes(&entry.artifact_url).await?;
        let gomod = self
            .get_bytes(&self.proxy_url_for(&entry.name, &format!("{version}.mod")))
            .await?;
        let info = self
            .get_bytes(&self.proxy_url_for(&entry.name, &format!("{version}.info")))
            .await?;
        Ok(GoModuleFiles { zip, gomod, info })
    }

    /// Materialize a module into the go download cache layout:
    /// `{gomodcache}/cache/download/{escaped-module}/@v/{version}.{zip,mod,info,ziphash}`
    /// — readable with `GOPROXY=off`. The `.ziphash` is the REAL sha256 of
    /// the stored zip bytes (computed, not copied).
    /// Materialize module vào layout go download cache:
    /// `{gomodcache}/cache/download/{module-escaped}/@v/{version}.{zip,mod,info,ziphash}`
    /// — đọc được với `GOPROXY=off`. `.ziphash` là sha256 THẬT của byte zip
    /// đã lưu (tính toán, không copy).
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        files: &GoModuleFiles,
        gomodcache: &Path,
    ) -> MgResult<PathBuf> {
        let version = format!("v{}", entry.version.trim_start_matches('v'));
        let dir = gomodcache
            .join("cache")
            .join("download")
            .join(escape_module_path(&entry.name))
            .join("@v");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{version}.zip")), &files.zip)?;
        std::fs::write(dir.join(format!("{version}.mod")), &files.gomod)?;
        std::fs::write(dir.join(format!("{version}.info")), &files.info)?;
        std::fs::write(
            dir.join(format!("{version}.ziphash")),
            super::sha256_hex(&files.zip),
        )?;
        Ok(dir.join(format!("{version}.zip")))
    }
    /// Resolve one KNOWN version end to end (artifact URL → .info time →
    /// .mod graph → integrity). Shared by the list flow and the
    /// exact-pin fast path; `version` is raw without a `v` prefix.
    /// (Resolve một version ĐÃ BIẾT trọn vẹn.)
    async fn resolve_version(&self, module: &str, version: &str) -> MgResult<ResolvedEntry> {
        let version = version.to_string();
        let artifact_url = self.proxy_url_for(module, &format!("v{version}.zip"));

        let mut markers = Vec::new();

        // `.info` → release time marker (provenance).
        // (`.info` → marker thời gian phát hành (provenance).)
        let info_url = self.proxy_url_for(module, &format!("v{version}.info"));
        let (info_status, info_body) = self.get_text(&info_url).await?;
        if (200..300).contains(&info_status)
            && let Ok(info) = serde_json::from_str::<GoInfo>(&info_body)
            && !info.time.is_empty()
        {
            markers.push(format!("time:{}", info.time));
        }

        // `.mod` → the module's own requirements (with replace/exclude and
        // `// indirect` notes applied honestly).
        // (`.mod` → requirement của chính module (áp replace/exclude và ghi
        // chú `// indirect` trung thực).)
        let mod_url = self.proxy_url_for(module, &format!("v{version}.mod"));
        let (mod_status, mod_body) = self.get_text(&mod_url).await?;
        let mut deps = Vec::new();
        if (200..300).contains(&mod_status) {
            let gomod = parse_go_mod(&mod_body)?;
            for req in &gomod.requires {
                if req.indirect {
                    markers.push(format!("indirect:{}", req.path));
                }
                // `exclude` in the dependency's go.mod removes that exact
                // (module, version) pair — record, never select it.
                // (`exclude` trong go.mod của dependency loại cặp (module,
                // version) đúng đó — ghi lại, không bao giờ chọn.)
                if gomod
                    .excludes
                    .iter()
                    .any(|(p, v)| p == &req.path && v.trim_start_matches('v') == req.version)
                {
                    markers.push(format!("excluded:{}@{}", req.path, req.version));
                    continue;
                }
                // `replace` rewrites the requirement when the module path
                // (and pinned version, when present) matches.
                // (`replace` viết lại requirement khi path module (và version
                // ghim, nếu có) khớp.)
                let mut dep_path = req.path.clone();
                let mut dep_version = req.version.clone();
                for rep in &gomod.replaces {
                    let version_ok = rep
                        .old_version
                        .as_ref()
                        .is_none_or(|v| v.trim_start_matches('v') == dep_version);
                    if rep.old_path == dep_path && version_ok {
                        markers.push(format!("replace:{}=>{}", rep.old_path, rep.new_path));
                        dep_path = rep.new_path.clone();
                        if let Some(nv) = &rep.new_version {
                            dep_version = nv.clone();
                        }
                        break;
                    }
                }
                deps.push((dep_path, dep_version.trim_start_matches('v').to_string()));
            }
        } else {
            return Err(MgError::Network(format!(
                "GET {mod_url} returned {mod_status} — go.mod is required to build the graph (fail-closed)"
            )));
        }

        // Integrity: proxy `.ziphash` first, sumdb record second.
        // (Toàn vẹn: `.ziphash` của proxy trước, bản ghi sumdb sau.)
        let sha256 = match self.fetch_ziphash(module, &format!("v{version}")).await? {
            Some(hash) => hash,
            None => {
                let zip_hash = self
                    .fetch_sumdb_ziphash(module, &format!("v{version}"))
                    .await?;
                markers.push(format!("sumdb-ziphash:{zip_hash}"));
                String::new()
            }
        };

        Ok(ResolvedEntry {
            name: module.to_string(),
            version,
            deps,
            artifact_url,
            sha256,
            extra_markers: markers,
        })
    }

    /// Bare exact pin from a range string (`1.2.3`, `=1.2.3`,
    /// `v0.0.0-20161208181325-20d25e280405`) — anything with operators,
    /// wildcards, commas, spaces or `||` is NOT exact. Returns the raw
    /// version WITHOUT a `v` prefix, or `None`.
    /// (Pin chính xác dạng trần từ chuỗi range.)
    fn exact_pin_version(range: &str) -> Option<String> {
        let mut raw = range.trim();
        raw = raw.strip_prefix('=').unwrap_or(raw);
        raw = raw.strip_prefix('v').unwrap_or(raw);
        if raw.is_empty() || raw.contains([',', '|', ' ', '\t', '*', '<', '>', '^', '~', '=', '!'])
        {
            return None;
        }
        // Must parse (validates shape, pseudo-version pres included) —
        // and re-serializing must not be needed since we keep it raw.
        // (Phải parse được — giữ chuỗi thô.)
        Version::parse(raw).ok()?;
        Some(raw.to_string())
    }

    /// Does `{module}@v{version}` exist on the proxy (via its `.info`)?
    /// 404/other non-2xx = absent (fall back to list flow), transport
    /// errors propagate.
    /// (Version ghim có tồn tại trên proxy không (qua `.info`)?)
    async fn direct_pin_exists(&self, module: &str, version: &str) -> MgResult<bool> {
        let url = self.proxy_url_for(module, &format!("v{version}.info"));
        let (status, _) = self.get_text(&url).await?;
        Ok((200..300).contains(&status))
    }
}

impl Default for GoModProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl RegistryProtocol for GoModProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let module = name.trim();
        // Exact-pin fast path (mirrors `go get module@version`): a bare
        // version (no operators/wildcards — including old-style
        // pseudo-versions like `0.0.0-20161208181325-20d25e280405` that
        // `@v/list` omits) is fetched DIRECTLY via `@v/<version>.info`
        // instead of failing against an incomplete list. A 404 falls
        // through to the list flow below.
        // (Pin chính xác: tải trực tiếp `.info`, không qua list.)
        if let Some(pinned) = Self::exact_pin_version(range)
            && self.direct_pin_exists(module, &pinned).await?
        {
            return self.resolve_version(module, &pinned).await;
        }
        // Version list first; on a 404 fall back to `@latest` (modules with
        // only one released version may have no cached list).
        // (Danh sách version trước; 404 thì fallback `@latest` — module chỉ
        // có một bản phát hành có thể chưa có list cache.)
        let list_url = self.proxy_url_for(module, "list");
        let (status, body) = self.get_text(&list_url).await?;
        let mut best: Option<(Version, String)> = None;
        if (200..300).contains(&status) {
            for line in body.lines() {
                let v = line.trim().trim_start_matches('v');
                // Pseudo-versions (v0.0.0-<timestamp>-<rev>) are never
                // selected — a registry release is required.
                // (Pseudo-version không bao giờ được chọn — bắt buộc bản
                // phát hành registry.)
                if line.trim().starts_with("v0.0.0-") || v.is_empty() {
                    continue;
                }
                let Ok(version) = Version::parse(v) else {
                    continue;
                };
                if !go_matches(range, &version) {
                    continue;
                }
                if best.as_ref().is_none_or(|(bv, _)| version > *bv) {
                    best = Some((version, v.to_string()));
                }
            }
        }
        if best.is_none() {
            let latest_url = self.proxy_url_for(module, "latest");
            let (latest_status, latest_body) = self.get_text(&latest_url).await?;
            if (200..300).contains(&latest_status) {
                let latest: GoLatest = serde_json::from_str(&latest_body)
                    .map_err(|e| MgError::Other(format!("parse go @latest failed: {e}")))?;
                let v = latest.version.trim_start_matches('v').to_string();
                if let Ok(version) = Version::parse(&v)
                    && go_matches(range, &version)
                {
                    best = Some((version, v));
                }
            }
        }
        let (_, version) = best.ok_or_else(|| {
            MgError::Other(format!(
                "no version of module {module} matches range '{range}'"
            ))
        })?;
        self.resolve_version(module, &version).await
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        self.get_bytes(&entry.artifact_url).await
    }

    /// Verify overrides the shared default: sha256 (proxy .ziphash) when
    /// declared, otherwise the sumdb `h1:` directory hash recorded in the
    /// entry markers, otherwise fail-closed — a module with NO verifiable
    /// integrity source is never installed.
    /// (Verify ghi đè mặc định: sha256 (proxy .ziphash) khi có, ngược lại
    /// directory hash `h1:` của sumdb ghi trong marker, ngược lại fail-closed
    /// — module KHÔNG có nguồn toàn vẹn xác minh được không bao giờ được cài.)
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        if !entry.sha256.is_empty() {
            let actual = super::sha256_hex(bytes);
            if !actual.eq_ignore_ascii_case(&entry.sha256) {
                return Err(MgError::Integrity(format!(
                    "sha256 mismatch for {}@{}: expected {}, got {}",
                    entry.name, entry.version, entry.sha256, actual
                )));
            }
            return Ok(());
        }
        let marker = entry
            .extra_markers
            .iter()
            .find(|m| m.starts_with("sumdb-ziphash:h1:"))
            .ok_or_else(|| {
                MgError::Integrity(format!(
                    "no integrity source for {}@{} (no .ziphash, no sumdb record) — fail-closed",
                    entry.name, entry.version
                ))
            })?;
        let expected = marker.trim_start_matches("sumdb-ziphash:");
        let actual = dirhash_zip(&zip_reader::read_zip_entries(bytes)?)?;
        if actual != expected {
            return Err(MgError::Integrity(format!(
                "sumdb dirhash mismatch for {}@{}: expected {expected}, got {actual}",
                entry.name, entry.version
            )));
        }
        Ok(())
    }
}

/// Go version-range subset: `*`, bare version = EXACT (go.mod pins are
/// minimum-version-selected already — the pin is the lock), plus `^`, `~`,
/// `=`, `>=`, `<=`, `>`, `<` (comma = AND).
/// Subset khoảng version Go: `*`, version trần = CHÍNH XÁC (pin go.mod đã
/// qua minimum-version-selection — pin chính là lock), cộng `^`, `~`, `=`,
/// `>=`, `<=`, `>`, `<` (phẩy = AND).
fn go_matches(range: &str, version: &Version) -> bool {
    let range = range.trim().trim_start_matches('v');
    if range.is_empty() || range == "*" {
        return true;
    }
    range
        .split(',')
        .map(str::trim)
        .all(|part| go_part(part, version))
}

fn go_part(part: &str, version: &Version) -> bool {
    if part == "*" {
        return true;
    }
    if let Some(p) = part.strip_prefix(">=") {
        return Version::parse(p).map(|t| version >= &t).unwrap_or(false);
    }
    if let Some(p) = part.strip_prefix("<=") {
        return Version::parse(p).map(|t| version <= &t).unwrap_or(false);
    }
    if let Some(p) = part.strip_prefix('>') {
        return Version::parse(p).map(|t| version > &t).unwrap_or(false);
    }
    if let Some(p) = part.strip_prefix('<') {
        return Version::parse(p).map(|t| version < &t).unwrap_or(false);
    }
    if let Some(p) = part.strip_prefix('^') {
        return go_caret(p, version);
    }
    if let Some(p) = part.strip_prefix('~') {
        return go_tilde(p, version);
    }
    if let Some(p) = part.strip_prefix('=') {
        return Version::parse(p).map(|t| version == &t).unwrap_or(false);
    }
    // Bare version = exact pin (go.mod semantics).
    // (Version trần = ghim chính xác (ngữ nghĩa go.mod).)
    Version::parse(part).map(|t| version == &t).unwrap_or(false)
}

fn go_caret(raw: &str, version: &Version) -> bool {
    let Ok(target) = Version::parse(raw.trim()) else {
        return false;
    };
    if version < &target {
        return false;
    }
    if target.major > 0 {
        version.major == target.major
    } else if target.minor > 0 {
        version.major == 0 && version.minor == target.minor
    } else {
        version.major == 0 && version.minor == 0 && version.patch == target.patch
    }
}

fn go_tilde(raw: &str, version: &Version) -> bool {
    let Ok(target) = Version::parse(raw.trim()) else {
        return false;
    };
    version >= &target && version.major == target.major && version.minor == target.minor
}

/// Proxy module-path escape: every uppercase ASCII letter becomes
/// `!` + lowercase (cache/download and sumdb paths).
/// Escape path module của proxy: mọi chữ hoa ASCII thành `!` + chữ thường
/// (đường dẫn cache/download và sumdb).
fn escape_module_path(module: &str) -> String {
    let mut out = String::with_capacity(module.len());
    for c in module.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Sumdb `h1:` directory hash over a module zip (dirhash.HashZip):
/// sha256 of the sorted per-file lines `"<file-sha256-hex>  <name>\n"`,
/// base64-std encoded behind the `h1:` tag.
/// Directory hash `h1:` của sumdb trên zip module (dirhash.HashZip): sha256
/// của các dòng `"<file-sha256-hex>  <name>\n"` đã sắp xếp theo tên file,
/// mã hóa base64-std sau tag `h1:`.
pub fn dirhash_zip(entries: &[zip_reader::ZipEntry]) -> MgResult<String> {
    let mut files: Vec<&zip_reader::ZipEntry> = entries.iter().collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let mut hasher = Sha256::new();
    for entry in files {
        let file_hash = Sha256::digest(&entry.data);
        hasher.update(format!("{:x}  {}\n", file_hash, entry.name).as_bytes());
    }
    let digest = hasher.finalize();
    Ok(format!(
        "h1:{}",
        base64::engine::general_purpose::STANDARD.encode(digest)
    ))
}

/// Parsed go.mod: requirements, replacements and exclusions.
/// go.mod đã parse: requirement, replace và exclude.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoModFile {
    pub requires: Vec<GoRequire>,
    pub replaces: Vec<GoReplace>,
    pub excludes: Vec<(String, String)>,
}

/// One `require` line — `// indirect` captured as a flag, not a dep.
/// Một dòng `require` — `// indirect` ghi thành flag, không phải dep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoRequire {
    pub path: String,
    pub version: String,
    pub indirect: bool,
}

/// One `replace` directive: `old [oldv] => new [newv]`.
/// Một directive `replace`: `old [oldv] => new [newv]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoReplace {
    pub old_path: String,
    pub old_version: Option<String>,
    pub new_path: String,
    pub new_version: Option<String>,
}

/// Parse go.mod text (line-based, comment-aware): single-line and
/// parenthesized `require` / `replace` / `exclude` blocks. `module`/`go`/
/// `toolchain`/`godebug` directives are ignored.
/// Parse nội dung go.mod (theo dòng, hiểu comment): khối `require` /
/// `replace` / `exclude` một dòng và trong ngoặc. Bỏ qua directive
/// `module`/`go`/`toolchain`/`godebug`.
pub fn parse_go_mod(content: &str) -> MgResult<GoModFile> {
    let mut file = GoModFile::default();
    #[derive(PartialEq, Eq, Clone, Copy)]
    enum Block {
        None,
        Require,
        Replace,
        Exclude,
    }
    let mut block = Block::None;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        let indirect = trimmed.contains("// indirect");
        // Strip trailing comments before tokenizing.
        // (Cắt comment cuối trước khi tách token.)
        let line = trimmed.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            if block != Block::None && trimmed.starts_with(')') {
                block = Block::None;
            }
            continue;
        }
        if block == Block::None {
            if let Some(rest) = line.strip_prefix("require") {
                let rest = rest.trim();
                if rest == "(" {
                    block = Block::Require;
                } else {
                    push_require(&mut file, rest, indirect)?;
                }
            } else if let Some(rest) = line.strip_prefix("replace") {
                let rest = rest.trim();
                if rest == "(" {
                    block = Block::Replace;
                } else {
                    push_replace(&mut file, rest)?;
                }
            } else if let Some(rest) = line.strip_prefix("exclude") {
                let rest = rest.trim();
                if rest == "(" {
                    block = Block::Exclude;
                } else {
                    push_exclude(&mut file, rest)?;
                }
            }
            continue;
        }
        // Inside a block: `)` closes, anything else is a directive body.
        // (Trong khối: `)` đóng, còn lại là thân directive.)
        if trimmed.starts_with(')') {
            block = Block::None;
            continue;
        }
        match block {
            Block::Require => push_require(&mut file, line, indirect)?,
            Block::Replace => push_replace(&mut file, line)?,
            Block::Exclude => push_exclude(&mut file, line)?,
            Block::None => {}
        }
    }
    Ok(file)
}

fn push_require(file: &mut GoModFile, body: &str, indirect: bool) -> MgResult<()> {
    let mut parts = body.split_whitespace();
    let Some(path) = parts.next() else {
        return Ok(());
    };
    let Some(version) = parts.next() else {
        return Err(MgError::Other(format!(
            "go.mod require without version: '{body}'"
        )));
    };
    // Quoted module paths are legal go.mod (`require "gopkg.in/check.v1" vX`
    // — gopkg.in/yaml.v3 does this) — strip the quotes or every downstream
    // URL carries them and 404s.
    // (Path module có quote là hợp lệ trong go.mod — cắt quote.)
    file.requires.push(GoRequire {
        path: path.trim_matches('"').to_string(),
        version: version.trim_start_matches('v').to_string(),
        indirect,
    });
    Ok(())
}

fn push_replace(file: &mut GoModFile, body: &str) -> MgResult<()> {
    let Some((old, new)) = body.split_once("=>") else {
        return Err(MgError::Other(format!(
            "go.mod replace missing '=>': '{body}'"
        )));
    };
    let mut old_parts = old.split_whitespace();
    let Some(old_path) = old_parts.next() else {
        return Err(MgError::Other(format!(
            "go.mod replace without old path: '{body}'"
        )));
    };
    let old_version = old_parts
        .next()
        .map(|v| v.trim_start_matches('v').to_string());
    let mut new_parts = new.split_whitespace();
    let Some(new_path) = new_parts.next() else {
        return Err(MgError::Other(format!(
            "go.mod replace without new target: '{body}'"
        )));
    };
    let new_version = new_parts
        .next()
        .map(|v| v.trim_start_matches('v').to_string());
    // Quoted paths are legal go.mod — strip like push_require.
    // (Path có quote là hợp lệ — cắt như push_require.)
    file.replaces.push(GoReplace {
        old_path: old_path.trim_matches('"').to_string(),
        old_version,
        new_path: new_path.trim_matches('"').to_string(),
        new_version,
    });
    Ok(())
}

fn push_exclude(file: &mut GoModFile, body: &str) -> MgResult<()> {
    let mut parts = body.split_whitespace();
    let Some(path) = parts.next() else {
        return Ok(());
    };
    let Some(version) = parts.next() else {
        return Err(MgError::Other(format!(
            "go.mod exclude without version: '{body}'"
        )));
    };
    file.excludes.push((
        path.trim_matches('"').to_string(),
        version.trim_start_matches('v').to_string(),
    ));
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GoInfo {
    #[serde(default)]
    time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GoLatest {
    #[serde(default)]
    version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_require_paths_are_unquoted() {
        // gopkg.in/yaml.v3 quotes its module paths — legal go.mod that
        // must not leak quotes into registry URLs (404s).
        // (Path có quote trong go.mod là hợp lệ — phải cắt quote.)
        let pom = "module \"example.com/m\"\n\ngo 1.21\n\nrequire (\n\t\"gopkg.in/check.v1\" v0.0.0-20161208181325-20d25e280405\n)\n";
        let file = parse_go_mod(pom).unwrap();
        assert_eq!(file.requires.len(), 1);
        assert_eq!(file.requires[0].path, "gopkg.in/check.v1");
        assert_eq!(
            file.requires[0].version,
            "0.0.0-20161208181325-20d25e280405"
        );
    }

    #[test]
    fn exact_pin_version_accepts_bare_and_pseudo() {
        assert_eq!(
            GoModProtocol::exact_pin_version("1.2.3").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            GoModProtocol::exact_pin_version("v0.0.0-20161208181325-20d25e280405").as_deref(),
            Some("0.0.0-20161208181325-20d25e280405")
        );
        assert_eq!(
            GoModProtocol::exact_pin_version("=2.0.0").as_deref(),
            Some("2.0.0")
        );
        assert!(GoModProtocol::exact_pin_version("^1.2.3").is_none());
        assert!(GoModProtocol::exact_pin_version(">=1.0.0").is_none());
        assert!(GoModProtocol::exact_pin_version("*").is_none());
        assert!(GoModProtocol::exact_pin_version("").is_none());
    }

    #[test]
    fn go_mod_parse_blocks_and_directives() {
        let gomod = parse_go_mod(
            "module example.com/m\n\ngo 1.22\n\nrequire (\n\tgithub.com/pkg/errors v0.9.1 // indirect\n\tgolang.org/x/text v0.14.0\n)\n\nreplace github.com/old/pkg => github.com/new/pkg v1.2.3\n\nexclude github.com/bad/pkg v0.1.0\n",
        )
        .unwrap();
        assert_eq!(gomod.requires.len(), 2);
        assert_eq!(gomod.requires[0].path, "github.com/pkg/errors");
        assert_eq!(gomod.requires[0].version, "0.9.1");
        assert!(gomod.requires[0].indirect);
        assert!(!gomod.requires[1].indirect);
        assert_eq!(gomod.replaces.len(), 1);
        assert_eq!(gomod.replaces[0].old_path, "github.com/old/pkg");
        assert_eq!(gomod.replaces[0].new_path, "github.com/new/pkg");
        assert_eq!(gomod.replaces[0].new_version.as_deref(), Some("1.2.3"));
        assert_eq!(
            gomod.excludes,
            vec![("github.com/bad/pkg".to_string(), "0.1.0".to_string())]
        );
    }

    #[test]
    fn go_mod_single_line_directives() {
        let gomod = parse_go_mod(
            "module m\nrequire golang.org/x/sys v0.15.0 // indirect\nreplace a/b v1.0.0 => c/d\nexclude e/f v2.0.0\n",
        )
        .unwrap();
        assert_eq!(gomod.requires.len(), 1);
        assert!(gomod.requires[0].indirect);
        assert_eq!(gomod.replaces[0].old_version.as_deref(), Some("1.0.0"));
        assert_eq!(gomod.replaces[0].new_version, None);
        assert_eq!(gomod.excludes.len(), 1);
    }

    #[test]
    fn go_pin_semantics_and_operators() {
        let v = Version::parse("1.2.3").unwrap();
        // Bare version = exact pin (go.mod lock semantics).
        // (Version trần = ghim chính xác (ngữ nghĩa lock go.mod).)
        assert!(go_matches("1.2.3", &v));
        assert!(!go_matches("1.2.4", &v));
        assert!(go_matches("*", &v));
        assert!(go_matches(">=1.0, <2.0", &v));
        assert!(go_matches("^1.2", &v));
        assert!(!go_matches("^2.0", &v));
        assert!(go_matches("~1.2.0", &v));
        assert!(
            go_matches("v1.2.3", &v),
            "v-prefix on the range is tolerated"
        );
    }

    #[test]
    fn module_path_escape() {
        assert_eq!(
            escape_module_path("github.com/BurntSushi/toml"),
            "github.com/!burnt!sushi/toml"
        );
        assert_eq!(escape_module_path("golang.org/x/text"), "golang.org/x/text");
    }

    #[test]
    fn dirhash_matches_known_shape() {
        // Two files → sorted lines hashed; stable across call order.
        // (Hai file → hash các dòng đã sắp xếp; ổn định bất kể thứ tự gọi.)
        let mk = |name: &str, data: &[u8]| zip_reader::ZipEntry {
            name: name.to_string(),
            data: data.to_vec(),
        };
        let a = vec![mk("m@v1/b", b"bbb"), mk("m@v1/a", b"aaa")];
        let b = vec![mk("m@v1/a", b"aaa"), mk("m@v1/b", b"bbb")];
        assert_eq!(dirhash_zip(&a).unwrap(), dirhash_zip(&b).unwrap());
        assert!(dirhash_zip(&a).unwrap().starts_with("h1:"));
    }
}
