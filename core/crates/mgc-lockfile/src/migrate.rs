//! Lockfile version migrations (v1 → v2, v2 → v3)
//! Migration phiên bản lockfile (v1 → v2, v2 → v3)

use crate::schema::{LOCKFILE_SCHEMA_VERSION, Provenance, SOURCE_KIND_REGISTRY_IMPORT};
use crate::{Lockfile, LockfileError, LockfileResult, Package};
use serde::{Deserialize, Serialize};

/// Lockfile v1 structure (legacy) — Cấu trúc lockfile v1 (legacy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileV1 {
    pub version: String,
    #[serde(rename = "package")]
    pub packages: Vec<PackageV1>,
}

/// Package v1 structure (no integrity field) — Cấu trúc package v1 (không có integrity)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageV1 {
    pub name: String,
    pub version: String,
    pub resolved: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Detect lockfile version from TOML string — Phát hiện version lockfile từ chuỗi TOML
pub fn detect_lockfile_version(toml_str: &str) -> LockfileResult<u8> {
    // Try to parse version field
    let parsed: toml::Value = toml::from_str(toml_str)?;

    let version = parsed
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LockfileError::ParseError("missing version field".to_string()))?;

    match version {
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        "4" => Ok(4),
        _ => Err(LockfileError::ParseError(format!(
            "unknown version: {}",
            version
        ))),
    }
}

/// Migrate lockfile v1 to v2 — Migrate lockfile v1 sang v2
///
/// Emits the v2 shape on purpose (version pinned to "2") so the step stays
/// composable; `auto_upgrade_lockfile` chains it into `migrate_v2_to_v3`.
/// Cố ý xuất shape v2 (version ghim "2") để bước này ghép được;
/// `auto_upgrade_lockfile` sẽ nối tiếp bằng `migrate_v2_to_v3`.
pub fn migrate_v1_to_v2(lockfile_v1: LockfileV1) -> LockfileResult<Lockfile> {
    let mut lockfile_v2 = Lockfile::new();
    // Pin the intermediate schema version — the name of this function is a
    // contract (v1 → v2), the final bump happens in migrate_v2_to_v3.
    // Ghim version schema trung gian — tên hàm là hợp đồng (v1 → v2),
    // bước nâng cuối nằm ở migrate_v2_to_v3.
    lockfile_v2.version = "2".to_string();

    // Migrate packages
    for pkg_v1 in lockfile_v1.packages {
        let pkg_v2 = Package {
            name: pkg_v1.name.clone(),
            version: pkg_v1.version.clone(),
            resolved: pkg_v1.resolved.clone(),
            // L7 FIX: Use BLAKE3 for placeholder (still placeholder but cryptographic)
            // Production: download tarball and hash, or warn user to re-install
            integrity: format!("blake3-{}", blake3_placeholder_hash(&pkg_v1.resolved)),
            dependencies: pkg_v1.dependencies.clone(),
            // v3 fields keep their defaults (ecosystem=other, provenance=None);
            // migrate_v2_to_v3 fills provenance right after.
            // Field v3 giữ mặc định (ecosystem=other, provenance=None);
            // migrate_v2_to_v3 sẽ điền provenance ngay sau đó.
            ..Default::default()
        };

        lockfile_v2.add_package(pkg_v2);
    }

    Ok(lockfile_v2)
}

/// Migrate lockfile v2 to v3 — Migrate lockfile v2 sang v3
///
/// v2 files carry no ecosystem data, so after migration every package has
/// `ecosystem = other` (the serde default applied during the v2 parse) and
/// `provenance = registry-import` where provenance was empty. Ecosystem is
/// NOT force-reset: running the migration on a v3 lock must not clobber
/// existing ecosystem data.
/// File v2 không mang dữ liệu ecosystem, nên sau migration mọi package có
/// `ecosystem = other` (mặc định serde khi parse v2) và
/// `provenance = registry-import` ở chỗ provenance còn trống. KHÔNG ép
/// reset ecosystem: chạy migration trên lock v3 không được xoá dữ liệu
/// ecosystem sẵn có.
pub fn migrate_v2_to_v3(mut lockfile: Lockfile) -> Lockfile {
    lockfile.version = LOCKFILE_SCHEMA_VERSION.to_string();

    for pkg in &mut lockfile.packages {
        if pkg.provenance.is_none() {
            pkg.provenance = Some(Provenance {
                source_kind: SOURCE_KIND_REGISTRY_IMPORT.to_string(),
                tool: None,
                imported_from: None,
            });
        }
    }

    lockfile
}

