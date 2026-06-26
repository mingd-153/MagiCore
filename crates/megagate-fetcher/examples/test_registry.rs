use reqwest::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let url = "https://registry.npmjs.org/lodash";
    println!("Fetching: {}", url);
    
    let response = client.get(url).send().await?;
    println!("Status: {}", response.status());
    
    let text = response.text().await?;
    println!("Text length: {}", text.len());
    println!("Preview: {}", &text[..std::cmp::min(200, text.len())]);
    
    Ok(())
}