//! Publish command — 11 bước orchestration (01 §4, §5 CLI surface)
//! (Lệnh publish: orchestrate 11 bước — git check, version bump, lifecycle, pack, registry select, PUT, 409, dist-tags, output)

use anyhow::{anyhow, bail, Result};
use clap::Args;
use semver::Version;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mg_ui::info;
use mg_config::npmrc::NpmRc;
use mg_config::project::ProjectConfig;
use mg_pack::ignore::select_files;
use mg_pack::manifest::{dep_fields, sanitize};
use mg_pack::tarball::pack;
use mg_publish::auth::resolve_auth;
use mg_types::PublishSummary;

/// Default registry (hardcode warning — OK: default const, overrideable via config/env)
const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";

#[derive(Args, Debug, Clone)]
pub struct PublishArgs {
    #[arg(long, help = "dist-tag (default: latest)")]
    pub tag: Option<String>,
    #[arg(long, help = "access level: public|restricted")]
    pub access: Option<String>,
    #[arg(long, help = "pack + verify, do not publish")]
    pub dry_run: bool,
    #[arg(long, help = "output JSON")]
    pub json: bool,
    #[arg(long, help = "OTP code for 2FA")]
    pub otp: Option<String>,
    #[arg(long, help = "delete existing version then republish (409)")]
    pub force: bool,
    #[arg(long, help = "skip lifecycle scripts")]
    pub ignore_scripts: bool,
    #[arg(long, help = "skip git checks")]
    pub no_git_checks: bool,
    #[arg(long, help = "publish branch (default: current branch tracking remote)")]
    pub publish_branch: Option<String>,
    #[arg(long, help = "all in one PUT (Phase 3)")]
    pub batch: bool,
    #[arg(long, help = "write JSON report file")]
    pub report_summary: bool,
    #[arg(long, help = "version bump patch")]
    pub patch: bool,
    #[arg(long, help = "version bump minor")]
    pub minor: bool,
    #[arg(long, help = "version bump major")]
    pub major: bool,
    #[arg(long, help = "override registry URL")]
    pub registry: Option<String>,
    #[arg(long, help = "override token (env MG_NPM_TOKEN recommended)")]
    pub token: Option<String>,
}pub async fn run(args: PublishArgs, recursive: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;

    if recursive {
        let project_root = ProjectConfig::find_project_root(&cwd)
            .ok_or_else(|| anyhow!("Could not find project root — run mg init"))?;
        let mut workspaces = super::install::discover_workspace_projects(&project_root)?
            .ok_or_else(|| anyhow!("--recursive requires megagate.workspace.toml (mode = \"monorepo\")"))?;
        workspaces = topo_sort_workspaces(workspaces)?;
        for ws in &workspaces {
            info(&format!("Publishing workspace: {}", ws.display()));
            publish_project(&args, ws).await?;
        }
        return Ok(());
    }
    publish_project(&args, &cwd).await
}

/// Sắp workspace theo topological — package phụ thuộc published trước
fn topo_sort_workspaces(workspaces: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let names: std::collections::HashMap<String, PathBuf> = workspaces
        .iter()
        .filter_map(|ws| {
            let pkg = ws.join("package.json");
            if !pkg.exists() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(pkg).ok()?).ok()?;
            Some((v.get("name")?.as_str()?.to_string(), ws.clone()))
        })
        .collect();

    let mut visited = std::collections::HashSet::new();
    let mut order = vec![];
    for ws in &workspaces {
        visit_ws(ws, &names, &mut visited, &mut order);
    }
    Ok(order)
}

fn visit_ws(
    ws: &Path,
    names: &std::collections::HashMap<String, PathBuf>,
    visited: &mut std::collections::HashSet<PathBuf>,
    order: &mut Vec<PathBuf>,
) {
    if !visited.insert(ws.to_path_buf()) {
        return;
    }
    let pkg = ws.join("package.json");
    if let Ok(raw) = fs::read_to_string(&pkg) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let mut deps: Vec<String> = vec![];
            for key in ["dependencies", "devDependencies", "peerDependencies"] {
                if let Some(map) = v.get(key).and_then(|d| d.as_object()) {
                    deps.extend(map.keys().cloned());
                }
            }
            for dep in deps {
                if let Some(dep_ws) = names.get(&dep) {
                    visit_ws(dep_ws, names, visited, order);
                }
            }
        }
    }
    order.push(ws.to_path_buf());
}

