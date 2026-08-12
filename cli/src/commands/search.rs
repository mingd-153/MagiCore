use anyhow::Result;
use mg_ui::info;
use serde::Serialize;

use crate::commands::web_registry_config::{search_endpoint, web_registry_url};

/// mg search <query> [--core <core>] — search packages across all 8 MegaGate cores.
/// Groups results by Core & Language when no --core flag is provided.
/// Accepts an optional `core_filter` to narrow results to a specific ecosystem.
pub async fn run(query: String, json: bool, exact: bool, page: Option<u32>) -> Result<()> {
    // NOTE: `core_filter` is read from CLI global flag `--core` and passed here via context.
    // For now we read it from the environment to keep the signature backward-compatible.
    let core_filter = std::env::var("MG_CORE_FILTER").ok();
    run_with_core(query, json, exact, page, core_filter.as_deref()).await
}

pub async fn run_with_core(
    query: String,
    json: bool,
    exact: bool,
    page: Option<u32>,
    core_filter: Option<&str>,
) -> Result<()> {
    let search_query = if exact {
        format!("exact:{}", query)
    } else {
        query.clone()
    };
    let size = 20u32;
    let from = page.unwrap_or(1).saturating_sub(1) * size;

    let registry_url = web_registry_url();
    let url = search_endpoint(&registry_url, &search_query, size, from)?;

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    let data: serde_json::Value = resp.json().await?;
    let total = data["total"].as_u64().unwrap_or(0);
    let objects = data["objects"].as_array().cloned().unwrap_or_default();

    if objects.is_empty() {
        info("No results found");
        return Ok(());
    }

    // Map results and label each one with a Core
    let results: Vec<SearchResult> = objects
        .iter()
        .map(|obj| {
            let pkg = &obj["package"];
            let name = pkg["name"].as_str().unwrap_or("?").to_string();
            let version = pkg["version"].as_str().unwrap_or("?").to_string();
            let description = pkg["description"].as_str().unwrap_or("").to_string();
            let score = obj["score"]["final"]
                .as_f64()
                .map(|s| format!("{:.3}", s))
                .unwrap_or_default();
            let core_label = detect_core_label(&name, &description);
            SearchResult {
                name,
                version,
                description,
                score,
                core_label,
            }
        })
        .filter(|r| {
            // Apply --core filter if set
            if let Some(filter) = core_filter {
                r.core_label.to_lowercase().contains(filter)
            } else {
                true
            }
        })
        .collect();

    if json {
        let output = SearchOutput { total, results };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    info(&format!(
        "Found {} result(s) (total: {}){}:",
        results.len(),
        total,
        core_filter
            .map(|f| format!(" [filter: --core {}]", f))
            .unwrap_or_default()
    ));
    mg_ui::blank_line();

    for r in &results {
        info(&format!("  {} {}@{}", r.core_label, r.name, r.version));
        if !r.description.is_empty() {
            info(&format!("    {}", r.description));
        }
    }

    if total > objects.len() as u64 {
        let current = page.unwrap_or(1);
        info(&format!(
            "Page {} of ~{}",
            current,
            total.div_ceil(size as u64)
        ));
    }

    Ok(())
}

/// Detect which MegaGate Core a package belongs to based on name/description heuristics.
fn detect_core_label(name: &str, desc: &str) -> String {
    let combined = format!("{} {}", name, desc).to_lowercase();

    if combined.contains("react")
        || combined.contains("vue")
        || combined.contains("vite")
        || combined.contains("next")
        || combined.contains("svelte")
        || combined.contains("astro")
        || combined.contains("tailwind")
        || combined.contains("angular")
        || combined.contains("frontend")
        || combined.contains("css")
    {
        return "[Core: Web / Frontend]".to_string();
    }
    if combined.contains("express")
        || combined.contains("fastify")
        || combined.contains("hono")
        || combined.contains("server")
        || combined.contains("rest api")
        || combined.contains("backend")
        || combined.contains("graphql")
    {
        return "[Core: Web / Backend]".to_string();
    }
    if combined.contains("langchain")
        || combined.contains("openai")
        || combined.contains("llm")
        || combined.contains("agent")
        || combined.contains("embedding")
        || combined.contains("machine learning")
    {
        return "[Core: AI / ML]".to_string();
    }
    if combined.contains("aws")
        || combined.contains("gcp")
        || combined.contains("azure")
        || combined.contains("cloud")
        || combined.contains("terraform")
        || combined.contains("pulumi")
    {
        return "[Core: Cloud (clo)]".to_string();
    }
    if combined.contains("github-action")
        || combined.contains("ci/cd")
        || combined.contains("pipeline")
        || combined.contains("deploy")
    {
        return "[Core: CI/CD]".to_string();
    }
    if combined.contains("bevy")
        || combined.contains("godot")
        || combined.contains("game")
        || combined.contains("rendering engine")
    {
        return "[Core: Game]".to_string();
    }
    if combined.contains("flutter")
        || combined.contains("mobile")
        || combined.contains("tauri")
        || combined.contains("electron")
    {
        return "[Core: App (Mobile/Desktop)]".to_string();
    }
    "[Core: Web / Node]".to_string()
}

#[derive(Serialize)]
struct SearchOutput {
    total: u64,
    results: Vec<SearchResult>,
}

#[derive(Serialize)]
struct SearchResult {
    name: String,
    version: String,
    description: String,
    score: String,
    core_label: String,
}
