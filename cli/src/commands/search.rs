use anyhow::Result;
use mg_ui::info;
use serde::Serialize;

/// mg search — search packages in npm registry
pub async fn run(query: String, json: bool, exact: bool, page: Option<u32>) -> Result<()> {
    let search_query = if exact {
        format!("exact:{}", query)
    } else {
        query.clone()
    };
    let size = 20u32;
    let from = page.unwrap_or(1).saturating_sub(1) * size;

    let url = format!(
        "https://registry.npmjs.org/-/v1/search?text={}&size={}&from={}",
        urlencoding(&search_query),
        size,
        from
    );
    let client = reqwest::Client::new();

    let resp = client.get(&url).send().await?;
    let data: serde_json::Value = resp.json().await?;
    let objects = data["objects"].as_array().map(|a| a.len()).unwrap_or(0);
    let total = data["total"].as_u64().unwrap_or(0);

    if json {
        let results: Vec<SearchResult> = data["objects"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|obj| {
                        let pkg = &obj["package"];
                        SearchResult {
                            name: pkg["name"].as_str().unwrap_or("").to_string(),
                            version: pkg["version"].as_str().unwrap_or("").to_string(),
                            description: pkg["description"].as_str().unwrap_or("").to_string(),
                            score: obj["score"]["final"]
                                .as_f64()
                                .map(|s| format!("{:.3}", s))
                                .unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let output = SearchOutput {
            total,
            results,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if objects == 0 {
        info("No results found");
        return Ok(());
    }

    info(&format!("Found {} result(s) (total: {}):", objects, total));
    println!();

    for obj in data["objects"].as_array().unwrap_or(&vec![]) {
        let pkg = &obj["package"];
        let name = pkg["name"].as_str().unwrap_or("?");
        let version = pkg["version"].as_str().unwrap_or("?");
        let desc = pkg["description"].as_str().unwrap_or("");
        let score = obj["score"]["final"].as_f64().unwrap_or(0.0);

        info(&format!("  {}@{} (score: {:.3})", name, version, score));
        if !desc.is_empty() {
            info(&format!("    {}", desc));
        }
    }

    if total > objects as u64 {
        let current = page.unwrap_or(1);
        info(&format!(
            "Page {} of ~{}",
            current,
            (total + size as u64 - 1) / size as u64
        ));
    }

    Ok(())
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
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
}