async fn publish_project(args: &PublishArgs, project_root: &Path) -> Result<()> {
    // 1. Git checks
    if !args.no_git_checks {
        git_checks(args)?;
    }

    // 2. Load project config
    let mut project = ProjectConfig::load(project_root)?
        .ok_or_else(|| anyhow!("mg.toml does not exist — run mg init"))?;

    // 3. Version bump (Q4)
    let bump = match (args.patch, args.minor, args.major) {
        (true, false, false) => "patch",
        (false, true, false) => "minor",
        (false, false, true) => "major",
        _ => "",
    };
    let new_version = if !bump.is_empty() {
        let mut v = Version::parse(&project.version)?;
        match bump {
            "patch" => v.patch += 1,
            "minor" => { v.minor += 1; v.patch = 0; }
            "major" => { v.major += 1; v.minor = 0; v.patch = 0; }
            _ => {}
        }
        let new_v = v.to_string();
        project.version = new_v.clone();
        new_v
    } else {
        project.version.clone()
    };

    // 4. Read package.json (web/lib ts)
    let pkg_json_path = project_root.join("package.json");
    let mut pkg_json: serde_json::Value = if pkg_json_path.exists() {
        serde_json::from_str(&fs::read_to_string(&pkg_json_path)?)?
    } else {
        serde_json::json!({})
    };

    // Update package.json name/version
    if pkg_json.get("name").is_none() {
        pkg_json["name"] = serde_json::Value::String(project.name.clone());
    }
    pkg_json["version"] = serde_json::Value::String(new_version.clone());

    // 5. Lifecycle scripts
    if !args.ignore_scripts {
        run_lifecycle(&pkg_json, "prepublishOnly")?;
        run_lifecycle(&pkg_json, "prepublish")?;
        run_lifecycle(&pkg_json, "prepare")?;
    }

    // 6. Manifest sanitize (exportable)
    let sanitized = sanitize(pkg_json.clone(), &project.name, &new_version)?;
    let dep_fields_map = dep_fields(&sanitized.manifest);
    // Tên publish = package.json name (scoped) — mg.toml name có thể thiếu scope
    let publish_name = pkg_json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|n| !n.is_empty())
        .unwrap_or(&project.name)
        .to_string();

    // 7. Files selection
    let files = select_files(&project_root)?;

    // 8. mg-pack: build tarball (filename dùng unscoped — scoped name chứa '/' làm tempdir join fail)
    let entry_prefix = format!("{}-{}", project.name, new_version);
    let filename = format!("{}-{}.tgz", project.name.replace(['/', '@'], "-"), new_version);
    let temp_dir = tempfile::tempdir()?;
    let tarball_path = temp_dir.path().join(filename);
    let pack_result = pack(&project_root, &tarball_path, &entry_prefix)?;

    // 9. Registry select (01 §2)
    let registry_url = select_registry(&pkg_json, &project_root, &project, &args)?;

    // 10. Auth resolution
    let npmrc = NpmRc::load(&project_root)?;
    let config_reg = project.registries.iter().find(|r| r.url == registry_url);
    let auth = resolve_auth(&npmrc, &registry_url, config_reg, args.token.as_deref())?;

    // 11. Pre-flight whoami (npmjs only)
    if registry_url.contains("registry.npmjs.org") && !args.dry_run {
        verify_token(&registry_url, &auth).await?;
    }

    // 12. Publish PUT
    if !args.dry_run {
        upload_tarball_client(&registry_url, &auth, &publish_name, &new_version, &tarball_path).await?;
        publish_put(
            &registry_url, &auth, &publish_name, &new_version,
            &sanitized.manifest, &dep_fields_map, &pack_result,
            &args,
        ).await?;
    }

    // 13. Dist-tags
    if !args.dry_run && args.tag.as_deref() != Some("latest") {
        set_dist_tag(&registry_url, &auth, &publish_name, &new_version, args.tag.as_deref().unwrap_or("latest")).await?;
    }

    // 14. Output summary
    let summary = PublishSummary {
        name: project.name.clone(),
        version: new_version,
        tag: args.tag.clone().unwrap_or_else(|| "latest".into()),
        size: pack_result.size,
        unpacked_size: pack_result.unpacked_size,
        shasum: pack_result.shasum.clone(),
        integrity: pack_result.integrity.clone(),
        files: files.len(),
        entry_count: pack_result.entry_count,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("✓ Published {}@{}", summary.name, summary.version);
        println!("  tag: {}", summary.tag);
        println!("  size: {} bytes (unpacked: {})", summary.size, summary.unpacked_size);
        println!("  shasum: {}", summary.shasum);
        println!("  integrity: {}", summary.integrity);
        println!("  files: {}, entries: {}", summary.files, summary.entry_count);
    }

    // Save updated version to mg.toml + package.json
    project.save(&project_root)?;
    if pkg_json_path.exists() {
        fs::write(&pkg_json_path, serde_json::to_string_pretty(&pkg_json)?)?;
    }

    Ok(())
}

