//! Publish command — 11 bước orchestration (01 §4, §5 CLI surface)
//! (Lệnh publish: orchestrate 11 bước — git check, version bump, lifecycle, pack, registry select, PUT, 409, dist-tags, output)

use anyhow::{Result, anyhow, bail};
use clap::Args;
use semver::Version;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mgc_config::npmrc::NpmRc;
use mgc_config::project::ProjectConfig;
use mgc_pack::ignore::select_files;
use mgc_pack::manifest::{dep_fields, sanitize};
use mgc_pack::tarball::pack;
use mgc_publish::auth::resolve_auth;
use mgc_types::PublishSummary;
use mgc_ui::info;

/// Default registry (hardcode warning — OK: default const, overrideable via config/env)
const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";
const DEFAULT_PUBLISH_LIFECYCLE_TIMEOUT_SECS: u64 = 300;
const MAX_TRUSTED_EXCHANGE_RESPONSE_BYTES: usize = 4 * 1024;
const OCI_UPLOAD_CHUNK_BYTES: usize = 1024 * 1024;
const TRUSTED_TOKEN_REFRESH_MARGIN_SECS: u64 = 60;
const TRUSTED_TOKEN_EXPIRY_SKEW_SECS: u64 = 5;
const TRUSTED_DOCKER_MAX_PUSH_SECS: u64 = 1800;
const TRUSTED_NPM_REQUEST_TIMEOUT_SECS: u64 = 600;
const TRUSTED_PYPI_REQUEST_TIMEOUT_SECS: u64 = 300;
const TRUSTED_OCI_REQUEST_MAX_TIMEOUT_SECS: u64 = 1800;
const PUBLISH_LIFECYCLE_TIMEOUT_ENV: &str = "MGC_PUBLISH_LIFECYCLE_TIMEOUT_SECS";
const PUBLISH_LIFECYCLE_HOOKS: &[&str] = &["prepublishOnly", "prepublish", "prepare"];

#[derive(Args, Debug, Clone)]
pub struct PublishArgs {
    #[arg(long, value_parser = ["npm", "pypi", "oci"], default_value = "npm")]
    pub protocol: String,
    #[arg(long, requires = "trusted", help = "package name for PyPI or OCI")]
    pub package: Option<String>,
    #[arg(
        long,
        requires = "trusted",
        help = "version for PyPI or OCI tag for OCI"
    )]
    pub version: Option<String>,
    #[arg(
        long = "artifact",
        requires = "trusted",
        help = "PyPI or OCI artifact file (repeatable)"
    )]
    pub artifacts: Vec<PathBuf>,
    #[arg(
        long,
        requires = "trusted",
        help = "local Docker image reference for OCI image publish"
    )]
    pub image: Option<String>,
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
    #[arg(long, help = "explicitly allow package publish lifecycle scripts")]
    pub allow_scripts: bool,
    #[arg(long, help = "skip git checks")]
    pub no_git_checks: bool,
    #[arg(
        long,
        help = "publish branch (default: current branch tracking remote)"
    )]
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
    #[arg(long, help = "override token (env MGC_NPM_TOKEN recommended)")]
    pub token: Option<String>,
    #[arg(
        long,
        conflicts_with = "token",
        help = "publish with a short-lived CI OIDC token"
    )]
    pub trusted: bool,
    #[arg(
        long,
        requires = "trusted",
        help = "OIDC audience (defaults to the registry origin)"
    )]
    pub trusted_audience: Option<String>,
}
pub async fn run(args: PublishArgs, core: Option<&str>, recursive: bool) -> Result<()> {
    if args.protocol != "npm" {
        return publish_trusted_non_npm(&args, core).await;
    }
    let cwd = std::env::current_dir()?;

    if recursive {
        let project_root = ProjectConfig::find_project_root(&cwd)
            .ok_or_else(|| anyhow!("Could not find project root — run mgc init"))?;
        let mut workspaces = super::install::discover_workspace_projects(&project_root)?
            .ok_or_else(|| {
                anyhow!("--recursive requires magicore.workspace.toml (mode = \"monorepo\")")
            })?;
        workspaces = topo_sort_workspaces(workspaces)?;
        for ws in &workspaces {
            info(&format!("Publishing workspace: {}", ws.display()));
            publish_project(&args, ws).await?;
        }
        return Ok(());
    }
    publish_project(&args, &cwd).await
}

/// `mgc stage` — pack project vào .mgc-stage/ (không đăng registry).
/// Kiểm tra nhanh trước khi publish: tarball hợp lệ, files selection đúng.
pub async fn stage(dir: Option<String>) -> Result<()> {
    // Resolve --dir against the real cwd: downstream pack/tar joins
    // assume an absolute root, and a bare relative dir surfaces as a
    // cryptic empty-path IO error.
    // (--dir tương đối phải nối với cwd — pack/tar cần root tuyệt đối.)
    let cwd = match dir {
        Some(d) => {
            let p = PathBuf::from(&d);
            if p.is_absolute() {
                p
            } else {
                std::env::current_dir()?.join(p)
            }
        }
        None => std::env::current_dir()?,
    };
    let project_root =
        ProjectConfig::find_project_root(&cwd).ok_or_else(crate::error::project_root_missing)?;
    let project = ProjectConfig::load(&project_root)?.ok_or_else(crate::error::mgc_toml_missing)?;

    let entry_prefix = format!("{}-{}", project.name, project.version);
    let filename = format!(
        "{}-{}.tgz",
        project.name.replace(['/', '@'], "-"),
        project.version
    );
    let stage_dir = project_root.join(".mgc-stage");
    fs::create_dir_all(&stage_dir)?;
    let tarball_path = stage_dir.join(&filename);
    pack(&project_root, &tarball_path, &entry_prefix)?;

    let size = fs::metadata(&tarball_path).map(|m| m.len()).unwrap_or(0);
    mgc_ui::success(&format!(
        "staged {} ({:.1} KiB) → {}",
        filename,
        size as f64 / 1024.0,
        tarball_path.display()
    ));
    mgc_ui::info("next: `mgc publish` to upload, or `mgc publish --dry-run` to verify first");
    Ok(())
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
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(raw) = fs::read_to_string(&pkg)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw)
    {
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
    order.push(ws.to_path_buf());
}

