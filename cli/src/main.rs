#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

/// MagiCore CLI - Universal Package Manager
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

mod bundler;
mod commands;
mod context;
mod dispatch;
pub mod error;
mod factory;
mod offline; // T4.1: Offline mode state
pub mod scaffold;
mod wizard;

#[derive(Parser)]
#[command(name = "mgc")]
#[command(about = "MagiCore - Universal Package Manager", long_about = None)]
#[command(version)]
pub(crate) struct Cli {
    /// Target core (web, game, ai, clo, cicd, iot, app, lib)
    #[arg(global = true, long)]
    core: Option<String>,

    /// Fail installations if packages are under quarantine (published < 24h)
    #[arg(global = true, long)]
    audit_strict: bool,

    /// Run the command for each project in the workspace
    #[arg(global = true, short = 'r', long)]
    recursive: bool,

    /// Reduce non-essential output for CI and benchmarks
    #[arg(global = true, short = 'q', long)]
    quiet: bool,

    /// Filter workspace targets when --recursive (pnpm --filter parity: `./apps/*`, `@core/*`, exact name)
    #[arg(global = true, long)]
    filter: Option<String>,

    /// Run the command from another directory (pnpm -C parity)
    #[arg(global = true, short = 'C', long)]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

pub(crate) use crate::commands::definitions::Commands;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dispatch::run(Cli::parse()).await
}
