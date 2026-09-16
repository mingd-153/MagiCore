//! Swift Package Manager native engine (SwiftPM registry + git deps).
//! Engine native Swift Package Manager (SwiftPM registry + dep git).
//!
//! Registry spec: version list at `{registry}/{scope}/{name}.json`
//! (`{"versions":[...]}`), source archive at
//! `{registry}/{scope}/{name}/{version}.zip` and the mandatory checksum at
//! `{registry}/{scope}/{name}/{version}.sha256` (hex sha256 of the zip —
//! a registry that omits it is a fail-closed integrity error). Selection
//! mirrors SwiftPM: the HIGHEST matching version wins. Transitive deps are
//! read from the `Package.swift` inside the archive (hand-rolled scan of
//! `.package(...)` declarations — no Swift toolchain needed during
//! resolve). Git-only deps (`.package(url:)`) resolve through
//! `git clone --depth 1 --branch {tag}` via mgc-exec (allowlisted), with
//! the commit SHA recorded as provenance (`git-commit:` marker); tags are
//! discovered with `git ls-remote --tags` and the highest matching tag is
//! selected. Materialization is the mgc checkouts layout
//! `{swift_root}/checkouts/{identity}-{version}` plus a SwiftPM-compatible
//! `Package.resolved` export.
//! Spec registry: danh sách version tại `{registry}/{scope}/{name}.json`
//! (`{"versions":[...]}`), archive tại
//! `{registry}/{scope}/{name}/{version}.zip` và checksum bắt buộc tại
//! `{registry}/{scope}/{name}/{version}.sha256` (hex sha256 của zip —
//! registry thiếu checksum là lỗi integrity fail-closed). Selection theo
//! SwiftPM: version CAO NHẤT khớp thắng. Dep bắc cầu đọc từ `Package.swift`
//! bên trong archive (quét thủ công khai báo `.package(...)` — không cần
//! toolchain Swift khi resolve). Dep chỉ-git (`.package(url:)`) resolve qua
//! `git clone --depth 1 --branch {tag}` bằng mgc-exec (đã allowlist), SHA
//! commit được ghi làm provenance (marker `git-commit:`); tag dò bằng
//! `git ls-remote --tags` và tag cao nhất khớp được chọn. Materialize là
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
    client: reqwest::Client,
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
            client: reqwest::Client::new(),
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
            .send()
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

    /// Git-only resolve (name is a scheme-less host path or a file:// URL).
    /// Resolve chỉ-git (name là host path không scheme hoặc URL file://).
    async fn resolve_git(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let url = git_name_to_url(name);
        // Tag requirement → select the highest matching remote tag; branch /
        // revision → use as-is (fail-closed on fetch errors).
        // (Yêu cầu tag → chọn tag remote cao nhất khớp; branch / revision →
        // dùng nguyên bản (lỗi fetch fail-closed).)
        let ref_ = match parse_requirement_kind(range) {
            RequirementKind::Branch(b) => b,
            RequirementKind::Revision(r) => r,
            RequirementKind::Version => {
                // A `tag:` prefix is a version requirement over tags — strip
                // it before range matching.
                // (Tiền tố `tag:` là yêu cầu version trên tag — bỏ trước
                // khi khớp khoảng.)
                let tag_range = range.trim().strip_prefix("tag:").unwrap_or(range);
                let tag = highest_matching_tag(&url, tag_range)?;
                tag.ok_or_else(|| {
                    MgError::Other(format!(
                        "no tag of {url} matches range '{range}' (fail-closed)"
                    ))
                })?
            }
        };
        let workdir = temp_checkout_dir(name)?;
        clone_shallow(&url, &ref_, &workdir)?;
        let commit = run_git_capture(&["rev-parse", "HEAD"], Some(&workdir))?
            .trim()
            .to_string();
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(MgError::Integrity(format!(
                "git rev-parse returned no commit SHA for {url} @ {ref_} (fail-closed)"
            )));
        }
        let manifest_text = std::fs::read_to_string(workdir.join("Package.swift"))
            .map_err(|e| MgError::Other(format!("read cloned Package.swift: {e}")))?;
        let deps = parse_swift_package_deps(&manifest_text)
            .into_iter()
            .map(|d| (d.dep_name(), d.requirement_text()))
            .collect();
        let _ = std::fs::remove_dir_all(&workdir);
        Ok(ResolvedEntry {
            name: name.to_string(),
            version: ref_.clone(),
            deps,
            artifact_url: url.clone(),
            sha256: String::new(),
            extra_markers: vec![
                "swift-git".to_string(),
                format!("git-commit:{commit}"),
                format!("git-ref:{ref_}"),
            ],
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

    /// Materialize a GIT entry: `git clone --depth 1 --branch {ref}` into
    /// `{swift_root}/checkouts/{repo}-{version}`, then verify the checkout's
    /// HEAD against the resolve-time provenance SHA when one is recorded
    /// (a moved tag fails closed). Without a recorded SHA the checkout is
    /// produced but flagged unverified (loud warning — no silent trust).
    /// Materialize entry GIT: `git clone --depth 1 --branch {ref}` vào
    /// `{swift_root}/checkouts/{repo}-{version}`, rồi xác minh HEAD của
    /// checkout theo SHA provenance lúc resolve khi có ghi (tag bị dịch là
    /// fail-closed). Không có SHA đã ghi thì checkout vẫn tạo nhưng bị đánh
    /// dấu chưa xác minh (cảnh báo ỒN ÀO — không tin tưởng âm thầm).
    pub fn materialize_git(
        &self,
        entry: &ResolvedEntry,
        expected_sha: Option<&str>,
        swift_root: &Path,
    ) -> MgResult<PathBuf> {
        let dir = swift_root.join("checkouts").join(format!(
            "{}-{}",
            git_repo_name(&entry.name),
            entry.version
        ));
        let ref_ = entry
            .extra_markers
            .iter()
            .find_map(|m| m.strip_prefix("git-ref:"))
            .unwrap_or(&entry.version)
            .to_string();
        let url = git_name_to_url(&entry.name);
        if dir.exists() {
            // Idempotent re-run: verify the existing checkout instead of
            // clobbering it.
            // (Chạy lại idempotent: xác minh checkout có sẵn thay vì ghi đè.)
            let head = run_git_capture(&["rev-parse", "HEAD"], Some(&dir))?
                .trim()
                .to_string();
            if let Some(expected) = expected_sha
                && head != expected
            {
                return Err(MgError::Integrity(format!(
                    "checkout {} moved: expected {expected}, found {head} (fail-closed)",
                    dir.display()
                )));
            }
            return Ok(dir);
        }
        std::fs::create_dir_all(swift_root.join("checkouts"))?;
        match clone_shallow(&url, &ref_, &dir) {
            Ok(()) => {}
            Err(e) => {
                // A partially-created clone must not poison later runs.
                // (Clone tạo dở không được làm hỏng lần chạy sau.)
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
        let head = run_git_capture(&["rev-parse", "HEAD"], Some(&dir))?
            .trim()
            .to_string();
        match expected_sha {
            Some(expected) if head != expected => {
                return Err(MgError::Integrity(format!(
                    "{} checkout SHA {head} does not match the resolve-time pin {expected} (fail-closed)",
                    entry.name
                )));
            }
            Some(_) => {}
            None => eprintln!(
                "WARNING: no resolve-time SHA recorded for {} @ {} — checkout pinned at {head} UNVERIFIED",
                entry.name, entry.version
            ),
        }
        Ok(dir)
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
        // Shared default sha256 check (the registry checksum is mandatory
        // and already carried on the entry).
        // (Kiểm tra sha256 chung (checksum registry là bắt buộc và đã mang
        // trên entry).)
        if entry.sha256.is_empty() {
            return Ok(());
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

/// Git-name detection: registry names are `scope/name` (exactly one slash,
/// dot-less scope); anything with a host-like first segment, `.git` suffix
/// or an explicit file:// scheme is a git-only dependency.
/// Dò tên git: tên registry là `scope/name` (đúng một dấu sẹo, scope không
/// chấm); mọi tên có segment đầu kiểu host, đuôi `.git` hoặc scheme file://
/// tường minh là dep chỉ-git.
pub fn is_git_name(name: &str) -> bool {
    name.starts_with("file://")
        || name.ends_with(".git")
        || name.matches('/').count() > 1
        || name
            .split('/')
            .next()
            .is_some_and(|first| first.contains('.') && !first.contains(".."))
}

/// Map a dependency name to its clone URL.
/// Ánh xạ tên dep sang URL clone.
fn git_name_to_url(name: &str) -> String {
    if name.starts_with("file://") {
        return name.to_string();
    }
    if name.ends_with(".git") {
        format!("https://{name}")
    } else {
        format!("https://{name}.git")
    }
}

/// Map a `.package(url:)` value to the graph-edge name (scheme + `.git`
/// stripped; file:// URLs ride as-is — PackageName accepts `:`).
/// Ánh xạ giá trị `.package(url:)` sang tên cạnh graph (bỏ scheme + `.git`;
/// URL file:// giữ nguyên — PackageName chấp nhận `:`).
pub fn git_url_to_name(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        rest.trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or(rest.trim_end_matches('/'))
            .to_string()
    } else {
        url.to_string()
    }
}

/// Repository display name (last path segment, `.git` stripped).
/// Tên hiển thị repo (segment cuối, bỏ `.git`).
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

enum RequirementKind {
    /// A tag-selectable version requirement (from/exact/range/bare pin/*).
    /// Yêu cầu version chọn được theo tag (from/exact/range/pin trần/*).
    Version,
    Branch(String),
    Revision(String),
}

fn parse_requirement_kind(range: &str) -> RequirementKind {
    let range = range.trim();
    if let Some(b) = range.strip_prefix("branch:") {
        return RequirementKind::Branch(b.trim().to_string());
    }
    if let Some(r) = range.strip_prefix("revision:") {
        return RequirementKind::Revision(r.trim().to_string());
    }
    RequirementKind::Version
}

/// Highest remote tag matching `range` (SPM accepts tags with or without a
/// leading `v`). Requires `git ls-remote --tags` via mgc-exec (allowlisted).
/// Tag remote cao nhất khớp `range` (SPM nhận tag có hoặc không có tiền tố
/// `v`). Cần `git ls-remote --tags` qua mgc-exec (đã allowlist).
fn highest_matching_tag(url: &str, range: &str) -> MgResult<Option<String>> {
    let out = run_git_capture(&["ls-remote", "--tags", url], None)?;
    let mut best: Option<(Version, String)> = None;
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        let Some(_sha) = parts.next() else {
            continue;
        };
        let Some(ref_name) = parts.next() else {
            continue;
        };
        let Some(tag) = ref_name.rsplit('/').next() else {
            continue;
        };
        // Skip peeled `^{}` duplicate lines — keep the ref tag only.
        // (Bỏ dòng `^{}` đã bóc — chỉ giữ ref tag.)
        if tag.contains('^') {
            continue;
        }
        let Ok(v) = Version::parse(tag) else {
            continue;
        };
        if swift_matches(range, &v) && best.as_ref().is_none_or(|(bv, _)| v > *bv) {
            best = Some((v, tag.to_string()));
        }
    }
    Ok(best.map(|(_, tag)| tag))
}

/// `git clone --depth 1 --branch {ref}` (mgc-exec allowlisted; `git` is on
/// the install-scope allowlist). The clone runs with cwd = dest's parent
/// and a RELATIVE dest name — the exec path-traversal guard canonicalizes
/// every path-looking arg against the cwd boundary, so absolute paths
/// outside it are rejected; a relative single-component dest never trips
/// the guard and the file:// URL arg fails canonicalize (nonexistent) and
/// carries no `..`, which the guard accepts as a URL.
/// `git clone --depth 1 --branch {ref}` (mgc-exec allowlist; `git` nằm
/// trong allowlist scope install). Clone chạy với cwd = cha của dest và
/// dest TƯƠNG ĐỐI — guard traversal của exec canonicalize mọi arg kiểu path
/// theo biên cwd, nên path tuyệt đối ngoài biên bị từ chối; dest tương đối
/// một thành phần không đụng guard và URL file:// không canonicalize được
/// (không tồn tại) cùng không mang `..` — guard chấp nhận như URL.
fn clone_shallow(url: &str, ref_: &str, dest: &Path) -> MgResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| MgError::Other(format!("clone dest has no parent: {}", dest.display())))?;
    std::fs::create_dir_all(parent)?;
    let dest_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            MgError::Other(format!(
                "clone dest is not a plain name: {}",
                dest.display()
            ))
        })?
        .to_string();
    let args = [
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "--branch".to_string(),
        ref_.to_string(),
        url.to_string(),
        dest_name,
    ];
    run_git(&args, Some(parent))
}

