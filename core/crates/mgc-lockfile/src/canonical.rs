//! Lockfile v4 document + canonical payload (design §§1–2).
//! Tài liệu lockfile v4 + payload canonical.
//!
//! The WHOLE file stays TOML-parseable; the digest covers ONLY the
//! canonical payload (a dedicated struct — the compiler forces every new
//! field to be classified in-payload or out-of-payload, so no silent
//! drift when fields are added).
//! (Cả file vẫn parse được bằng TOML; digest chỉ phủ payload canonical
//! (struct riêng — compiler ép mọi field mới phải phân loại tường minh).)

use std::collections::BTreeMap;

use crate::schema::{ArtifactRef, CrossEdge, Provenance, WorkspaceTopology};
use crate::v4::{Edge, PackageKey, SignatureBlock, SourceRef};
use serde::{Deserialize, Serialize};

/// v4 lockfile document — Tài liệu lockfile v4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockfileV4 {
    /// Schema version (`"4"`) — Phiên bản schema.
    pub version: String,
    /// Metadata (generator is in-payload; hash/signature/timestamp out).
    pub metadata: LockfileV4Metadata,
    /// Source table (replayable without mgc.toml) — Bảng nguồn.
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    /// Canonical peer-set table: digest → sorted [(name, version)].
    #[serde(default)]
    pub peer_contexts: BTreeMap<String, Vec<(String, String)>>,
    /// Root direct-dependency edges (`name@version` strings).
    #[serde(default)]
    pub root_dependencies: Vec<String>,
    /// Exact direct pins per core. New values use
    /// `mgc-root-v1:<ecosystem>:<name@version>` so one core can own multiple
    /// ecosystems without overwriting sibling roots.
    /// Pin trực tiếp chính xác theo core; định danh có ecosystem giữ được
    /// nhiều hệ sinh thái cùng core mà không ghi đè root của nhau.
    #[serde(default)]
    pub root_dependencies_by_owner: BTreeMap<String, Vec<String>>,
    /// Locked instances — Các instance đã lock.
    #[serde(rename = "package", default)]
    pub packages: Vec<PackageV4>,
    /// Workspace topology (reused v3 shape) — Topology workspace.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_v4_workspace"
    )]
    pub workspace: Option<WorkspaceTopology>,
    /// Optimizer profile stamp — Nhãn profile tối ưu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimizer_profile: Option<String>,
}

/// v4 metadata — Metadata v4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockfileV4Metadata {
    /// Generation timestamp, ISO-8601 (OUT-of-payload: two runs over the
    /// same graph must digest identically).
    pub generated_at: String,
    /// Generator id (`mgc/1.2.0`) — IN-payload.
    pub generator: String,
    /// Digest of the canonical payload (`blake3-<base64>`, OUT).
    pub lockfile_hash: String,
    /// Inline signature (OUT) — Chữ ký nhúng.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignatureBlock>,
}

/// One locked instance with structured edges — Một instance đã lock với
/// cạnh có cấu trúc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageV4 {
    /// Instance identity — Danh tính instance.
    pub key: PackageKey,
    /// Outgoing dependency edges — Cạnh phụ thuộc ra.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Physical artifact reference — Tham chiếu artifact vật lý.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_v4_artifact"
    )]
    pub artifact: Option<ArtifactRef>,
    /// How this pin entered the lock — Pin vào lock bằng đường nào.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_v4_provenance"
    )]
    pub provenance: Option<Provenance>,
    /// Toolchain requirement — Yêu cầu toolchain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<String>,
    /// Lifecycle scripts policy — Chính sách lifecycle script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts_policy: Option<String>,
    /// CAS object reference — Tham chiếu object CAS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4ArtifactRef {
    url: String,
    #[serde(default)]
    size_bytes: Option<u64>,
    content_hash: String,
    downloaded_from: String,
    /// SRI integrity (optional — absent on older v4 documents).
    /// Integrity SRI (tùy chọn — vắng trên tài liệu v4 cũ).
    #[serde(default)]
    integrity_sri: Option<String>,
}