async fn publish_project(args: &PublishArgs, project_root: &Path) -> Result<()> {
    // 1. Git checks
    if !args.no_git_checks {
        git_checks(args.publish_branch.as_deref(), project_root)?;
    }

    // 2. Load project config
    let mut project = ProjectConfig::load(project_root)?
        .ok_or_else(|| anyhow!("mgc.toml does not exist — run mgc init"))?;

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
            "minor" => {
                v.minor += 1;
                v.patch = 0;
            }
            "major" => {
                v.major += 1;
                v.minor = 0;
                v.patch = 0;
            }
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
    if publish_lifecycle_decision(&pkg_json, args.ignore_scripts, args.allow_scripts)? {
        run_lifecycle(&pkg_json, "prepublishOnly", project_root)?;
        run_lifecycle(&pkg_json, "prepublish", project_root)?;
        run_lifecycle(&pkg_json, "prepare", project_root)?;
    }

    // 6. Manifest sanitize (exportable)
    let sanitized = sanitize(pkg_json.clone(), &project.name, &new_version)?;
    let dep_fields_map = dep_fields(&sanitized.manifest);
    // Tên publish = package.json name (scoped) — mgc.toml name có thể thiếu scope
    let publish_name = pkg_json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|n| !n.is_empty())
        .unwrap_or(&project.name)
        .to_string();

    // 7. Files selection
    let files = select_files(project_root)?;

    // 8. mgc-pack: build tarball (filename dùng unscoped — scoped name chứa '/' làm tempdir join fail)
    let entry_prefix = format!("{}-{}", project.name, new_version);
    let filename = format!(
        "{}-{}.tgz",
        project.name.replace(['/', '@'], "-"),
        new_version
    );
    let temp_dir = tempfile::tempdir()?;
    let tarball_path = temp_dir.path().join(filename);
    let pack_result = pack(project_root, &tarball_path, &entry_prefix)?;

    // 9. Registry select (01 §2)
    let registry_url = select_registry(&pkg_json, project_root, &project, args)?;

    // 10. Auth resolution
    let npmrc = NpmRc::load(project_root)?;
    let config_reg = project.registries.iter().find(|r| r.url == registry_url);
    let mut trusted_session = None;
    let auth = if args.trusted && !args.dry_run {
        let session = trusted_publish_session(
            &registry_url,
            &publish_name,
            "npm",
            args.trusted_audience.as_deref(),
        )
        .await?;
        let auth = mgc_publish::auth::Auth {
            token: Some(session.token.clone()),
            ..Default::default()
        };
        trusted_session = Some(session);
        auth
    } else if args.trusted {
        mgc_publish::auth::Auth::default()
    } else {
        resolve_auth(&npmrc, &registry_url, config_reg, args.token.as_deref())?
    };

    // 11. Pre-flight whoami (npmjs only)
    if registry_url.contains("registry.npmjs.org") && !args.dry_run && !args.trusted {
        verify_token(&registry_url, &auth).await?;
    }

    // 12. Publish PUT
    if !args.dry_run {
        upload_tarball_client(
            &registry_url,
            &auth,
            &publish_name,
            &new_version,
            &tarball_path,
            trusted_session.as_mut(),
        )
        .await?;
        publish_put(
            &registry_url,
            &auth,
            &publish_name,
            &new_version,
            &sanitized.manifest,
            &dep_fields_map,
            &pack_result,
            args,
            trusted_session.as_mut(),
        )
        .await?;
    }

    // 13. Dist-tags
    if !args.dry_run && !args.trusted && args.tag.as_deref() != Some("latest") {
        set_dist_tag(
            &registry_url,
            &auth,
            &publish_name,
            &new_version,
            args.tag.as_deref().unwrap_or("latest"),
        )
        .await?;
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
        println!(
            "  size: {} bytes (unpacked: {})",
            summary.size, summary.unpacked_size
        );
        println!("  shasum: {}", summary.shasum);
        println!("  integrity: {}", summary.integrity);
        println!(
            "  files: {}, entries: {}",
            summary.files, summary.entry_count
        );
    }

    // Save updated version to mgc.toml + package.json under the writer
    // lock (static-gate finding): a concurrent mutation must not
    // interleave with the release version commit.
    // (Lock writer quanh commit version release.)
    let _guard = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        project_root,
        crate::commands::core::shared::writer_lock_timeout(project_root),
    )
    .map_err(|e| anyhow::anyhow!("publish cannot acquire the project writer lock: {e}"))?;
    project.save(project_root)?;
    if pkg_json_path.exists() {
        fs::write(&pkg_json_path, serde_json::to_string_pretty(&pkg_json)?)?;
    }

    Ok(())
}

#[derive(Clone)]
struct TrustedPublishSession {
    token: String,
    expires_at: tokio::time::Instant,
    registry: String,
    package: String,
    protocol: String,
    audience: Option<String>,
}

impl TrustedPublishSession {
    fn token(&self) -> &str {
        &self.token
    }

    fn remaining(&self) -> Duration {
        self.expires_at
            .saturating_duration_since(tokio::time::Instant::now())
    }

    async fn refresh_if_needed(&mut self) -> Result<()> {
        self.refresh_for(Duration::from_secs(TRUSTED_TOKEN_REFRESH_MARGIN_SECS))
            .await
    }

    async fn refresh_for(&mut self, minimum_lifetime: Duration) -> Result<()> {
        let now = tokio::time::Instant::now();
        if !trusted_token_expires_within(now, self.expires_at, minimum_lifetime) {
            return Ok(());
        }
        let refreshed = trusted_publish_session(
            &self.registry,
            &self.package,
            &self.protocol,
            self.audience.as_deref(),
        )
        .await
        .map_err(|error| {
            anyhow!("could not refresh the trusted token before expiry; verify that this CI environment can mint a fresh OIDC token: {error}")
        })?;
        if trusted_token_expires_within(
            tokio::time::Instant::now(),
            refreshed.expires_at,
            minimum_lifetime,
        ) {
            bail!(
                "refreshed trusted token lifetime is shorter than the next upload request timeout"
            );
        }
        *self = refreshed;
        Ok(())
    }
}

fn trusted_token_expires_within(
    now: tokio::time::Instant,
    expires_at: tokio::time::Instant,
    minimum_lifetime: Duration,
) -> bool {
    expires_at <= now + minimum_lifetime
}

fn trusted_token_deadline(
    exchange_started: tokio::time::Instant,
    expires_in: Duration,
) -> tokio::time::Instant {
    exchange_started + expires_in
}

