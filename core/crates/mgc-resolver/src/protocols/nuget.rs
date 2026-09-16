//! NuGet v3 native engine (.NET packages).
//! Engine native NuGet v3 (package .NET).
//!
//! V3 protocol: the service index (`{index}/index.json`) advertises the
//! `RegistrationsBaseUrl/3.6.0` (or Versioned) and `PackageBaseAddress/3.0.0`
//! resources. Versions come from the flat container
//! `{PackageBaseAddress}/{id-lower}/index.json`; the authoritative integrity
//! hash lives in the registration's `catalogEntry.packageHash`
//! (base64 SHA-512 of the nupkg bytes — verified with the same algorithm the
//! server declares, fail-closed otherwise); `listed: false` entries are
//! never selected. Dependencies live in the nuspec INSIDE the nupkg, but the
//! flat container also serves `{PackageBaseAddress}/{id}/{version}/{id}.nuspec`
//! — fetched there so no zip handling is needed during resolve. Selection
//! mirrors NuGet's own minimum-version resolution: the LOWEST matching
//! listed version wins.
//! Protocol v3: service index (`{index}/index.json`) công bố resource
//! `RegistrationsBaseUrl/3.6.0` (hoặc Versioned) và `PackageBaseAddress/3.0.0`.
//! Version lấy từ flat container `{PackageBaseAddress}/{id-lower}/index.json`;
//! hash toàn vẹn chính thống nằm ở `catalogEntry.packageHash` của
//! registration (base64 SHA-512 của byte nupkg — xác minh đúng thuật toán
//! server khai báo, không đúng thì fail-closed); entry `listed: false` không
//! bao giờ được chọn. Dep nằm trong nuspec BÊN TRONG nupkg, nhưng flat
//! container cũng phục vụ `{PackageBaseAddress}/{id}/{version}/{id}.nuspec`
//! — tải từ đó để resolve không cần xử lý zip. Selection phản chiếu
//! minimum-version resolution của chính NuGet: version thấp nhất khớp và
//! listed thắng.

use super::{RegistryProtocol, ResolvedEntry};
use async_trait::async_trait;
use base64::Engine as _;
use mgc_types::{MgError, MgResult, Version};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::path::{Path, PathBuf};

const DEFAULT_INDEX_URL: &str = "https://api.nuget.org/v3/index.json";

/// Native NuGet v3 engine.
/// Engine native NuGet v3.
#[derive(Debug, Clone)]
pub struct NuGetProtocol {
    registrations_base: String,
    package_base: String,
    client: reqwest::Client,
}