/// Git checks — tree clean, on publish branch, remote not lagging
fn git_checks(args: &PublishArgs) -> Result<()> {
    // Tree clean (except ignored)
    let status = Command::new("git").args(["status", "--porcelain"]).output()?;
    if !status.stdout.is_empty() {
        bail!("Git tree not clean — commit or stash changes first (or --no-git-checks)");
    }

    // Current branch
    let branch = args.publish_branch.clone().unwrap_or_else(|| {
        Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"]).output()
            .ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_string())
            .unwrap_or_default()
    });

    // Has upstream
    let upstream = Command::new("git").args(["rev-parse", "--abbrev-ref", &format!("{}@{{upstream}}", branch)]).output();
    if upstream.is_ok() && upstream.unwrap().status.success() {
        // Check lag
        let lag = Command::new("git").args(["rev-list", "--count", &format!("HEAD..{}@{{upstream}}", branch)]).output()?;
        if lag.status.success() {
            let count = String::from_utf8_lossy(&lag.stdout).trim().parse::<usize>().unwrap_or(0);
            if count > 0 {
                bail!("Branch {} lags {} commits behind upstream — pull first", branch, count);
            }
        }
    }

    Ok(())
}

/// Select registry per 01 §2 priority
fn select_registry(
    pkg_json: &serde_json::Value,
    project_root: &Path,
    project: &ProjectConfig,
    args: &PublishArgs,
) -> Result<String> {
    // 1. --registry flag
    if let Some(r) = &args.registry { return Ok(r.clone()); }

    // 2. publishConfig.registry in package.json
    if let Some(url) = pkg_json.get("publishConfig").and_then(|p| p.get("registry")).and_then(|v| v.as_str()) {
        return Ok(url.to_string());
    }

    // 3. .npmrc registry / scope registry
    let npmrc = NpmRc::load(project_root)?;
    let scope = pkg_json.get("name").and_then(|v| v.as_str()).and_then(|n| n.strip_prefix('@').map(|s| format!("@{}", s)));
    if let Some(url) = npmrc.registry_for(scope.as_deref()) {
        return Ok(url);
    }

    // 4. mg.toml [registry]
    if let Some(reg) = project.registries.first() {
        return Ok(reg.url.clone());
    }

    // 5. Default
    Ok(DEFAULT_REGISTRY.to_string())
}

/// Run lifecycle script if defined
fn run_lifecycle(pkg_json: &serde_json::Value, script: &str) -> Result<()> {
    if let Some(cmd) = pkg_json.get("scripts").and_then(|s| s.get(script)).and_then(|v| v.as_str()) {
        println!("> {}: {}", script, cmd);
        let status = Command::new("sh").args(["-c", cmd]).status()?;
        if !status.success() {
            bail!("Lifecycle script '{}' failed", script);
        }
    }
    Ok(())
}

/// Verify token with whoami endpoint (npmjs)
async fn verify_token(registry_url: &str, auth: &mg_publish::auth::Auth) -> Result<()> {
    let client = reqwest::Client::new();
    let whoami_url = format!("{}/-/whoami", registry_url.trim_end_matches('/'));
    let req = client.get(&whoami_url);
    let req = if let Some(h) = auth.header_value() { req.header("Authorization", h) } else { req };
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        bail!("Token invalid (whoami failed: {})", status);
    }
    Ok(())
}