fn docker_push_timeout(remaining: Duration) -> Result<Duration> {
    let usable = remaining
        .checked_sub(Duration::from_secs(TRUSTED_TOKEN_EXPIRY_SKEW_SECS))
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| anyhow!("trusted publish token expires too soon to start a Docker push"))?;
    Ok(usable.min(Duration::from_secs(TRUSTED_DOCKER_MAX_PUSH_SECS)))
}

fn oci_request_timeout(remaining: Duration) -> Result<Duration> {
    let usable = remaining
        .checked_sub(Duration::from_secs(TRUSTED_TOKEN_EXPIRY_SKEW_SECS))
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| anyhow!("trusted publish token expires too soon for an OCI request"))?;
    Ok(usable.min(Duration::from_secs(TRUSTED_OCI_REQUEST_MAX_TIMEOUT_SECS)))
}

fn trusted_npm_request_timeout(remaining: Duration) -> Result<Duration> {
    let usable = remaining
        .checked_sub(Duration::from_secs(TRUSTED_TOKEN_EXPIRY_SKEW_SECS))
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| anyhow!("trusted publish token expires too soon for an npm request"))?;
    Ok(usable.min(Duration::from_secs(TRUSTED_NPM_REQUEST_TIMEOUT_SECS)))
}

/// Refreshes trusted npm credentials and bounds one registry request by token expiry.
/// Làm mới token npm trusted và giới hạn thời gian request theo hạn token.
async fn apply_npm_request_auth(
    request: reqwest::RequestBuilder,
    auth: &mgc_publish::auth::Auth,
    trusted_session: Option<&mut TrustedPublishSession>,
) -> Result<reqwest::RequestBuilder> {
    if let Some(session) = trusted_session {
        session
            .refresh_for(Duration::from_secs(
                TRUSTED_NPM_REQUEST_TIMEOUT_SECS + TRUSTED_TOKEN_EXPIRY_SKEW_SECS,
            ))
            .await?;
        let timeout = trusted_npm_request_timeout(session.remaining())?;
        Ok(request.timeout(timeout).bearer_auth(session.token()))
    } else if let Some(header) = auth.header_value() {
        Ok(request.header("Authorization", header))
    } else {
        Ok(request)
    }
}

async fn trusted_publish_session(
    registry: &str,
    package: &str,
    protocol: &str,
    explicit_audience: Option<&str>,
) -> Result<TrustedPublishSession> {
    let registry_url =
        url::Url::parse(registry).map_err(|_| crate::error::trusted_registry_url_invalid())?;
    if !registry_url.username().is_empty()
        || registry_url.password().is_some()
        || registry_url.fragment().is_some()
        || registry_url.query().is_some()
    {
        return Err(crate::error::trusted_registry_url_invalid());
    }
    let local_http = registry_url.scheme() == "http"
        && registry_url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if registry_url.scheme() != "https" && !local_http {
        return Err(crate::error::trusted_registry_requires_https());
    }

    let audience = explicit_audience
        .map(str::to_owned)
        .or_else(|| std::env::var("MGC_OIDC_AUDIENCE").ok())
        .unwrap_or_else(|| registry_url.origin().ascii_serialization());
    if audience.trim().is_empty() {
        return Err(crate::error::trusted_audience_required());
    }
    let oidc_token = mgc_oidc::fetch::fetch_token(&audience).await?;
    let sigstore_oidc_token = mgc_oidc::fetch::fetch_sigstore_token().await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    // Start conservatively before the request; server issuance and response transit consume TTL.
    // Bắt đầu tính trước khi gửi request; thời gian server cấp token và truyền response đều dùng TTL.
    let exchange_started = tokio::time::Instant::now();
    let response = client
        .post(format!(
            "{}/-/v1/trusted-publish/token",
            registry.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "protocol": protocol,
            "package": package,
            "oidc_token": oidc_token,
            "sigstore_oidc_token": sigstore_oidc_token,
        }))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(crate::error::trusted_token_exchange_failed(
            response.status().as_u16(),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TRUSTED_EXCHANGE_RESPONSE_BYTES as u64)
    {
        return Err(crate::error::trusted_token_response_invalid());
    }
    use futures_util::StreamExt as _;
    let mut stream = response.bytes_stream();
    let mut response_body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| crate::error::trusted_token_response_invalid())?;
        if chunk.len() > MAX_TRUSTED_EXCHANGE_RESPONSE_BYTES.saturating_sub(response_body.len()) {
            return Err(crate::error::trusted_token_response_invalid());
        }
        response_body.extend_from_slice(&chunk);
    }
    let exchanged: serde_json::Value = serde_json::from_slice(&response_body)?;
    let token = exchanged
        .get("token")
        .and_then(serde_json::Value::as_str)
        .filter(|token| !token.is_empty() && !token.bytes().any(|byte| byte.is_ascii_control()))
        .ok_or_else(crate::error::trusted_token_response_invalid)?;
    let expires_in = exchanged
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .filter(|seconds| (1..=900).contains(seconds))
        .ok_or_else(crate::error::trusted_token_response_invalid)?;
    if exchanged.get("token_type").and_then(|value| value.as_str()) != Some("Bearer") {
        return Err(crate::error::trusted_token_response_invalid());
    }
    Ok(TrustedPublishSession {
        token: token.to_owned(),
        expires_at: trusted_token_deadline(exchange_started, Duration::from_secs(expires_in)),
        registry: registry.to_owned(),
        package: package.to_owned(),
        protocol: protocol.to_owned(),
        audience: explicit_audience.map(str::to_owned),
    })
}

