//! Swift registry archive resolver. Git-source dependencies are parsed but fail closed.
//! Resolver archive Swift registry. Dependency nguồn Git được parse nhưng fail-closed.
//!
//! Registry spec: version list at `{registry}/{scope}/{name}.json`
//! (`{"versions":[...]}`), source archive at
//! `{registry}/{scope}/{name}/{version}.zip` and the mandatory checksum at
//! `{registry}/{scope}/{name}/{version}.sha256` (hex sha256 of the zip —
//! a registry that omits it is a fail-closed integrity error). Selection
//! mirrors SwiftPM: the HIGHEST matching version wins. Transitive deps are
//! read from the `Package.swift` inside the archive (hand-rolled scan of
//! `.package(...)` declarations — no Swift toolchain needed during
//! resolve). Git-only deps (`.package(url:)`) are rejected until a native
//! HTTP Git transport exists; MagiCore never spawns `git` for dependency
//! operations. Materialization is the mgc checkouts layout
//! `{swift_root}/checkouts/{identity}-{version}` plus a SwiftPM-compatible
//! `Package.resolved` export.
//! Spec registry: danh sách version tại `{registry}/{scope}/{name}.json`
//! (`{"versions":[...]}`), archive tại
//! `{registry}/{scope}/{name}/{version}.zip` và checksum bắt buộc tại
//! `{registry}/{scope}/{name}/{version}.sha256` (hex sha256 của zip —
//! registry thiếu checksum là lỗi integrity fail-closed). Selection theo
//! SwiftPM: version CAO NHẤT khớp thắng. Dep bắc cầu đọc từ `Package.swift`
//! bên trong archive (quét thủ công khai báo `.package(...)` — không cần
//! toolchain Swift khi resolve). Dep chỉ-Git bị từ chối cho tới khi có
//! HTTP Git transport native; MagiCore không spawn `git` trong dependency
//! operations. Materialize là
//! layout checkouts của mgc `{swift_root}/checkouts/{identity}-{version}`
//! cộng export `Package.resolved` tương thích SwiftPM.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use mgc_types::{MgError, MgResult, Version};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Native SwiftPM registry engine.
/// Engine native SwiftPM registry.
#[derive(Debug)]
pub struct SwiftRegistryProtocol {
    registry_base: String,
    client: mgc_http::HttpClient,
    // Resolved archives are cached so the install download never re-fetches
    // what resolve already pulled (resolve reads Package.swift from the zip).
    // (Archive đã resolve được cache để download lúc install không tải lại
    // thứ resolve đã kéo (resolve đọc Package.swift từ zip).)
    zip_cache: Mutex<HashMap<String, Vec<u8>>>,
}

/// A dependency declared by a Package.swift / dump-package document.
/// Một dependency được khai báo trong Package.swift / dump-package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwiftDep {
    /// `.package(id: "scope.name", …)` — registry-sourced.
    /// `.package(id: "scope.name", …)` — nguồn registry.
    Registry {
        identity: String,
        requirement: String,
    },
    /// `.package(url: "…", …)` — git-only.
    /// `.package(url: "…", …)` — chỉ git.
    Git { url: String, requirement: String },
}

impl SwiftDep {
    /// Graph edge name: registry identity `scope.name` → `scope/name`;
    /// git URL → scheme-less host path (`https://host/x/y.git` →
    /// `host/x/y`, file:// URLs ride as-is — PackageName accepts `:`).
    /// Tên cạnh graph: registry identity `scope.name` → `scope/name`;
    /// git URL → host path không scheme (`https://host/x/y.git` →
    /// `host/x/y`, URL file:// giữ nguyên — PackageName chấp nhận `:`).
    pub fn dep_name(&self) -> String {
        match self {
            SwiftDep::Registry { identity, .. } => identity_to_dep_name(identity),
            SwiftDep::Git { url, .. } => git_url_to_name(url),
        }
    }
}

/// One pin imported from (or exported to) a Package.resolved file.
/// Một pin nhập từ (hoặc xuất ra) file Package.resolved.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SwiftResolvedPin {
    pub identity: String,
    pub version: Option<String>,
    pub revision: Option<String>,
    pub branch: Option<String>,
    pub location: Option<String>,
}