impl From<V4ArtifactRef> for ArtifactRef {
    fn from(value: V4ArtifactRef) -> Self {
        Self {
            url: value.url,
            size_bytes: value.size_bytes,
            content_hash: value.content_hash,
            downloaded_from: value.downloaded_from,
            integrity_sri: value.integrity_sri,
        }
    }
}

fn deserialize_v4_artifact<'de, D>(deserializer: D) -> Result<Option<ArtifactRef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<V4ArtifactRef>::deserialize(deserializer)
        .map(|artifact| artifact.map(ArtifactRef::from))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Provenance {
    source_kind: String,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    imported_from: Option<String>,
}

impl From<V4Provenance> for Provenance {
    fn from(value: V4Provenance) -> Self {
        Self {
            source_kind: value.source_kind,
            tool: value.tool,
            imported_from: value.imported_from,
        }
    }
}

fn deserialize_v4_provenance<'de, D>(deserializer: D) -> Result<Option<Provenance>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<V4Provenance>::deserialize(deserializer).map(|provenance| provenance.map(Into::into))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4WorkspaceTopology {
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    cross_core_edges: Vec<V4CrossEdge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V4CrossEdge {
    from_core: String,
    to_core: String,
    package: String,
}

impl From<V4WorkspaceTopology> for WorkspaceTopology {
    fn from(value: V4WorkspaceTopology) -> Self {
        Self {
            members: value.members,
            cross_core_edges: value
                .cross_core_edges
                .into_iter()
                .map(|edge| CrossEdge {
                    from_core: edge.from_core,
                    to_core: edge.to_core,
                    package: edge.package,
                })
                .collect(),
        }
    }
}

fn deserialize_v4_workspace<'de, D>(deserializer: D) -> Result<Option<WorkspaceTopology>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<V4WorkspaceTopology>::deserialize(deserializer)
        .map(|workspace| workspace.map(WorkspaceTopology::from))
}

impl LockfileV4 {
    /// New empty v4 document — Tài liệu v4 rỗng mới.
    pub fn new(generator: &str) -> Self {
        Self {
            version: crate::v4::LOCKFILE_SCHEMA_V4.to_string(),
            metadata: LockfileV4Metadata {
                generated_at: String::new(),
                generator: generator.to_string(),
                lockfile_hash: String::new(),
                signature: None,
            },
            sources: Vec::new(),
            peer_contexts: BTreeMap::new(),
            root_dependencies: Vec::new(),
            root_dependencies_by_owner: BTreeMap::new(),
            packages: Vec::new(),
            workspace: None,
            optimizer_profile: None,
        }
    }

    /// The canonical payload this document signs — Payload canonical mà
    /// tài liệu này ký.
    pub fn payload(&self) -> LockfilePayload {
        LockfilePayload {
            version: self.version.clone(),
            generator: self.metadata.generator.clone(),
            sources: self.sources.clone(),
            peer_contexts: self.peer_contexts.clone(),
            root_dependencies: self.root_dependencies.clone(),
            root_dependencies_by_owner: self.root_dependencies_by_owner.clone(),
            packages: self.packages.clone(),
            workspace: self.workspace.clone(),
            optimizer_profile: self.optimizer_profile.clone(),
        }
    }
}

/// The exact signed subset — Tập con chính xác được ký.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockfilePayload {
    pub version: String,
    pub generator: String,
    pub sources: Vec<SourceRef>,
    pub peer_contexts: BTreeMap<String, Vec<(String, String)>>,
    pub root_dependencies: Vec<String>,
    pub root_dependencies_by_owner: BTreeMap<String, Vec<String>>,
    pub packages: Vec<PackageV4>,
    /// Cross-core membership and edges affect graph ownership and replay.
    /// Thành viên/cạnh liên core ảnh hưởng ownership và phát lại đồ thị.
    pub workspace: Option<WorkspaceTopology>,
    /// Optimizer configuration recorded for reproducible execution.
    /// Cấu hình optimizer ghi lại để thực thi có thể tái lập.
    pub optimizer_profile: Option<String>,
}

