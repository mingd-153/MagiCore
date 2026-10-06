use anyhow::Result;
use serde::Serialize;

#[cfg(feature = "web")]
use crate::commands::web_registry_config::web_registry_url;
use crate::context::ProjectContext;
use mgc_ui::{info, success};

/// mgc outdated — check for outdated packages
pub async fn run(core: Option<&str>, json: bool) -> Result<()> {
    let outdated_pkgs = query_outdated(core).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outdated_pkgs)?);
        return Ok(());
    }

    if outdated_pkgs.is_empty() {
        success("All packages are up to date!");
    } else {
        for pkg in &outdated_pkgs {
            let severity = severity_label(&pkg.current, pkg.major);
            info(&format!(
                "  {}: {} → {} ({})",
                pkg.name, pkg.current, pkg.latest, severity
            ));
        }
        info(&format!(
            "{} package(s) outdated. Run 'mgc update' to update.",
            outdated_pkgs.len(),
        ));
    }

    Ok(())
}

/// Return outdated package data as JSON without printing it.
/// Trả dữ liệu package lỗi thời dạng JSON mà không in ra stdout.
pub async fn outdated_json(core: Option<&str>) -> Result<String> {
    Ok(serde_json::to_string_pretty(&query_outdated(core).await?)?)
}

async fn query_outdated(core: Option<&str>) -> Result<Vec<OutdatedPkg>> {
    let ctx = ProjectContext::load_with_core(core)?;
    let adapter = ctx.adapter();
    if adapter.core_id() != "web" {
        return query_outdated_native(&ctx).await;
    }

    #[cfg(feature = "web")]
    {
        query_outdated_web(ctx).await
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = adapter;
        Err(crate::error::outdated_no_web_adapter())
    }
}

#[cfg(feature = "web")]
async fn query_outdated_web(ctx: ProjectContext) -> Result<Vec<OutdatedPkg>> {
    let adapter = ctx.adapter();
    // P0/F6: arm the age gate from THIS operation's project (broken
    // config fails here, not silently unfiltered).
    // P0/F6: bật age gate từ project của thao tác này; config lỗi phải fail.
    adapter.arm_age_gate_for(ctx.root())?;
    let policy = mgc_web_adapter::WebAdapter::load_age_policy_for(ctx.root())?;

    let manifest = adapter.parse_manifest(ctx.root()).await?;
    let all_deps: Vec<_> = manifest.all_dependencies().collect();
    if all_deps.is_empty() {
        return Ok(Vec::new());
    }

    let registry = mgc_web_adapter::native::npm_registry::NpmRegistry::new(&web_registry_url());
    registry.set_age_gate_armed(policy.is_some_and(|p| p.cutoff_hours > 0));

    let mut outdated_pkgs: Vec<OutdatedPkg> = Vec::new();
    let mut checked = 0usize;
    let mut failed: Vec<String> = Vec::new();

    for dep in all_deps {
        match registry.fetch_metadata(dep.name.as_str()).await {
            Ok(meta) => {
                checked += 1;
                // Resolve latest through the age gate; never suggest a quarantined release.
                // Latest phải qua age gate; không đề xuất release đang quarantine.
                let latest = meta.dist_tags.get("latest");
                let latest_ver: Option<String> = match latest {
                    Some(candidate)
                        if mgc_web_adapter::provider::check_pinned_version(
                            &dep.name, &meta, candidate, policy,
                        )
                        .is_ok() =>
                    {
                        Some(candidate.clone())
                    }
                    _ => {
                        match mgc_web_adapter::provider::eligible_versions(&dep.name, &meta, policy)
                        {
                            Ok(eligible) => eligible
                                .into_iter()
                                .max()
                                .map(|version| version.to_string()),
                            Err(_) => None,
                        }
                    }
                };
                if let Some(latest_ver) = latest_ver
                    && let Ok(version) = mgc_types::Version::parse(&latest_ver)
                    && !dep.range.matches(&version)
                {
                    outdated_pkgs.push(OutdatedPkg {
                        name: dep.name.to_string(),
                        current: dep.range.to_string(),
                        latest: latest_ver,
                        major: version.major,
                        minor: version.minor,
                        patch: version.patch,
                    });
                }
            }
            Err(_) => failed.push(dep.name.to_string()),
        }
    }

    // Fail closed if no package metadata could be checked; partial failures remain visible.
    // Nếu không kiểm tra được package nào thì fail; lỗi một phần được báo rõ.
    if checked == 0 {
        return Err(crate::error::outdated_no_registry_response(
            failed.join(", "),
        ));
    }
    if !failed.is_empty() {
        mgc_ui::warning(&format!(
            "could not check {} package(s) (registry unreachable): {}",
            failed.len(),
            failed.join(", ")
        ));
    }
    Ok(outdated_pkgs)
}

