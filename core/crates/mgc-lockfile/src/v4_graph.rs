//! Lossless v4 lockfile → install-graph conversion + manifest closure.
//! Single home for both: CLI install paths and adapter resolve
//! short-circuits MUST call these instead of keeping local copies, so the
//! lossy-shape refusal rules cannot drift between copies.
//! (Chuyển lock v4 → graph install lossless + closure manifest — nhà
//! duy nhất: mọi đường install/resolve đều gọi vào đây, không copy riêng.)

use std::collections::{HashMap, HashSet};

use mgc_types::{Manifest, PackageId, ResolvedGraph, ResolvedPackage, Version};

use crate::canonical::LockfileV4;
use crate::policy::check_install_sri;
use crate::root_pin::{format_root_pin, parse_root_pin};
use crate::v4::{EdgeKind, canonical_name};
use crate::{LockfileError, LockfileResult};

fn bad(why: String) -> LockfileError {
    LockfileError::UnusableForInstall(why)
}

/// Build an install graph from a v4 lock document WITHOUT flattening v4
/// identity — every lossy shape fails closed instead of guessing:
/// duplicate `name@version` instances (distinct variant/source would
/// collapse into one node), pins without installer-enforced SRI or
/// without an artifact URL (the materializer could never verify them),
/// edges naming no locked package, and unparseable ids all refuse.
/// Edge kinds map losslessly: Normal/Optional/Dev → `deps`,
/// Peer → `peer_deps`; `dev` = reachable from owner roots only through
/// Dev edges (roots themselves are never dev).
/// (Dựng graph install từ lock v4 KHÔNG ép identity — shape hao hụt nào
/// cũng fail-closed thay vì đoán.)
pub fn graph_from_v4_lockfile(doc: &LockfileV4, owner_core: &str) -> LockfileResult<ResolvedGraph> {
    // 1. Index by name@version; a second instance would collapse.
    let mut index: HashMap<String, usize> = HashMap::new();
    for (position, package) in doc.packages.iter().enumerate() {
        let id = format!("{}@{}", package.key.name, package.key.version);
        if index.insert(id.clone(), position).is_some() {
            return Err(bad(format!(
                "duplicate locked instance '{id}' (distinct variant/source cannot share one graph node)"
            )));
        }
    }

    // 2. Owner roots (qualified pins normalize to plain ids).
    let empty_roots: Vec<String> = Vec::new();
    let raw_roots = doc
        .root_dependencies_by_owner
        .get(owner_core)
        .unwrap_or(&empty_roots);
    let raw_roots: &[String] = if raw_roots.is_empty() {
        &doc.root_dependencies
    } else {
        raw_roots
    };
    let root_ids: HashSet<String> = raw_roots
        .iter()
        .map(|root| parse_root_pin(root).package_id.to_string())
        .collect();

    // 3. Adjacency for edge mapping + dev reachability.
    let mut adjacency: Vec<Vec<(usize, bool)>> = vec![Vec::new(); doc.packages.len()];
    for (position, package) in doc.packages.iter().enumerate() {
        for edge in &package.edges {
            let target_id = format!("{}@{}", edge.target_key.name, edge.target_key.version);
            let Some(&target) = index.get(&target_id) else {
                return Err(bad(format!(
                    "edge from '{}@{}' names no locked package ('{target_id}')",
                    package.key.name, package.key.version
                )));
            };
            adjacency[position].push((target, matches!(edge.kind, EdgeKind::Dev)));
        }
    }
    // Reachable through any edge vs reachable without Dev edges.
    let mut reachable = vec![false; doc.packages.len()];
    let mut prod_reachable = vec![false; doc.packages.len()];
    let mut stack: Vec<usize> = root_ids
        .iter()
        .filter_map(|id| index.get(id).copied())
        .collect();
    for &root in &stack {
        reachable[root] = true;
        prod_reachable[root] = true;
    }
    while let Some(current) = stack.pop() {
        for &(target, is_dev) in &adjacency[current] {
            if !reachable[target] {
                reachable[target] = true;
                stack.push(target);
            }
            if !is_dev && !prod_reachable[target] {
                prod_reachable[target] = true;
                stack.push(target);
            }
        }
    }

    // 4. Packages.
    let mut packages = Vec::with_capacity(doc.packages.len());
    for (position, package) in doc.packages.iter().enumerate() {
        let id = format!("{}@{}", package.key.name, package.key.version);
        let package_id = PackageId::parse(&id).map_err(|error| {
            bad(format!(
                "locked instance '{id}' is not a valid package id: {error}"
            ))
        })?;
        let artifact = package.artifact.as_ref().ok_or_else(|| {
            bad(format!(
                "pin '{id}' has no artifact reference (no URL to fetch)"
            ))
        })?;
        if artifact.url.is_empty() {
            return Err(bad(format!("pin '{id}' has an empty artifact URL")));
        }
        let Some(sri) = artifact.integrity_sri.as_deref() else {
            return Err(bad(format!(
                "pin '{id}' has no installer-enforced SRI (re-resolve a lane that supplies registry SRI)"
            )));
        };
        if let Err(error) = check_install_sri(sri) {
            return Err(bad(format!("pin '{id}' carries {error}")));
        }
        let mut deps = Vec::new();
        let mut peer_deps = Vec::new();
        for edge in &package.edges {
            let target_id = format!("{}@{}", edge.target_key.name, edge.target_key.version);
            let target = PackageId::parse(&target_id).map_err(|error| {
                bad(format!(
                    "edge target '{target_id}' is not a valid package id: {error}"
                ))
            })?;
            match edge.kind {
                EdgeKind::Normal | EdgeKind::Optional | EdgeKind::Dev => deps.push(target),
                EdgeKind::Peer => peer_deps.push(target),
            }
        }
        let qualified_root = format_root_pin(package.key.ecosystem, &id);
        packages.push(ResolvedPackage {
            id: package_id,
            integrity: sri.to_string(),
            tarball_url: artifact.url.clone(),
            deps,
            peer_deps,
            direct: root_ids.contains(&id) || raw_roots.contains(&qualified_root),
            dev: reachable[position] && !prod_reachable[position],
        });
    }
    Ok(ResolvedGraph { packages })
}

