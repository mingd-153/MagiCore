//! Maven Central native engine (Java/Maven repositories).
//! Engine native Maven Central (repository Java/Maven).
//!
//! Repository layout: `{repo}/{group-path}/{artifact}/maven-metadata.xml`
//! lists `<versions>`; each version's POM at
//! `{gpath}/{artifact}/{version}/{artifact}-{version}.pom` carries the
//! dependency graph (`<dependencies>` — `dependencyManagement` blocks are
//! stripped first so managed entries are never treated as real deps; only
//! compile/runtime scopes enter the graph; `${property}`/missing versions
//! are honestly skipped with markers — full dependencyManagement resolution
//! is P2.1). A `<packaging>pom</packaging>` POM is a BOM: no transitive
//! graph, main lock only. The artifact jar is verified against its
//! published `.jar.sha256` (preferred) or `.jar.sha1` checksum — fail-closed
//! when neither exists.
//! Layout repository: `{repo}/{group-path}/{artifact}/maven-metadata.xml`
//! liệt kê `<versions>`; POM của từng version tại
//! `{gpath}/{artifact}/{version}/{artifact}-{version}.pom` mang graph
//! dependency (khối `<dependencies>` — khối `dependencyManagement` bị cắt
//! trước nên entry managed không bao giờ bị coi là dep thật; chỉ scope
//! compile/runtime vào graph; version `${property}`/thiếu được skip trung
//! thực kèm marker — resolve dependencyManagement đầy đủ là P2.1). POM có
//! `<packaging>pom</packaging>` là BOM: không graph bắc cầu, chỉ lock
//! chính. Jar artifact được xác minh theo checksum `.jar.sha256` (ưu tiên)
//! hoặc `.jar.sha1` công bố — fail-closed khi không có cái nào.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use mgc_types::{MgError, MgResult, Version};
// sha1 0.10 and sha2 0.10 share the same digest 0.10 `Digest` trait — one
// import covers both hashers (no new crate: sha1 is a pinned workspace dep).
// (sha1 0.10 và sha2 0.10 dùng chung trait `Digest` của digest 0.10 — một
// import phủ cả hai hasher — không thêm crate: sha1 là workspace dep đã ghim.)
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const DEFAULT_REPO_URL: &str = "https://repo.maven.apache.org/maven2";

/// Native Maven repository engine.
/// Engine native repository Maven.
#[derive(Debug, Clone)]
pub struct MavenProtocol {
    repo_url: String,
    client: reqwest::Client,
}