/// Migrate lockfile v3 to v4 — Migrate lockfile v3 sang v4.
///
/// Returns the v4 document PLUS every lossy decision as warnings (never
/// silent):
/// - `(name, version)` identity splits by registry: same name+version
///   from two registries becomes two PackageKeys (the v3 merge lie,
///   undone).
/// - v3 `dependencies: [names]` carry no range/version: an edge is kept
///   only when exactly one locked package bears the name (range `"*"`,
///   transitive origin); zero or several matches → edge dropped +
///   warning (no fabricated target).
/// - v3 `peers: [names]` carry no versions: peer digests cannot be
///   computed → peers are dropped + warning (a digest from thin air
///   would be fake data).
/// - packages without a registry land on the explicit `"unknown"`
///   source (labeled, never a guessed URL).
/// - empty v3 integrity stays empty (v4 verify will fail on it — the
///   failure belongs to the artifact, not the migration).
pub fn migrate_v3_to_v4(
    lockfile: Lockfile,
) -> LockfileResult<(crate::canonical::LockfileV4, Vec<String>)> {
    use crate::canonical::{LockfileV4, LockfileV4Metadata, PackageV4};
    use crate::v4::{Edge, EdgeKind, EdgeOrigin, PackageKey, SourceRef, VariantKey};

    let mut warnings = Vec::new();
    let mut sources: Vec<SourceRef> = Vec::new();
    let mut source_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // One source entry per distinct registry URL; registry-less pins
    // share the labeled "unknown" source.
    // (Một entry nguồn cho mỗi URL registry khác nhau.)
    for package in &lockfile.packages {
        let Some(registry) = package.registry.as_deref() else {
            continue;
        };
        if source_ids.contains_key(registry) {
            continue;
        }
        let id = format!("src-{}", &blake3_short_hex(registry)[..12]);
        source_ids.insert(registry.to_string(), id.clone());
        sources.push(SourceRef {
            id,
            url: registry.to_string(),
            ecosystem: package.ecosystem,
            priority: 100,
            claims: vec!["*".to_string()],
            trusted: false,
            allow_hosts: Vec::new(),
            allow_cidrs: Vec::new(),
            allow_protocols: vec!["https".to_string()],
        });
    }
    let unknown_used = lockfile.packages.iter().any(|p| p.registry.is_none());
    if unknown_used {
        sources.push(SourceRef {
            id: "unknown".to_string(),
            url: String::new(),
            ecosystem: crate::EcosystemTag::Other,
            priority: crate::v4::UNKNOWN_SOURCE_PRIORITY,
            claims: Vec::new(),
            trusted: false,
            allow_hosts: Vec::new(),
            allow_cidrs: Vec::new(),
            allow_protocols: Vec::new(),
        });
        warnings.push(
            "some pins have no registry: they share source \"unknown\" — re-resolve to attribute them"
                .to_string(),
        );
    }

    let mut packages_v4 = Vec::new();
    for package in &lockfile.packages {
        let source_id = package
            .registry
            .as_deref()
            .and_then(|r| source_ids.get(r))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let key = PackageKey {
            ecosystem: package.ecosystem,
            name: crate::v4::canonical_name(package.ecosystem, &package.name),
            version: crate::v4::canonical_version(&package.version),
            source_id,
            variant: VariantKey {
                peer_context: None,
                feature_set: package.extras.clone(),
                target: None,
            },
        };
        let mut edges = Vec::new();
        for dep_name in &package.dependencies {
            let hits: Vec<&Package> = lockfile
                .packages
                .iter()
                .filter(|p| &p.name == dep_name)
                .collect();
            match hits.as_slice() {
                [hit] => edges.push(Edge {
                    target_key: PackageKey {
                        ecosystem: hit.ecosystem,
                        name: crate::v4::canonical_name(hit.ecosystem, &hit.name),
                        version: crate::v4::canonical_version(&hit.version),
                        source_id: hit
                            .registry
                            .as_deref()
                            .and_then(|r| source_ids.get(r))
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string()),
                        variant: VariantKey::default(),
                    },
                    range: "*".to_string(),
                    kind: EdgeKind::Normal,
                    origin: EdgeOrigin::Transitive {
                        from_key: Box::new(key.clone()),
                    },
                    marker: None,
                }),
                [] => warnings.push(format!(
                    "dropped edge {} -> {}: no locked package bears that name",
                    package.name, dep_name
                )),
                _ => warnings.push(format!(
                    "dropped edge {} -> {}: {} locked packages share that name (ambiguous without versions)",
                    package.name,
                    dep_name,
                    hits.len()
                )),
            }
        }
        if let Some(peers) = package.peers.as_ref()
            && !peers.is_empty()
        {
            warnings.push(format!(
                "dropped {} peer(s) of {}: v3 peers carry no versions, peer digests cannot be computed",
                peers.len(),
                package.name
            ));
        }
        if package.integrity.is_empty() {
            warnings.push(format!(
                "pin {}@{} has no integrity: v4 verify will fail on it until re-fetched",
                package.name, package.version
            ));
        }
        packages_v4.push(PackageV4 {
            key,
            edges,
            artifact: package.artifact.clone(),
            provenance: package.provenance.clone(),
            toolchain: package.toolchain.clone(),
            scripts_policy: package.scripts_policy.clone(),
            store_ref: package.store_ref.clone(),
        });
    }

    let mut root_dependencies = lockfile.root_dependencies.clone();
    root_dependencies.sort();
    let v4 = LockfileV4 {
        version: crate::v4::LOCKFILE_SCHEMA_V4.to_string(),
        metadata: LockfileV4Metadata {
            generated_at: String::new(),
            generator: lockfile.metadata.generator.clone(),
            lockfile_hash: String::new(),
            signature: None,
        },
        sources,
        peer_contexts: Default::default(),
        root_dependencies,
        packages: packages_v4,
        workspace: lockfile.workspace.clone(),
        optimizer_profile: lockfile.optimizer_profile.clone(),
    };
    Ok((v4, warnings))
}

