//! Registry command — mg registry serve / user add / user rm (10-task-plan Phase 3)
//! (Lệnh registry: serve server, quản lý user — add/rm)

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use tracing_subscriber::EnvFilter;

/// Default local registry URL for user management (RULE §13: port chứa 4·3·1·5)
const DEFAULT_REGISTRY: &str = "http://127.0.0.1:4315";

#[derive(Args, Debug, Clone)]
pub struct RegistryArgs {
    #[command(subcommand)]
    pub cmd: RegistryCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum RegistryCmd {
    /// Start the private registry server (1 process, /npm + /v2)
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "4315")] // RULE §13
        port: u16,
        #[arg(long, default_value = "./data/registry")]
        store_dir: String,
        #[arg(long, env = "MEGAGATE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
        #[arg(long, default_value = "104857600")]
        max_body_size: usize,
        #[arg(long, default_value = "0", help = "rate limit (req/s/IP), 0 = off")]
        rate_limit: usize,
    },
    /// Manage registry users
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum UserCmd {
    /// Create a user, print the token
    Add {
        name: String,
        #[arg(long, help = "password (prompt if omitted)")]
        password: Option<String>,
        #[arg(long = "scope", help = "package scope patterns, vd: @megagate/*")]
        scopes: Vec<String>,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
    },
    /// Delete a user (requires admin token)
    Rm {
        name: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MEGAGATE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
    },
}

pub async fn run(args: RegistryArgs) -> Result<()> {
    match args.cmd {
        RegistryCmd::Serve {
            host,
            port,
            store_dir,
            admin_token,
            max_body_size,
            rate_limit,
        } => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info")),
                )
                .try_init(); // main.rs đã init global — không panic nếu set rồi
            mg_registry_server::serve(host, port, store_dir, admin_token, max_body_size, rate_limit)
                .await
        }
        RegistryCmd::User { cmd } => match cmd {
            UserCmd::Add {
                name,
                password,
                scopes,
                registry,
            } => user_add(&name, password, &scopes, &registry).await,
            UserCmd::Rm {
                name,
                registry,
                admin_token,
            } => user_rm(&name, &registry, admin_token).await,
        },
    }
}

async fn user_add(name: &str, password: Option<String>, scopes: &[String], registry: &str) -> Result<()> {
    let password = match password {
        Some(p) => p,
        None => {
            print!("Password for {}: ", name);
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line.trim().to_string()
        }
    };

    let url = format!(
        "{}/-/user/org.couchdb.user:{}",
        registry.trim_end_matches('/'),
        name
    );
    let body = serde_json::json!({ "name": name, "password": password, "email": "", "scopes": scopes });
    let client = reqwest::Client::new();
    let resp = client.put(&url).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("Add user failed: {} - {}", status, text);
    }
    let json: serde_json::Value = serde_json::from_str(&text)?;
    let token = json["token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No token in response: {}", text))?;
    println!("User {} created. Token: {}", name, token);
    println!("Set it in .npmrc via: mg login --username {} --password <pw>", name);
    Ok(())
}

async fn user_rm(name: &str, registry: &str, admin_token: Option<String>) -> Result<()> {
    let Some(token) = admin_token else {
        bail!("Admin token required (--admin-token or MEGAGATE_REGISTRY_ADMIN_TOKEN)");
    };
    let url = format!(
        "{}/-/user/org.couchdb.user:{}",
        registry.trim_end_matches('/'),
        name
    );
    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;
    let status = resp.status();
    if status.as_u16() == 404 {
        bail!("User {} not found", name);
    }
    if !status.is_success() {
        bail!("Delete user failed: {}", status);
    }
    println!("User {} deleted", name);
    Ok(())
}