impl MavenProtocol {
    /// Build with an explicit repository base URL.
    /// Dựng với URL gốc repository tường minh.
    pub fn new(repo_url: &str) -> Self {
        Self {
            repo_url: repo_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Build from environment: `MGC_MAVEN_REPO_URL`.
    /// Dựng từ môi trường: `MGC_MAVEN_REPO_URL`.
    pub fn from_env() -> Self {
        let repo_url = std::env::var("MGC_MAVEN_REPO_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_REPO_URL.to_string());
        Self::new(&repo_url)
    }

    /// `{repo}/{group-path}/{artifact}/{version}/{file}` — group
    /// `com.example.foo` → path `com/example/foo`.
    /// `{repo}/{group-path}/{artifact}/{version}/{file}` — group
    /// `com.example.foo` → path `com/example/foo`.
    fn repo_url_for(&self, group: &str, artifact: &str, version: &str, file: &str) -> String {
        let gpath = group.replace('.', "/");
        if version.is_empty() {
            format!("{}/{gpath}/{artifact}/{file}", self.repo_url)
        } else {
            format!("{}/{gpath}/{artifact}/{version}/{file}", self.repo_url)
        }
    }

    /// Split a `groupId:artifactId` coordinate.
    /// Tách tọa độ `groupId:artifactId`.
    pub fn split_coordinate(name: &str) -> MgResult<(String, String)> {
        let (group, artifact) = name.split_once(':').ok_or_else(|| {
            MgError::Other(format!(
                "maven coordinate must be 'group:artifact': '{name}'"
            ))
        })?;
        if group.is_empty() || artifact.is_empty() {
            return Err(MgError::Other(format!(
                "maven coordinate has an empty part: '{name}'"
            )));
        }
        Ok((group.to_string(), artifact.to_string()))
    }

    async fn get_text(&self, url: &str) -> MgResult<(u16, String)> {
        let resp = self
            .client
            .get(url)
            .send()
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
            .send()
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

    /// Fetch the POM for a concrete `(group, artifact, version)`.
    /// Tải POM cho `(group, artifact, version)` cụ thể.
    pub async fn download_pom(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
    ) -> MgResult<Vec<u8>> {
        self.get_bytes(&self.repo_url_for(
            group,
            artifact,
            version,
            &format!("{artifact}-{version}.pom"),
        ))
        .await
    }

    /// Materialize a resolved artifact into the local Maven repository
    /// layout `{m2_root}/repository/{gpath}/{artifact}/{version}/` (jar +
    /// pom + published checksums) — readable by `mvn -o`.
    /// Materialize artifact đã resolve vào layout local Maven repository
    /// Resolve one dependency version: literal → properties →
    /// dependencyManagement → lazy parent chain → fail-closed error.
    /// Parent POMs fetch ONLY when needed (an unneeded unreachable parent
    /// must not break resolution); at most 3 ancestors, cycle-guarded.
    /// (Resolve version một dependency: literal → properties →
    /// dependencyManagement → chuỗi parent lười → lỗi fail-closed. POM cha
    /// chỉ fetch khi cần; tối đa 3 đời, chặn chu trình.)
    #[allow(clippy::too_many_arguments)]
    async fn maven_dep_version(
        &self,
        pom: &str,
        group: &str,
        artifact: &str,
        version: Option<&str>,
        props: &mut std::collections::HashMap<String, String>,
        managed: &mut std::collections::HashMap<(String, String), String>,
        visited_parents: &mut std::collections::HashSet<String>,
    ) -> MgResult<String> {
        // Merge parent knowledge lazily until this dep resolves or the
        // chain is exhausted (at most 3 ancestor fetches, cycle-guarded;
        // `current` walks up the chain so grandparents resolve too).
        // (Merge tri thức cha lười tới khi dep này resolve xong hoặc hết
        // chuỗi.)
        //
        // REVIEW: an empty `<version></version>` is a missing version, not
        // an empty-string edge — it takes the managed/parent path exactly
        // like an absent version (real Maven semantics).
        let version = version.filter(|v| !v.trim().is_empty());
        let mut current = pom.to_string();
        for _ in 0..4 {
            if let Some(raw) = version {
                if !raw.contains("${") {
                    return Ok(raw.to_string());
                }
                if let Some(resolved) = substitute_properties(raw, props) {
                    return Ok(resolved);
                }
            } else if let Some(pinned) = managed.get(&(group.to_string(), artifact.to_string())) {
                return Ok(pinned.clone());
            }
            match self
                .merge_parent_data(&current, props, managed, visited_parents)
                .await?
            {
                Some(parent_pom) => current = parent_pom,
                None => break,
            }
        }
        match version {
            Some(raw) => Err(MgError::Other(format!(
                "unresolvable version '{raw}' for {group}:{artifact} — declare a literal version, a <properties> entry, or a resolvable parent POM (fail-closed, no silent drop)"
            ))),
            None => Err(MgError::Other(format!(
                "no version for {group}:{artifact} and no dependencyManagement entry (checked parent chain) — declare one (fail-closed, no silent drop)"
            ))),
        }
    }

    /// Merge ONE ancestor level (properties + managed versions, child
    /// wins): returns the parent POM text when a new ancestor was merged,
    /// None when the chain ends here. Unreachable parents fail (needed
    /// data must not come from nowhere); a missing/unparseable `<parent>`
    /// block simply ends the chain.
    /// (Merge MỘT tầng tổ tiên: trả text POM cha khi merge được tầng mới,
    /// None khi hết chuỗi.)
    async fn merge_parent_data(
        &self,
        pom: &str,
        props: &mut std::collections::HashMap<String, String>,
        managed: &mut std::collections::HashMap<(String, String), String>,
        visited_parents: &mut std::collections::HashSet<String>,
    ) -> MgResult<Option<String>> {
        let Some((group, artifact, version)) = parse_parent_coords(pom) else {
            return Ok(None);
        };
        let coordinate = format!("{group}:{artifact}:{version}");
        if visited_parents.contains(&coordinate) || visited_parents.len() >= 3 {
            return Ok(None);
        }
        visited_parents.insert(coordinate);
        let url = self.repo_url_for(
            &group,
            &artifact,
            &version,
            &format!("{artifact}-{version}.pom"),
        );
        let (status, parent_pom) = self.get_text(&url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {url} returned {status} — parent POM is required to resolve versions (fail-closed)"
            )));
        }
        for (key, value) in collect_pom_properties(&parent_pom) {
            props.entry(key).or_insert(value);
        }
        for (key, value) in collect_managed_versions(&parent_pom) {
            managed.entry(key).or_insert(value);
        }
        Ok(Some(parent_pom))
    }

    /// `{m2_root}/repository/{gpath}/{artifact}/{version}/` (jar + pom +
    /// checksum công bố) — đọc được bởi `mvn -o`.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        jar_bytes: &[u8],
        pom_bytes: &[u8],
        m2_root: &std::path::Path,
    ) -> MgResult<PathBuf> {
        let (group, artifact) = Self::split_coordinate(&entry.name)?;
        let gpath = group.replace('.', "/");
        let version = &entry.version;
        let dir = m2_root
            .join("repository")
            .join(gpath)
            .join(&artifact)
            .join(version);
        std::fs::create_dir_all(&dir)?;
        let jar_name = format!("{artifact}-{version}.jar");
        let pom_name = format!("{artifact}-{version}.pom");
        std::fs::write(dir.join(&jar_name), jar_bytes)?;
        std::fs::write(dir.join(&pom_name), pom_bytes)?;
        // Write the checksums we actually verified — mvn -o re-checks them.
        // (Viết đúng checksum mình đã xác minh — mvn -o kiểm tra lại.)
        if entry.sha256.is_empty() {
            // sha1-only provenance recorded in markers.
            // (Nguồn sha1-only được ghi trong marker.)
            if let Some(marker) = entry
                .extra_markers
                .iter()
                .find(|m| m.starts_with("checksum-sha1:"))
            {
                std::fs::write(
                    dir.join(format!("{jar_name}.sha1")),
                    marker.trim_start_matches("checksum-sha1:"),
                )?;
            }
        } else {
            std::fs::write(dir.join(format!("{jar_name}.sha256")), &entry.sha256)?;
        }
        Ok(dir.join(jar_name))
    }
}

