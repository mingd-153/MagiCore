//! Native registry engine wiring for the lib adapter (rust/python).
//! Đi dây engine registry native cho lib adapter (rust/python).
//!
//! Turns a `RegistryProtocol` engine's transitive resolution into BOTH the
//! `ResolvedGraph` (for the `DependencyResolver::resolve` trait) AND the
//! mgc.lock v3 `Package` entries (ecosystem/provenance/registry/markers/
//! artifact). The blake3 `content_hash` + `store_ref` are filled at install
//! time (only the download can produce them); resolve records everything the
//! registry index itself vouches for.
//! Chuyển kết quả resolve bắc cầu của engine `RegistryProtocol` thành CẢ
//! `ResolvedGraph` (cho trait `DependencyResolver::resolve`) LẪN entry
//! `Package` v3 của mgc.lock (ecosystem/provenance/registry/markers/artifact).
//! `content_hash` blake3 + `store_ref` điền lúc install (chỉ download mới sinh
//! ra được); resolve ghi mọi thứ mà chính registry index đảm bảo.

use mgc_lockfile::{ArtifactRef, EcosystemTag, Package, Provenance, SOURCE_KIND_NATIVE_RESOLVE};
use mgc_resolver::protocols::{RegistryProtocol, ResolvedEntry};
use mgc_types::{
    Manifest, MgResult, PackageId, PackageName, ResolvedGraph, ResolvedPackage, Version,
};
use std::collections::HashMap;

/// A native resolution: the graph for install + the v3 lock entries.
/// Một lần resolve native: graph cho install + entry lock v3.
#[derive(Debug, Clone)]
pub struct NativeResolution {
    pub graph: ResolvedGraph,
    pub lock_packages: Vec<Package>,
}

/// Resolve a manifest's regular dependencies through a native protocol engine.
/// Resolve dependencies thường của manifest qua một engine protocol native.
pub async fn resolve_with_protocol(
    protocol: &dyn RegistryProtocol,
    ecosystem: EcosystemTag,
    registry: &str,
    manifest: &Manifest,
) -> MgResult<NativeResolution> {
    resolve_with_protocol_inner(protocol, ecosystem, registry, manifest, false).await
}

/// Resolve both runtime and development dependencies through a native protocol.
/// Resolve cả dependency runtime và development qua protocol native.
///
/// App ecosystems such as Flutter need dev packages in the installed graph so
/// `flutter test --no-pub` can use SDK-provided test packages without Pub taking
/// over resolution. Other callers retain the existing runtime-only contract.
/// (Các ecosystem App như Flutter cần cài graph dev để `flutter test --no-pub`
/// dùng package test của SDK mà không giao quyền resolve cho Pub.)
pub async fn resolve_with_protocol_including_dev(
    protocol: &dyn RegistryProtocol,
    ecosystem: EcosystemTag,
    registry: &str,
    manifest: &Manifest,
) -> MgResult<NativeResolution> {
    resolve_with_protocol_inner(protocol, ecosystem, registry, manifest, true).await
}

async fn resolve_with_protocol_inner(
    protocol: &dyn RegistryProtocol,
    ecosystem: EcosystemTag,
    registry: &str,
    manifest: &Manifest,
    include_dev_dependencies: bool,
) -> MgResult<NativeResolution> {
    // Resolve every direct dep once, collecting the deduped transitive set.
    // (Resolve mỗi dep trực tiếp một lần, gom tập bắc cầu đã khử trùng.)
    let mut chosen: HashMap<String, ResolvedEntry> = HashMap::new();
    let mut entries: Vec<ResolvedEntry> = Vec::new();
    let mut runtime_reachable = std::collections::HashSet::new();
    let mut roots = manifest
        .dependencies
        .iter()
        .map(|dependency| (dependency, false))
        .collect::<Vec<_>>();
    if include_dev_dependencies {
        roots.extend(
            manifest
                .dev_dependencies
                .iter()
                .map(|dependency| (dependency, true)),
        );
    }
    for (dep, is_dev_root) in &roots {
        let sub = protocol
            .resolve_graph(dep.name.as_str(), dep.range.as_str())
            .await?;
        for entry in sub {
            if !*is_dev_root {
                runtime_reachable.insert(entry.name.clone());
            }
            if let Some(existing) = chosen.get(&entry.name) {
                if existing != &entry {
                    return Err(mgc_types::MgError::DependencyConflict(format!(
                        "registry resolution for {} is inconsistent across root dependencies ({} vs {}); refusing to write an ambiguous lock graph",
                        entry.name, existing.version, entry.version
                    )));
                }
            } else {
                chosen.insert(entry.name.clone(), entry.clone());
                entries.push(entry);
            }
        }
    }

    let mut packages = Vec::with_capacity(entries.len());
    let mut lock_packages = Vec::with_capacity(entries.len());
    for entry in &entries {
        let name = PackageName::new(entry.name.clone())
            .map_err(|_| mgc_types::MgError::InvalidPackageName(entry.name.clone()))?;
        let version = Version::parse(&entry.version)
            .map_err(|_| mgc_types::MgError::InvalidVersion(entry.version.clone()))?;
        let direct = roots
            .iter()
            .any(|(dependency, _)| dependency.name.as_str() == entry.name);

        let deps: Vec<PackageId> = entry
            .deps
            .iter()
            .map(|(dep_name, range)| {
                let package_name = PackageName::new(dep_name.clone())
                    .map_err(|_| mgc_types::MgError::InvalidPackageName(dep_name.clone()))?;
                let resolved = chosen.get(package_name.as_str()).ok_or_else(|| {
                    mgc_types::MgError::DependencyConflict(format!(
                        "resolved package {}@{} references {} ({range}) which is absent from the resolved graph",
                        entry.name, entry.version, dep_name
                    ))
                })?;
                let dep_version = Version::parse(&resolved.version)
                    .map_err(|_| mgc_types::MgError::InvalidVersion(resolved.version.clone()))?;
                Ok(PackageId::new(package_name, dep_version))
            })
            .collect::<mgc_types::MgResult<Vec<_>>>()?;

        let integrity = if entry.sha256.is_empty() {
            String::new()
        } else {
            format!("sha256-{}", entry.sha256)
        };

        packages.push(ResolvedPackage {
            id: PackageId::new(name, version),
            integrity: integrity.clone(),
            tarball_url: entry.artifact_url.clone(),
            deps,
            peer_deps: Vec::new(),
            direct,
            dev: !runtime_reachable.contains(&entry.name),
        });

        lock_packages.push(Package {
            owner_core: None,
            name: entry.name.clone(),
            version: entry.version.clone(),
            resolved: entry.artifact_url.clone(),
            integrity,
            dependencies: entry.deps.iter().map(|(n, _)| n.clone()).collect(),
            ecosystem,
            registry: Some(registry.to_string()),
            artifact: Some(ArtifactRef {
                url: entry.artifact_url.clone(),
                size_bytes: None,
                // blake3 filled at install (requires the downloaded bytes).
                content_hash: String::new(),
                downloaded_from: registry.to_string(),
            }),
            provenance: Some(Provenance {
                source_kind: SOURCE_KIND_NATIVE_RESOLVE.to_string(),
                tool: Some("mgc-resolver".to_string()),
                imported_from: None,
            }),
            markers: if entry.extra_markers.is_empty() {
                None
            } else {
                Some(entry.extra_markers.clone())
            },
            extras: None,
            peers: None,
            store_ref: None,
            toolchain: None,
            scripts_policy: None,
        });
    }

    Ok(NativeResolution {
        graph: ResolvedGraph { packages },
        lock_packages,
    })
}
