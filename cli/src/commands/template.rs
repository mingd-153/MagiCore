//! (Lệnh template: publish/fetch kernel-làm registry — Bun/pnpm create thành mg-create-*, Q13)
//! publish: pack templates/{core}/{name} thành tarball mg-create-<core>-<name>
//! fetch: tải tarball → ~/.mg/templates/{core}/{name} (cache; resolve sẽ thấy Disk)
use anyhow::{bail, Result};
use clap::Parser;
use mg_config::npmrc::NpmRc;
use mg_publish::auth::resolve_auth;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile;

const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";
/// Tên package trên registry: `mg-create-<core>-<name>` (Bun/pnpm naming, plan r3, T5).
/// Name chứa dấu / (layer rel như backend/go/gin) được dash-hoá;
/// nếu name đã bắt đầu "<core>/" thì core không lặp (publish-all truyền rel đầy đủ).
pub fn registry_package_name(core: &str, name: &str) -> String {
    let prefix = format!("{core}/");
    let inner = name.strip_prefix(&prefix).unwrap_or(name);
    format!("mg-create-{core}-{}", inner.replace('/', "-"))
}

/// Cache dir: ~/.mg/templates (override qua MG_TEMPLATES_DIR).
pub fn templates_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MG_TEMPLATES_DIR") {
        return PathBuf::from(dir);
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".mg").join("templates")
}

/// Rel path của layer trong templates/ — name flat (react-vite → web/frontend/react-vite)
/// hoặc full rel (backend/go/gin → giữ nguyên).
/// Cache cũng nằm theo rel này để TemplateRoot::resolve thấy.
pub fn layer_rel(core: &str, name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    if core == "web" {
        format!("web/frontend/{name}")
    } else {
        format!("{core}/{name}")
    }
}

/// Đường dẫn cache của layer — flat (v1: chỉ giữ latest; version pin là P2).
pub fn cached_layer_path(core: &str, name: &str) -> PathBuf {
    templates_cache_dir().join(layer_rel(core, name))
}

/// `mg template` subcommands
#[derive(Debug, Parser, Clone)]
pub enum TemplateCmd {
    /// Fetch template layer từ registry về ~/.mg/templates (create sẽ tự thấy)
    #[command(about = "Fetch template layer from registry into cache ~/.mg/templates")]
    Fetch(TemplateFetchArgs),
    /// Publish kernel template thành package mg-create-<core>-<name>
    #[command(about = "Publish kernel template layer to registry")]
    Publish(TemplatePublishArgs),
    /// Publish mọi layer web có template.toml+sources (registry-first source of truth)
    #[command(about = "Publish every template layer under templates/web to registry")]
    PublishAll,
}

/// `mg template fetch <core> <name> [--registry URL] [--tag latest]`
#[derive(Debug, Parser, Clone)]
pub struct TemplateFetchArgs {
    pub core: String,
    pub name: String,
    #[arg(long, help = "override registry URL")]
    pub registry: Option<String>,
    #[arg(long, help = "dist-tag (default: latest)")]
    pub tag: Option<String>,
}

/// `mg template publish <core> <name> [--registry URL] [--version]`
#[derive(Debug, Parser, Clone)]
pub struct TemplatePublishArgs {
    pub core: String,
    pub name: String,
    #[arg(long, help = "override registry URL")]
    pub registry: Option<String>,
    #[arg(long, help = "package version (default: 0.1.0)")]
    pub version: Option<String>,
}

/// Dispatch mg template subcommands.
pub async fn run(cmd: TemplateCmd) -> Result<()> {
    match cmd {
        TemplateCmd::Fetch(args) => {
            fetch(args).await?;
        }
        TemplateCmd::Publish(args) => {
            publish(args).await?;
        }
        TemplateCmd::PublishAll => {
            publish_all().await?;
        }
    }
    Ok(())
}

/// Publish toàn bộ layer web có đủ template.toml+sources.
/// Duyệt templates/web/** — mỗi folder có template.toml + sources thành 1 package.
async fn publish_all() -> Result<()> {
    let root = crate::scaffold::template_root::workspace_root()
        .join("templates")
        .join("web");
    if !root.is_dir() {
        return Err(crate::error::web_templates_missing(&root));
    }
    let mut layers: Vec<PathBuf> = Vec::new();
    collect_web_layers(&root, &mut layers);
    if layers.is_empty() {
        return Err(crate::error::no_web_layer_found(&root));
    }
    for layer in &layers {
        let rel = layer
            .strip_prefix(root.parent().unwrap())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        // rel = "web/frontend/react-vite" → publish với name = rel đầy đủ
        let name = rel.to_string();
        let pkg = registry_package_name("web", &name);
        println!("  publishing {name} (as {pkg})");
        publish(TemplatePublishArgs {
            core: "web".to_string(),
            name,
            registry: None,
            version: None,
        })
        .await?;
    }
    println!("Publish-all done: {} layer web", layers.len());
    Ok(())
}