impl SwiftRegistryProtocol {
    /// Build with an explicit registry base (testability). The base is the
    /// registry service root (no trailing slash).
    /// Dựng với registry base tường minh (cho test). Base là gốc dịch vụ
    /// registry (không dấu sẹo cuối).
    pub fn with_registry(registry_base: &str) -> Self {
        Self {
            registry_base: registry_base.trim_end_matches('/').to_string(),
            client: mgc_http::HttpClient::default(),
            zip_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Build from environment: `MGC_SWIFT_REGISTRY_URL`. SwiftPM has no
    /// universal default registry — an unset variable yields a protocol
    /// whose every resolve fails closed with explicit guidance (never a
    /// silent empty graph).
    /// Dựng từ môi trường: `MGC_SWIFT_REGISTRY_URL`. SwiftPM không có
    /// registry mặc định toàn cục — biến không đặt cho ra protocol mà mọi
    /// resolve fail-closed kèm hướng dẫn rõ ràng (không bao giờ graph rỗng
    /// âm thầm).
    pub fn from_env() -> Self {
        match std::env::var("MGC_SWIFT_REGISTRY_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            Some(url) => Self::with_registry(url.trim()),
            None => {
                eprintln!(
                    "WARNING: MGC_SWIFT_REGISTRY_URL is not set — Swift registry \
                     resolve calls will fail closed (SwiftPM has no default registry)"
                );
                Self::with_registry("")
            }
        }
    }

    /// Host[:port] of the configured registry base (lock provenance tag).
    /// Host[:port] của registry base đã cấu hình (tag provenance lock).
    pub fn registry_host(&self) -> Option<String> {
        let base = self.registry_base.as_str();
        let rest = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
        let host = rest
            .split('/')
            .next()
            .unwrap_or_default()
            .trim_end_matches('/');
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    }

    async fn get_bytes(&self, url: &str) -> MgResult<(u16, Vec<u8>)> {
        let resp = self
            .client
            .get(url)
            .await
            .map_err(|e| MgError::Network(format!("GET {url} failed: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MgError::Network(format!("read {url} body failed: {e}")))?;
        Ok((status, bytes.to_vec()))
    }

    /// `{registry}/{scope}/{name}/{file}` URL for a registry artifact.
    /// URL `{registry}/{scope}/{name}/{file}` cho artifact registry.
    fn registry_url_for(&self, dep_name: &str, file: &str) -> String {
        format!("{}/{}", self.registry_base, dep_name) + "/" + file
    }

    /// Registry-path resolve (name is `scope/name`).
    /// Resolve đường registry (name là `scope/name`).
    async fn resolve_registry(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        if self.registry_base.is_empty() {
            return Err(MgError::Other(
                "no Swift registry configured — set MGC_SWIFT_REGISTRY_URL (fail-closed)"
                    .to_string(),
            ));
        }
        // Registry packages REQUIRE the scope/name form (SwiftPM registries
        // are scoped; a bare name cannot be addressed).
        // (Package registry BẮT BUỘC dạng scope/name (registry SwiftPM có
        // scope; tên trần không định vị được).)
        let Some((scope, _pkg)) = name.split_once('/') else {
            return Err(MgError::Other(format!(
                "Swift registry packages require a scope/name identifier — '{name}' has no scope (fail-closed)"
            )));
        };
        let versions_url = self.registry_url_for(name, &format!("{scope}.json"));
        let (status, body) = {
            let (s, b) = self.get_bytes(&versions_url).await?;
            (s, String::from_utf8_lossy(&b).into_owned())
        };
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {versions_url} returned {status} — no versions for {name} (fail-closed)"
            )));
        }
        let doc: Value = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse swift version list failed: {e}")))?;
        let versions: Vec<&str> = doc
            .get("versions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        // SwiftPM selection: the HIGHEST matching version wins.
        // (Selection SwiftPM: version CAO NHẤT khớp thắng.)
        let mut best: Option<(Version, String)> = None;
        for raw in &versions {
            let Ok(v) = Version::parse(raw) else {
                continue;
            };
            if swift_matches(range, &v) && best.as_ref().is_none_or(|(bv, _)| v > *bv) {
                best = Some((v, (*raw).to_string()));
            }
        }
        let version = best
            .ok_or_else(|| MgError::Other(format!("no version of {name} matches range '{range}'")))?
            .1;

        // Mandatory registry checksum (hex sha256 of the zip) — fetched
        // BEFORE the archive; a registry without it is fail-closed.
        // (Checksum registry bắt buộc (hex sha256 của zip) — tải TRƯỚC
        // archive; registry thiếu là fail-closed.)
        let sha_url = self.registry_url_for(name, &format!("{version}.sha256"));
        let (sha_status, sha_body) = self.get_bytes(&sha_url).await?;
        if !(200..300).contains(&sha_status) {
            return Err(MgError::Integrity(format!(
                "GET {sha_url} returned {sha_status} — the registry publishes no checksum \
                 for {name} {version} (fail-closed)"
            )));
        }
        let sha256 = String::from_utf8_lossy(&sha_body)
            .trim()
            .to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(MgError::Integrity(format!(
                "malformed .sha256 for {name} {version}: '{sha256}'"
            )));
        }

        // Source archive (cached for the install download).
        // (Archive nguồn (cache cho download lúc install).)
        let zip_url = self.registry_url_for(name, &format!("{version}.zip"));
        let (zip_status, zip_bytes) = self.get_bytes(&zip_url).await?;
        if !(200..300).contains(&zip_status) {
            return Err(MgError::Network(format!(
                "GET {zip_url} returned {zip_status} (fail-closed)"
            )));
        }
        // Transitive deps come from the Package.swift at the archive root.
        // (Dep bắc cầu lấy từ Package.swift ở gốc archive.)
        let deps = swift_deps_from_archive(&zip_bytes)?;
        self.zip_cache
            .lock()
            .expect("swift zip cache poisoned")
            .insert(format!("{name}@{version}"), zip_bytes);

        Ok(ResolvedEntry {
            name: name.to_string(),
            version,
            deps,
            artifact_url: zip_url,
            sha256,
            extra_markers: vec![format!("swift-registry:{scope}")],
        })
    }

    /// Git URLs are parsed for manifest compatibility but are not fetched via
    /// an external Git executable. Native HTTP Git transport is not shipped.
    /// URL Git được parse để tương thích manifest nhưng không fetch bằng Git
    /// executable bên ngoài; native HTTP Git transport chưa được triển khai.
    async fn resolve_git(&self, name: &str, _range: &str) -> MgResult<ResolvedEntry> {
        Err(MgError::Unsupported {
            core: "app",
            capability: "native Swift Git dependency transport",
            guidance: format!(
                "Swift dependency '{name}' uses a Git source; MagiCore refuses to spawn Git until native Git transport is implemented"
            ),
        })
    }

    /// Materialize a REGISTRY entry into the mgc checkouts layout
    /// `{swift_root}/checkouts/{identity}.{name}-{version}/` — the zip is
    /// extracted as-is (SPM registry archives are rooted at the package
    /// root). `bytes` must be the verified archive.
    /// Materialize entry REGISTRY vào layout checkouts của mgc
    /// `{swift_root}/checkouts/{identity}-{version}/` — zip được giải nén
    /// nguyên bản (archive registry SPM gốc là package root). `bytes` phải
    /// là archive đã xác minh.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        bytes: &[u8],
        swift_root: &Path,
    ) -> MgResult<PathBuf> {
        let dir = swift_root.join("checkouts").join(format!(
            "{}-{}",
            entry.name.replace('/', "."),
            entry.version
        ));
        std::fs::create_dir_all(&dir)?;
        super::zip_reader::extract_zip(bytes, &dir)?;
        Ok(dir)
    }

    /// Git materialization is unsupported until native transport exists.
    pub fn materialize_git(
        &self,
        _entry: &ResolvedEntry,
        _expected_sha: Option<&str>,
        _swift_root: &Path,
    ) -> MgResult<PathBuf> {
        Err(MgError::Unsupported {
            core: "app",
            capability: "native Swift Git dependency materialization",
            guidance: "this lock entry requires Git transport; MagiCore refuses to invoke the Git executable".to_string(),
        })
    }

    /// Export a SwiftPM-compatible (v2) `Package.resolved` from resolved
    /// pins. Existing files are overwritten — the resolution is the truth.
    /// Xuất `Package.resolved` tương thích SwiftPM (v2) từ các pin đã
    /// resolve. File có sẵn bị ghi đè — kết quả resolve là chân lý.
    pub fn export_package_resolved(
        &self,
        pins: &[SwiftResolvedPin],
        project_root: &Path,
    ) -> MgResult<PathBuf> {
        let pins_json: Vec<Value> = pins
            .iter()
            .map(|pin| {
                let mut state = serde_json::Map::new();
                if let Some(v) = &pin.version {
                    state.insert("version".into(), Value::String(v.clone()));
                }
                if let Some(r) = &pin.revision {
                    state.insert("revision".into(), Value::String(r.clone()));
                }
                if let Some(b) = &pin.branch {
                    state.insert("branch".into(), Value::String(b.clone()));
                }
                let kind = if pin.location.as_ref().is_some_and(|l| l.contains("://")) {
                    "remote"
                } else {
                    "registry"
                };
                serde_json::json!({
                    "identity": pin.identity,
                    "kind": kind,
                    "location": pin.location.clone().unwrap_or_else(|| {
                        format!("registry+{}/{}", self.registry_base, pin.identity)
                    }),
                    "state": Value::Object(state),
                })
            })
            .collect();
        let doc = serde_json::json!({ "version": 2, "pins": pins_json });
        let path = project_root.join("Package.resolved");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n",
        )
        .map_err(|e| MgError::Other(format!("write Package.resolved: {e}")))?;
        Ok(path)
    }
}