/// Sort key for packages: (ecosystem, name, version, source_id,
/// variant-debug) — Khóa sắp xếp package.
fn package_sort_key(package: &PackageV4) -> (String, String, String, String, String) {
    // Feature identity is a set; sorting keys must match serialized normalization.
    // Danh tính feature là tập; khóa sort phải khớp chuẩn hóa khi serialize.
    let mut variant = package.key.variant.clone();
    if let Some(features) = &mut variant.feature_set {
        features.sort();
    }
    (
        package.key.ecosystem.as_str().to_owned(),
        package.key.name.clone(),
        package.key.version.clone(),
        package.key.source_id.clone(),
        format!("{variant:?}"),
    )
}

/// Sort key for edges — Khóa sắp xếp cạnh.
fn edge_sort_key(edge: &Edge) -> (String, String, String, String, String, String) {
    (
        edge.target_key.ecosystem.as_str().to_owned(),
        edge.target_key.name.clone(),
        edge.target_key.version.clone(),
        edge.range.clone(),
        format!("{:?}", edge.kind),
        {
            // Break ties with every authenticated edge field, not input order.
            // Phân xử bằng mọi field đã ký, không phụ thuộc thứ tự đầu vào.
            let mut rendered = String::new();
            push_edge(&mut rendered, edge);
            rendered
        },
    )
}

/// TOML basic-string escaping (spec-conformant, locale-independent).
/// Escape chuỗi basic TOML (đúng spec, không phụ thuộc locale).
fn escape_toml_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn push_str_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = ");
    out.push_str(&escape_toml_string(value));
    out.push('\n');
}

fn push_string_array(out: &mut String, key: &str, values: &[String]) {
    out.push_str(key);
    out.push_str(" = [");
    let mut first = true;
    for value in values {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(&escape_toml_string(value));
    }
    out.push_str("]\n");
}

fn push_source(out: &mut String, source: &SourceRef) {
    out.push_str("[[sources]]\n");
    push_str_field(out, "id", &source.id);
    push_str_field(out, "url", &source.url);
    push_str_field(out, "ecosystem", source.ecosystem.as_str());
    out.push_str(&format!("priority = {}\n", source.priority));
    push_string_array(
        out,
        "claims",
        &source
            .claims
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );
    out.push_str(&format!("trusted = {}\n", source.trusted));
    push_string_array(
        out,
        "allow_hosts",
        &source
            .allow_hosts
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );
    push_string_array(
        out,
        "allow_cidrs",
        &source
            .allow_cidrs
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );
    push_string_array(
        out,
        "allow_protocols",
        &source
            .allow_protocols
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
    );
}

fn push_package_key(out: &mut String, key: &PackageKey) {
    out.push_str("[package.key]\n");
    push_str_field(out, "ecosystem", key.ecosystem.as_str());
    push_str_field(out, "name", &key.name);
    push_str_field(out, "version", &key.version);
    push_str_field(out, "source_id", &key.source_id);
    push_variant(out, "package.key.variant", &key.variant);
}

fn push_variant(out: &mut String, table: &str, variant: &crate::v4::VariantKey) {
    if variant.peer_context.is_some() || variant.feature_set.is_some() || variant.target.is_some() {
        out.push('[');
        out.push_str(table);
        out.push_str("]\n");
        if let Some(peer_context) = &variant.peer_context {
            push_str_field(out, "peer_context", peer_context);
        }
        if let Some(feature_set) = &variant.feature_set {
            let mut sorted = feature_set.clone();
            sorted.sort();
            push_string_array(out, "feature_set", &sorted);
        }
        if let Some(target) = &variant.target {
            push_str_field(out, "target", target);
        }
    }
}