async fn publish_trusted_non_npm(args: &PublishArgs, core: Option<&str>) -> Result<()> {
    if !args.trusted {
        bail!("PyPI and OCI publishing currently require --trusted");
    }
    let registry = args
        .registry
        .as_deref()
        .ok_or_else(|| anyhow!("--registry is required for trusted PyPI and OCI publishing"))?;
    let package = args
        .package
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("--package is required for trusted PyPI and OCI publishing"))?;
    let version = args
        .version
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("--version is required for trusted PyPI and OCI publishing"))?;

    let effective_package = match args.protocol.as_str() {
        "pypi" if !args.artifacts.is_empty() && args.image.is_none() => package.to_owned(),
        "oci" if !args.artifacts.is_empty() && args.image.is_none() && core.is_some() => {
            effective_trusted_oci_repository(core, package)?
        }
        "oci" if args.artifacts.is_empty() && args.image.is_some() => {
            if !valid_oci_tag(version) || args.image.as_deref().is_some_and(str::is_empty) {
                bail!("OCI publish requires a valid --version tag and --image reference");
            }
            effective_trusted_oci_repository(core, package)?
        }
        "pypi" => bail!("PyPI publish requires one or more --artifact values and no --image"),
        "oci" if !args.artifacts.is_empty() && args.image.is_none() => {
            bail!("OCI artifact publish requires --core <core>")
        }
        "oci" => bail!("OCI publish requires --image or --core with one or more --artifact values"),
        _ => bail!("unsupported trusted publish protocol: {}", args.protocol),
    };

    if args.dry_run {
        for artifact in &args.artifacts {
            if !fs::metadata(artifact)?.is_file() {
                bail!("artifact is not a regular file: {}", artifact.display());
            }
        }
        let output = serde_json::json!({
            "protocol": args.protocol,
            "package": effective_package,
            "core": core,
            "version": version,
            "dry_run": true,
        });
        if args.json {
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!(
                "Validated trusted {} publish for {}@{} (dry run).",
                args.protocol, effective_package, version
            );
        }
        return Ok(());
    }

    let mut auth = trusted_publish_session(
        registry,
        &effective_package,
        &args.protocol,
        args.trusted_audience.as_deref(),
    )
    .await?;
    match args.protocol.as_str() {
        "pypi" => {
            auth.refresh_if_needed().await?;
            publish_trusted_pypi(
                registry,
                &effective_package,
                version,
                &args.artifacts,
                &mut auth,
            )
            .await
        }
        "oci" => {
            if let Some(image) = args.image.as_deref() {
                publish_trusted_oci(registry, &effective_package, version, image, &mut auth).await
            } else {
                publish_trusted_oci_artifacts(
                    registry,
                    &effective_package,
                    core.ok_or_else(|| anyhow!("OCI artifact publish requires --core <core>"))?,
                    version,
                    &args.artifacts,
                    &mut auth,
                )
                .await
            }
        }
        _ => bail!("unsupported trusted publish protocol: {}", args.protocol),
    }
}

/// Resolve and validate the OCI repository, preserving legacy unscoped image names.
/// Chuẩn hóa và xác thực OCI repository, giữ nguyên tên image legacy không có core.
fn effective_trusted_oci_repository(core: Option<&str>, package: &str) -> Result<String> {
    let repository = match core {
        Some(core) => mgc_registry_server::trusted::core_scoped_oci_repository(core, package)
            .map_err(|_| anyhow!("invalid core-scoped OCI repository"))?,
        None => package.to_owned(),
    };
    mgc_registry_server::trusted::scoped_package("oci", &repository)
        .map_err(|_| anyhow!("invalid OCI repository"))?;
    Ok(repository)
}

/// Build an OCI 1.1 artifact manifest whose descriptors bind the files to one core.
/// Tạo manifest OCI 1.1 gắn descriptor của các file với đúng một core.
fn trusted_oci_artifact_manifest(
    core: &str,
    repository: &str,
    version: &str,
    layers: &[(String, String, u64)],
) -> Result<Vec<u8>> {
    use sha2::Digest as _;

    let core_id = mgc_types::Ecosystem::from_str(core)
        .ok_or_else(|| anyhow!("unknown MagiCore core: {core}"))?
        .as_str();
    let prefix = format!("{core_id}/");
    let package = repository
        .strip_prefix(&prefix)
        .filter(|package| !package.is_empty())
        .ok_or_else(|| anyhow!("OCI repository does not match the selected core"))?;
    let expected_repository =
        mgc_registry_server::trusted::core_scoped_oci_repository(core_id, package)
            .map_err(|_| anyhow!("invalid core-scoped OCI repository"))?;
    if expected_repository != repository || !valid_oci_tag(version) || layers.is_empty() {
        bail!("invalid OCI artifact manifest metadata");
    }

    let layer_descriptors = layers
        .iter()
        .map(|(filename, digest, size)| {
            let Some(hash) = digest.strip_prefix("sha256:") else {
                bail!("OCI artifact layer has an invalid SHA-256 digest");
            };
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || filename.is_empty()
                || filename.len() > 255
                || filename
                    .bytes()
                    .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\'))
            {
                bail!("OCI artifact layer metadata is invalid");
            }
            Ok(serde_json::json!({
                "mediaType": "application/vnd.magicore.artifact.layer.v1",
                "digest": digest,
                "size": size,
                "annotations": {
                    "org.opencontainers.image.title": filename,
                },
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let config_digest = format!("sha256:{}", hex::encode(sha2::Sha256::digest(b"{}")));
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "artifactType": "application/vnd.magicore.artifact.v1",
        "config": {
            "mediaType": "application/vnd.oci.empty.v1+json",
            "digest": config_digest,
            "size": 2,
        },
        "layers": layer_descriptors,
        "annotations": {
            "io.magicore.core": core_id,
            "io.magicore.package": repository,
            "io.magicore.version": version,
        },
    });
    Ok(serde_json::to_vec(&manifest)?)
}

async fn begin_oci_blob_upload(
    client: &reqwest::Client,
    registry: &url::Url,
    repository: &str,
    token: &str,
    timeout: Duration,
) -> Result<url::Url> {
    let start_url = registry.join(&format!("/v2/{repository}/blobs/uploads/"))?;
    let response = client
        .post(start_url)
        .bearer_auth(token)
        .timeout(timeout)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::ACCEPTED {
        return Err(crate::error::trusted_oci_upload_start_failed(
            response.status().as_u16(),
        ));
    }
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(crate::error::trusted_oci_upload_location_missing)?;
    safe_oci_upload_location(registry, repository, location)
}

/// Accept upload URLs only from the configured registry and repository.
/// Chỉ chấp nhận URL upload cùng registry và đúng repository đã cấu hình.
fn safe_oci_upload_location(
    registry: &url::Url,
    repository: &str,
    location: &str,
) -> Result<url::Url> {
    let upload_url = registry.join(location)?;
    let expected_prefix = format!("/v2/{repository}/blobs/uploads/");
    let session = upload_url
        .path()
        .strip_prefix(&expected_prefix)
        .filter(|session| {
            !session.is_empty()
                && session.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
                })
                && !matches!(*session, "." | "..")
        });
    if upload_url.origin() != registry.origin()
        || !upload_url.username().is_empty()
        || upload_url.password().is_some()
        || upload_url.fragment().is_some()
        || session.is_none()
    {
        bail!("OCI registry returned an unsafe upload Location");
    }
    Ok(upload_url)
}

/// Bearer token and deadline that must apply to one OCI upload request.
/// Token Bearer và thời hạn bắt buộc áp dụng cho một request tải OCI.
struct OciRequestAuth<'a> {
    token: &'a str,
    timeout: Duration,
}

