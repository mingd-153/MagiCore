use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "mgc-dist")]
#[command(about = "Build MagiCore distribution packages")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List supported distribution packages
    List,
    /// Build one distribution package into dist/<package>/<target>/
    Build {
        package: String,
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "release")]
        profile: String,
    },
    /// Build the currently supported bootstrap packages
    BuildBootstrap {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "release")]
        profile: String,
    },
    /// Build every package manifest in packaging/packages/
    BuildAll {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "release")]
        profile: String,
    },
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
    name: String,
    binary: String,
    description: String,
    mode: String,
    primary_core: String,
    no_default_features: bool,
    features: Vec<String>,
    install_hint: String,
}

#[derive(Debug, Serialize)]
struct BuildReceipt<'a> {
    package: &'a str,
    binary: &'a str,
    description: &'a str,
    mode: &'a str,
    primary_core: &'a str,
    install_hint: &'a str,
    target: String,
    profile: String,
    features: Vec<String>,
    source_binary: String,
    output_binary: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::List => {
            for package in supported_packages()? {
                println!("{package}");
            }
        }
        Commands::Build {
            package,
            target,
            profile,
        } => {
            let package = load_manifest(&package)?;
            build_package(&package, target.as_deref(), &profile)?;
        }
        Commands::BuildBootstrap { target, profile } => {
            for package_name in ["magicore", "magicore-web"] {
                let package = load_manifest(package_name)?;
                build_package(&package, target.as_deref(), &profile)?;
            }
        }
        Commands::BuildAll { target, profile } => {
            for package_name in supported_packages()? {
                let package = load_manifest(&package_name)?;
                build_package(&package, target.as_deref(), &profile)?;
            }
        }
    }
    Ok(())
}

fn supported_packages() -> Result<Vec<String>> {
    let packages_dir = Path::new("packaging").join("packages");
    let mut packages = Vec::new();
    for entry in fs::read_dir(&packages_dir)
        .with_context(|| format!("failed to read {}", packages_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            packages.push(stem.to_string());
        }
    }
    packages.sort();
    Ok(packages)
}

fn load_manifest(package: &str) -> Result<PackageManifest> {
    let manifest_path = Path::new("packaging")
        .join("packages")
        .join(format!("{package}.toml"));
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", manifest_path.display()))
}

fn build_package(
    manifest: &PackageManifest,
    target_override: Option<&str>,
    profile: &str,
) -> Result<()> {
    let target = match target_override {
        Some(target) => target.to_owned(),
        None => detect_host_target()?,
    };

    let mut args = vec![
        "build".to_string(),
        "-p".to_string(),
        "mgc".to_string(),
        "--bin".to_string(),
        manifest.binary.clone(),
        "--profile".to_string(),
        profile.to_string(),
    ];

    if manifest.no_default_features {
        args.push("--no-default-features".to_string());
    }
    if !manifest.features.is_empty() {
        args.push("--features".to_string());
        args.push(manifest.features.join(","));
    }
    args.push("--target".to_string());
    args.push(target.clone());
    args.push("--locked".to_string());
    args.push("--offline".to_string());

    let options = executor_options(false)?;
    mgc_exec::run::run_inherited("cargo", &args, &options).with_context(|| {
        format!(
            "mgc-exec refused or failed Cargo build for package '{}' with target '{}'",
            manifest.name, target
        )
    })?;

    let exe_suffix = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let binary_name = format!("{}{}", manifest.binary, exe_suffix);
    let source_binary = Path::new("target")
        .join(&target)
        .join(profile)
        .join(&binary_name);
    let output_dir = Path::new("dist").join(&manifest.name).join(&target);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_binary = output_dir.join(&binary_name);
    fs::copy(&source_binary, &output_binary).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source_binary.display(),
            output_binary.display()
        )
    })?;

    let receipt = BuildReceipt {
        package: &manifest.name,
        binary: &manifest.binary,
        description: &manifest.description,
        mode: &manifest.mode,
        primary_core: &manifest.primary_core,
        install_hint: &manifest.install_hint,
        target: target.clone(),
        profile: profile.to_string(),
        features: manifest.features.clone(),
        source_binary: source_binary.display().to_string(),
        output_binary: output_binary.display().to_string(),
    };

    let receipt_path = output_dir.join("build-receipt.json");
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)
        .with_context(|| format!("failed to write {}", receipt_path.display()))?;

    println!("Built {} -> {}", manifest.name, output_binary.display());
    Ok(())
}

fn executor_options(capture_full_stdout: bool) -> Result<mgc_exec::run::ExecOptions> {
    Ok(mgc_exec::run::ExecOptions {
        cwd: Some(std::env::current_dir()?),
        log_path: Some(Path::new(".magicore").join("exec.log")),
        execution_scope: Some(mgc_exec::allowlist::ExecutionScope::BuildRunner),
        capture_full_stdout,
        ..Default::default()
    })
}

fn detect_host_target() -> Result<String> {
    let options = executor_options(true)?;
    let report = mgc_exec::run::run("rustc", &["-vV".to_string()], &options)
        .context("failed to query rustc host target through mgc-exec")?;
    report
        .stdout_full
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("rustc -vV output did not contain a host target"))
}