impl SwiftDep {
    /// Range string carried on the graph edge.
    /// Chuỗi range mang trên cạnh graph.
    pub fn requirement_text(&self) -> String {
        match self {
            SwiftDep::Registry { requirement, .. } | SwiftDep::Git { requirement, .. } => {
                requirement.clone()
            }
        }
    }
}

#[async_trait]
impl RegistryProtocol for SwiftRegistryProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        // Local paths never resolve anywhere: neither the git transport
        // (default-blocked) nor the scoped registry can serve them.
        // (Đường dẫn local không resolve ở đâu cả.)
        if name.starts_with("file://") {
            return Err(MgError::Other(
                "file:// dependencies are blocked — local paths never enter the git transport or the registry (fail-closed)".to_string(),
            ));
        }
        if is_git_name(name) {
            self.resolve_git(name, range).await
        } else {
            self.resolve_registry(name, range).await
        }
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        if entry.extra_markers.iter().any(|m| m == "swift-git") {
            // Git checkouts are directories, not archive bytes — the
            // "download" is the pinned commit SHA (ASCII), the only
            // verifiable artifact identity (provenance, not payload).
            // (Checkout git là thư mục, không phải byte archive —
            // "download" là SHA commit đã ghim (ASCII), định danh artifact
            // duy nhất xác minh được (provenance, không phải payload).)
            return Ok(entry
                .extra_markers
                .iter()
                .find_map(|m| m.strip_prefix("git-commit:"))
                .unwrap_or_default()
                .as_bytes()
                .to_vec());
        }
        let key = format!("{}@{}", entry.name, entry.version);
        if let Some(bytes) = self
            .zip_cache
            .lock()
            .expect("swift zip cache poisoned")
            .get(&key)
        {
            return Ok(bytes.clone());
        }
        let (status, bytes) = self.get_bytes(&entry.artifact_url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {} returned {status}",
                entry.artifact_url
            )));
        }
        Ok(bytes)
    }

    /// Verify overrides the shared default for GIT entries: the downloaded
    /// "artifact" is the pinned commit SHA — it must equal the resolve-time
    /// marker (a moved ref fails closed). Registry entries keep the shared
    /// sha256 check (the registry checksum is mandatory).
    /// (Verify ghi đè mặc định cho entry GIT: "artifact" tải về là SHA
    /// commit đã ghim — phải bằng marker lúc resolve (ref bị dịch là
    /// fail-closed). Entry registry giữ kiểm tra sha256 chung (checksum
    /// registry là bắt buộc).)
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        if entry.extra_markers.iter().any(|m| m == "swift-git") {
            let expected = entry
                .extra_markers
                .iter()
                .find_map(|m| m.strip_prefix("git-commit:"))
                .ok_or_else(|| {
                    MgError::Integrity(format!(
                        "no git-commit provenance for {} @ {} (fail-closed)",
                        entry.name, entry.version
                    ))
                })?;
            let downloaded = String::from_utf8_lossy(bytes);
            let actual = downloaded.trim();
            if actual != expected {
                return Err(MgError::Integrity(format!(
                    "git provenance mismatch for {} @ {}: expected {expected}, got {actual}",
                    entry.name, entry.version
                )));
            }
            return Ok(());
        }
        // Registry entries carry a mandatory checksum: an empty digest
        // is a corrupt registry record, failed closed like the shared
        // default (V1.2 zero-trust).
        // (Entry registry mang checksum bắt buộc: digest rỗng là bản ghi
        // registry hỏng, fail-closed như default chung.)
        if entry.sha256.is_empty() {
            return Err(MgError::Integrity(format!(
                "refusing artifact without digest for {}@{} (Swift registry checksum missing)",
                entry.name, entry.version
            )));
        }
        let actual = super::sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(MgError::Integrity(format!(
                "sha256 mismatch for {}@{}: expected {}, got {}",
                entry.name, entry.version, entry.sha256, actual
            )));
        }
        Ok(())
    }
}

/// Detect Git-source names only so resolution can reject them without
/// spawning an external Git process.
/// Dò tên git: tên registry là `scope/name`; mọi tên có segment đầu kiểu
/// host hoặc đuôi `.git` là dep chỉ-git. `file://` KHÔNG được nhận diện.
pub fn is_git_name(name: &str) -> bool {
    // A scheme other than https (file/http/ssh/git) is never a git
    // transport candidate — rejected here so it can fail explicitly
    // downstream instead of riding the git path.
    // (Scheme khác https không bao giờ là ứng viên transport git.)
    if name.contains("://") && !name.starts_with("https://") {
        return false;
    }
    name.ends_with(".git")
        || name.matches('/').count() > 1
        || name
            .split('/')
            .next()
            .is_some_and(|first| first.contains('.') && !first.contains(".."))
}

/// Map a `.package(url:)` value to the graph-edge name (https scheme +
/// `.git` stripped; anything else rides through unchanged and fails
/// explicitly at resolve time — no silent acceptance).
/// Ánh xạ giá trị `.package(url:)` sang tên cạnh graph.
pub fn git_url_to_name(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("https://") {
        rest.trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or(rest.trim_end_matches('/'))
            .to_string()
    } else {
        url.to_string()
    }
}

/// Git transport is disabled until a native HTTP implementation exists;
/// no allowlist or environment variable enables an external Git process.
/// Transport Git bị tắt cho tới khi có native HTTP implementation.
pub fn git_transport_allowed(
    _url: &str,
    _allow_git_deps: bool,
    _hosts: &[&str],
) -> Result<(), MgError> {
    Err(MgError::Unsupported {
        core: "app",
        capability: "native Swift Git transport",
        guidance: "Git transport is disabled; MGC_GIT_DEPS and host allowlists do not enable an external Git executable".to_string(),
    })
}

/// Repository display name (last path segment, `.git` stripped).
/// Tên hiển thị repo (segment cuối, bỏ `.git`).
#[cfg(test)]
fn git_repo_name(name: &str) -> String {
    name.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_string()
}