async fn upload_oci_chunk(
    client: &reqwest::Client,
    registry: &url::Url,
    repository: &str,
    upload_url: &mut url::Url,
    offset: u64,
    chunk: &[u8],
    request_auth: OciRequestAuth<'_>,
) -> Result<()> {
    if chunk.is_empty() {
        return Ok(());
    }
    let end = offset
        .checked_add(chunk.len() as u64)
        .and_then(|next| next.checked_sub(1))
        .ok_or_else(|| anyhow!("OCI blob size overflow"))?;
    let response = client
        .patch(upload_url.clone())
        .bearer_auth(request_auth.token)
        .timeout(request_auth.timeout)
        .header("Content-Range", format!("{offset}-{end}"))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(chunk.to_vec())
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::ACCEPTED {
        return Err(crate::error::trusted_oci_upload_chunk_failed(
            response.status().as_u16(),
        ));
    }
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(crate::error::trusted_oci_upload_location_missing)?;
    *upload_url = safe_oci_upload_location(registry, repository, location)?;
    let expected_range = format!("0-{end}");
    if response
        .headers()
        .get(reqwest::header::RANGE)
        .and_then(|value| value.to_str().ok())
        != Some(expected_range.as_str())
    {
        return Err(crate::error::trusted_oci_upload_range_invalid());
    }
    Ok(())
}

async fn finish_oci_blob_upload(
    client: &reqwest::Client,
    upload_url: &url::Url,
    digest: &str,
    token: &str,
    timeout: Duration,
) -> Result<()> {
    let mut finish_url = upload_url.clone();
    finish_url.query_pairs_mut().append_pair("digest", digest);
    let response = client
        .put(finish_url)
        .bearer_auth(token)
        .timeout(timeout)
        .body(Vec::new())
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::CREATED {
        return Err(crate::error::trusted_oci_upload_complete_failed(
            response.status().as_u16(),
        ));
    }
    Ok(())
}

async fn upload_oci_blob_bytes(
    client: &reqwest::Client,
    registry: &url::Url,
    repository: &str,
    bytes: &[u8],
    session: &mut TrustedPublishSession,
) -> Result<(String, u64)> {
    use sha2::Digest as _;

    session.refresh_if_needed().await?;
    let mut upload_url = begin_oci_blob_upload(
        client,
        registry,
        repository,
        session.token(),
        oci_request_timeout(session.remaining())?,
    )
    .await?;
    for (index, chunk) in bytes.chunks(OCI_UPLOAD_CHUNK_BYTES).enumerate() {
        let offset = (index * OCI_UPLOAD_CHUNK_BYTES) as u64;
        session.refresh_if_needed().await?;
        upload_oci_chunk(
            client,
            registry,
            repository,
            &mut upload_url,
            offset,
            chunk,
            OciRequestAuth {
                token: session.token(),
                timeout: oci_request_timeout(session.remaining())?,
            },
        )
        .await?;
    }
    let digest = format!("sha256:{}", hex::encode(sha2::Sha256::digest(bytes)));
    session.refresh_if_needed().await?;
    finish_oci_blob_upload(
        client,
        &upload_url,
        &digest,
        session.token(),
        oci_request_timeout(session.remaining())?,
    )
    .await?;
    Ok((digest, bytes.len() as u64))
}

async fn upload_oci_blob_file(
    client: &reqwest::Client,
    registry: &url::Url,
    repository: &str,
    path: &Path,
    session: &mut TrustedPublishSession,
) -> Result<(String, u64)> {
    use sha2::Digest as _;
    use tokio::io::AsyncReadExt as _;

    let mut file = tokio::fs::File::open(path).await?;
    if !file.metadata().await?.is_file() {
        bail!("OCI artifact is not a regular file: {}", path.display());
    }
    session.refresh_if_needed().await?;
    let mut upload_url = begin_oci_blob_upload(
        client,
        registry,
        repository,
        session.token(),
        oci_request_timeout(session.remaining())?,
    )
    .await?;
    let mut hasher = sha2::Sha256::new();
    let mut offset = 0u64;
    let mut buffer = vec![0; OCI_UPLOAD_CHUNK_BYTES];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
        session.refresh_if_needed().await?;
        upload_oci_chunk(
            client,
            registry,
            repository,
            &mut upload_url,
            offset,
            chunk,
            OciRequestAuth {
                token: session.token(),
                timeout: oci_request_timeout(session.remaining())?,
            },
        )
        .await?;
        hasher.update(chunk);
        offset = offset
            .checked_add(count as u64)
            .ok_or_else(|| anyhow!("OCI artifact size overflow"))?;
    }
    let digest = format!("sha256:{}", hex::encode(hasher.finalize()));
    session.refresh_if_needed().await?;
    finish_oci_blob_upload(
        client,
        &upload_url,
        &digest,
        session.token(),
        oci_request_timeout(session.remaining())?,
    )
    .await?;
    Ok((digest, offset))
}

async fn publish_trusted_oci_artifacts(
    registry: &str,
    repository: &str,
    core: &str,
    version: &str,
    artifacts: &[PathBuf],
    session: &mut TrustedPublishSession,
) -> Result<()> {
    use sha2::Digest as _;

    let registry_url = trusted_oci_registry_root(registry)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(TRUSTED_OCI_REQUEST_MAX_TIMEOUT_SECS))
        .build()?;

    for artifact in artifacts {
        if !fs::metadata(artifact)?.is_file() {
            bail!("OCI artifact is not a regular file: {}", artifact.display());
        }
        oci_artifact_filename(artifact)?;
    }

    let (config_digest, config_size) =
        upload_oci_blob_bytes(&client, &registry_url, repository, b"{}", session).await?;
    if config_digest != format!("sha256:{}", hex::encode(sha2::Sha256::digest(b"{}")))
        || config_size != 2
    {
        bail!("OCI empty config digest invariant failed");
    }

    let mut layers = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let filename = oci_artifact_filename(artifact)?.to_owned();
        let (digest, size) =
            upload_oci_blob_file(&client, &registry_url, repository, artifact, session).await?;
        layers.push((filename, digest, size));
    }

    let manifest = trusted_oci_artifact_manifest(core, repository, version, &layers)?;
    let manifest_url = registry_url.join(&format!("/v2/{repository}/manifests/{version}"))?;
    session.refresh_if_needed().await?;
    let response = client
        .put(manifest_url)
        .bearer_auth(session.token())
        .timeout(oci_request_timeout(session.remaining())?)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/vnd.oci.image.manifest.v1+json",
        )
        .body(manifest)
        .send()
        .await?;
    if !response.status().is_success() {
        bail!(
            "OCI artifact manifest upload failed (HTTP {})",
            response.status()
        );
    }
    println!(
        "Published {} OCI artifact layer(s) for {repository}:{version} (core {core}) with trusted provenance.",
        artifacts.len()
    );
    Ok(())
}

