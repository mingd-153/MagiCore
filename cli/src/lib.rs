//! MagiCore CLI library - expose modules for testing

#![allow(clippy::unwrap_used)]

pub mod bundler;
pub mod commands;
pub mod context;
pub mod error;
pub mod factory;
pub mod offline;
pub mod scaffold;
pub mod wizard;

// Re-export Cli từ main.rs via include! (binary crate pattern)
// Test files sẽ dùng Cli từ binary, không phải stub
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "MagiCore - Universal Package Manager", long_about = None)]
#[command(version)]
pub struct Cli {
    #[arg(global = true, long)]
    pub core: Option<String>,
    #[arg(global = true, long)]
    pub audit_strict: bool,
    #[arg(global = true, short = 'r', long)]
    pub recursive: bool,
    #[arg(global = true, short = 'q', long)]
    pub quiet: bool,
    #[arg(global = true, long)]
    pub filter: Option<String>,
    #[arg(global = true, short = 'C', long)]
    pub dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<crate::commands::definitions::Commands>,
}