/// Registry identity → graph name: `scope.name` → `scope/name`; a name
/// that already uses `/` rides as-is; a scope-less identity stays as-is and
/// fails closed at resolve time with guidance.
/// Registry identity → tên graph: `scope.name` → `scope/name`; tên đã dùng
/// `/` giữ nguyên; identity không scope giữ nguyên và fail-closed lúc
/// resolve kèm hướng dẫn.
fn identity_to_dep_name(identity: &str) -> String {
    let identity = identity.trim();
    if identity.contains('/') || !identity.contains('.') {
        return identity.to_string();
    }
    match identity.split_once('.') {
        Some((scope, name)) if !scope.is_empty() && !name.is_empty() => {
            format!("{scope}/{name}")
        }
        _ => identity.to_string(),
    }
}

/// Extract the Package.swift text from a registry archive (root entry —
/// nested copies are ignored; missing manifest fails closed).
/// Trích nội dung Package.swift từ archive registry (entry gốc — bản lồng
/// sâu bị bỏ qua; thiếu manifest fail-closed).
fn swift_deps_from_archive(zip_bytes: &[u8]) -> MgResult<Vec<(String, String)>> {
    let entries = super::zip_reader::read_zip_entries(zip_bytes)?;
    let manifest = entries
        .iter()
        .find(|e| e.name == "Package.swift")
        .ok_or_else(|| {
            MgError::Other(
                "registry archive has no root Package.swift — deps cannot be discovered (fail-closed)"
                    .to_string(),
            )
        })?;
    Ok(
        parse_swift_package_deps(&String::from_utf8_lossy(&manifest.data))
            .into_iter()
            .map(|d| (d.dep_name(), d.requirement_text()))
            .collect(),
    )
}

/// Scan `Package.swift` text for `.package(...)` declarations (balanced-
/// paren capture; `//` comments are not stripped — string literals carry
/// the data and comment text cannot form a balanced `.package(...)` call
/// shape in practice).
/// Quét nội dung `Package.swift` tìm khai báo `.package(...)` (bắt ngoặc
/// cân bằng; comment `//` không bị cắt — dữ liệu nằm trong string literal
/// và văn bản comment không thể tạo hình `.package(...)` cân bằng trong
/// thực tế).
pub fn parse_swift_package_deps(text: &str) -> Vec<SwiftDep> {
    let mut deps = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find(".package") {
        let after_marker = &rest[pos + ".package".len()..];
        // Only an immediately-following `(` is a dependency call — longer
        // identifiers (`.packages(...)`, `.packageRegistry(...)`) advance
        // the scan window past this occurrence.
        // (Chỉ `(` đi ngay sau mới là lời gọi dep — định danh dài hơn
        // (`.packages(...)`, `.packageRegistry(...)`) đẩy cửa sổ quét qua
        // lần xuất hiện này.)
        if !after_marker.starts_with('(') {
            rest = after_marker;
            continue;
        }
        let after = &after_marker[1..];
        let Some(close) = balanced_close(after) else {
            break;
        };
        let body = &after[..close];
        rest = &after[close + 1..];
        if let Some(dep) = parse_package_call(body) {
            deps.push(dep);
        }
    }
    deps
}

