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
    // Resolve every direct dep once, collecting the deduped transitive set.
    // (Resolve mỗi dep trực tiếp một lần, gom tập bắc cầu đã khử trùng.)
    let mut chosen: HashMap<String, String> = HashMap::new();
    let mut entries: Vec<ResolvedEntry> = Vec::new();
    for dep in &manifest.dependencies {
        if chosen.contains_key(dep.name.as_str()) {
            continue;
        }
        let sub = protocol
            .resolve_graph(dep.name.as_str(), dep.range.as_str())
            .await?;
        for entry in sub {
            if !chosen.contains_key(&entry.name) {
                chosen.insert(entry.name.clone(), entry.version.clone());
                entries.push(entry);
            }
        }
    }

    let mut packages = Vec::with_capacity(entries.len());
    let mut lock_packages = Vec::with_capacity(entries.len());
    for entry in &entries {
        let Ok(name) = PackageName::new(entry.name.clone()) else {
            continue;
        };
        let Ok(version) = Version::parse(&entry.version) else {
            continue;
        };
        let direct = manifest
            .dependencies
            .iter()
            .any(|d| d.name.as_str() == entry.name);

        let deps: Vec<PackageId> = entry
            .deps
            .iter()
            .filter_map(|(dep_name, _)| {
                let dep_name = PackageName::new(dep_name.clone()).ok()?;
                let dep_version = chosen.get(dep_name.as_str())?;
                let dep_version = Version::parse(dep_version).ok()?;
                Some(PackageId::new(dep_name, dep_version))
            })
            .collect();

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
            dev: false,
        });

        lock_packages.push(Package {
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