/// Publish PUT to registry
async fn publish_put(
    registry_url: &str, auth: &mg_publish::auth::Auth, name: &str, version: &str,
    manifest: &serde_json::Value, deps: &serde_json::Map<String, serde_json::Value>,
    pack_result: &mg_pack::tarball::PackResult, args: &PublishArgs,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}", registry_url.trim_end_matches('/'), name);

    // Build body
    let mut body = serde_json::Map::new();
    body.insert("_id".into(), serde_json::Value::String(name.into()));
    body.insert("name".into(), serde_json::Value::String(name.into()));
    body.insert("dist-tags".into(), serde_json::json!({"latest": version}));
    body.insert("maintainers".into(), serde_json::json!([]));
    body.insert("time".into(), serde_json::json!({"created": "", "modified": ""}));
    body.insert("private".into(), serde_json::json!(false));
    let mut versions = serde_json::Map::new();
    let mut version_obj = manifest.clone();
    let unscoped = name.rsplit('/').next().unwrap_or(name);
    version_obj.as_object_mut().unwrap().insert("dist".into(), serde_json::json!({
        "tarball": format!("{}/{}/-/{}-{}.tgz", registry_url.trim_end_matches('/'), name, unscoped, version),
        "shasum": pack_result.shasum,
        "integrity": pack_result.integrity,
        "fileCount": pack_result.entry_count,
        "unpackedSize": pack_result.unpacked_size,
    }));
    for (k, v) in deps {
        version_obj.as_object_mut().unwrap().insert(k.clone(), v.clone());
    }
    
    // Access field for scoped packages
    if let Some(access) = &args.access {
        version_obj.as_object_mut().unwrap().insert("access".into(), serde_json::Value::String(access.clone()));
    }
    
    // OTP
    if let Some(otp) = &args.otp {
        body.insert("otp".into(), serde_json::Value::String(otp.clone()));
    }
    
    versions.insert(version.into(), version_obj);
    body.insert("versions".into(), serde_json::Value::Object(versions));

    let req = client.put(&url).json(&body);
    let req = if let Some(h) = auth.header_value() { req.header("Authorization", h) } else { req };
    let resp = req.send().await?;
    let status = resp.status();

    if status.as_u16() == 409 {
        if args.force {
            // DELETE old version then retry
            let del_url = format!("{}/-/{}-{}.tgz", registry_url.trim_end_matches('/'), name, version);
            let del_req = client.delete(&del_url);
            let del_req = if let Some(h) = auth.header_value() { del_req.header("Authorization", h) } else { del_req };
            del_req.send().await?;
            // Retry PUT - need to rebuild request
            let body_retry = client.put(&url).json(&body);
            let body_retry = if let Some(h) = auth.header_value() { body_retry.header("Authorization", h) } else { body_retry };
            let retry = body_retry.send().await?;
            let retry_status = retry.status();
            if !retry_status.is_success() {
                let txt = retry.text().await?;
                bail!("Publish failed after force delete: {}", txt);
            }
        } else {
            bail!("Version {} already exists — use --force to overwrite", version);
        }
    } else if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Publish failed: {} - {}", status, txt);
    }

    Ok(())
}

async fn upload_tarball_client(
    registry_url: &str, auth: &mg_publish::auth::Auth, name: &str, version: &str,
    tarball_path: &std::path::Path,
) -> Result<()> {
    let client = reqwest::Client::new();
    let unscoped = name.rsplit('/').next().unwrap_or(name);
    let url = format!("{}/{}/-/{}-{}.tgz", registry_url.trim_end_matches('/'), name, unscoped, version);
    let data = fs::read(tarball_path)?;
    let req = client.put(&url).body(data);
    let req = if let Some(h) = auth.header_value() { req.header("Authorization", h) } else { req };
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Tarball upload failed: {} - {}", status, txt);
    }
    Ok(())
}

/// Set dist-tag
async fn set_dist_tag(registry_url: &str, auth: &mg_publish::auth::Auth, name: &str, version: &str, tag: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/-/package/{}/dist-tags/{}", registry_url.trim_end_matches('/'), name, tag);
    let body = serde_json::json!({ "version": version });
    let req = client.put(&url).json(&body);
    let req = if let Some(h) = auth.header_value() { req.header("Authorization", h) } else { req };
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Set dist-tag failed: {} - {}", status, txt);
    }
    Ok(())
}
