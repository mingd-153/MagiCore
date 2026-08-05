//! Patch command — add/rm/ls/verify package patches (16 §6)
//! (Lệnh patch: vá lỗi package như pnpm patchedDependencies)

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use mg_config::project::ProjectConfig;
use mg_resolver::patches::{apply_patch, get_patches_dir, verify_patch_integrity};
use mg_types::{LockPatch, PatchKind, PatchSpec};
use std::fs;
use std::path::Path;

#[derive(Args, Debug, Clone)]
pub struct PatchArgs {
    #[command(subcommand)]
    pub cmd: PatchCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PatchCmd {
    /// Add a patch for a package
    Add {
        #[arg(required = true, help = "Package name (e.g. react, @scope/pkg)")]
        package: String,
        #[arg(long, short = 'f', help = "Patch file (unified diff)")]
        file: String,
        #[arg(long, help = "Version range (e.g. 1.0.0 - 1.2.0)")]
        range: Option<String>,
    },
    /// Remove a patch for a package
    Remove {
        #[arg(required = true, help = "Package name")]
        package: String,
    },
    /// List active patches
    List,
    /// Verify patch integrity against lockfile
    Verify,
}

pub async fn run(args: PatchArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let project_root = ProjectConfig::find_project_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Project root not found — run mg init"))?;
    let mut project = ProjectConfig::load(&project_root)?
        .ok_or_else(|| anyhow::anyhow!("mg.toml not found — run mg init"))?;

    match args.cmd {
        PatchCmd::Add { package, file, range } => {
            add_patch(&mut project, &project_root, &package, &file, range).await
        }
        PatchCmd::Remove { package } => {
            remove_patch(&mut project, &project_root, &package).await
        }
        PatchCmd::List => list_patches(&project).await,
        PatchCmd::Verify => verify_patches(&project, &project_root).await,
    }
}

async fn add_patch(
    project: &mut ProjectConfig,
    project_root: &Path,
    package: &str,
    file: &str,
    range: Option<String>,
) -> Result<()> {
    let patch_path = Path::new(file);
    if !patch_path.exists() {
        bail!("Patch file not found: {}", file);
    }

    // Read patch content and compute SHA256
    let content = fs::read(patch_path)?;
    let integrity = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&content);
        format!("sha256-{}", hex::encode(h.finalize()))
    };

    // Copy patch to patches dir
    let patches_dir = get_patches_dir(Some(project_root))?;
    fs::create_dir_all(&patches_dir)?;
    let dest_name = format!("{}.patch", package.replace('/', "_"));
    let dest = patches_dir.join(&dest_name);
    fs::copy(patch_path, &dest)?;

    // Add to project patches
    let version_range = range
        .map(|r| mg_types::VersionRange::parse(&r))
        .transpose()?
        .unwrap_or_else(|| mg_types::VersionRange::parse("*").unwrap());

    let spec = PatchSpec::new(
        package.to_string(),
        version_range,
        dest_name,
        integrity,
    );

    // Store in mg.toml [patches]
    project.patches.push(spec);
    project.save(project_root)?;

    println!("Added patch for {} -> {}", package, dest.display());
    Ok(())
}

async fn remove_patch(project: &mut ProjectConfig, project_root: &Path, package: &str) -> Result<()> {
    let len_before = project.patches.len();
    project.patches.retain(|p| p.package != package);
    if project.patches.len() == len_before {
        bail!("No patch found for package: {}", package);
    }
    project.save(project_root)?;
    println!("Removed patch for {}", package);
    Ok(())
}

async fn list_patches(project: &ProjectConfig) -> Result<()> {
    if project.patches.is_empty() {
        println!("No patches configured");
        return Ok(());
    }
    println!("Active patches:");
    for p in &project.patches {
        println!("  {} @ {} -> {} ({})", p.package, p.version_range, p.patch_path, p.integrity);
    }
    Ok(())
}

async fn verify_patches(project: &ProjectConfig, project_root: &Path) -> Result<()> {
    let patches_dir = get_patches_dir(Some(project_root))?;
    let mut all_ok = true;

    for spec in &project.patches {
        let patch_path = patches_dir.join(&spec.patch_path);
        if !patch_path.exists() {
            println!("✗ {}: patch file missing ({})", spec.package, patch_path.display());
            all_ok = false;
            continue;
        }
        match verify_patch_integrity(&patch_path, &spec.integrity) {
            Ok(true) => println!("✓ {}: integrity OK", spec.package),
            Ok(false) => {
                println!("✗ {}: integrity MISMATCH", spec.package);
                all_ok = false;
            }
            Err(e) => {
                println!("✗ {}: error verifying — {}", spec.package, e);
                all_ok = false;
            }
        }
    }

    if !all_ok {
        bail!("Some patches failed verification");
    }
    println!("All patches verified");
    Ok(())
}
