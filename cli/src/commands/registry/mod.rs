//! Registry command — mgc registry serve / user add / user rm (10-task-plan Phase 3)
//! (Lệnh registry: serve server, quản lý user — add/rm)

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use tracing_subscriber::EnvFilter;

/// Default local registry URL for user management (RULE §13: port chứa 4·3·1·5)
const DEFAULT_REGISTRY: &str = "http://127.0.0.1:4315";

/// Apply a core namespace only to OCI bindings; preserve existing npm/PyPI names.
/// Chỉ thêm namespace core cho binding OCI; giữ nguyên tên npm/PyPI hiện hữu.
fn effective_trusted_binding_package(
    protocol: &str,
    core: Option<&str>,
    package: &str,
) -> Result<String> {
    if protocol == "oci"
        && let Some(core) = core
    {
        return mgc_registry_server::trusted::core_scoped_oci_repository(core, package)
            .map_err(|_| anyhow::anyhow!("invalid core-scoped OCI repository"));
    }
    Ok(package.to_owned())
}

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
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
        #[arg(long, default_value = "104857600")]
        max_body_size: usize,
        #[arg(long, default_value = "0", help = "rate limit (req/s/IP), 0 = off")]
        rate_limit: usize,
        #[arg(long, help = "upstream registry URL — GET-miss proxy + cache (ITEM 4)")]
        upstream: Option<String>,
        #[arg(
            long,
            help = "blob storage: \"local\" or \"s3://bucket/prefix\" (ITEM 5)"
        )]
        storage: Option<String>,
        #[arg(long = "oidc-issuer", env = "MAGICORE_REGISTRY_OIDC_ISSUER")]
        oidc_issuers: Vec<String>,
        #[arg(long, env = "MAGICORE_REGISTRY_OIDC_AUDIENCE")]
        oidc_audience: Option<String>,
        /// Comma-separated peer IPs of TLS proxies allowed to assert X-Forwarded-Proto.
        /// Danh sách IP peer proxy TLS được phép xác nhận X-Forwarded-Proto, phân tách bằng dấu phẩy.
        #[arg(
            long,
            env = "MAGICORE_REGISTRY_TRUSTED_PROXY_IPS",
            value_delimiter = ','
        )]
        trusted_proxy_ips: Vec<std::net::IpAddr>,
    },
    /// Bind a package to a trusted CI repository (requires admin token).
    /// Ràng buộc package với repository CI đáng tin (cần admin token).
    Trust {
        package: String,
        repository: String,
        #[arg(long, value_parser = ["npm", "pypi", "oci"], default_value = "npm")]
        protocol: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
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
        #[arg(long = "scope", help = "package scope patterns, vd: @magicore/*")]
        scopes: Vec<String>,
        #[arg(
            long,
            default_value = "publisher",
            help = "role: viewer|publisher|admin (ITEM 6)"
        )]
        role: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
    },
    /// Delete a user (requires admin token)
    Rm {
        name: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
    },
    /// Revoke an access token (requires admin token)
    Revoke {
        token: String,
        #[arg(long, default_value = DEFAULT_REGISTRY)]
        registry: String,
        #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
        admin_token: Option<String>,
    },
}

pub async fn run(args: RegistryArgs, core: Option<&str>) -> Result<()> {
    match args.cmd {
        RegistryCmd::Serve {
            host,
            port,
            store_dir,
            admin_token,
            max_body_size,
            rate_limit,
            upstream,
            storage,
            oidc_issuers,
            oidc_audience,
            trusted_proxy_ips,
        } => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
                )
                .try_init(); // main.rs đã init global — không panic nếu set rồi
            mgc_registry_server::serve(mgc_registry_server::RegistryServerConfig {
                host,
                port,
                store_dir,
                admin_token,
                max_body_size,
                rate_limit_rps: rate_limit,
                upstream,
                storage,
                oidc_issuers,
                oidc_audience,
                trusted_proxy_ips,
            })
            .await
        }
        RegistryCmd::Trust {
            package,
            repository,
            protocol,
            registry,
            admin_token,
        } => {
            let package = effective_trusted_binding_package(&protocol, core, &package)?;
            trust_publisher(&package, &repository, &protocol, &registry, admin_token).await
        }
        RegistryCmd::User { cmd } => match cmd {
            UserCmd::Add {
                name,
                password,
                scopes,
                role,
                registry,
            } => user_add(&name, password, &scopes, &role, &registry).await,
            UserCmd::Rm {
                name,
                registry,
                admin_token,
            } => user_rm(&name, &registry, admin_token).await,
            UserCmd::Revoke {
                token,
                registry,
                admin_token,
            } => revoke_token(&token, &registry, admin_token).await,
        },
    }
}

async fn trust_publisher(
    package: &str,
    repository: &str,
    protocol: &str,
    registry: &str,
    admin_token: Option<String>,
) -> Result<()> {
    let admin_token = admin_token.ok_or_else(crate::error::trusted_registry_admin_required)?;
    let parsed_registry =
        url::Url::parse(registry).map_err(|_| crate::error::trusted_registry_url_invalid())?;
    if !parsed_registry.username().is_empty()
        || parsed_registry.password().is_some()
        || parsed_registry.query().is_some()
        || parsed_registry.fragment().is_some()
    {
        return Err(crate::error::trusted_registry_url_invalid());
    }
    let local_http = parsed_registry.scheme() == "http"
        && parsed_registry.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if parsed_registry.scheme() != "https" && !local_http {
        return Err(crate::error::trusted_registry_requires_https());
    }
    let url = format!(
        "{}/-/v1/trusted-publish/bindings",
        registry.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let response = client
        .put(url)
        .bearer_auth(admin_token)
        .json(&serde_json::json!({
            "protocol": protocol,
            "package": package,
            "repository": repository,
        }))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(crate::error::trusted_binding_failed(
            response.status().as_u16(),
        ));
    }
    println!("Trusted {protocol} publisher bound: {package} ← {repository}");
    Ok(())
}

async fn user_add(
    name: &str,
    password: Option<String>,
    scopes: &[String],
    role: &str,
    registry: &str,
) -> Result<()> {
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
    let body = serde_json::json!({
        "name": name,
        "password": password,
        "email": "",
        "scopes": scopes,
        "role": role
    });
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
        .ok_or_else(|| crate::error::no_token_in_response(&text))?;
    println!("User {} created. Token: {}", name, token);
    println!(
        "Set it in .npmrc via: mgc login --username {} --password <pw>",
        name
    );
    Ok(())
}

async fn user_rm(name: &str, registry: &str, admin_token: Option<String>) -> Result<()> {
    let Some(token) = admin_token else {
        bail!("Admin token required (--admin-token or MAGICORE_REGISTRY_ADMIN_TOKEN)");
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

async fn revoke_token(token: &str, registry: &str, admin_token: Option<String>) -> Result<()> {
    let Some(admin) = admin_token else {
        bail!("Admin token required (--admin-token or MAGICORE_REGISTRY_ADMIN_TOKEN)");
    };
    let url = format!("{}/-/user/token/{}", registry.trim_end_matches('/'), token);
    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", admin))
        .send()
        .await?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(crate::error::token_not_found(token));
    }
    if !status.is_success() {
        bail!("Revoke token failed: {}", status);
    }
    println!("Token revoked");
    Ok(())
}

#[cfg(test)]
#[path = "test/mod.rs"]
mod tests;