fn trusted_oci_registry_root(registry: &str) -> Result<url::Url> {
    let registry_url =
        url::Url::parse(registry).map_err(|_| crate::error::trusted_registry_url_invalid())?;
    let local_http = registry_url.scheme() == "http"
        && registry_url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if registry_url.scheme() != "https" && !local_http {
        return Err(crate::error::trusted_registry_requires_https());
    }
    if !registry_url.path().trim_matches('/').is_empty()
        || registry_url.query().is_some()
        || registry_url.fragment().is_some()
        || !registry_url.username().is_empty()
        || registry_url.password().is_some()
    {
        return Err(crate::error::trusted_registry_url_invalid());
    }
    Ok(registry_url)
}

fn oci_artifact_filename(path: &Path) -> Result<&str> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("OCI artifact must have a UTF-8 filename"))?;
    if filename.len() > 255
        || filename
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\'))
    {
        bail!("OCI artifact filename is invalid");
    }
    Ok(filename)
}

async fn publish_trusted_pypi(
    registry: &str,
    package: &str,
    version: &str,
    artifacts: &[PathBuf],
    session: &mut TrustedPublishSession,
) -> Result<()> {
    use sha2::Digest as _;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(TRUSTED_PYPI_REQUEST_TIMEOUT_SECS))
        .build()?;
    for artifact in artifacts {
        session
            .refresh_for(Duration::from_secs(
                TRUSTED_PYPI_REQUEST_TIMEOUT_SECS + TRUSTED_TOKEN_EXPIRY_SKEW_SECS,
            ))
            .await?;
        let filename = artifact
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow!("PyPI artifact must have a UTF-8 filename"))?;
        let content = fs::read(artifact)?;
        let digest = hex::encode(sha2::Sha256::digest(&content));
        let form = reqwest::multipart::Form::new()
            .text(":action", "file_upload")
            .text("name", package.to_owned())
            .text("version", version.to_owned())
            .text("sha256_digest", digest)
            .part(
                "content",
                reqwest::multipart::Part::bytes(content).file_name(filename.to_owned()),
            );
        let response = client
            .post(format!("{}/pypi/legacy/", registry.trim_end_matches('/')))
            .bearer_auth(session.token())
            .multipart(form)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(crate::error::trusted_token_exchange_failed(
                response.status().as_u16(),
            ));
        }
    }
    println!(
        "Published {} PyPI artifact(s) for {}=={} with trusted provenance.",
        artifacts.len(),
        package,
        version
    );
    Ok(())
}

async fn publish_trusted_oci(
    registry: &str,
    package: &str,
    tag: &str,
    image: &str,
    session: &mut TrustedPublishSession,
) -> Result<()> {
    let registry_url =
        url::Url::parse(registry).map_err(|_| crate::error::trusted_registry_url_invalid())?;
    if !registry_url.path().trim_matches('/').is_empty()
        || registry_url.query().is_some()
        || registry_url.fragment().is_some()
        || !registry_url.username().is_empty()
        || registry_url.password().is_some()
    {
        return Err(crate::error::trusted_registry_url_invalid());
    }
    let host = registry_url
        .host_str()
        .ok_or_else(crate::error::trusted_registry_url_invalid)?;
    let authority = registry_url
        .port()
        .map(|port| format!("{host}:{port}"))
        .unwrap_or_else(|| host.to_owned());
    let destination = format!("{authority}/{package}:{tag}");
    let docker_config = tempfile::tempdir()?;

    session.refresh_if_needed().await?;
    let login_token = session.token().to_owned();
    docker_command(
        &[
            "login".into(),
            "--username".into(),
            "mgc-oidc".into(),
            "--password-stdin".into(),
            authority.clone(),
        ],
        docker_config.path(),
        Some(session.token()),
        Duration::from_secs(30),
        "Docker login exceeded 30 seconds",
    )
    .await?;
    docker_command(
        &[
            "image".into(),
            "tag".into(),
            image.to_owned(),
            destination.clone(),
        ],
        docker_config.path(),
        None,
        Duration::from_secs(30),
        "Docker image tag operation exceeded 30 seconds",
    )
    .await?;
    session.refresh_if_needed().await?;
    if session.token() != login_token {
        docker_command(
            &[
                "login".into(),
                "--username".into(),
                "mgc-oidc".into(),
                "--password-stdin".into(),
                authority,
            ],
            docker_config.path(),
            Some(session.token()),
            Duration::from_secs(30),
            "Docker login exceeded 30 seconds",
        )
        .await?;
    }
    let pushed = docker_command(
        &["push".into(), destination.clone()],
        docker_config.path(),
        None,
        docker_push_timeout(session.remaining())?,
        "Docker push reached the trusted token expiry safety deadline",
    )
    .await;
    let _ = docker_command(
        &["image".into(), "rm".into(), "--force".into(), destination],
        docker_config.path(),
        None,
        Duration::from_secs(10),
        "Docker cleanup exceeded 10 seconds",
    )
    .await;
    pushed?;
    println!("Pushed OCI image {package}:{tag} with trusted provenance.");
    Ok(())
}

async fn docker_command(
    args: &[String],
    docker_config: &Path,
    secret_stdin: Option<&str>,
    timeout: Duration,
    timeout_message: &'static str,
) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt as _;

    let mut command = tokio::process::Command::new("docker");
    command
        .args(args)
        .env("DOCKER_CONFIG", docker_config)
        .stdin(if secret_stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| anyhow!("could not start Docker"))?;
    if let Some(secret) = secret_stdin {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Docker login input is unavailable"))?;
        stdin.write_all(secret.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
    }
    let status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| anyhow!(timeout_message))??;
    if !status.success() {
        bail!("Docker operation failed with status {status}");
    }
    Ok(())
}