fn push_edge(out: &mut String, edge: &Edge) {
    out.push_str("[[package.edges]]\n");
    out.push_str("[package.edges.target_key]\n");
    push_str_field(out, "ecosystem", edge.target_key.ecosystem.as_str());
    push_str_field(out, "name", &edge.target_key.name);
    push_str_field(out, "version", &edge.target_key.version);
    push_str_field(out, "source_id", &edge.target_key.source_id);
    push_str_field(out, "range", &edge.range);
    push_str_field(
        out,
        "kind",
        match edge.kind {
            crate::v4::EdgeKind::Normal => "normal",
            crate::v4::EdgeKind::Dev => "dev",
            crate::v4::EdgeKind::Optional => "optional",
            crate::v4::EdgeKind::Peer => "peer",
        },
    );
    match &edge.origin {
        crate::v4::EdgeOrigin::Manifest => {
            push_str_field(out, "origin", "manifest");
        }
        crate::v4::EdgeOrigin::Transitive { from_key } => {
            out.push_str("[package.edges.origin.transitive]\n");
            push_str_field(out, "ecosystem", from_key.ecosystem.as_str());
            push_str_field(out, "name", &from_key.name);
            push_str_field(out, "version", &from_key.version);
            push_str_field(out, "source_id", &from_key.source_id);
        }
    }
    if let Some(marker) = &edge.marker {
        push_str_field(out, "marker", marker);
    }
    // Full edge identities must be signed, including target and origin variants.
    // Ký đầy đủ danh tính cạnh, gồm variant của target và origin.
    push_variant(
        out,
        "package.edges.target_key.variant",
        &edge.target_key.variant,
    );
    if let crate::v4::EdgeOrigin::Transitive { from_key } = &edge.origin {
        push_variant(
            out,
            "package.edges.origin.transitive.variant",
            &from_key.variant,
        );
    }
}

/// Deterministic TOML serialization of the canonical payload — the ONLY
/// bytes the digest covers. Fixed section order, sorted collections,
/// `\n` endings on every platform (including Windows).
///
/// Precondition: integer fields fit i64 (TOML integers are i64-only).
/// Violations cannot sneak through: out-of-range values emit TOML that
/// fails to parse back, loudly — never silently wrong bytes. All
/// in-tree producers use small values (priorities, byte sizes).
/// Serialize TOML tất định của payload canonical — byte DUY NHẤT digest
/// phủ.
pub fn canonical_toml(payload: &LockfilePayload) -> String {
    let mut out = String::new();
    push_str_field(&mut out, "version", &payload.version);
    if let Some(profile) = &payload.optimizer_profile {
        push_str_field(&mut out, "optimizer_profile", profile);
    }
    out.push('\n');
    out.push_str("[metadata]\n");
    push_str_field(&mut out, "generator", &payload.generator);
    out.push('\n');

    let mut sources = payload.sources.clone();
    sources.sort_by(|a, b| a.id.cmp(&b.id));
    for source in &sources {
        push_source(&mut out, source);
        out.push('\n');
    }

    if !payload.peer_contexts.is_empty() {
        out.push_str("[peer_contexts]\n");
        for (digest, peers) in &payload.peer_contexts {
            let mut sorted = peers.clone();
            sorted.sort();
            let rendered: Vec<String> = sorted
                .iter()
                .map(|(name, version)| {
                    format!(
                        "[{}, {}]",
                        escape_toml_string(name),
                        escape_toml_string(version)
                    )
                })
                .collect();
            out.push_str(&escape_toml_string(digest));
            out.push_str(" = [");
            out.push_str(&rendered.join(", "));
            out.push_str("]\n");
        }
        out.push('\n');
    }

    if !payload.root_dependencies.is_empty() {
        let mut roots = payload.root_dependencies.clone();
        roots.sort();
        push_string_array(&mut out, "root_dependencies", &roots);
        out.push('\n');
    }

    if !payload.root_dependencies_by_owner.is_empty() {
        out.push_str("[root_dependencies_by_owner]\n");
        for (owner, roots) in &payload.root_dependencies_by_owner {
            let mut roots = roots.clone();
            roots.sort();
            roots.dedup();
            let rendered: Vec<String> = roots.iter().map(|root| escape_toml_string(root)).collect();
            out.push_str(&escape_toml_string(owner));
            out.push_str(" = [");
            out.push_str(&rendered.join(", "));
            out.push_str("]\n");
        }
        out.push('\n');
    }

    if let Some(workspace) = &payload.workspace {
        out.push_str("[workspace]\n");
        let mut members = workspace.members.clone();
        members.sort();
        push_string_array(&mut out, "members", &members);

        let mut cross_core_edges = workspace.cross_core_edges.clone();
        cross_core_edges.sort_by(|left, right| {
            (&left.from_core, &left.to_core, &left.package).cmp(&(
                &right.from_core,
                &right.to_core,
                &right.package,
            ))
        });
        for edge in &cross_core_edges {
            out.push_str("[[workspace.cross_core_edges]]\n");
            push_str_field(&mut out, "from_core", &edge.from_core);
            push_str_field(&mut out, "to_core", &edge.to_core);
            push_str_field(&mut out, "package", &edge.package);
            out.push('\n');
        }
        out.push('\n');
    }

    let mut packages = payload.packages.clone();
    packages.sort_by_cached_key(package_sort_key);
    for package in &packages {
        out.push_str("[[package]]\n");
        push_package_key(&mut out, &package.key);
        let mut edges = package.edges.clone();
        edges.sort_by_cached_key(edge_sort_key);
        for edge in &edges {
            push_edge(&mut out, edge);
        }
        if let Some(artifact) = &package.artifact {
            out.push_str("[package.artifact]\n");
            push_str_field(&mut out, "url", &artifact.url);
            if let Some(size) = artifact.size_bytes {
                out.push_str(&format!("size_bytes = {size}\n"));
            }
            push_str_field(&mut out, "content_hash", &artifact.content_hash);
            push_str_field(&mut out, "downloaded_from", &artifact.downloaded_from);
            // Only present when a lane supplied registry SRI — documents
            // without it digest exactly as before (no silent digest change).
            // Chỉ có khi lane cung cấp SRI — tài liệu thiếu field digest
            // y hệt như trước (không đổi digest ngầm).
            if let Some(sri) = artifact.integrity_sri.as_deref() {
                push_str_field(&mut out, "integrity_sri", sri);
            }
        }
        if let Some(provenance) = &package.provenance {
            out.push_str("[package.provenance]\n");
            push_str_field(&mut out, "source_kind", &provenance.source_kind);
            if let Some(tool) = &provenance.tool {
                push_str_field(&mut out, "tool", tool);
            }
            if let Some(imported_from) = &provenance.imported_from {
                push_str_field(&mut out, "imported_from", imported_from);
            }
        }
        if let Some(toolchain) = &package.toolchain {
            push_str_field(&mut out, "toolchain", toolchain);
        }
        if let Some(policy) = &package.scripts_policy {
            push_str_field(&mut out, "scripts_policy", policy);
        }
        if let Some(store_ref) = &package.store_ref {
            push_str_field(&mut out, "store_ref", store_ref);
        }
        out.push('\n');
    }
    out
}