/// Index of the `)` balancing the open `(` at position 0 of `body`.
/// Vị trí `)` cân bằng với `(` mở tại vị trí 0 của `body`.
fn balanced_close(body: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (i, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse one `.package(<body>)` argument list.
/// Parse một danh sách tham số `.package(<body>)`.
fn parse_package_call(body: &str) -> Option<SwiftDep> {
    let id = quoted_value(body, "id:");
    let url = quoted_value(body, "url:");
    let requirement = extract_requirement(body);
    if let Some(id) = id {
        return Some(SwiftDep::Registry {
            identity: id,
            requirement,
        });
    }
    url.map(|url| SwiftDep::Git { url, requirement })
}

/// Requirement encoding on graph edges — `from:X` (upToNextMajor),
/// `minor:X` (upToNextMinor), `exact:X`, `range:A..<B`, `branch:B`,
/// `revision:R`, bare pin `X`, `*` (unconstrained).
/// Mã hóa requirement trên cạnh graph — `from:X` (upToNextMajor),
/// `minor:X` (upToNextMinor), `exact:X`, `range:A..<B`, `branch:B`,
/// `revision:R`, pin trần `X`, `*` (không ràng buộc).
fn extract_requirement(body: &str) -> String {
    for (needle, prefix) in [
        (".upToNextMajor(from:", "from:"),
        (".upToNextMinor(from:", "minor:"),
        ("upToNextMajor(from:", "from:"),
        ("upToNextMinor(from:", "minor:"),
    ] {
        if let Some(v) = quoted_after(body, needle) {
            return format!("{prefix}{v}");
        }
    }
    if let Some(v) = quoted_after(body, "exact:") {
        return format!("exact:{v}");
    }
    if let Some(v) = quoted_after(body, "branch:") {
        return format!("branch:{v}");
    }
    if let Some(v) = quoted_after(body, "revision:") {
        return format!("revision:{v}");
    }
    if let Some(v) = quoted_after(body, "from:") {
        return format!("from:{v}");
    }
    // Literal range: "A"..<"B" (quotes stripped by the parser below).
    // (Khoảng literal: "A"..<"B" (parser bên dưới bỏ dấu ngoặc kép).)
    if let Some((a, b)) = quoted_range(body) {
        return format!("range:{a}..<{b}");
    }
    if let Some(v) = bare_quoted_version(body) {
        return v;
    }
    "*".to_string()
}

/// Value of `"needle" "value"` (e.g. `from: "1.2.3"`).
/// Giá trị của `"needle" "value"` (vd `from: "1.2.3"`).
fn quoted_value(body: &str, needle: &str) -> Option<String> {
    quoted_after(body, needle)
}

fn quoted_after(body: &str, needle: &str) -> Option<String> {
    let pos = body.find(needle)?;
    let rest = body[pos + needle.len()..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' {
        return None;
    }
    rest[1..].split(quote).next().map(str::to_string)
}

/// `"A"..<"B"` literal range.
/// Khoảng literal `"A"..<"B"`.
fn quoted_range(body: &str) -> Option<(String, String)> {
    let pos = body.find("..<")?;
    let left = body[..pos].trim();
    let right = body[pos + 3..].trim();
    let a = left.rsplit('"').nth(1)?.to_string();
    let b = right.split('"').nth(1)?.to_string();
    Some((a, b))
}

/// A bare quoted version (`"1.2.3"` — exact pin) that is not part of a
/// requirement call.
/// Version trần trong ngoặc kép (`"1.2.3"` — pin chính xác) không thuộc
/// lời gọi requirement.
fn bare_quoted_version(body: &str) -> Option<String> {
    for tok in body.split_whitespace() {
        let cleaned = tok.trim_matches(',');
        if cleaned.starts_with('"') && Version::parse(cleaned.trim_matches('"')).is_ok() {
            return Some(cleaned.trim_matches('"').to_string());
        }
    }
    None
}

/// Parse `swift package dump-package` JSON output (tolerant — the source
/// array shape varies across toolchain versions).
/// Parse JSON output của `swift package dump-package` (khoan dung — hình
/// dạng mảng source đổi theo version toolchain).
pub fn parse_dump_package(json: &str) -> MgResult<Vec<SwiftDep>> {
    let doc: Value = serde_json::from_str(json)
        .map_err(|e| MgError::Other(format!("parse dump-package JSON failed: {e}")))?;
    let mut deps = Vec::new();
    for dep in doc
        .get("dependencies")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        // New shape (Swift 5.7+, what real toolchains emit):
        // `{"registry": [{"identity": ..., "requirement": {...}}]}` and
        // `{"remote": [{"url": ..., ...}]}`.
        if let Some(items) = dep.get("registry").and_then(Value::as_array) {
            for item in items {
                let identity = item
                    .get("identity")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let requirement = dump_requirement(item.get("requirement"));
                push_dump_dep(&mut deps, "registry", identity, requirement);
            }
            continue;
        }
        if let Some(items) = dep.get("remote").and_then(Value::as_array) {
            for item in items {
                let url = item.get("url").and_then(Value::as_str).unwrap_or_default();
                let requirement = dump_requirement(item.get("requirement"));
                push_dump_dep(&mut deps, "remote", url, requirement);
            }
            continue;
        }
        // Legacy shape: `{"source": ["registry"|"remote", target, req]}`.
        let Some(source) = dep.get("source").and_then(Value::as_array) else {
            continue;
        };
        let Some(kind) = source.first().and_then(Value::as_str) else {
            continue;
        };
        let target = source.get(1).and_then(Value::as_str).unwrap_or_default();
        let requirement = dump_requirement(source.get(2));
        push_dump_dep(&mut deps, kind, target, requirement);
    }
    Ok(deps)
}

/// Push one dump-package dep (shared by the new registry/remote shapes
/// and the legacy source-array shape).
fn push_dump_dep(deps: &mut Vec<SwiftDep>, kind: &str, target: &str, requirement: Option<String>) {
    let Some(requirement) = requirement else {
        // Unmappable requirement — honest skip (cannot pin
        // deterministically).
        // (Requirement không ánh xạ được — skip trung thực (không ghim
        // tất định được).)
        eprintln!(
            "WARNING: dump-package dependency '{target}' has an unmappable requirement — skipped"
        );
        return;
    };
    match kind {
        "registry" => deps.push(SwiftDep::Registry {
            identity: target.to_string(),
            requirement,
        }),
        "remote" => deps.push(SwiftDep::Git {
            url: target.to_string(),
            requirement,
        }),
        // local/editing deps are toolchain-owned paths — never resolved
        // against a registry.
        // (dep local/editing là path do toolchain giữ — không bao giờ
        // resolve qua registry.)
        other => eprintln!(
            "WARNING: dump-package dependency '{target}' source '{other}' is not registry-resolvable — skipped"
        ),
    }
}

/// Requirement object from dump-package → graph range string. `None` = the
/// shape is not understood (caller skips honestly).
/// Object requirement từ dump-package → chuỗi range graph. `None` = hình
/// dạng không hiểu (caller skip trung thực).
fn dump_requirement(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(v) = value.get("from").and_then(Value::as_str) {
        return Some(format!("from:{v}"));
    }
    if let Some(ranges) = value.get("ranges").and_then(Value::as_array) {
        let parts: Vec<String> = ranges
            .iter()
            .filter_map(|r| {
                // String form (`"1.0.0..<2.0.0"`) or bound objects
                // (`{"lowerBound": "1.0.0", "upperBound": "2.0.0"}` —
                // the shape real `dump-package` emits).
                if let Some(s) = r.as_str() {
                    return Some(format!("range:{}", s.trim()));
                }
                let lo = r.get("lowerBound").and_then(Value::as_str)?;
                let hi = r.get("upperBound").and_then(Value::as_str)?;
                Some(format!("range:{lo}..<{hi}"))
            })
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("||"));
        }
    }
    if let Some(r) = value.get("range").and_then(Value::as_str) {
        return Some(format!("range:{}", r.trim()));
    }
    // Singular `range` holding bound objects (the shape real
    // `dump-package` emits: `"range": [{"lowerBound": "1.0.0",
    // "upperBound": "2.0.0"}]`).
    if let Some(arr) = value.get("range").and_then(Value::as_array) {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|r| {
                let lo = r.get("lowerBound").and_then(Value::as_str)?;
                let hi = r.get("upperBound").and_then(Value::as_str)?;
                Some(format!("range:{lo}..<{hi}"))
            })
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("||"));
        }
    }
    if let Some(v) = value.get("exact").and_then(Value::as_str) {
        return Some(format!("exact:{v}"));
    }
    if let Some(v) = value.get("branch").and_then(Value::as_str) {
        return Some(format!("branch:{v}"));
    }
    if let Some(v) = value.get("revision").and_then(Value::as_str) {
        return Some(format!("revision:{v}"));
    }
    None
}

