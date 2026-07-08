use anyhow::Result;
use mg_ui::{info};

/// mg search — search packages in registry
pub async fn run(query: String) -> Result<()> {
    info(&format!("Searching for '{}'... (npm registry)", query));

    let url = format!("https://registry.npmjs.org/-/v1/search?text={}&size=20", urlencoding(&query));
    let client = reqwest::Client::new();

    match client.get(&url).send().await {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().await?;
            let objects = data["objects"].as_array().map(|a| a.len()).unwrap_or(0);

            if objects == 0 {
                info("No results found");
                return Ok(());
            }

            info(&format!("Found {} result(s):", objects));
            println!();

            for obj in data["objects"].as_array().unwrap_or(&vec![]) {
                let pkg = &obj["package"];
                let name = pkg["name"].as_str().unwrap_or("?");
                let version = pkg["version"].as_str().unwrap_or("?");
                let desc = pkg["description"].as_str().unwrap_or("");

                info(&format!("  {}@{}", name, version));
                if !desc.is_empty() {
                    info(&format!("    {}", desc));
                }
            }
        }
        Err(e) => {
            info(&format!("Search failed: {}", e));
        }
    }

    Ok(())
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