/// Thu thập mọi folder có template.toml + sources dưới root (không đệ quy vào leaf đã chứa).
fn collect_web_layers(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("template.toml").is_file() && path.join("sources").is_dir() {
                out.push(path);
            } else {
                collect_web_layers(&path, out);
            }
        }
    }
}

/// Publish kernel template layer `templates/{core}/{name}` thành package
/// `mg-create-<core>-<name>` trên registry (mg-pack tarball + npm PUT).
async fn publish(args: TemplatePublishArgs) -> Result<()> {
    let rel = layer_rel(&args.core, &args.name);
    let layer = crate::scaffold::template_root::workspace_root()
        .join("templates")
        .join(&rel);
    if !layer.join("template.toml").is_file() || !layer.join("sources").is_dir() {
        bail!(
            "Template layer '{}' missing template.toml/sources",
            layer.display()
        );
    }

    let version = args.version.unwrap_or_else(|| "0.1.0".to_string());
    let name = registry_package_name(&args.core, &args.name);
    let registry = select_registry(args.registry.as_deref())?;

    let temp_dir = tempfile::tempdir()?;
    let tarball_path = temp_dir
        .path()
        .join(format!("{}.tgz", name.replace('/', "-")));
    let pack_result = mg_pack::tarball::pack(&layer, &tarball_path, &format!("{name}-{version}"))?;

    let npmrc = NpmRc::load(Path::new("."))?;
    let auth = resolve_auth(&npmrc, &registry, None, None)?;
    if auth.is_empty() {
        let host = url::Url::parse(&registry)
            .map(|u| u.host_str().unwrap_or_default().to_string())
            .unwrap_or_default();
        if host.contains("npmjs") || host.contains("npm") {
            bail!("Registry '{}' requires auth — set MG_NPM_TOKEN", registry);
        }
    }

    upload_tarball(&registry, &auth, &name, &version, &tarball_path).await?;

    let manifest = serde_json::json!({
        "name": name,
        "version": version,
        "description": format!("MegaGate kernel template {}/{}", args.core, args.name),
        "main": "template.toml",
    });
    put_manifest(&registry, &auth, &name, &version, &manifest, &pack_result).await?;
    println!(
        "Template '{}' published → {}/{}/{}",
        name,
        registry.trim_end_matches('/'),
        name,
        version
    );
    Ok(())
}

/// Fetch template layer về cache registry.
pub async fn fetch(args: TemplateFetchArgs) -> Result<PathBuf> {
    let registry = select_registry(args.registry.as_deref())?;
    let name = registry_package_name(&args.core, &args.name);
    let tag = args.tag.unwrap_or_else(|| "latest".to_string());

    let client = reqwest::Client::new();
    let auto = register_fetch_auth(&registry).await;
    // GET full manifest: registry server chỉ route GET /:name (không có /:name/:tag).
    let mut req = client.get(format!("{}/{}", registry.trim_end_matches('/'), name));
    if let Some(h) = auto.header_value() {
        req = req.header("Authorization", h);
    }
    let meta_resp = req.send().await?;
    if !meta_resp.status().is_success() {
        return Err(crate::error::template_not_published(&name, meta_resp.status().as_u16()));
    }
    let meta: serde_json::Value = meta_resp.json().await?;

    // dist-tags[tag] → version → versions[version].dist.tarball
    let version = meta
        .pointer(&format!("/dist-tags/{tag}"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::registry_missing_field(&format!("dist-tags/{tag}")))?;
    let tarball_url = meta
        .pointer(&format!("/versions/{version}/dist/tarball"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::registry_missing_field("dist.tarball"))?;

    let bytes = {
        let mut treq = client.get(tarball_url);
        if let Some(h) = auto.header_value() {
            treq = treq.header("Authorization", h);
        }
        treq.send().await?.bytes().await?
    };

    let target = cached_layer_path(&args.core, &args.name);
    fs::create_dir_all(&target)?;
    extract_tarball_into(&bytes, &target)?;

    println!("Template '{}' fetched → {}", name, target.display());
    Ok(target)
}

/// Đảm bảo template layer có sẵn (disk / cache) — nếu thiếu và registry có config,
/// tự fetch. Registry-first: klass pnpm đua `create-*` từ registry.
/// Trả về true nếu template khả dụng (có template.toml + sources).
pub async fn ensure_layer(rel: &str) -> bool {
    // Check disk / cache trước (TemplateRoot::resolve đã theo đúng priority
    // env → workspace disk → cache).
    let root = crate::scaffold::template_root::TemplateRoot::resolve(rel);
    if root.exists("") && root.exists("template.toml") && root.exists("sources") {
        return true;
    }
    // Registry fetch khi chưa có. Core lấy từ segment đầu của rel (web/... → web,
    // game/bevy → game) để package name khớp mg-create-<core>-<name>.
    let core = rel.split('/').next().unwrap_or("web").to_string();
    if let Ok(registry) = select_registry(None) {
        let args = TemplateFetchArgs {
            core,
            name: rel.to_string(),
            registry: Some(registry),
            tag: None,
        };
        if fetch(args).await.is_ok() {
            return true;
        }
    }
    false
}

/// Extract tarball entries vào target, bỏ segment đầu của entry name
/// (npm tarball prefix `mg-create-.../...`).
fn extract_tarball_into(bytes: &[u8], target: &Path) -> Result<()> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut components = path.components();
        let _first = components.next(); // strip package prefix
        let rel: PathBuf = components.as_path().to_path_buf();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = target.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&dest)?,
            _ => {
                let mut out = fs::File::create(&dest)?;
                std::io::copy(&mut entry, &mut out)?;
            }
        }
    }
    Ok(())
}