/// Short BLAKE3 hex for generated source ids — Hex BLAKE3 ngắn cho id
/// nguồn sinh ra.
fn blake3_short_hex(input: &str) -> String {
    use mgc_crypto::blake3_signer::Blake3Hasher;
    Blake3Hasher::hash_string(input).to_hex()
}
/// Parse lockfile v1 from TOML string — Parse lockfile v1 từ chuỗi TOML
pub fn parse_lockfile_v1(toml_str: &str) -> LockfileResult<LockfileV1> {
    let lockfile_v1: LockfileV1 = toml::from_str(toml_str)?;

    if lockfile_v1.version != "1" {
        return Err(LockfileError::ParseError(format!(
            "expected version 1, got {}",
            lockfile_v1.version
        )));
    }

    Ok(lockfile_v1)
}

/// Auto-upgrade lockfile to the latest v3 schema (v1/v2 → v3) — Tự động nâng cấp lockfile lên schema v3 mới nhất (v1/v2 → v3).
///
/// v4 is NEVER an automatic target: the major bump changes identity,
/// digest coverage, and signature shape, so migration stays an explicit
/// `mgc migrate lock --to v4` decision. A v4 document here is an error
/// pointing at the v4 reader.
/// (v4 KHÔNG BAO GIỜ là đích tự động: bump major đổi identity, phủ
/// digest và shape chữ ký, nên migration là quyết định tường minh.)
pub fn auto_upgrade_lockfile(toml_str: &str) -> LockfileResult<Lockfile> {
    let version = detect_lockfile_version(toml_str)?;

    match version {
        1 => {
            let lockfile_v1 = parse_lockfile_v1(toml_str)?;
            // Chain both steps so old locks land on the canonical v3 graph.
            // Nối cả hai bước để lock cũ về đồ thị canonical v3.
            Ok(migrate_v2_to_v3(migrate_v1_to_v2(lockfile_v1)?))
        }
        2 => {
            let lockfile = crate::parser::parse_lockfile(toml_str)?;
            Ok(migrate_v2_to_v3(lockfile))
        }
        3 => crate::parser::parse_lockfile(toml_str),
        4 => Err(LockfileError::ParseError(
            "v4 lockfile: use the v4 reader (migration is explicit, never automatic)".to_string(),
        )),
        _ => Err(LockfileError::ParseError(format!(
            "unsupported version: {}",
            version
        ))),
    }
}

/// L7 FIX: BLAKE3 placeholder hash for migration (cryptographic, not DefaultHasher)
/// Still placeholder — production should warn user to re-install packages
fn blake3_placeholder_hash(s: &str) -> String {
    use mgc_crypto::blake3_signer::Blake3Hasher;
    let hash = Blake3Hasher::hash_string(s);
    hash.to_base64()
}