fn run_git(args: &[String], cwd: Option<&Path>) -> MgResult<()> {
    let opts = mgc_exec::run::ExecOptions {
        cwd: cwd.map(Path::to_path_buf),
        ..Default::default()
    };
    let report = mgc_exec::run::run("git", args, &opts)
        .map_err(|e| MgError::Other(format!("git {} failed: {e}", args.join(" "))))?;
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "git {} exited {}: {}",
            args.join(" "),
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }
    Ok(())
}

/// git stdout capture with FULL stdout (ls-remote tag lists exceed the
/// line-bounded tail).
/// Bắt stdout git ĐẦY ĐỦ (danh sách tag ls-remote vượt tail giới hạn dòng).
fn run_git_capture(args: &[&str], cwd: Option<&Path>) -> MgResult<String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let opts = mgc_exec::run::ExecOptions {
        cwd: cwd.map(Path::to_path_buf),
        capture_full_stdout: true,
        ..Default::default()
    };
    let report = mgc_exec::run::run("git", &owned, &opts)
        .map_err(|e| MgError::Other(format!("git {} failed: {e}", owned.join(" "))))?;
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "git {} exited {}: {}",
            owned.join(" "),
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }
    Ok(report.stdout_full)
}

/// Unique temp work dir for clones (runtime — no tempfile dependency).
/// Thư mục làm việc temp duy nhất cho clone (runtime — không phụ thuộc
/// tempfile).
fn temp_checkout_dir(label: &str) -> MgResult<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("mgc-swift-{}-{nanos}", sanitize_temp_label(label)));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn sanitize_temp_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(48)
        .collect()
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
        let Some(source) = dep.get("source").and_then(Value::as_array) else {
            continue;
        };
        let Some(kind) = source.first().and_then(Value::as_str) else {
            continue;
        };
        let target = source.get(1).and_then(Value::as_str).unwrap_or_default();
        let requirement = dump_requirement(source.get(2));
        if requirement.is_none() {
            // Unmappable requirement — honest skip (cannot pin
            // deterministically).
            // (Requirement không ánh xạ được — skip trung thực (không ghim
            // tất định được).)
            eprintln!(
                "WARNING: dump-package dependency '{target}' has an unmappable requirement — skipped"
            );
            continue;
        }
        match kind {
            "registry" => deps.push(SwiftDep::Registry {
                identity: target.to_string(),
                requirement: requirement.unwrap_or_default(),
            }),
            "remote" => deps.push(SwiftDep::Git {
                url: target.to_string(),
                requirement: requirement.unwrap_or_default(),
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
    Ok(deps)
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
            .filter_map(Value::as_str)
            .map(|r| format!("range:{}", r.trim()))
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("||"));
        }
    }
    if let Some(r) = value.get("range").and_then(Value::as_str) {
        return Some(format!("range:{}", r.trim()));
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
        assert!(is_git_name("file:///tmp/repo"));
        assert_eq!(
            git_url_to_name("https://github.com/example/repo.git"),
            "github.com/example/repo"
        );
        assert_eq!(git_url_to_name("file:///tmp/repo"), "file:///tmp/repo");
        assert_eq!(git_repo_name("github.com/example/repo"), "repo");
        assert_eq!(git_repo_name("host/x.git"), "x");
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