/// Import pins from a Package.resolved (v1 object.pins / v2/v3 pins —
/// tolerant: unknown shapes are skipped).
/// Nhập pin từ Package.resolved (v1 object.pins / v2/v3 pins — khoan dung:
/// hình dạng lạ bị bỏ qua).
pub fn parse_package_resolved(text: &str) -> MgResult<Vec<SwiftResolvedPin>> {
    let doc: Value = serde_json::from_str(text)
        .map_err(|e| MgError::Other(format!("parse Package.resolved failed: {e}")))?;
    let pins = doc
        .get("pins")
        .or_else(|| doc.get("object").and_then(|o| o.get("pins")))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut out = Vec::new();
    for pin in pins {
        let identity = pin
            .get("identity")
            .or_else(|| pin.get("package"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if identity.is_empty() {
            continue;
        }
        let state = pin.get("state");
        let get = |k: &str| {
            state
                .and_then(|s| s.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        out.push(SwiftResolvedPin {
            identity,
            version: get("version"),
            revision: get("revision"),
            branch: get("branch"),
            location: pin
                .get("location")
                .or_else(|| pin.get("repositoryURL"))
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Ok(out)
}

/// Swift range semantics: `*`/empty = any, `exact:X`/bare `X` = pin,
/// `from:X` = >=X same major, `minor:X` = >=X same major+minor,
/// `range:A..<B` = >=A && <B, `||` alternatives; branch/revision prefixes
/// never match a registry version (fail-closed at the caller).
/// Ngữ nghĩa range Swift: `*`/rỗng = bất kỳ, `exact:X`/`X` trần = pin,
/// `from:X` = >=X cùng major, `minor:X` = >=X cùng major+minor,
/// `range:A..<B` = >=A && <B, phương án `||`; tiền tố branch/revision
/// không bao giờ khớp version registry (fail-closed ở caller).
pub fn swift_matches(range: &str, version: &Version) -> bool {
    range.trim().split("||").any(|part| {
        let part = part.trim();
        if part.is_empty() || part == "*" {
            return true;
        }
        swift_part(part, version)
    })
}

fn swift_part(part: &str, version: &Version) -> bool {
    if let Some(v) = part.strip_prefix("exact:") {
        return Version::parse(v.trim())
            .map(|t| version == &t)
            .unwrap_or(false);
    }
    if let Some(v) = part.strip_prefix("from:") {
        return from_matches(v.trim(), version, false);
    }
    if let Some(v) = part.strip_prefix("minor:") {
        return from_matches(v.trim(), version, true);
    }
    if let Some(range) = part.strip_prefix("range:") {
        let Some((a, b)) = range.split_once("..<") else {
            return false;
        };
        let lower = Version::parse(a.trim());
        let upper = Version::parse(b.trim());
        return lower.map(|t| version >= &t).unwrap_or(false)
            && upper.map(|t| version < &t).unwrap_or(false);
    }
    if part.starts_with("branch:") || part.starts_with("revision:") || part.starts_with("tag:") {
        // Non-version requirements never select a registry version.
        // (Yêu cầu phi-version không bao giờ chọn version registry.)
        return false;
    }
    // Bare version = exact pin (Package.resolved semantics).
    // (Version trần = pin chính xác (ngữ nghĩa Package.resolved).)
    Version::parse(part.trim())
        .map(|t| version == &t)
        .unwrap_or(false)
}

fn from_matches(raw: &str, version: &Version, minor_only: bool) -> bool {
    let Ok(target) = Version::parse(raw) else {
        return false;
    };
    version >= &target
        && version.major == target.major
        && (!minor_only || version.minor == target.minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_swift_scan_registry_git_and_requirements() {
        let text = r#"
// swift-tools-version:5.9
let package = Package(
    name: "App",
    dependencies: [
        .package(id: "scope.lib", from: "1.0.0"),
        .package(url: "https://github.com/example/repo.git", .upToNextMajor(from: "2.1.0")),
        .package(url: "https://github.com/example/pinned.git", exact: "3.0.0"),
        .package(url: "https://github.com/example/br.git", branch: "main"),
        .package(id: "other.lib", "4.0.0"..<"5.0.0"),
        .packages([])  // not a dependency
    ]
)"#;
        let deps = parse_swift_package_deps(text);
        assert_eq!(deps.len(), 5, "{deps:?}");
        assert_eq!(
            deps[0],
            SwiftDep::Registry {
                identity: "scope.lib".into(),
                requirement: "from:1.0.0".into()
            }
        );
        assert_eq!(deps[0].dep_name(), "scope/lib");
        assert_eq!(
            deps[1],
            SwiftDep::Git {
                url: "https://github.com/example/repo.git".into(),
                requirement: "from:2.1.0".into()
            }
        );
        assert_eq!(deps[1].dep_name(), "github.com/example/repo");
        assert_eq!(deps[2].requirement_text(), "exact:3.0.0", "{:?}", deps[2]);
        assert_eq!(deps[3].requirement_text(), "branch:main", "{:?}", deps[3]);
        assert_eq!(
            deps[4].requirement_text(),
            "range:4.0.0..<5.0.0",
            "{:?}",
            deps[4]
        );
    }

    #[test]
    fn dump_package_registry_remote_and_local() {
        let json = r#"{"name":"App","dependencies":[
            {"source":["registry","scope.lib",{"ranges":["1.0.0..<2.0.0"]}]},
            {"source":["remote","https://github.com/example/repo.git",{"from":"2.1.0"}]},
            {"source":["local","../sibling"]},
            {"source":["registry","weird.lib",{"weird":true}]}
        ]}"#;
        let deps = parse_dump_package(json).unwrap();
        assert_eq!(deps.len(), 2, "{deps:?}");
        assert_eq!(deps[0].dep_name(), "scope/lib");
        assert_eq!(deps[0].requirement_text(), "range:1.0.0..<2.0.0");
        assert_eq!(deps[1].dep_name(), "github.com/example/repo");
        assert_eq!(deps[1].requirement_text(), "from:2.1.0");
    }

    #[test]
    fn package_resolved_v1_and_v2_pins() {
        let v2 = r#"{"version":2,"pins":[
            {"identity":"scope.lib","kind":"registry","location":"registry+https://reg","state":{"version":"1.2.3"}},
            {"identity":"repo","kind":"remote","location":"https://github.com/example/repo.git","state":{"revision":"abc123","version":"2.0.0"}}
        ]}"#;
        let pins = parse_package_resolved(v2).unwrap();
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].identity, "scope.lib");
        assert_eq!(pins[0].version.as_deref(), Some("1.2.3"));
        assert_eq!(pins[1].revision.as_deref(), Some("abc123"));

        let v1 = r#"{"version":1,"object":{"pins":[
            {"package":"Repo","repositoryURL":"https://github.com/example/repo.git","state":{"version":"2.0.0"}}
        ]}}"#;
        let pins = parse_package_resolved(v1).unwrap();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].identity, "Repo");
        assert_eq!(pins[0].version.as_deref(), Some("2.0.0"));
        assert_eq!(
            pins[0].location.as_deref(),
            Some("https://github.com/example/repo.git")
        );
    }

    #[test]
    fn swift_range_semantics() {
        let v = Version::parse("1.2.3").unwrap();
        assert!(swift_matches("*", &v));
        assert!(swift_matches("1.2.3", &v), "bare = exact pin");
        assert!(!swift_matches("1.2.4", &v));
        assert!(swift_matches("exact:1.2.3", &v));
        assert!(swift_matches("from:1.0.0", &v));
        assert!(swift_matches("from:1.2.3", &v), "from is lower-inclusive");
        assert!(!swift_matches("from:2.0.0", &v));
        assert!(
            !swift_matches("from:1.3.0", &v),
            "upToNextMajor caps the major"
        );
        assert!(swift_matches("minor:1.2.0", &v));
        assert!(!swift_matches("minor:1.3.0", &v));
        assert!(swift_matches("range:1.0.0..<2.0.0", &v));
        assert!(swift_matches("range:1.2.3..<2.0.0", &v), "lower-inclusive");
        assert!(swift_matches("range:1.2.3..<2.0.0", &v));
        assert!(!swift_matches("range:1.2.4..<2.0.0", &v));
        assert!(!swift_matches("branch:main", &v));
        assert!(!swift_matches("revision:abc", &v));
        assert!(swift_matches("branch:main||from:1.0.0", &v), "alternatives");
    }

    #[test]
    fn git_name_url_and_repo_name_mapping() {
        assert!(!is_git_name("scope/lib"));
        assert!(is_git_name("github.com/example/repo"));
        assert!(is_git_name("host/x.git"));
        // file:// is NOT a git transport anymore (default-block §13.3):
        // local paths must never reach the clone machinery.
        assert!(!is_git_name("file:///tmp/repo"));
        assert_eq!(
            git_url_to_name("https://github.com/example/repo.git"),
            "github.com/example/repo"
        );
        assert_eq!(
            git_url_to_name("http://github.com/example/repo.git"),
            "http://github.com/example/repo.git"
        );
        assert_eq!(git_repo_name("github.com/example/repo"), "repo");
        assert_eq!(git_repo_name("host/x.git"), "x");
    }

    #[test]
    fn git_transport_is_disabled_even_when_allowlisted() {
        for (url, enabled, hosts) in [
            ("https://github.com/o/r.git", false, &[][..]),
            ("https://github.com/o/r.git", true, &["github.com"][..]),
            ("file:///tmp/repo", true, &["localhost"][..]),
        ] {
            let error = git_transport_allowed(url, enabled, hosts).unwrap_err();
            match error {
                MgError::Unsupported { guidance, .. } => {
                    assert!(guidance.contains("Git transport is disabled"));
                    assert!(guidance.contains("external Git executable"));
                }
                other => panic!("expected unsupported transport error, got: {other}"),
            }
        }
    }

    #[test]
    fn registry_base_urls_are_wellformed() {
        let p = SwiftRegistryProtocol::with_registry("http://127.0.0.1:1/");
        assert_eq!(
            p.registry_url_for("scope/lib", "1.0.0.zip"),
            "http://127.0.0.1:1/scope/lib/1.0.0.zip"
        );
    }
}

