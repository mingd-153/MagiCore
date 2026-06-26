use clap::{Parser, Subcommand};
use megagate_core::MegagateCore;
use megagate_types::config::MegagateConfig;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "megagate")]
#[command(about = "Multi-platform package manager core")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install {
        target: Option<String>,
    },
    Add {
        package: String,
        #[arg(short, long)]
        dev: bool,
    },
    Update {
        package: Option<String>,
    },
    Remove {
        package: String,
    },
    List {
        #[arg(short, long)]
        graph: bool,
        #[arg(short, long, default_value = "0")]
        depth: u32,
    },
    Audit,
    Lock {
        #[command(subcommand)]
        action: Option<LockAction>,
    },
}

#[derive(Subcommand)]
enum LockAction {
    Verify,
    Export {
        format: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();
    let config = MegagateConfig::default();
    let core = MegagateCore::new(config).await?;
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();

    match cli.command {
        Commands::Install { target } => {
            let dir = target.unwrap_or_else(|| cwd.clone());
            let result = core.install(&dir).await?;
            println!("Install completed: {} added", result.added.len());
        }
        Commands::Add { package, dev } => {
            let result = core.add(&cwd, &package, dev).await?;
            println!("Added {}: {} added", package, result.added.len());
        }
        Commands::Update { package } => {
            let result = core.update(&cwd, package.as_deref()).await?;
            println!("Update completed: {} updated", result.updated.len());
        }
        Commands::Remove { package } => {
            let result = core.remove(&cwd, &package).await?;
            println!("Removed {}: {} removed", package, result.removed.len());
        }
        Commands::List { graph, depth } => {
            let deps = core.list(&cwd, depth).await?;
            if graph {
                println!("{}", serde_json::to_string_pretty(&deps)?);
            } else {
                for (pkg, version) in &deps {
                    println!("{}@{}", pkg, version);
                }
            }
        }
        Commands::Audit => {
            let result = core.audit(&cwd).await?;
            println!("{}", result.summary);
            for vuln in &result.vulnerabilities {
                println!("  Vulnerable: {}", vuln);
            }
        }
        Commands::Lock { action } => match action {
            Some(LockAction::Verify) | None => {
                let result = core.verify_lockfile(&cwd).await?;
                println!("{}", result);
            }
            Some(LockAction::Export { format }) => {
                if format == "json" {
                    println!("Export not yet implemented");
                } else {
                    anyhow::bail!("Unsupported format: {}", format);
                }
            }
        },
    }

    Ok(())
}