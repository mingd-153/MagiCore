use anyhow::Result;
use mg_ui::info;

#[cfg(feature = "web")]
use serde::Serialize;

/// mg info — show package information
pub async fn run(package: String, json: bool) -> Result<()> {
    #[cfg(feature = "web")]
    return info_web(package, json).await;
    #[cfg(not(feature = "web"))]
    {
        let _ = (package, json);
        anyhow::bail!("'mg info' is not available in this build (requires web core)")
    }
}

#[cfg(feature = "web")]
async fn info_web(package: String, json: bool) -> Result<()> {
    let registry =
        mg_web_adapter::native::npm_registry::NpmRegistry::new("https://registry.npmjs.org");

    let meta = match registry.fetch_metadata(&package).await {
        Ok(m) => m,
        Err(e) => anyhow::bail!("Could not fetch package info: {}", e),
    };

    let local_version = detect_local_version(&package);

    if json {
        let output = InfoJson {
            name: meta.name.clone(),
            description: meta.description.clone().unwrap_or_default(),
            version_count: meta.versions.len(),
            dist_tags: meta
                .dist_tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            local_version: local_version.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!();
    info(&format!("Package: {}", meta.name));
    if let Some(desc) = &meta.description {
        info(&format!("Description: {}", desc));
    }
    if let Some(version) = &local_version {
        info(&format!("Installed: {}", version));
    }
    info(&format!("Versions: {}", meta.versions.len()));

    if !meta.dist_tags.is_empty() {
        println!();
        for (tag, ver) in &meta.dist_tags {
            info(&format!("  {}: {}", tag, ver));
        }
    }

    let mut sorted: Vec<_> = meta.versions.keys().collect();
    sorted.sort();
    let recent: Vec<_> = sorted.iter().rev().take(5).collect();
    if !recent.is_empty() {
        println!();
        info("Recent versions:");
        for v in recent {
            info(&format!("  {}", v));
        }
    }

    Ok(())
}

#[cfg(feature = "web")]
fn detect_local_version(package: &str) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let root = crate::commands::core::shared::find_project_root(&cwd).ok()??;
    let manifest_path = root.join("package.json");
    if !manifest_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(manifest_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    for section in &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(deps) = parsed.get(*section).and_then(|v| v.as_object()) {
            if let Some(ver) = deps.get(package).and_then(|v| v.as_str()) {
                return Some(ver.to_string());
            }
        }
    }
    None
}

#[cfg(feature = "web")]
#[derive(Serialize)]
struct InfoJson {
    name: String,
    description: String,
    version_count: usize,
    dist_tags: Vec<(String, String)>,
    local_version: Option<String>,
}