fn valid_oci_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        && tag.as_bytes()[0].is_ascii_alphanumeric()
}

/// Closed set of read-only Git queries used by publish integrity checks.
/// Tập đóng các truy vấn Git chỉ đọc dùng cho kiểm tra tính toàn vẹn publish.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GitReadQuery {
    ValidateBranch(String),
    WorkingTreeStatus,
    CurrentBranch,
    UpstreamRef(String),
    CommitsBehind(String),
}

impl GitReadQuery {
    fn argv(&self) -> Vec<String> {
        match self {
            Self::ValidateBranch(branch) => {
                vec!["check-ref-format".into(), format!("refs/heads/{branch}")]
            }
            Self::WorkingTreeStatus => vec!["status".into(), "--porcelain".into()],
            Self::CurrentBranch => vec![
                "rev-parse".into(),
                "--verify".into(),
                "--abbrev-ref".into(),
                "--end-of-options".into(),
                "HEAD".into(),
            ],
            Self::UpstreamRef(branch) => vec![
                "rev-parse".into(),
                "--verify".into(),
                "--abbrev-ref".into(),
                "--symbolic-full-name".into(),
                "--end-of-options".into(),
                format!("{branch}@{{upstream}}"),
            ],
            Self::CommitsBehind(branch) => vec![
                "rev-list".into(),
                "--count".into(),
                "--end-of-options".into(),
                format!("{branch}..{branch}@{{upstream}}"),
            ],
        }
    }
}

/// Check the selected project's tree and tracking branch before publishing.
/// Kiểm tra working tree và upstream của đúng project trước khi publish.
fn git_checks(publish_branch: Option<&str>, project_root: &Path) -> Result<()> {
    // Tree clean (except ignored)
    let status = run_git_capture(project_root, GitReadQuery::WorkingTreeStatus)?;
    if !status.trim().is_empty() {
        bail!("Git tree not clean — commit or stash changes first (or --no-git-checks)");
    }

    // Current branch
    let branch = match publish_branch {
        Some(branch) => {
            run_git_capture(
                project_root,
                GitReadQuery::ValidateBranch(branch.to_string()),
            )?;
            branch.to_string()
        }
        None => run_git_capture(project_root, GitReadQuery::CurrentBranch)?
            .trim()
            .to_string(),
    };
    if branch.is_empty() {
        bail!("Could not determine the Git branch to publish");
    }

    // Has upstream
    let upstream = run_git_capture(project_root, GitReadQuery::UpstreamRef(branch.clone()));
    if upstream.is_err() {
        if publish_branch.is_some() {
            bail!("Publish branch {branch} has no resolvable upstream");
        }
        return Ok(());
    }

    // Check lag between the selected local branch and its matching upstream.
    // So sánh nhánh cục bộ đã chọn với upstream tương ứng để phát hiện commit bị chậm.
    let lag = run_git_capture(project_root, GitReadQuery::CommitsBehind(branch.clone()))?;
    let count = parse_git_behind_count(&lag)?;
    if count > 0 {
        bail!(
            "Branch {} lags {} commits behind upstream — pull first",
            branch,
            count
        );
    }

    Ok(())
}

fn parse_git_behind_count(output: &str) -> Result<usize> {
    output
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow!("Git returned an invalid upstream commit count"))
}

fn git_exec_options(project_root: &Path) -> mgc_exec::prelude::ExecOptions {
    mgc_exec::prelude::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        clean_env: true,
        ..Default::default()
    }
}

fn run_git_capture(project_root: &Path, query: GitReadQuery) -> Result<String> {
    let args = query.argv();
    let opts = git_exec_options(project_root);
    let report = mgc_exec::prelude::run("git", &args, &opts)?;
    Ok(report.stdout_tail)
}

/// Select registry per 01 §2 priority
fn select_registry(
    pkg_json: &serde_json::Value,
    project_root: &Path,
    project: &ProjectConfig,
    args: &PublishArgs,
) -> Result<String> {
    // 1. --registry flag
    if let Some(r) = &args.registry {
        return Ok(r.clone());
    }

    // 2. publishConfig.registry in package.json
    if let Some(url) = pkg_json
        .get("publishConfig")
        .and_then(|p| p.get("registry"))
        .and_then(|v| v.as_str())
    {
        return Ok(url.to_string());
    }

    // 3. .npmrc registry / scope registry
    let npmrc = NpmRc::load(project_root)?;
    let scope = pkg_json
        .get("name")
        .and_then(|v| v.as_str())
        .and_then(|n| n.strip_prefix('@').map(|s| format!("@{}", s)));
    if let Some(url) = npmrc.registry_for(scope.as_deref()) {
        return Ok(url);
    }

    // 4. mgc.toml [registry]
    if let Some(reg) = project.registries.first() {
        return Ok(reg.url.clone());
    }

    // 5. Default
    Ok(DEFAULT_REGISTRY.to_string())
}

fn publish_lifecycle_decision(
    pkg_json: &serde_json::Value,
    ignore_scripts: bool,
    allow_scripts: bool,
) -> Result<bool> {
    if ignore_scripts {
        return Ok(false);
    }
    let Some(scripts_value) = pkg_json.get("scripts") else {
        return Ok(false);
    };
    let Some(scripts) = scripts_value.as_object() else {
        return Err(crate::error::publish_lifecycle_policy_invalid());
    };

    let mut has_publish_hook = false;
    for hook in PUBLISH_LIFECYCLE_HOOKS {
        if let Some(value) = scripts.get(*hook) {
            if !value.is_string() {
                return Err(crate::error::publish_lifecycle_policy_invalid());
            }
            has_publish_hook = true;
        }
    }
    if !has_publish_hook {
        return Ok(false);
    }
    if !allow_scripts {
        return Err(crate::error::publish_lifecycle_opt_in_required());
    }
    Ok(true)
}