impl Default for MavenProtocol {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl RegistryProtocol for MavenProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let (group, artifact) = Self::split_coordinate(name)?;

        // maven-metadata.xml → published versions.
        // (maven-metadata.xml → các version đã công bố.)
        let metadata_url = self.repo_url_for(&group, &artifact, "", "maven-metadata.xml");
        let (status, body) = self.get_text(&metadata_url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {metadata_url} returned {status} — no version metadata for {name} (fail-closed)"
            )));
        }
        let mut best: Option<(Version, String)> = None;
        for versions_block in element_blocks(&body, "versions") {
            for v in element_blocks(versions_block, "version") {
                let raw = v.trim();
                let Ok(version) = Version::parse(raw) else {
                    continue;
                };
                if !maven_matches(range, &version) {
                    continue;
                }
                // Tie-break by raw string so distinct Maven qualifiers
                // (1.2.3.Final vs 1.2.3) stay deterministic.
                // (Tie-break theo chuỗi thô để qualifier Maven khác nhau
                // (1.2.3.Final vs 1.2.3) vẫn tất định.)
                if best
                    .as_ref()
                    .is_none_or(|(bv, br)| version > *bv || (version == *bv && raw > br.as_str()))
                {
                    best = Some((version, raw.to_string()));
                }
            }
        }
        let (_, version) = best.ok_or_else(|| {
            MgError::Other(format!("no version of {name} matches range '{range}'"))
        })?;

        let pom_url = self.repo_url_for(
            &group,
            &artifact,
            &version,
            &format!("{artifact}-{version}.pom"),
        );
        let (pom_status, pom) = self.get_text(&pom_url).await?;
        if !(200..300).contains(&pom_status) {
            return Err(MgError::Network(format!(
                "GET {pom_url} returned {pom_status} — the POM is required to build the graph (fail-closed)"
            )));
        }

        let mut markers = Vec::new();
        let (deps_xml, packaging) = collect_pom_dependencies(&pom);
        // Own-POM knowledge first; parent POMs merge lazily below, child
        // wins on every key (standard Maven inheritance).
        // (Tri thức POM hiện tại trước; POM cha merge lười bên dưới, con
        // thắng mọi key.)
        let mut props = collect_pom_properties(&pom);
        let mut managed = collect_managed_versions(&pom);
        let mut visited_parents: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut deps = Vec::new();
        if packaging.as_deref() == Some("pom") {
            // BOM: no transitive graph — the main lock pins the BOM itself.
            // (BOM: không graph bắc cầu — lock chính ghim chính BOM.)
            markers.push("bom-packaging".to_string());
        } else {
            for dep in &deps_xml {
                let (Some(g), Some(a)) = (&dep.group, &dep.artifact) else {
                    continue;
                };
                let scope = dep.scope.as_deref().unwrap_or("compile");
                // Only compile/runtime scopes enter the graph.
                // (Chỉ scope compile/runtime vào graph.)
                if !matches!(scope, "compile" | "runtime" | "") {
                    markers.push(format!("scope-skip:{scope}:{g}:{a}"));
                    continue;
                }
                if dep.optional {
                    markers.push(format!("optional:{g}:{a}"));
                    continue;
                }
                let version = self
                    .maven_dep_version(
                        &pom,
                        g,
                        a,
                        dep.version.as_deref(),
                        &mut props,
                        &mut managed,
                        &mut visited_parents,
                    )
                    .await?;
                deps.push((format!("{g}:{a}"), version));
            }
        }

        // Jar checksum: .sha256 preferred, .sha1 fallback, neither →
        // fail-closed Integrity.
        // (Checksum jar: .sha256 ưu tiên, .sha1 fallback, không có gì →
        // fail-closed Integrity.)
        let jar_name = format!("{artifact}-{version}.jar");
        let sha256_url =
            self.repo_url_for(&group, &artifact, &version, &format!("{jar_name}.sha256"));
        let sha1_url = self.repo_url_for(&group, &artifact, &version, &format!("{jar_name}.sha1"));
        let (sha256_status, sha256_body) = self.get_text(&sha256_url).await?;
        let sha256 = if (200..300).contains(&sha256_status) {
            let hex = sha256_body.trim().to_ascii_lowercase();
            if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(MgError::Integrity(format!(
                    "malformed .jar.sha256 for {name}:{version}: '{hex}'"
                )));
            }
            hex
        } else {
            let (sha1_status, sha1_body) = self.get_text(&sha1_url).await?;
            if !(200..300).contains(&sha1_status) {
                return Err(MgError::Integrity(format!(
                    "no .sha256/.sha1 published for {name}:{version} jar — cannot verify (fail-closed)"
                )));
            }
            let hex = sha1_body.trim().to_ascii_lowercase();
            if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(MgError::Integrity(format!(
                    "malformed .jar.sha1 for {name}:{version}: '{hex}'"
                )));
            }
            markers.push(format!("checksum-sha1:{hex}"));
            String::new()
        };

        let jar_url = self.repo_url_for(&group, &artifact, &version, &jar_name);
        Ok(ResolvedEntry {
            name: name.to_string(),
            version,
            deps,
            artifact_url: jar_url,
            sha256,
            extra_markers: markers,
        })
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        self.get_bytes(&entry.artifact_url).await
    }

    /// Verify overrides the shared default: sha256 when published, the
    /// recorded sha1 marker otherwise, and fail-closed when neither is
    /// available (an unverified jar is never installed).
    /// (Verify ghi đè mặc định: sha256 khi có, marker sha1 đã ghi nếu
    /// ngược lại, fail-closed khi không có cái nào — jar chưa xác minh
    /// không bao giờ được cài.)
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        if !entry.sha256.is_empty() {
            let actual = hex_sha256(bytes);
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
            .find(|m| m.starts_with("checksum-sha1:"))
            .ok_or_else(|| {
                MgError::Integrity(format!(
                    "no integrity source for {}@{} (no sha256, no sha1) — fail-closed",
                    entry.name, entry.version
                ))
            })?;
        let expected = marker.trim_start_matches("checksum-sha1:");
        let mut hasher = Sha1::new();
        hasher.update(bytes);
        let actual = hex::encode(hasher.finalize());
        if actual != expected {
            return Err(MgError::Integrity(format!(
                "sha1 mismatch for {}@{}: expected {expected}, got {actual}",
                entry.name, entry.version
            )));
        }
        Ok(())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Maven version-range subset: `*`, bare version = caret (soft requirement
/// approximation — Maven's "soft" pin means ≥ this within the stream),
/// `[x]` exact, `[a,b)` / `(a,b]` intervals, comma = AND.
/// Subset khoảng version Maven: `*`, version trần = caret (xấp xỉ
/// requirement mềm — pin "mềm" của Maven nghĩa là ≥ giá trị đó trong cùng
/// dòng), `[x]` chính xác, khoảng `[a,b)` / `(a,b]`, phẩy = AND.
fn maven_matches(range: &str, version: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    if range.starts_with('[') || range.starts_with('(') {
        return maven_interval(range, version);
    }
    range
        .split(',')
        .map(str::trim)
        .all(|part| maven_part(part, version))
}

fn maven_interval(range: &str, version: &Version) -> bool {
    let Some(close) = range.rfind([')', ']']) else {
        return false;
    };
    let (open_ch, close_ch) = (range.as_bytes()[0] as char, range.as_bytes()[close] as char);
    let inner = &range[1..close];
    let bounds: Vec<&str> = inner.split(',').map(str::trim).collect();
    match bounds.len() {
        1 => {
            // [x] exact; (x,) style is not a valid single-bound form.
            // ([x] chính xác; dạng (x,) một biên không hợp lệ.)
            open_ch == '['
                && close_ch == ']'
                && Version::parse(bounds[0])
                    .map(|t| version == &t)
                    .unwrap_or(false)
        }
        2 => {
            let lower_ok = match (open_ch, bounds[0].is_empty()) {
                (_, true) => true,
                ('[', false) => Version::parse(bounds[0])
                    .map(|t| version >= &t)
                    .unwrap_or(false),
                ('(', false) => Version::parse(bounds[0])
                    .map(|t| version > &t)
                    .unwrap_or(false),
                _ => false,
            };
            let upper_ok = match (close_ch, bounds[1].is_empty()) {
                (_, true) => true,
                (']', false) => Version::parse(bounds[1])
                    .map(|t| version <= &t)
                    .unwrap_or(false),
                (')', false) => Version::parse(bounds[1])
                    .map(|t| version < &t)
                    .unwrap_or(false),
                _ => false,
            };
            lower_ok && upper_ok
        }
        _ => false,
    }
}

fn maven_part(part: &str, version: &Version) -> bool {
    if part == "*" {
        return true;
    }
    if let Some(p) = part.strip_prefix("[") {
        // Inside a comma-split bare range this is a nested exact pin.
        // (Trong khoảng trần tách bằng phẩy, đây là pin chính xác lồng nhau.)
        let p = p.trim_end_matches(']');
        return Version::parse(p).map(|t| version == &t).unwrap_or(false);
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
    // Bare version = caret (minimum-version stream semantics).
    // (Version trần = caret (ngữ nghĩa dòng minimum-version).)
    let Some(target) = Version::parse(part).ok() else {
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

/// One `<dependency>` entry of a POM.
/// Một entry `<dependency>` của POM.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PomDependency {
    pub group: Option<String>,
    pub artifact: Option<String>,
    pub version: Option<String>,
    pub scope: Option<String>,
    pub optional: bool,
}

/// Return the inner text of every `<tag>…</tag>` block (same-tag nesting is
/// counted; self-closing and attribute-carrying open tags are handled).
/// Trả nội dung bên trong của mọi khối `<tag>…</tag>` (đếm lồng cùng tên;
/// xử lý tag tự đóng và tag mở có attribute).
pub fn element_blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open_prefix) {
        let after = &rest[start + open_prefix.len()..];
        // Boundary check: `<version>` must not match `<versions>`.
        // (Kiểm tra biên: `<version>` không được khớp `<versions>`.)
        let boundary_ok = after.starts_with('>')
            || after.starts_with('/')
            || after.starts_with(|c: char| c.is_whitespace());
        if !boundary_ok {
            rest = &rest[start + open_prefix.len()..];
            continue;
        }
        if after.starts_with('/') {
            // Self-closing `<tag/>` — no content.
            // (Tự đóng `<tag/>` — không có nội dung.)
            let Some(gt) = rest[start..].find('>') else {
                break;
            };
            rest = &rest[start + gt + 1..];
            continue;
        }
        let Some(gt_rel) = rest[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        // Self-closing with attributes (`<dependency id="x" />`) — empty
        // content, keep scanning (a missing close tag must not abort the
        // whole scan).
        // (Tự đóng có attribute (`<dependency id="x" />`) — nội dung rỗng,
        // quét tiếp (thiếu close tag không được làm hỏng cả vòng quét).)
        if rest[start..=gt].ends_with("/>") {
            out.push("");
            rest = &rest[gt + 1..];
            continue;
        }
        let content_start = gt + 1;
        let mut depth = 1usize;
        let mut cursor = content_start;
        let mut end = None;
        while let Some(idx) = rest[cursor..].find('<') {
            let abs = cursor + idx;
            if rest[abs..].starts_with(&close) {
                depth -= 1;
                if depth == 0 {
                    end = Some(abs);
                    break;
                }
                cursor = abs + close.len();
                continue;
            }
            if rest[abs..].starts_with(&open_prefix) {
                let after2 = &rest[abs + open_prefix.len()..];
                if after2.starts_with('>') || after2.starts_with(|c: char| c.is_whitespace()) {
                    depth += 1;
                }
            }
            cursor = abs + 1;
        }
        match end {
            Some(e) => {
                out.push(&rest[content_start..e]);
                rest = &rest[e + close.len()..];
            }
            None => break,
        }
    }
    out
}

/// Remove every `<tag>…</tag>` block (used to strip
/// `dependencyManagement` before dependency collection).
/// Xóa mọi khối `<tag>…</tag>` (dùng để cắt `dependencyManagement` trước
/// khi gom dependency).
fn remove_element_blocks(xml: &str, tag: &str) -> String {
    let blocks = element_blocks(xml, tag);
    let mut out = xml.to_string();
    for block in blocks {
        // element_blocks returns non-overlapping, in-order slices — replace
        // each `<tag>…</tag>` occurrence once.
        // (element_blocks trả các slice không chồng lấn, theo thứ tự — thay
        // mỗi lần xuất hiện `<tag>…</tag>` đúng một lần.)
        let full = format!("<{tag}>{block}</{tag}>");
        out = out.replacen(&full, "", 1);
    }
    out
}

/// Collect a POM's real dependencies: `dependencyManagement` blocks are
/// removed first; returns `(deps, packaging)`. Public so adapters parse
/// their project pom.xml with the SAME parser as the engine.
/// Gom dependency thật của POM: khối `dependencyManagement` bị cắt trước;
/// trả `(deps, packaging)`. Public để adapter parse pom.xml của project
/// bằng CÙNG parser với engine.
pub fn collect_pom_dependencies(pom: &str) -> (Vec<PomDependency>, Option<String>) {
    let packaging = element_blocks(pom, "packaging")
        .first()
        .map(|s| s.trim().to_string());
    let stripped = remove_element_blocks(pom, "dependencyManagement");
    let mut deps = Vec::new();
    for block in element_blocks(&stripped, "dependencies") {
        for dep_xml in element_blocks(block, "dependency") {
            let field = |tag: &str| {
                element_blocks(dep_xml, tag)
                    .first()
                    .map(|s| s.trim().to_string())
            };
            deps.push(PomDependency {
                group: field("groupId"),
                artifact: field("artifactId"),
                version: field("version"),
                scope: field("scope"),
                optional: field("optional").is_some_and(|v| v.eq_ignore_ascii_case("true")),
            });
        }
    }
    (deps, packaging)
}

/// Properties of one POM: its `<properties>` entries plus the
/// `project.*` built-ins (groupId/artifactId/version of the POM itself).
/// Later merges never overwrite keys already present (child wins).
/// (Properties của một POM: entry `<properties>` cộng built-in
/// `project.*`. Merge sau không bao giờ ghi đè key đã có (con thắng).)
pub fn collect_pom_properties(pom: &str) -> std::collections::HashMap<String, String> {
    let mut props = std::collections::HashMap::new();
    for block in element_blocks(pom, "properties").iter().take(1) {
        for (key, value) in pom_direct_entries(block) {
            props.entry(key).or_insert(value);
        }
    }
    let stripped = remove_element_blocks(pom, "dependencyManagement");
    let stripped = remove_element_blocks(&stripped, "dependencies");
    let field = |tag: &str| {
        element_blocks(&stripped, tag)
            .first()
            .map(|s| s.trim().to_string())
    };
    if let (Some(group), Some(artifact), Some(version)) =
        (field("groupId"), field("artifactId"), field("version"))
    {
        props.entry("project.groupId".to_string()).or_insert(group);
        props
            .entry("project.artifactId".to_string())
            .or_insert(artifact);
        props
            .entry("project.version".to_string())
            .or_insert(version);
    }
    props
}

/// Direct `key → text` children of a block (nested blocks ignored).
/// (Các con trực tiếp `key → text` của một khối.)
fn pom_direct_entries(block: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = block;
    while let Some(start) = rest.find('<') {
        if rest[start..].starts_with("</") {
            break;
        }
        let after = &rest[start + 1..];
        let end = after
            .find(['>', ' ', '/'])
            .unwrap_or(after.len());
        let tag = &after[..end];
        if tag.is_empty() || tag.starts_with('?') || tag.starts_with('!') {
            rest = &after[end.min(after.len())..];
            continue;
        }
        let inner = element_blocks(rest, tag).first().map(|s| s.to_string());
        let Some(text) = inner else {
            break;
        };
        // Advance past this whole element for the next sibling.
        // (Tiến qua cả phần tử này để tới anh em kế.)
        if let Some(close) = rest.find(&format!("</{tag}>")) {
            rest = &rest[close + tag.len() + 3..];
        } else if let Some(gt) = rest[start..].find('>') {
            rest = &rest[start + gt + 1..];
        } else {
            break;
        }
        if text.contains('<') {
            continue;
        }
        out.push((tag.to_string(), text.trim().to_string()));
    }
    out
}

/// `dependencyManagement` versions with literal (non-`${}`) versions:
/// `(group, artifact) → version`.
/// (Version `dependencyManagement` dạng literal.)
pub fn collect_managed_versions(pom: &str) -> std::collections::HashMap<(String, String), String> {
    let mut managed = std::collections::HashMap::new();
    for management in element_blocks(pom, "dependencyManagement") {
        for block in element_blocks(management, "dependencies") {
            for dep_xml in element_blocks(block, "dependency") {
                let field = |tag: &str| {
                    element_blocks(dep_xml, tag)
                        .first()
                        .map(|s| s.trim().to_string())
                };
                if let (Some(g), Some(a), Some(v)) =
                    (field("groupId"), field("artifactId"), field("version"))
                    && !v.contains("${")
                {
                    managed.entry((g, a)).or_insert(v);
                }
            }
        }
    }
    managed
}

/// Coordinates of the `<parent>` block, if all three are literal.
/// (Tọa độ khối `<parent>`, nếu cả ba đều literal.)
pub fn parse_parent_coords(pom: &str) -> Option<(String, String, String)> {
    let block = element_blocks(pom, "parent").first()?.to_string();
    let field = |tag: &str| {
        element_blocks(&block, tag)
            .first()
            .map(|s| s.trim().to_string())
    };
    let (g, a, v) = (field("groupId")?, field("artifactId")?, field("version")?);
    if g.contains("${") || a.contains("${") || v.contains("${") {
        return None;
    }
    Some((g, a, v))
}

/// Substitute `${key}` from props (iterated: values may nest one level).
/// Returns None when any `${…}` remains unresolvable.
/// (Thế `${key}` từ props. Trả None khi còn `${…}` không resolve được.)
pub fn substitute_properties(
    raw: &str,
    props: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let mut current = raw.to_string();
    for _ in 0..5 {
        if !current.contains("${") {
            return Some(current);
        }
        let mut next = current.clone();
        let mut keys: Vec<&String> = props.keys().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        for key in keys {
            let placeholder = format!("${{{key}}}");
            if next.contains(&placeholder) {
                next = next.replace(&placeholder, &props[key]);
            }
        }
        if next == current {
            return None;
        }
        current = next;
    }
    if current.contains("${") {
        None
    } else {
        Some(current)
    }
}
/// Project coordinates `(groupId, artifactId)` of a pom.xml: dependency
/// blocks are stripped first so nested artifactIds can never shadow the
/// project's own coordinates.
/// Tọa độ project `(groupId, artifactId)` của pom.xml: khối dependency bị
/// cắt trước để artifactId lồng nhau không bao giờ che tọa độ của project.
pub fn pom_project_coordinates(pom: &str) -> Option<(String, String)> {
    let stripped = remove_element_blocks(pom, "dependencyManagement");
    let stripped = remove_element_blocks(&stripped, "dependencies");
    let group = element_blocks(&stripped, "groupId")
        .first()
        .map(|s| s.trim().to_string())?;
    let artifact = element_blocks(&stripped, "artifactId")
        .first()
        .map(|s| s.trim().to_string())?;
    Some((group, artifact))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_blocks_finds_and_counts_nesting() {
        let xml = "<a><versions><version>1.0</version><version>2.0</version></versions></a>";
        let blocks = element_blocks(xml, "versions");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], "<version>1.0</version><version>2.0</version>");
        let versions = element_blocks(blocks[0], "version");
        assert_eq!(versions, vec!["1.0", "2.0"]);
        // The `<version>` search must return exactly the two leaf elements —
        // it must NOT swallow the `<versions>` block as a match.
        // (Tìm `<version>` phải trả đúng hai phần tử lá — KHÔNG được nuốt
        // khối `<versions>` như một kết quả khớp.)
        assert_eq!(element_blocks(xml, "version"), vec!["1.0", "2.0"]);
    }

    #[test]
    fn dependency_management_is_stripped_before_collection() {
        let pom = r#"<project>
          <packaging>jar</packaging>
          <dependencyManagement>
            <dependencies>
              <dependency>
                <groupId>com.dm</groupId><artifactId>managed</artifactId><version>9.9</version>
              </dependency>
            </dependencies>
          </dependencyManagement>
          <dependencies>
            <dependency>
              <groupId>com.example</groupId><artifactId>core</artifactId><version>1.2.3</version>
            </dependency>
            <dependency>
              <groupId>com.example</groupId><artifactId>test-dep</artifactId><version>2.0</version><scope>test</scope>
            </dependency>
            <dependency>
              <groupId>com.example</groupId><artifactId>opt</artifactId><version>0.1</version><optional>true</optional>
            </dependency>
            <dependency>
              <groupId>com.example</groupId><artifactId>prop</artifactId><version>${core.version}</version>
            </dependency>
          </dependencies>
        </project>"#;
        let (deps, packaging) = collect_pom_dependencies(pom);
        assert_eq!(packaging.as_deref(), Some("jar"));
        assert_eq!(deps.len(), 4, "managed entries must not be collected");
        let core = &deps[0];
        assert_eq!(core.group.as_deref(), Some("com.example"));
        assert_eq!(core.artifact.as_deref(), Some("core"));
        assert_eq!(core.version.as_deref(), Some("1.2.3"));
        assert_eq!(core.scope, None, "no scope element = compile");
        assert!(!core.optional);
    }

    #[test]
    fn maven_range_semantics() {
        let v = Version::parse("1.2.3").unwrap();
        assert!(maven_matches("1.2", &v), "bare = caret");
        assert!(!maven_matches("2.0", &v));
        assert!(maven_matches("[1.2.3]", &v));
        assert!(!maven_matches("[1.2.4]", &v));
        assert!(maven_matches("[1.0,2.0)", &v));
        assert!(!maven_matches("[1.0,1.2.3)", &v));
        assert!(maven_matches("(1.0,]", &v));
        assert!(maven_matches("*", &v));
    }
}
