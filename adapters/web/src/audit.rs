//! `audit.rs` — Security audits, supply-chain checks, and advisory query execution for WebAdapter.

use mg_types::adapter::{AuditReport, Vulnerability, VulnerabilitySeverity};
use mg_types::{DependencySpec, MgError, MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::Path;

use crate::lockfile::{read_web_lockfile_checked, write_web_lockfile_with_state};
use crate::manifest::{parse_manifest, write_manifest};

pub fn allow_insecure_loopback_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "http"
        && matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        )
}

pub fn is_tarball_url_trusted(tarball_url: &str, registry_url: &str) -> bool {
    let Ok(tarball_parsed) = url::Url::parse(tarball_url) else {
        return false;
    };

    let Ok(registry_parsed) = url::Url::parse(registry_url) else {
        return false;
    };

    let Some(tarball_host) = tarball_parsed.host_str() else {
        return false;
    };

    let Some(registry_host) = registry_parsed.host_str() else {
        return false;
    };

    if tarball_host == "127.0.0.1" || tarball_host == "localhost" || tarball_host == "::1" {
        return true;
    }

    if tarball_host == registry_host {
        return true;
    }

    if registry_host == "registry.npmjs.org" {
        return tarball_host == "registry.npmjs.org"
            || tarball_host.ends_with(".npmjs.org")
            || tarball_host == "registry.yarnpkg.com";
    }

    if let Ok(allowed) = std::env::var("MEGAGATE_WEB_ALLOWED_TARBALL_HOSTS") {
        let allowed_hosts: Vec<&str> = allowed
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if allowed_hosts.contains(&tarball_host) {
            return true;
        }
    }

    false
}

pub fn registry_advisory_bulk_endpoint(registry_url: &str) -> MgResult<url::Url> {
    let base = url::Url::parse(registry_url).map_err(|err| {
        MgError::Other(format!(
            "invalid web registry URL '{}': {}",
            registry_url, err
        ))
    })?;

    base.join("-/npm/v1/security/advisories/bulk")
        .map_err(|err| {
            MgError::Other(format!(
                "invalid advisory endpoint for registry '{}': {}",
                registry_url, err
            ))
        })
}

pub async fn run_audit(project_root: &Path, registry_url: &str) -> MgResult<AuditReport> {
    let lockfile = match read_web_lockfile_checked(project_root)? {
        Some(lock) => lock,
        None => return Ok(AuditReport::clean(0)),
    };

    if lockfile.packages.is_empty() {
        return Ok(AuditReport::clean(0));
    }

    let mut body = serde_json::Map::new();
    for pkg in &lockfile.packages {
        let key = pkg.name.to_string();
        let version_entry = serde_json::json!([pkg.version.clone()]);
        body.insert(key, version_entry);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(format!("megagate/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| MgError::Network(format!("audit client error: {e}")))?;

    let advisory_endpoint = registry_advisory_bulk_endpoint(registry_url)?;
    let response = client
        .post(advisory_endpoint)
        .json(&body)
        .send()
        .await
        .map_err(|e| MgError::Network(format!("audit request failed: {e}")))?;

    let package_count = lockfile.packages.len();

    if !response.status().is_success() {
        return Err(MgError::Network(format!(
            "audit API returned {}",
            response.status()
        )));
    }

    let advisories: serde_json::Value = response
        .json()
        .await
        .map_err(|e| MgError::Other(format!("audit response parse error: {e}")))?;

    let mut vulnerabilities = Vec::new();
    if let Some(map) = advisories.as_object() {
        for (pkg_name, advisory_list) in map {
            if let Some(advisories_arr) = advisory_list.as_array() {
                for advisory in advisories_arr {
                    let title = advisory["title"]
                        .as_str()
                        .unwrap_or("Unknown vulnerability")
                        .to_string();
                    let severity_str = advisory["severity"].as_str().unwrap_or("info").to_string();
                    let cve = advisory["cves"]
                        .as_array()
                        .and_then(|cves| cves.first())
                        .and_then(|c| c.as_str())
                        .unwrap_or("CVE-UNKNOWN")
                        .to_string();
                    let patched = advisory["vulnerable_versions"]
                        .as_str()
                        .map(|s| s.to_string());
                    let url = advisory["url"].as_str().map(|s| s.to_string());
                    let version = advisory["findings"]
                        .as_array()
                        .and_then(|f| f.first())
                        .and_then(|f| f["version"].as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let Ok(pkg_name_parsed) = PackageName::new(pkg_name.clone()) else {
                        continue;
                    };
                    let ver = Version::parse(&version).unwrap_or_else(|_| Version::new(0, 0, 0));

                    vulnerabilities.push(Vulnerability {
                        package: PackageId::new(pkg_name_parsed, ver),
                        title,
                        severity: severity_str.clone(),
                        cve,
                        severity_level: VulnerabilitySeverity::from_str(&severity_str),
                        patched_versions: patched,
                        url,
                    });
                }
            }
        }
    }

    let vuln_count = vulnerabilities.len();
    Ok(AuditReport {
        packages_audited: package_count,
        vulnerability_count: vuln_count,
        vulnerabilities,
    })
}

pub async fn run_audit_fix<F, Fut>(
    project_root: &Path,
    vulnerable: &[PackageId],
    resolve_fn: F,
) -> MgResult<usize>
where
    F: FnOnce(mg_types::Manifest) -> Fut,
    Fut: std::future::Future<Output = MgResult<mg_types::adapter::ResolvedGraph>>,
{
    if vulnerable.is_empty() {
        return Ok(0);
    }

    let mut manifest = parse_manifest(project_root)?;
    let names: std::collections::HashSet<&str> =
        vulnerable.iter().map(|id| id.name_str()).collect();

    let flags = [
        (false, false, false),
        (true, false, false),
        (false, false, true),
        (false, true, false),
    ];
    let mut bumps: Vec<(DependencySpec, bool, bool, bool)> = Vec::new();
    for (group, (dev, optional, peer)) in manifest.dep_groups().iter().zip(flags) {
        for spec in group.1 {
            if names.contains(spec.name.as_str()) {
                bumps.push((spec.clone(), dev, optional, peer));
            }
        }
    }
    if bumps.is_empty() {
        return Err(MgError::Other(
            "audit --fix: found no matching dependency to bump (validate manifest)".into(),
        ));
    }

    for (spec, dev, optional, peer) in &bumps {
        let new_spec = DependencySpec::new(spec.name.clone(), VersionRange::star());
        manifest.remove_dep(spec.name.as_str());
        manifest.add_dep(new_spec, *dev, *optional, *peer);
    }

    let graph = resolve_fn(manifest.clone()).await?;
    write_manifest(project_root, &manifest)?;
    write_web_lockfile_with_state(project_root, &graph, "fixed")?;
    Ok(bumps.len())
}