/// Bump a `from:`/`exact:` requirement in Package.swift text to a new
/// version, returning the edited text. Matches by registry identity
/// (`scope.name` or edge form `scope/name`) or git URL tail. Branch,
/// revision, range and minor requirements are NOT bumpable (honest
/// `None`, never a guess); multiple matches are ambiguous (`None`);
/// the result is verified by re-scan before returning.
/// (Nâng version requirement trong Package.swift, verify bằng quét lại.)
pub fn bump_swift_requirement(text: &str, dep_key: &str, new_version: &str) -> Option<String> {
    let key = dep_key.trim().to_lowercase();
    // Collect (call_start, call_end, body) spans like the parser.
    let mut calls: Vec<(usize, usize, String)> = Vec::new();
    let mut rest = text;
    let mut base = 0;
    while let Some(pos) = rest.find(".package") {
        let after_marker = &rest[pos + ".package".len()..];
        if !after_marker.starts_with('(') {
            rest = after_marker;
            base += pos + ".package".len();
            continue;
        }
        let after = &after_marker[1..];
        let Some(close) = balanced_close(after) else {
            break;
        };
        let body = after[..close].to_string();
        let start = base + pos;
        calls.push((start, base + pos + ".package".len() + 1 + close + 1, body));
        rest = &after[close + 1..];
        base += pos + ".package".len() + 1 + close + 1;
    }
    let mut hit: Option<(usize, usize, String)> = None;
    for (start, end, body) in &calls {
        if !call_matches_key(body, &key) {
            continue;
        }
        // Exactly one bumpable match allowed.
        if hit.is_some() {
            return None;
        }
        // Only `from:` / `exact:` carry a bumpable version.
        let needle = if body.contains("from:") {
            "from:"
        } else if body.contains("exact:") {
            "exact:"
        } else {
            return None;
        };
        let rel = body.find(needle)?;
        let after_needle = &body[rel + needle.len()..];
        let qstart = after_needle.find('"')? + 1;
        let qend = after_needle[qstart..].find('"')?;
        let mut new_body = body.clone();
        new_body.replace_range(
            rel + needle.len() + qstart..rel + needle.len() + qstart + qend,
            new_version,
        );
        hit = Some((*start, *end, new_body));
    }
    let (start, end, new_body) = hit?;
    let mut out = text.to_string();
    // Re-wrap: the span covers `.package(<body>)` but new_body is the
    // inner body only — dropping the wrapper would corrupt the source
    // (and the re-scan below would rightfully reject it).
    out.replace_range(start..end, &format!(".package({new_body})"));
    // Verify by re-scan: exactly one call matches the key and its
    // parsed requirement encodes the new version (proves the edit
    // landed on the requirement, not on coincidental text).
    let mut verified = false;
    let mut rest = out.as_str();
    while let Some(pos) = rest.find(".package") {
        let after_marker = &rest[pos + ".package".len()..];
        if !after_marker.starts_with('(') {
            rest = after_marker;
            continue;
        }
        let after = &after_marker[1..];
        let Some(close) = balanced_close(after) else {
            break;
        };
        let body = &after[..close];
        if call_matches_key(body, &key) {
            let req = extract_requirement(body);
            let pins_new =
                req == format!("from:{new_version}") || req == format!("exact:{new_version}");
            if !pins_new || verified {
                return None;
            }
            verified = true;
        }
        rest = &after[close + 1..];
    }
    verified.then_some(out)
}

/// Scheme-less `host/owner/repo` tail of a git URL (`.git` trimmed,
/// lowercased) for dep-key matching.
/// (Đuôi host/owner/repo của URL git để khớp dep.)
fn git_tail(url: &str) -> String {
    let no_scheme = url.split("://").last().unwrap_or(url);
    no_scheme
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_lowercase()
}

/// Does a `.package(...)` call body match a dep key? Identity
/// (`scope.name`, either dot or slash form) or git URL (full tail or
/// owner/repo suffix).
fn call_matches_key(body: &str, key: &str) -> bool {
    if let Some(id) = quoted_after(body, "id:") {
        let id = id.to_lowercase();
        return id == key || id.replace('.', "/") == key;
    }
    if let Some(url) = quoted_after(body, "url:") {
        let tail = git_tail(&url);
        return tail == key || tail.ends_with(&format!("/{key}"));
    }
    false
}