impl NuGetProtocol {
    /// Build with explicit registration + flat-container bases (testability).
    /// Dựng với base registration + flat container tường minh (cho test).
    pub fn with_bases(registrations_base: &str, package_base: &str) -> Self {
        Self {
            registrations_base: registrations_base.trim_end_matches('/').to_string(),
            package_base: package_base.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Build from environment: `MGC_NUGET_INDEX_URL` (service index whose
    /// resources are probed), defaulting to the nuget.org service index. A
    /// probe failure degrades to degenerate bases with a LOUD warning — every
    /// subsequent resolve fails closed instead of pretending to work.
    /// Dựng từ môi trường: `MGC_NUGET_INDEX_URL` (service index được dò
    /// resource), mặc định là service index của nuget.org. Dò lỗi thì hạ xuống
    /// base suy biến kèm cảnh báo ỒN ÀO — mọi resolve sau đó fail-closed thay
    /// vì giả vờ hoạt động.
    pub async fn from_env() -> Self {
        let index_url = std::env::var("MGC_NUGET_INDEX_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_INDEX_URL.to_string());
        Self::from_service_index(&index_url)
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "WARNING: NuGet service index {index_url} probe failed ({e}); \
                 no resource bases resolved — resolve calls will fail closed"
                );
                // Degenerate bases: both URLs collapse to the index URL so any
                // resolve attempt fails with a clear network error (fail-closed).
                // (Base suy biến: cả hai URL dồn về index URL nên mọi lần resolve
                // fail với lỗi mạng rõ ràng (fail-closed).)
                Self::with_bases(&index_url, &index_url)
            })
    }

    /// Probe a service index for the registration + flat-container bases
    /// (public: tests and adapters probe explicit indexes).
    /// Dò service index để lấy base registration + flat container (public:
    /// test và adapter dò index tường minh).
    pub async fn from_service_index(index_url: &str) -> MgResult<Self> {
        let client = reqwest::Client::new();
        let resp = client
            .get(index_url)
            .send()
            .await
            .map_err(|e| MgError::Network(format!("GET {index_url} failed: {e}")))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| MgError::Network(format!("read {index_url} body failed: {e}")))?;
        if !status.is_success() {
            return Err(MgError::Network(format!(
                "GET {index_url} returned {status}"
            )));
        }
        let doc: Value = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse nuget service index failed: {e}")))?;
        let mut registrations = None;
        let mut package_base = None;
        // Preferred order: 3.6.0 → Versioned → plain RegistrationsBaseUrl.
        // (Thứ tự ưu tiên: 3.6.0 → Versioned → RegistrationsBaseUrl thường.)
        for ty in [
            "RegistrationsBaseUrl/3.6.0",
            "RegistrationsBaseUrl/Versioned",
            "RegistrationsBaseUrl",
        ] {
            if let Some(id) = find_resource(&doc, ty) {
                registrations = Some(id);
                break;
            }
        }
        if let Some(id) = find_resource(&doc, "PackageBaseAddress/3.0.0") {
            package_base = Some(id);
        }
        let registrations = registrations.ok_or_else(|| {
            MgError::Other("service index has no RegistrationsBaseUrl resource".to_string())
        })?;
        let package_base = package_base.ok_or_else(|| {
            MgError::Other("service index has no PackageBaseAddress resource".to_string())
        })?;
        Ok(Self::with_bases(&registrations, &package_base))
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

    fn nupkg_url(&self, id: &str, version: &str) -> String {
        format!(
            "{}/{}/{}/{}.{}.nupkg",
            self.package_base,
            id.to_lowercase(),
            version.to_lowercase(),
            id,
            version
        )
    }

    fn nuspec_url(&self, id: &str, version: &str) -> String {
        format!(
            "{}/{}/{}/{}.nuspec",
            self.package_base,
            id.to_lowercase(),
            version.to_lowercase(),
            id.to_lowercase()
        )
    }

    /// Registration leaves (flattened across pages) as raw JSON values.
    /// Các leaf registration (dẹt qua mọi page) dưới dạng JSON thô.
    async fn registration_leaves(&self, id: &str) -> MgResult<Vec<Value>> {
        let url = format!(
            "{}/{}/index.json",
            self.registrations_base,
            id.to_lowercase()
        );
        let (status, body) = self.get_text(&url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {url} returned {status} — no registration for {id} (fail-closed)"
            )));
        }
        let doc: Value = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse registration index failed: {e}")))?;
        let mut leaves = Vec::new();
        collect_leaves(&doc, &mut leaves);
        Ok(leaves)
    }

    /// Materialize a package into the global-packages layout
    /// `{nuget_root}/{id-lower}/{version-lower}/`: the nupkg bytes, the
    /// extracted package contents and the `.nupkg.sha512` sidecar — the
    /// layout `dotnet restore --source`/global fallback folders read.
    /// Materialize package vào layout global-packages
    /// `{nuget_root}/{id-lower}/{version-lower}/`: byte nupkg, nội dung
    /// package đã giải nén và sidecar `.nupkg.sha512` — layout mà
    /// `dotnet restore --source`/global fallback folder đọc.
    pub fn materialize(
        &self,
        entry: &ResolvedEntry,
        nupkg_bytes: &[u8],
        nuget_root: &Path,
    ) -> MgResult<PathBuf> {
        let dir = nuget_root
            .join(entry.name.to_lowercase())
            .join(entry.version.to_lowercase());
        std::fs::create_dir_all(&dir)?;
        let nupkg_name = format!("{}.{}.nupkg", entry.name, entry.version);
        std::fs::write(dir.join(&nupkg_name), nupkg_bytes)?;
        super::zip_reader::extract_zip(nupkg_bytes, &dir)?;
        let sha512 = Sha512::digest(nupkg_bytes);
        std::fs::write(
            dir.join(format!("{nupkg_name}.sha512")),
            base64::engine::general_purpose::STANDARD.encode(sha512),
        )?;
        Ok(dir.join(nupkg_name))
    }
}