/// v4 manifest-closure check. Two root modes:
/// - explicit roots (owner slice, else global list): roots must resolve,
///   every manifest range must match ≥1 same-name instance, every root
///   must be claimed by the manifest, and every locked package of the
///   manifest-selected ecosystems must be reachable from the roots;
/// - no roots (web locks deliberately carry none — manifest is the root
///   source of truth): each manifest dependency must match EXACTLY ONE
///   same-name instance, and every package of the manifest-selected
///   ecosystems must be reachable from those anchors.
///
/// (Kiểm tra closure v4: có roots thì roots phải thật + manifest khớp +
/// không mồ côi; không roots thì manifest làm neo với khớp duy-nhất.)
pub fn v4_graph_matches_manifest_closure(
    doc: &LockfileV4,
    manifest: &Manifest,
    owner_core: &str,
) -> bool {
    if doc.packages.is_empty() {
        return manifest.all_dependencies().next().is_none();
    }
    let by_id: HashMap<String, usize> = doc
        .packages
        .iter()
        .enumerate()
        .map(|(position, package)| {
            (
                format!("{}@{}", package.key.name, package.key.version),
                position,
            )
        })
        .collect();
    let name_matches = |dependency_name: &str| -> Vec<usize> {
        doc.packages
            .iter()
            .enumerate()
            .filter(|(_, package)| {
                package.key.name == dependency_name
                    || package.key.name == canonical_name(package.key.ecosystem, dependency_name)
            })
            .map(|(position, _)| position)
            .collect()
    };
    let empty_roots: Vec<String> = Vec::new();
    let raw_roots = doc
        .root_dependencies_by_owner
        .get(owner_core)
        .unwrap_or(&empty_roots);
    let raw_roots: &[String] = if raw_roots.is_empty() {
        &doc.root_dependencies
    } else {
        raw_roots
    };
    // Seed anchors: explicit roots, else exactly-one manifest matches.
    let mut anchors = Vec::new();
    if raw_roots.is_empty() {
        for dependency in manifest.all_dependencies() {
            let candidates: Vec<usize> = name_matches(dependency.name.as_str())
                .into_iter()
                .filter(|position| {
                    Version::parse(&doc.packages[*position].key.version)
                        .is_ok_and(|version| dependency.range.matches(&version))
                })
                .collect();
            if candidates.len() != 1 {
                return false;
            }
            anchors.push(candidates[0]);
        }
    } else {
        for root in raw_roots {
            let id = parse_root_pin(root).package_id.to_string();
            let Some(&position) = by_id.get(&id) else {
                return false;
            };
            anchors.push(position);
        }
        // Every manifest range must be satisfied (any-match).
        let ranges_ok = manifest.all_dependencies().all(|dependency| {
            name_matches(dependency.name.as_str())
                .into_iter()
                .filter_map(|position| Version::parse(&doc.packages[position].key.version).ok())
                .any(|version| dependency.range.matches(&version))
        });
        if !ranges_ok {
            return false;
        }
        // Every explicit root must be claimed by the manifest.
        let claimed = anchors.iter().all(|position| {
            let package = &doc.packages[*position];
            manifest.all_dependencies().any(|dependency| {
                (package.key.name == dependency.name.as_str()
                    || package.key.name
                        == canonical_name(package.key.ecosystem, dependency.name.as_str()))
                    && Version::parse(&package.key.version)
                        .is_ok_and(|version| dependency.range.matches(&version))
            })
        });
        if !claimed {
            return false;
        }
    }
    // BFS over all edges; every package of the selected ecosystems must
    // be reachable (orphan-free). Selected = ecosystems of the anchors.
    let mut reached = vec![false; doc.packages.len()];
    let mut stack = anchors.clone();
    while let Some(current) = stack.pop() {
        if reached[current] {
            continue;
        }
        reached[current] = true;
        for edge in &doc.packages[current].edges {
            let target_id = format!("{}@{}", edge.target_key.name, edge.target_key.version);
            if let Some(&target) = by_id.get(&target_id) {
                stack.push(target);
            }
        }
    }
    let selected: HashSet<crate::EcosystemTag> = anchors
        .iter()
        .map(|position| doc.packages[*position].key.ecosystem)
        .collect();
    if selected.is_empty() {
        return reached.iter().all(|seen| *seen);
    }
    doc.packages
        .iter()
        .enumerate()
        .filter(|(_, package)| selected.contains(&package.key.ecosystem))
        .all(|(position, _)| reached[position])
}