/// Remove one dependency call from Package.swift text, returning the
/// edited text. Matches by registry identity (dot or slash form) or git
/// URL tail. Unknown keys and ambiguous (multiple) matches yield `None`;
/// the result is verified by re-scan (exactly one fewer matching call).
/// (Xóa dependency khỏi Package.swift, verify bằng quét lại.)
pub fn remove_swift_requirement(text: &str, dep_key: &str) -> Option<String> {
    let key = dep_key.trim().to_lowercase();
    // Collect call spans.
    let mut calls: Vec<(usize, usize)> = Vec::new();
    let mut rest = text;
    let mut base = 0;
    while let Some(pos) = rest.find(".package") {
        let after_marker = &rest[pos + ".package".len()..];
        if !after_marker.starts_with('(') {
            rest = after_marker;
            base += pos + ".package".len();
            continue;
        }
        let after = &after_marker[1..];
        let Some(close) = balanced_close(after) else {
            break;
        };
        calls.push((base + pos, base + pos + ".package".len() + 1 + close + 1));
        rest = &after[close + 1..];
        base += pos + ".package".len() + 1 + close + 1;
    }
    let mut hits = Vec::new();
    for (start, end) in &calls {
        let body = &text[start + ".package(".len()..*end - 1];
        if call_matches_key(body, &key) {
            hits.push((*start, *end));
        }
    }
    if hits.len() != 1 {
        return None;
    }
    let (start, end) = hits[0];
    let mut out = text.to_string();
    out.replace_range(start..end, "");
    // Hygiene: collapse the leftover blank line, then verify.
    while out.contains(",\n\n") {
        out = out.replacen(",\n\n", ",\n", 1);
    }
    let re = parse_swift_package_deps(&out);
    if re.iter().any(|d| {
        d.dep_name().to_lowercase() == key || d.dep_name().to_lowercase().replace('.', "/") == key
    }) {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod bump_tests {
    use super::*;

    const PKG: &str = concat!(
        "// swift-tools-version: 5.9\n",
        "import PackageDescription\n\n",
        "let package = Package(\n",
        "    name: \"demo\",\n",
        "    dependencies: [\n",
        "        .package(id: \"scope.lib\", from: \"1.0.0\"),\n",
        "        .package(url: \"https://github.com/example/other.git\", exact: \"2.0.0\"),\n",
        "        .package(url: \"https://github.com/example/pinned.git\", branch: \"main\"),\n",
        "    ],\n",
        ")\n",
    );

    #[test]
    fn bump_registry_from_requirement() {
        let out = bump_swift_requirement(PKG, "scope.lib", "1.1.0").expect("bump works");
        assert!(out.contains("from: \"1.1.0\""), "version bumped:\n{out}");
        assert!(!out.contains("from: \"1.0.0\""), "old pin gone:\n{out}");
        // Re-scan verifies the new pin (no toolchain needed).
        let deps = parse_swift_package_deps(&out);
        let hit = deps
            .iter()
            .find(|d| d.dep_name() == "scope/lib")
            .expect("re-scan finds dep");
        assert!(
            hit.requirement_text().contains("1.1.0"),
            "re-scan sees bump"
        );
    }

    #[test]
    fn bump_git_exact_requirement() {
        let out = bump_swift_requirement(PKG, "example/other", "2.1.0").expect("bump works");
        assert!(out.contains("exact: \"2.1.0\""), "exact bumped:\n{out}");
    }

    #[test]
    fn bump_refuses_branch_and_unknown() {
        // Branch pins have no version to bump — honest None, never a guess.
        assert!(bump_swift_requirement(PKG, "example/pinned", "9.9.9").is_none());
        assert!(bump_swift_requirement(PKG, "scope.missing", "1.0.0").is_none());
    }
}

#[cfg(test)]
mod dump_shape_tests {
    use super::*;

    /// Real `swift package dump-package` shape (registry dep with a
    /// lowerBound/upperBound range object) must parse to a usable dep.
    #[test]
    fn dump_registry_range_object_parses() {
        let doc = r#"{"dependencies":[{"source":["registry","scope.lib",{"range":[{"lowerBound":"1.0.0","upperBound":"2.0.0"}]}]}]}"#;
        let deps = parse_dump_package(doc).unwrap();
        assert_eq!(deps.len(), 1, "dep must parse");
        assert_eq!(deps[0].dep_name(), "scope/lib");
        assert!(
            deps[0].requirement_text().contains("1.0.0"),
            "range carries bounds: {}",
            deps[0].requirement_text()
        );
    }
}

#[cfg(test)]
mod dump_e2e_shape_tests {
    use super::*;

    /// Full realistic dump-package document (registry dep, range object,
    /// traits array) must yield a usable dep — this is the exact shape
    /// `swift package dump-package` emits.
    #[test]
    fn full_dump_document_yields_dep() {
        let doc = r#"{"dependencies":[{"registry":[{"identity":"scope.lib","productFilter":null,"requirement":{"range":[{"lowerBound":"1.0.0","upperBound":"2.0.0"}]},"traits":[{"name":"default"}]}]}]}"#;
        let deps = parse_dump_package(doc).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].dep_name(), "scope/lib");
        assert!(deps[0].requirement_text().contains("1.0.0"));
    }
}

#[cfg(test)]
mod remove_tests {
    use super::*;

    const PKG: &str = concat!(
        "// swift-tools-version: 5.9\n",
        "import PackageDescription\n\n",
        "let package = Package(\n",
        "    name: \"demo\",\n",
        "    dependencies: [\n",
        "        .package(id: \"scope.lib\", from: \"1.0.0\"),\n",
        "        .package(url: \"https://github.com/example/other.git\", exact: \"2.0.0\"),\n",
        "    ],\n",
        ")\n",
    );

    #[test]
    fn remove_registry_call() {
        let out = remove_swift_requirement(PKG, "scope.lib").expect("remove works");
        assert!(!out.contains("scope.lib"), "dep gone:\n{out}");
        assert!(out.contains("example/other"), "other survives:\n{out}");
        // Re-scan proves exactly one removal.
        let deps = parse_swift_package_deps(&out);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].dep_name(), "github.com/example/other");
    }

    #[test]
    fn remove_git_call_by_tail() {
        let out = remove_swift_requirement(PKG, "example/other").expect("remove works");
        assert!(!out.contains("other.git"), "dep gone");
        assert!(out.contains("scope.lib"), "other survives");
    }

    #[test]
    fn remove_unknown_is_none() {
        assert!(remove_swift_requirement(PKG, "scope.missing").is_none());
    }
}