/// Find a resource `@id` by `@type` in a service index document.
/// Tìm resource `@id` theo `@type` trong tài liệu service index.
fn find_resource(doc: &Value, ty: &str) -> Option<String> {
    doc.get("resources")?.as_array()?.iter().find_map(|r| {
        let matches = r.get("@type").and_then(Value::as_str) == Some(ty);
        if matches {
            r.get("@id")
                .and_then(Value::as_str)
                .map(|s| s.trim_end_matches('/').to_string())
        } else {
            None
        }
    })
}

/// Flatten registration leaves: a leaf has `catalogEntry`; pages/intermediate
/// nodes carry `items` arrays.
/// Dẹt leaf registration: leaf có `catalogEntry`; page/node trung gian mang
/// mảng `items`.
fn collect_leaves(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect_leaves(v, out)),
        Value::Object(obj) => {
            if obj.contains_key("catalogEntry") {
                out.push(value.clone());
                return;
            }
            if let Some(items) = obj.get("items") {
                collect_leaves(items, out);
            }
        }
        _ => {}
    }
}

#[async_trait]
impl RegistryProtocol for NuGetProtocol {
    async fn resolve(&self, name: &str, range: &str) -> MgResult<ResolvedEntry> {
        let id = name.trim();
        let id_lower = id.to_lowercase();

        // Flat container version list.
        // (Danh sách version flat container.)
        let versions_url = format!("{}/{}/index.json", self.package_base, id_lower);
        let (status, body) = self.get_text(&versions_url).await?;
        if !(200..300).contains(&status) {
            return Err(MgError::Network(format!(
                "GET {versions_url} returned {status} — no versions for {id} (fail-closed)"
            )));
        }
        let doc: Value = serde_json::from_str(&body)
            .map_err(|e| MgError::Other(format!("parse flat container index failed: {e}")))?;
        let versions: Vec<&str> = doc
            .get("versions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        // NuGet's minimum-version resolution: the LOWEST matching listed
        // version wins. Registration `listed: false` entries are skipped;
        // if ONLY unlisted versions match → warn + fail-closed (never a
        // silent pin to a delisted artifact).
        // (Resolve minimum-version của NuGet: version thấp nhất khớp và
        // listed thắng. Entry `listed: false` bị bỏ qua; nếu CHỈ có version
        // unlisted khớp → cảnh báo + fail-closed (không bao giờ ghim âm
        // thầm vào artifact đã gỡ khỏi listing).)
        let mut best: Option<(Version, String)> = None;
        for raw in &versions {
            let Ok(v) = Version::parse(raw) else {
                continue;
            };
            if !nuget_matches(range, &v) {
                continue;
            }
            if best.as_ref().is_none_or(|(bv, _)| v < *bv) {
                best = Some((v, (*raw).to_string()));
            }
        }
        let mut markers = Vec::new();
        let version = match best {
            Some((_, v)) => v,
            // Only unlisted versions matched → warn + fail-closed (never a
            // silent pin to a delisted artifact).
            // (Chỉ có version unlisted khớp → cảnh báo + fail-closed (không
            // bao giờ ghim âm thầm vào artifact đã gỡ khỏi listing).)
            None => {
                if !versions.is_empty() {
                    eprintln!(
                        "WARNING: {id}@{range} only matches versions with no listed registration — skipping (fail-closed)"
                    );
                }
                return Err(MgError::Other(format!(
                    "no listed version of {id} matches range '{range}'"
                )));
            }
        };

        // Registration leaf for the chosen version → authoritative hash.
        // (Leaf registration cho version đã chọn → hash chính thống.)
        let leaves = self.registration_leaves(&id_lower).await?;
        let leaf = leaves.iter().find(|leaf| {
            leaf.get("catalogEntry")
                .and_then(|c| c.get("version"))
                .and_then(Value::as_str)
                .is_some_and(|v| v.eq_ignore_ascii_case(&version))
        });
        let catalog = leaf
            .and_then(|l| l.get("catalogEntry"))
            .ok_or_else(|| {
                MgError::Integrity(format!(
                    "registration has no catalogEntry for {id} {version} — artifact cannot be verified (fail-closed)"
                ))
            })?;
        let listed = catalog
            .get("listed")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !listed {
            return Err(MgError::Integrity(format!(
                "{id} {version} is unlisted in the registration (fail-closed)"
            )));
        }
        let package_hash = catalog
            .get("packageHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let algorithm = catalog
            .get("packageHashAlgorithm")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !algorithm.eq_ignore_ascii_case("SHA512") || package_hash.is_empty() {
            return Err(MgError::Integrity(format!(
                "{id} {version} registration hash algorithm '{algorithm}' is not SHA512 or the hash is empty — fail-closed"
            )));
        }
        base64::engine::general_purpose::STANDARD
            .decode(package_hash)
            .map_err(|e| {
                MgError::Integrity(format!(
                    "{id} {version} packageHash is not valid base64: {e}"
                ))
            })?;
        markers.push(format!("sha512:{package_hash}"));

        // Dependencies from the flat-container nuspec (the nuspec also
        // lives inside the nupkg; resolve must not need zip handling).
        // (Dep từ nuspec của flat container (nuspec cũng nằm trong nupkg;
        // resolve không cần xử lý zip).)
        let mut deps = Vec::new();
        let nuspec_url = self.nuspec_url(id, &version);
        let (nuspec_status, nuspec) = self.get_text(&nuspec_url).await?;
        if !(200..300).contains(&nuspec_status) {
            return Err(MgError::Network(format!(
                "GET {nuspec_url} returned {nuspec_status} — the nuspec is required to build the graph (fail-closed)"
            )));
        }
        // Merge every group's deps (host TFM unknown); a group with an
        // explicit targetFramework is recorded as a marker.
        // (Gộp dep của mọi group (TFM host không biết); group có
        // targetFramework tường minh được ghi marker.)
        // tag_blocks keeps the OPENING tag, so targetFramework is readable;
        // element_blocks alone would return only the inner content.
        // (tag_blocks giữ tag MỞ nên đọc được targetFramework; chỉ dùng
        // element_blocks sẽ chỉ trả nội dung bên trong.)
        for group in tag_blocks(&nuspec, "group") {
            if let Some(tfm) = group_tfm(group) {
                markers.push(format!("group-tfm:{tfm}"));
            }
            // Dependencies are self-closing elements whose data lives in
            // attributes — iterate TAGS (tag_blocks), not inner content.
            // (Dependency là phần tử tự đóng mang dữ liệu trong attribute —
            // duyệt TAG (tag_blocks), không phải nội dung bên trong.)
            for dep_xml in tag_blocks(group, "dependency") {
                let attr = |a: &str| tag_attr(dep_xml, a);
                let Some(dep_id) = attr("id") else {
                    continue;
                };
                match attr("version") {
                    Some(v) if !v.is_empty() => deps.push((dep_id, v)),
                    // No version attribute → honest skip with a marker.
                    // (Thiếu attribute version → skip trung thực kèm marker.)
                    _ => markers.push(format!("unresolved-version:{dep_id}")),
                }
            }
        }

        let artifact_url = self.nupkg_url(id, &version);
        Ok(ResolvedEntry {
            name: id.to_string(),
            version,
            deps,
            artifact_url,
            // sha256 stays empty — the authoritative digest is SHA-512 and
            // rides the marker (verify override).
            // (sha256 giữ rỗng — digest chính thống là SHA-512 và nằm trong
            // marker (verify override).)
            sha256: String::new(),
            extra_markers: markers,
        })
    }

    async fn download(&self, entry: &ResolvedEntry) -> MgResult<Vec<u8>> {
        self.get_bytes(&entry.artifact_url).await
    }

    /// Verify overrides the shared default: the registration's base64
    /// SHA-512 (marker `sha512:…`) is the authoritative check — a nupkg
    /// with no recorded hash is never installed.
    /// (Verify ghi đè mặc định: SHA-512 base64 của registration (marker
    /// `sha512:…`) là kiểm tra chính thống — nupkg không có hash đã ghi
    /// không bao giờ được cài.)
    fn verify(&self, entry: &ResolvedEntry, bytes: &[u8]) -> MgResult<()> {
        let marker = entry
            .extra_markers
            .iter()
            .find(|m| m.starts_with("sha512:"))
            .ok_or_else(|| {
                MgError::Integrity(format!(
                    "no integrity source for {}@{} (no registration SHA-512) — fail-closed",
                    entry.name, entry.version
                ))
            })?;
        let expected = marker.trim_start_matches("sha512:");
        let actual = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes));
        if actual != expected {
            return Err(MgError::Integrity(format!(
                "sha512 mismatch for {}@{}",
                entry.name, entry.version
            )));
        }
        Ok(())
    }
}