/// Digest of the canonical payload (`blake3-<base64>`) — Digest của
/// payload canonical.
pub fn payload_digest(payload: &LockfilePayload) -> String {
    use mgc_crypto::blake3_signer::Blake3Hasher;
    let bytes = canonical_toml(payload);
    let hash = Blake3Hasher::hash_bytes(bytes.as_bytes());
    format!("blake3-{}", hash.to_base64())
}

/// Serialize the FULL v4 document (payload + hash + signature +
/// timestamps + workspace). Field ORDER here does not matter for the
/// digest — only `canonical_toml` output does. The document must parse
/// back into an identical `LockfileV4`.
/// (Serialize TOÀN BỘ tài liệu v4. Thứ tự field ở đây không ảnh hưởng
/// digest — chỉ output `canonical_toml` mới ảnh hưởng.)
pub fn write_v4_document(lock: &LockfileV4) -> crate::LockfileResult<String> {
    Ok(toml::to_string_pretty(lock)?)
}

/// Parse a full v4 document (any field order), rejecting non-v4 versions.
/// (Parse toàn bộ tài liệu v4, từ chối version khác.)
pub fn parse_v4_document(text: &str) -> crate::LockfileResult<LockfileV4> {
    let lock: LockfileV4 = toml::from_str(text)?;
    if lock.version != crate::v4::LOCKFILE_SCHEMA_V4 {
        return Err(crate::LockfileError::ParseError(format!(
            "expected v4 lockfile, got version '{}'",
            lock.version
        )));
    }
    Ok(lock)
}
