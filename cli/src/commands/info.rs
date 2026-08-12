use anyhow::Result;
#[cfg(feature = "web")]
use mg_ui::info;
#[cfg(feature = "web")]
use serde::Serialize;

#[cfg(feature = "web")]
use crate::commands::web_registry_config::web_registry_url;

/// mg info <pkg> — show package information.
/// Queries multiple registries, auto-labels Core/Language based on package metadata.
pub async fn run(package: String, json: bool) -> Result<()> {
    #[cfg(not(feature = "web"))]
    {
        let _ = (package, json);
        anyhow::bail!("package info currently requires the web core registry adapter, which is not included in this build");
    }

    #[cfg(feature = "web")]
    {
        // Query npm-compatible registry — đọc registry web qua config/env tập trung.
        let registry = mg_web_adapter::native::npm_registry::NpmRegistry::new(&web_registry_url());

        let meta = match registry.fetch_metadata(&package).await {
            Ok(m) => m,
            Err(e) => anyhow::bail!("Could not fetch package info from registry: {}", e),
        };

        let local_version = detect_local_version(&package);

        // Detect Core label from keywords / description heuristics
        let core_label = detect_core_label(&package, meta.description.as_deref().unwrap_or(""));

        if json {
            let output = InfoJson {
                name: meta.name.clone(),
                description: meta.description.clone().unwrap_or_default(),
                core_support: core_label.clone(),
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

        mg_ui::blank_line();
        info(&format!("Package:      {}", meta.name));
        info(&format!("Core Support: {}", core_label));
        if let Some(desc) = &meta.description {
            info(&format!("Description:  {}", desc));
        }
        if let Some(version) = &local_version {
            info(&format!("Installed:    {}", version));
        }
        info(&format!("Versions:     {}", meta.versions.len()));

        if !meta.dist_tags.is_empty() {
            mg_ui::blank_line();
            for (tag, ver) in &meta.dist_tags {
                info(&format!("  {}: {}", tag, ver));
            }
        }

        let mut sorted: Vec<_> = meta.versions.keys().collect();
        sorted.sort();
        let recent: Vec<_> = sorted.iter().rev().take(5).collect();
        if !recent.is_empty() {
            mg_ui::blank_line();
            info("Recent versions:");
            for v in recent {
                info(&format!("  {}", v));
            }
        }

        Ok(())
    }
}

/// Heuristic: detect which Core / language this package belongs to.
#[cfg(feature = "web")]
fn detect_core_label(name: &str, desc: &str) -> String {
    let combined = format!("{} {}", name, desc).to_lowercase();

    if combined.contains("react")
        || combined.contains("vue")
        || combined.contains("vite")
        || combined.contains("next")
        || combined.contains("nuxt")
        || combined.contains("svelte")
        || combined.contains("astro")
        || combined.contains("angular")
        || combined.contains("frontend")
        || combined.contains("ui library")
        || combined.contains("css")
        || combined.contains("tailwind")
    {
        return "[Core: Web / Frontend]".to_string();
    }

    if combined.contains("express")
        || combined.contains("fastify")
        || combined.contains("hono")
        || combined.contains("koa")
        || combined.contains("backend")
        || combined.contains("server")
        || combined.contains("rest api")
        || combined.contains("graphql")
    {
        return "[Core: Web / Backend]".to_string();
    }

    if combined.contains("langchain")
        || combined.contains("openai")
        || combined.contains("llm")
        || combined.contains("embedding")
        || combined.contains("agent")
        || combined.contains("machine learning")
        || combined.contains("tensorflow")
        || combined.contains("pytorch")
    {
        return "[Core: AI / ML]".to_string();
    }

    if combined.contains("pulumi")
        || combined.contains("terraform")
        || combined.contains("cloud")
        || combined.contains("aws")
        || combined.contains("gcp")
        || combined.contains("azure")
    {
        return "[Core: Cloud (clo)]".to_string();
    }

    if combined.contains("github-actions")
        || combined.contains("ci/cd")
        || combined.contains("pipeline")
        || combined.contains("deploy")
    {
        return "[Core: CI/CD]".to_string();
    }

    if combined.contains("bevy")
        || combined.contains("godot")
        || combined.contains("game")
        || combined.contains("3d engine")
        || combined.contains("rendering")
    {
        return "[Core: Game]".to_string();
    }

    if combined.contains("flutter")
        || combined.contains("mobile")
        || combined.contains("electron")
        || combined.contains("tauri")
        || combined.contains("desktop app")
    {
        return "[Core: App (Mobile/Desktop)]".to_string();
    }

    "[Core: General / Library]".to_string()
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
    core_support: String,
    version_count: usize,
    dist_tags: Vec<(String, String)>,
    local_version: Option<String>,
}