/// Resolve latest versions through the selected core's native resolver.
/// (Resolve phiên bản mới nhất qua resolver native của core đang chọn.)
async fn query_outdated_native(ctx: &ProjectContext) -> Result<Vec<OutdatedPkg>> {
    let adapter = ctx.adapter();
    adapter.probe_dependency_resolver()?;
    adapter.arm_age_gate_for(ctx.root())?;

    let manifest = adapter.parse_manifest(ctx.root()).await?;
    if manifest.all_dependencies().next().is_none() {
        return Ok(Vec::new());
    }

    let current =
        crate::commands::core::shared::load_locked_graph(ctx.root(), adapter.name(), &manifest)?
            .ok_or_else(|| crate::error::outdated_lock_unusable(adapter.core_id()))?;
    // Validate configured age policy; the fresh resolve never trusts lock timestamps.
    // Xác thực age policy; resolve tươi không dựa vào timestamp trong lock.
    crate::commands::core::shared::refresh_locked_graph_for_age_gate(
        adapter,
        ctx.root(),
        true,
        false,
    )?;
    let latest_manifest = latest_candidate_manifest(&manifest);
    let latest = adapter.resolve_fresh(&latest_manifest).await?;
    native_outdated_packages(&manifest, &current, &latest)
}

/// Clone a manifest with unconstrained ranges for a fresh latest-version query.
/// (Clone manifest với range mở để hỏi resolver bản mới nhất.)
fn latest_candidate_manifest(manifest: &mgc_types::Manifest) -> mgc_types::Manifest {
    let mut latest = manifest.clone();
    let mut dependencies = Vec::new();
    for group in [
        &mut latest.dependencies,
        &mut latest.dev_dependencies,
        &mut latest.peer_dependencies,
        &mut latest.optional_dependencies,
    ] {
        for mut dependency in std::mem::take(group) {
            dependency.range = mgc_types::VersionRange::star();
            dependency.dev = false;
            dependency.optional = false;
            dependency.peer = false;
            dependencies.push(dependency);
        }
    }
    // Resolve each package name once even if it appears in several manifest groups.
    // Resolve mỗi package một lần kể cả khi tên xuất hiện ở nhiều nhóm manifest.
    dependencies.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    dependencies.dedup_by(|left, right| left.name == right.name);
    latest.dependencies = dependencies;
    latest
}

/// Compare the verified current graph with a fresh graph, reporting direct updates only.
/// (So sánh graph hiện tại đã kiểm chứng với graph resolve mới, chỉ báo direct dependency.)
fn native_outdated_packages(
    manifest: &mgc_types::Manifest,
    current: &mgc_types::adapter::ResolvedGraph,
    latest: &mgc_types::adapter::ResolvedGraph,
) -> Result<Vec<OutdatedPkg>> {
    let mut outdated = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dependency in manifest.all_dependencies() {
        if !seen.insert(dependency.name.to_string()) {
            continue;
        }
        let current_matches = current
            .packages
            .iter()
            .filter(|package| package.direct && package.id.name() == &dependency.name)
            .collect::<Vec<_>>();
        let latest_matches = latest
            .packages
            .iter()
            .filter(|package| package.direct && package.id.name() == &dependency.name)
            .collect::<Vec<_>>();
        let [current_package] = current_matches.as_slice() else {
            anyhow::bail!(
                "native outdated needs exactly one current locked version for direct dependency '{}' (found {})",
                dependency.name,
                current_matches.len()
            );
        };
        let [latest_package] = latest_matches.as_slice() else {
            anyhow::bail!(
                "native outdated resolver did not return exactly one latest version for direct dependency '{}' (found {})",
                dependency.name,
                latest_matches.len()
            );
        };
        let current_version = current_package.id.version();
        let latest_version = latest_package.id.version();
        if latest_version < current_version {
            anyhow::bail!(
                "native outdated resolver returned {} below locked version {} for '{}'; refusing to report a downgrade",
                latest_version,
                current_version,
                dependency.name
            );
        }
        if latest_version > current_version {
            outdated.push(OutdatedPkg {
                name: dependency.name.to_string(),
                current: current_version.to_string(),
                latest: latest_version.to_string(),
                major: latest_version.major,
                minor: latest_version.minor,
                patch: latest_version.patch,
            });
        }
    }
    outdated.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(outdated)
}

fn severity_label(current: &str, latest_major: u64) -> &'static str {
    let cur_major = current
        .trim_start_matches('^')
        .trim_start_matches('~')
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if latest_major > cur_major {
        "major"
    } else if latest_major == cur_major {
        "minor"
    } else {
        "patch"
    }
}

#[derive(Debug, Serialize)]
struct OutdatedPkg {
    name: String,
    current: String,
    latest: String,
    major: u64,
    minor: u64,
    patch: u64,
}

#[cfg(test)]
#[path = "test/outdated.rs"]
mod tests;