/// Resolve registry: --registry flag → MG_NPM_REGISTRY env → .npmrc → default.
fn select_registry(flag: Option<&str>) -> Result<String> {
    if let Some(url) = flag {
        return Ok(url.to_string());
    }
    if let Ok(url) = std::env::var("MG_NPM_REGISTRY") {
        if !url.is_empty() {
            return Ok(url);
        }
    }
    if let Ok(npmrc) = NpmRc::load(Path::new(".")) {
        if let Some(url) = npmrc.registry_for(None) {
            return Ok(url);
        }
    }
    Ok(DEFAULT_REGISTRY.to_string())
}

/// Auth resolution cho fetch — registry công khai không cần; registry private
/// (token admin) yêu cầu header, nên lấy auth nếu cấu hình (npmrc/env).
async fn register_fetch_auth(registry: &str) -> mg_publish::auth::Auth {
    let npmrc = NpmRc::load(Path::new("."));
    let npmrc = match npmrc {
        Ok(n) => n,
        Err(_) => return mg_publish::auth::Auth::default(),
    };
    resolve_auth(&npmrc, registry, None, None).unwrap_or_else(|_| mg_publish::auth::Auth::default())
}

/// PUT tarball blob (npm-format: /{name}/-/{name}-{version}.tgz).
async fn upload_tarball(
    registry_url: &str,
    auth: &mg_publish::auth::Auth,
    name: &str,
    version: &str,
    tarball_path: &Path,
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
    let req = if let Some(h) = auth.header_value() {
        req.header("Authorization", h)
    } else {
        req
    };
    let resp = req.send().await?;
    if !resp.status().is_success() {
        bail!(
            "Tarball upload failed: {} - {}",
            resp.status(),
            resp.text().await?
        );
    }
    Ok(())
}

/// PUT manifest metadata (dist-tags + versions).
async fn put_manifest(
    registry_url: &str,
    auth: &mg_publish::auth::Auth,
    name: &str,
    version: &str,
    manifest: &serde_json::Value,
    pack_result: &mg_pack::tarball::PackResult,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/{}", registry_url.trim_end_matches('/'), name);

    let mut body = serde_json::Map::new();
    body.insert("_id".into(), serde_json::Value::String(name.into()));
    body.insert("name".into(), serde_json::Value::String(name.into()));
    body.insert("dist-tags".into(), serde_json::json!({"latest": version}));
    body.insert("maintainers".into(), serde_json::json!([]));
    body.insert(
        "time".into(),
        serde_json::json!({"created": "", "modified": ""}),
    );
    body.insert("private".into(), serde_json::json!(false));
    let mut versions = serde_json::Map::new();
    let mut version_obj = manifest.clone();
    let unscoped = name.rsplit('/').next().unwrap_or(name);
    version_obj
        .as_object_mut()
        .expect("manifest is a JSON object")
        .insert(
        "dist".into(),
        serde_json::json!({
            "tarball": format!("{}/{}/-/{}-{}.tgz", registry_url.trim_end_matches('/'), name, unscoped, version),
            "shasum": pack_result.shasum,
            "integrity": pack_result.integrity,
            "fileCount": pack_result.entry_count,
            "unpackedSize": pack_result.unpacked_size,
        }),
    );
    versions.insert(version.into(), version_obj);
    body.insert("versions".into(), serde_json::Value::Object(versions));

    let req = client.put(&url).json(&body);
    let req = if let Some(h) = auth.header_value() {
        req.header("Authorization", h)
    } else {
        req
    };
    let resp = req.send().await?;
    let status = resp.status();
    if status.as_u16() == 409 {
        bail!("Version {version} already exists — bump --version");
    }
    if !status.is_success() {
        bail!("Publish failed: {} - {}", status, resp.text().await?);
    }
    Ok(())
}
