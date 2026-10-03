//! mgc-registry — Private registry server binary (MagiCore)
//! Runs the private registry server with /npm and /v2 endpoints.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "mgc-registry", about = "MagiCore Private Registry Server")]
struct Args {
    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind to (RULE §13: chứa 4·3·1·5)
    #[arg(long, default_value = "4315")]
    port: u16,

    /// Storage directory for packages and blobs
    #[arg(long, default_value = "./data/registry")]
    store_dir: String,

    /// Admin token for registry operations
    #[arg(long, env = "MAGICORE_REGISTRY_ADMIN_TOKEN")]
    admin_token: Option<String>,

    /// Maximum request body size (bytes)
    #[arg(long, default_value = "104857600")]
    max_body_size: usize,

    /// Rate limit (requests/second/IP); 0 = disabled
    #[arg(long, default_value = "0")]
    rate_limit: usize,

    /// Upstream registry URL to proxy GET-miss (ITEM 4). None = private-only
    #[arg(long)]
    upstream: Option<String>,

    /// Blob storage: "local" hoặc "s3://bucket/prefix" (ITEM 5)
    #[arg(long)]
    storage: Option<String>,

    /// Trusted-publisher OIDC issuers; repeat for multiple trusted issuers.
    #[arg(long = "oidc-issuer", env = "MAGICORE_REGISTRY_OIDC_ISSUER")]
    oidc_issuers: Vec<String>,

    /// OIDC audience expected in CI workload JWTs.
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

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level)),
        )
        .init();

    mgc_registry_server::serve(mgc_registry_server::RegistryServerConfig {
        host: args.host,
        port: args.port,
        store_dir: args.store_dir,
        admin_token: args.admin_token,
        max_body_size: args.max_body_size,
        rate_limit_rps: args.rate_limit,
        upstream: args.upstream,
        storage: args.storage,
        oidc_issuers: args.oidc_issuers,
        oidc_audience: args.oidc_audience,
        trusted_proxy_ips: args.trusted_proxy_ips,
    })
    .await
}