fn run_lifecycle(pkg_json: &serde_json::Value, script: &str, project_root: &Path) -> Result<()> {
    if let Some(cmd) = pkg_json
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
    {
        println!("Running approved publish lifecycle hook: {script}");
        mgc_exec::allowlist::reject_forbidden_pm_script(cmd)?;
        let invocation = mgc_exec::allowlist::parse_script_invocation(cmd)?;
        let mut env = invocation.env;
        env.push(("INIT_CWD".to_string(), project_root.display().to_string()));
        if let Some(path) = std::env::var_os("PATH") {
            env.push(("PATH".to_string(), path.to_string_lossy().to_string()));
        }
        let opts = mgc_exec::prelude::ExecOptions {
            cwd: Some(project_root.to_path_buf()),
            timeout: Some(publish_lifecycle_timeout()),
            env,
            clean_env: true,
            execution_scope: Some(mgc_exec::allowlist::ExecutionScope::Install),
            ..Default::default()
        };
        mgc_exec::prelude::run_inherited(&invocation.program, &invocation.args, &opts)?;
    }
    Ok(())
}

fn publish_lifecycle_timeout() -> Duration {
    std::env::var(PUBLISH_LIFECYCLE_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_PUBLISH_LIFECYCLE_TIMEOUT_SECS))
}

/// Verify token with whoami endpoint (npmjs)
async fn verify_token(registry_url: &str, auth: &mgc_publish::auth::Auth) -> Result<()> {
    let client = reqwest::Client::new();
    let whoami_url = format!("{}/-/whoami", registry_url.trim_end_matches('/'));
    let req = client.get(&whoami_url);
    let req = if let Some(h) = auth.header_value() {
        req.header("Authorization", h)
    } else {
        req
    };
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        bail!("Token invalid (whoami failed: {})", status);
    }
    Ok(())
}

/// Publish PUT to registry.
/// Gửi payload publish lên registry bằng request đã xác thực.
#[allow(clippy::too_many_arguments)]
async fn publish_put(
    registry_url: &str,
    auth: &mgc_publish::auth::Auth,
    name: &str,
    version: &str,
    manifest: &serde_json::Value,
    deps: &serde_json::Map<String, serde_json::Value>,
    pack_result: &mgc_pack::tarball::PackResult,
    args: &PublishArgs,
    mut trusted_session: Option<&mut TrustedPublishSession>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}", registry_url.trim_end_matches('/'), name);

    // Build body
    let mut body = serde_json::Map::new();
    body.insert("_id".into(), serde_json::Value::String(name.into()));
    body.insert("name".into(), serde_json::Value::String(name.into()));
    let publish_tag = if args.trusted {
        args.tag.as_deref().unwrap_or("latest")
    } else {
        "latest"
    };
    body.insert(
        "dist-tags".into(),
        serde_json::json!({(publish_tag): version}),
    );
    body.insert("maintainers".into(), serde_json::json!([]));
    body.insert(
        "time".into(),
        serde_json::json!({"created": "", "modified": ""}),
    );
    body.insert("private".into(), serde_json::json!(false));
    let mut versions = serde_json::Map::new();
    let mut version_obj = manifest.clone();
    let unscoped = name.rsplit('/').next().unwrap_or(name);
    let version_object = version_obj
        .as_object_mut()
        .ok_or_else(crate::error::parse_manifest_failed)?;
    version_object.insert("dist".into(), serde_json::json!({
        "tarball": format!("{}/{}/-/{}-{}.tgz", registry_url.trim_end_matches('/'), name, unscoped, version),
        "shasum": pack_result.shasum,
        "integrity": pack_result.integrity,
        "fileCount": pack_result.entry_count,
        "unpackedSize": pack_result.unpacked_size,
    }));
    for (k, v) in deps {
        version_object.insert(k.clone(), v.clone());
    }

    // Access field for scoped packages
    if let Some(access) = &args.access {
        version_object.insert("access".into(), serde_json::Value::String(access.clone()));
    }

    // OTP
    if let Some(otp) = &args.otp {
        body.insert("otp".into(), serde_json::Value::String(otp.clone()));
    }

    versions.insert(version.into(), version_obj);
    body.insert("versions".into(), serde_json::Value::Object(versions));

    let req = client.put(&url).json(&body);
    let req = apply_npm_request_auth(req, auth, trusted_session.as_deref_mut()).await?;
    let resp = req.send().await?;
    let status = resp.status();

    if status.as_u16() == 409 {
        if args.force {
            // DELETE old version then retry
            let del_url = format!(
                "{}/-/{}-{}.tgz",
                registry_url.trim_end_matches('/'),
                name,
                version
            );
            let del_req = client.delete(&del_url);
            let del_req =
                apply_npm_request_auth(del_req, auth, trusted_session.as_deref_mut()).await?;
            del_req.send().await?;
            // Retry PUT - need to rebuild request
            let body_retry = client.put(&url).json(&body);
            let body_retry = apply_npm_request_auth(body_retry, auth, trusted_session).await?;
            let retry = body_retry.send().await?;
            let retry_status = retry.status();
            if !retry_status.is_success() {
                let txt = retry.text().await?;
                bail!("Publish failed after force delete: {}", txt);
            }
        } else {
            bail!(
                "Version {} already exists — use --force to overwrite",
                version
            );
        }
    } else if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Publish failed: {} - {}", status, txt);
    }

    Ok(())
}

async fn upload_tarball_client(
    registry_url: &str,
    auth: &mgc_publish::auth::Auth,
    name: &str,
    version: &str,
    tarball_path: &std::path::Path,
    trusted_session: Option<&mut TrustedPublishSession>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let unscoped = name.rsplit('/').next().unwrap_or(name);
    let url = format!(
        "{}/{}/-/{}-{}.tgz",
        registry_url.trim_end_matches('/'),
        name,
        unscoped,
        version
    );
    let data = fs::read(tarball_path)?;
    let req = client.put(&url).body(data);
    let req = apply_npm_request_auth(req, auth, trusted_session).await?;
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Tarball upload failed: {} - {}", status, txt);
    }
    Ok(())
}

/// Set dist-tag
async fn set_dist_tag(
    registry_url: &str,
    auth: &mgc_publish::auth::Auth,
    name: &str,
    version: &str,
    tag: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/-/package/{}/dist-tags/{}",
        registry_url.trim_end_matches('/'),
        name,
        tag
    );
    let body = serde_json::json!({ "version": version });
    let req = client.put(&url).json(&body);
    let req = if let Some(h) = auth.header_value() {
        req.header("Authorization", h)
    } else {
        req
    };
    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let txt = resp.text().await?;
        bail!("Set dist-tag failed: {} - {}", status, txt);
    }
    Ok(())
}

#[cfg(test)]
#[path = "test/publish.rs"]
mod tests;