/// Read the `targetFramework` attribute of a `<group>` block.
/// Đọc attribute `targetFramework` của khối `<group>`.
fn group_tfm(group_xml: &str) -> Option<String> {
    tag_attr(group_xml, "targetFramework")
}

/// Read attribute `attr="…"` from the FIRST opening tag in `xml` (up to the
/// first `>` — nested tags never contaminate the lookup).
/// Đọc attribute `attr="…"` từ tag mở ĐẦU TIÊN trong `xml` (đến `>` đầu
/// tiên — tag lồng không bao giờ làm bẩn kết quả).
fn tag_attr(xml: &str, attr: &str) -> Option<String> {
    let lt = xml.find('<')?;
    let gt_rel = xml[lt..].find('>')?;
    let tag = &xml[lt..lt + gt_rel];
    let needle = format!("{attr}=");
    let pos = tag.find(&needle)?;
    let rest = tag[pos + needle.len()..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    rest[1..].split(quote).next().map(str::to_string)
}

/// NuGet version-range subset: `*`/empty = any, bare = exact
/// (PackageReference minimum semantics), `x.*` float, `[a,b]`/`(a,b]`
/// intervals, comma = AND.
/// Subset khoảng version NuGet: `*`/rỗng = bất kỳ, trần = chính xác (ngữ
/// nghĩa minimum của PackageReference), `x.*` float, khoảng
/// `[a,b]`/`(a,b]`, phẩy = AND.
fn nuget_matches(range: &str, version: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    if range.starts_with('[') || range.starts_with('(') {
        return nuget_interval(range, version);
    }
    if let Some(prefix) = range.strip_suffix(".*") {
        // Floating major/minor: "1.*" → same major.
        // (Float major/minor: "1.*" → cùng major.)
        let parts: Vec<&str> = prefix.split('.').collect();
        return match parts.len() {
            1 => version.major == parts[0].parse().unwrap_or(u64::MAX),
            2 => {
                version.major == parts[0].parse().unwrap_or(u64::MAX)
                    && version.minor == parts[1].parse().unwrap_or(u64::MAX)
            }
            _ => false,
        };
    }
    range
        .split(',')
        .all(|part| nuget_part(part.trim(), version))
}

fn nuget_interval(range: &str, version: &Version) -> bool {
    let Some(close) = range.rfind([')', ']']) else {
        return false;
    };
    let (open_ch, close_ch) = (range.as_bytes()[0] as char, range.as_bytes()[close] as char);
    let inner = &range[1..close];
    let bounds: Vec<&str> = inner.split(',').map(str::trim).collect();
    match bounds.len() {
        1 => {
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

fn nuget_part(part: &str, version: &Version) -> bool {
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
    // Bare = exact minimum pin (PackageReference semantics).
    // (Trần = ghim minimum chính xác (ngữ nghĩa PackageReference).)
    Version::parse(part).map(|t| version == &t).unwrap_or(false)
}

/// One `<PackageReference Include="…" Version="…"/>` reference.
/// Một tham chiếu `<PackageReference Include="…" Version="…"/>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReference {
    pub id: String,
    pub version: Option<String>,
}

/// Parse all `<PackageReference>` entries from a .csproj (text scan, no
/// XML crate — attributes carry the data). Entries without a Version
/// attribute or with a floating `*` version are returned in `unresolved`
/// for honest skip+marker handling by the caller.
/// Parse mọi `<PackageReference>` từ .csproj (quét text, không XML crate —
/// dữ liệu nằm trong attribute). Entry thiếu attribute Version hoặc version
/// float `*` trả về trong `unresolved` để caller skip+marker trung thực.
pub fn parse_package_references(csproj: &str) -> (Vec<PackageReference>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for tag in tag_blocks(csproj, "PackageReference") {
        let Some(id) = tag_attr(tag, "Include") else {
            continue;
        };
        match tag_attr(tag, "Version") {
            Some(v) if !v.is_empty() && v != "*" && !v.ends_with('*') => {
                resolved.push(PackageReference {
                    id,
                    version: Some(v),
                });
            }
            _ => unresolved.push(id),
        }
    }
    (resolved, unresolved)
}

/// Extract each `<tag …>…</tag>` / `<tag …/>` raw block.
/// Lấy từng khối thô `<tag …>…</tag>` / `<tag …/>`.
fn tag_blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open_prefix = format!("<{tag}");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open_prefix) {
        let after = &rest[start + open_prefix.len()..];
        if !(after.starts_with('>')
            || after.starts_with('/')
            || after.starts_with(|c: char| c.is_whitespace()))
        {
            rest = &rest[start + open_prefix.len()..];
            continue;
        }
        let Some(gt_rel) = rest[start..].find('>') else {
            break;
        };
        let gt = start + gt_rel;
        if rest[start..gt + 1].ends_with("/>") {
            out.push(&rest[start..gt + 1]);
            rest = &rest[gt + 1..];
            continue;
        }
        let close = format!("</{tag}>");
        match rest[gt..].find(&close) {
            Some(end_rel) => {
                let end = gt + end_rel + close.len();
                out.push(&rest[start..end]);
                rest = &rest[end..];
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_reference_parsing() {
        let csproj = r#"<Project Sdk="Microsoft.NET.Sdk">
          <ItemGroup>
            <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
            <PackageReference Include="Wildcard" Version="1.*" />
            <PackageReference Include="NoVersion" />
            <PackageReference Include="Star" Version="*" />
            <PackageReference Include="Range" Version="[1.0, 2.0)" />
          </ItemGroup>
        </Project>"#;
        let (refs, unresolved) = parse_package_references(csproj);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].id, "Newtonsoft.Json");
        assert_eq!(refs[0].version.as_deref(), Some("13.0.3"));
        assert_eq!(refs[1].id, "Range");
        assert_eq!(unresolved, vec!["Wildcard", "NoVersion", "Star"]);
    }

    #[test]
    fn nuget_range_semantics() {
        let v = Version::parse("1.2.3").unwrap();
        assert!(nuget_matches("1.2.3", &v), "bare = exact pin");
        assert!(!nuget_matches("1.2.4", &v));
        assert!(nuget_matches("*", &v));
        assert!(nuget_matches("1.*", &v));
        assert!(!nuget_matches("2.*", &v));
        assert!(nuget_matches("[1.2.3]", &v));
        assert!(nuget_matches("[1.0,2.0)", &v));
        assert!(!nuget_matches("[1.0,1.2.3)", &v));
    }

    #[test]
    fn collect_leaves_flattens_pages_and_leaf_arrays() {
        let doc: Value = serde_json::from_str(
            r#"{"items":[{"items":[{"catalogEntry":{"version":"1.0.0"}},{"catalogEntry":{"version":"2.0.0"}}]}]}"#,
        )
        .unwrap();
        let mut leaves = Vec::new();
        collect_leaves(&doc, &mut leaves);
        assert_eq!(leaves.len(), 2);
    }

    #[test]
    fn xml_attr_and_tfm() {
        let group =
            r#"<group targetFramework="net6.0"><dependency id="A" version="[1.0]" /></group>"#;
        assert_eq!(group_tfm(group).as_deref(), Some("net6.0"));
        assert_eq!(tag_attr(group, "id"), None, "group has no id attribute");
        let dep = r#"<dependency id="A" version="[1.0]" exclude="Build,Analyzers" />"#;
        assert_eq!(tag_attr(dep, "id").as_deref(), Some("A"));
        assert_eq!(tag_attr(dep, "version").as_deref(), Some("[1.0]"));
        assert_eq!(tag_attr(dep, "exclude").as_deref(), Some("Build,Analyzers"));
    }
}
