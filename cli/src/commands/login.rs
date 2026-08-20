//! Login command — adduser flow, save token to ~/.npmrc (01 §1 line 34)
//! (Lệnh login: đăng ký/adduser vào registry, lưu token vào .npmrc)

use anyhow::{bail, Result};
use clap::Args;
use std::io::{BufRead, Write};

use mg_config::npmrc::NpmRc;

/// Default registry (hardcode warning — OK: default const, overrideable via flag)
const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";

#[derive(Args, Debug, Clone)]
pub struct LoginArgs {
    #[arg(long, help = "registry URL (default: config or npmjs)")]
    pub registry: Option<String>,
    #[arg(long, help = "username (prompt if omitted)")]
    pub username: Option<String>,
    #[arg(long, help = "password (prompt if omitted)")]
    pub password: Option<String>,
    #[arg(long, help = "write token to project .npmrc instead of ~/.npmrc")]
    pub local: bool,
}

/// Prompt on stdin, echo input
fn prompt(label: &str) -> Result<String> {
    print!("{}: ", label);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

pub async fn run(args: LoginArgs) -> Result<()> {
    let registry = match args.registry {
        Some(url) => url,
        None => NpmRc::load(&std::env::current_dir()?)
            .ok()
            .and_then(|rc| rc.registry)
            .unwrap_or_else(|| DEFAULT_REGISTRY.to_string()),
    };

    let username = match args.username {
        Some(u) => u,
        None => prompt("Username")?,
    };
    if username.is_empty() {
        bail!("Username is required");
    }
    let password = match args.password {
        Some(p) => p,
        None => prompt("Password")?,
    };

    let url = format!(
        "{}/-/user/org.couchdb.user:{}",
        registry.trim_end_matches('/'),
        username
    );
    let body = serde_json::json!({
        "name": username,
        "password": password,
        "email": "",
    });

    let client = reqwest::Client::new();
    let resp = client.put(&url).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("Login failed: {} - {}", status, text);
    }

    let json: serde_json::Value = serde_json::from_str(&text)?;
    let token = json["token"]
        .as_str()
        .ok_or_else(|| crate::error::no_token_in_response(&text))?;

    let host = url::Url::parse(&registry)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_else(|_| registry.trim_end_matches('/').to_string());

    let npmrc_path = if args.local {
        std::env::current_dir()?.join(".npmrc")
    } else {
        dirs::home_dir()
            .ok_or_else(crate::error::no_home_dir)?
            .join(".npmrc")
    };

    NpmRc::save_auth_token(&npmrc_path, &host, token)?;
    println!("Logged in as {} to {}", username, registry);
    println!("Token saved to {}", npmrc_path.display());

    Ok(())
}
