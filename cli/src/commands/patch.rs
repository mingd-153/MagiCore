//! Patch command — add/rm/ls/verify package patches (16 §6)
//! (Lệnh patch: vá lỗi package như pnpm patchedDependencies)

use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use mgc_config::project::ProjectConfig;
use mgc_resolver::patches::get_patches_dir;
use mgc_types::PatchSpec;
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_PATCH_VERIFY_BYTES: u64 = 64 * 1024 * 1024;

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
    let project_root =
        ProjectConfig::find_project_root(&cwd).ok_or_else(crate::error::project_root_missing)?;
    run_at_project_root(args, &project_root).await
}

async fn run_at_project_root(args: PatchArgs, project_root: &Path) -> Result<()> {
    match args.cmd {
        PatchCmd::Add {
            package,
            file,
            range,
        } => {
            let lock = acquire_patch_mutation_lock(project_root)?;
            let mut project =
                ProjectConfig::load(project_root)?.ok_or_else(crate::error::mgc_toml_missing)?;
            add_patch(&mut project, project_root, &package, &file, range, &lock).await
        }
        PatchCmd::Remove { package } => {
            let _lock = acquire_patch_mutation_lock(project_root)?;
            let mut project =
                ProjectConfig::load(project_root)?.ok_or_else(crate::error::mgc_toml_missing)?;
            remove_patch(&mut project, project_root, &package).await
        }
        PatchCmd::List => {
            let project =
                ProjectConfig::load(project_root)?.ok_or_else(crate::error::mgc_toml_missing)?;
            list_patches(&project).await
        }
        PatchCmd::Verify => {
            let project =
                ProjectConfig::load(project_root)?.ok_or_else(crate::error::mgc_toml_missing)?;
            verify_patches(&project, project_root).await
        }
    }
}

fn acquire_patch_mutation_lock(
    project_root: &Path,
) -> Result<mgc_lockfile::project_lock::ProjectWriteLock> {
    let lock = mgc_lockfile::project_lock::ProjectWriteLock::acquire(
        project_root,
        crate::commands::core::shared::writer_lock_timeout(project_root),
    )
    .map_err(|error| anyhow::anyhow!("patch cannot acquire the project writer lock: {error}"))?;
    crate::commands::core::shared::ensure_no_pending_remove_journal(project_root, &lock)?;
    Ok(lock)
}

async fn add_patch(
    project: &mut ProjectConfig,
    project_root: &Path,
    package: &str,
    file: &str,
    range: Option<String>,
    lock: &mgc_lockfile::project_lock::ProjectWriteLock,
) -> Result<()> {
    let package = mgc_types::PackageName::new(package.to_string())?;
    let version_range = range
        .map(|range| mgc_types::VersionRange::parse(&range))
        .transpose()?
        .unwrap_or_else(mgc_types::VersionRange::star);
    if project
        .patches
        .iter()
        .any(|patch| patch.package == package.as_str() && patch.version_range == version_range)
    {
        return Err(crate::error::patch_duplicate_identity());
    }

    let content = read_patch_source_bounded(Path::new(file))?;
    let integrity = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&content);
        format!("sha256-{}", hex::encode(h.finalize()))
    };

    // Copy patch to patches dir
    let patches_dir = get_patches_dir(Some(project_root))?;
    ensure_patch_directory(project_root, &patches_dir)?;
    let mut identity = sha2::Sha256::new();
    use sha2::Digest;
    identity.update(package.as_str().as_bytes());
    identity.update([0]);
    identity.update(version_range.as_str().as_bytes());
    let dest_name = format!("patch-{}.patch", hex::encode(identity.finalize()));
    let dest = patches_dir.join(&dest_name);
    mgc_lockfile::atomic::atomic_write_locked(
        lock,
        &dest,
        &content,
        std::time::Duration::from_secs(60),
    )?;

    // Add to project patches
    let spec = PatchSpec::new(package.to_string(), version_range, dest_name, integrity);

    // Store in mgc.toml [patches]
    project.patches.push(spec);
    project.save(project_root)?;

    println!(
        "Recorded patch configuration for {} -> {}; install will fail closed until patch application is supported",
        package,
        dest.display()
    );
    Ok(())
}

fn read_patch_source_bounded(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(crate::error::patch_path_rejected(
            "patch source is not a regular file",
        ));
    }
    if file.metadata()?.len() > MAX_PATCH_VERIFY_BYTES {
        return Err(crate::error::patch_path_rejected(
            "patch source exceeds the 64 MiB limit",
        ));
    }
    let mut content = Vec::new();
    file.take(MAX_PATCH_VERIFY_BYTES + 1)
        .read_to_end(&mut content)?;
    if content.len() as u64 > MAX_PATCH_VERIFY_BYTES {
        return Err(crate::error::patch_path_rejected(
            "patch source grew beyond the 64 MiB limit while reading",
        ));
    }
    Ok(content)
}

fn ensure_patch_directory(project_root: &Path, patches_dir: &Path) -> Result<()> {
    let expected = project_root.join(".magicore").join("patches");
    if patches_dir != expected {
        bail!(
            "refusing unexpected project patch directory: {}",
            patches_dir.display()
        );
    }
    if mgc_lockfile::project_lock::path_is_link_or_reparse(&expected) {
        bail!(
            "refusing symlink/reparse project patch directory: {}",
            expected.display()
        );
    }
    match fs::symlink_metadata(&expected) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => bail!(
            "project patch path is not a real directory: {}",
            expected.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&expected)?;
            let metadata = fs::symlink_metadata(&expected)?;
            if metadata.file_type().is_dir()
                && !mgc_lockfile::project_lock::path_is_link_or_reparse(&expected)
            {
                Ok(())
            } else {
                bail!(
                    "project patch path changed during creation: {}",
                    expected.display()
                )
            }
        }
        Err(error) => Err(error.into()),
    }
}

async fn remove_patch(
    project: &mut ProjectConfig,
    project_root: &Path,
    package: &str,
) -> Result<()> {
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
    println!("Configured patches (not applied by install):");
    for p in &project.patches {
        println!(
            "  {} @ {} -> {} ({})",
            p.package, p.version_range, p.patch_path, p.integrity
        );
    }
    Ok(())
}

async fn verify_patches(project: &ProjectConfig, project_root: &Path) -> Result<()> {
    let patches_dir = get_patches_dir(Some(project_root))?;
    let mut all_ok = true;

    for spec in &project.patches {
        let patch_path = resolve_project_patch_path(project_root, &patches_dir, &spec.patch_path)?;
        match verify_patch_file_no_follow(&patch_path, &spec.integrity) {
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

fn resolve_project_patch_path(
    project_root: &Path,
    patches_dir: &Path,
    patch_name: &str,
) -> Result<PathBuf> {
    if patch_name.is_empty()
        || patch_name.contains('/')
        || patch_name.contains('\\')
        || Path::new(patch_name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(crate::error::patch_path_rejected(
            "path must be one filename inside the project patch store",
        ));
    }

    let project_root = project_root.canonicalize()?;
    let magicore_dir = project_root.join(".magicore");
    let metadata = fs::symlink_metadata(&magicore_dir)?;
    if !metadata.file_type().is_dir()
        || mgc_lockfile::project_lock::path_is_link_or_reparse(&magicore_dir)
    {
        return Err(crate::error::patch_path_rejected(
            "project metadata directory is symlinked or invalid",
        ));
    }

    let canonical_patches = patches_dir.canonicalize()?;
    let expected_patches = magicore_dir.join("patches");
    if canonical_patches != expected_patches {
        return Err(crate::error::patch_path_rejected(
            "project patch store resolves outside its expected directory",
        ));
    }
    let metadata = fs::symlink_metadata(patches_dir)?;
    if !metadata.file_type().is_dir()
        || mgc_lockfile::project_lock::path_is_link_or_reparse(patches_dir)
    {
        return Err(crate::error::patch_path_rejected(
            "project patch directory is symlinked or invalid",
        ));
    }

    let patch_path = patches_dir.join(patch_name);
    let metadata = fs::symlink_metadata(&patch_path)?;
    if !metadata.file_type().is_file()
        || mgc_lockfile::project_lock::path_is_link_or_reparse(&patch_path)
    {
        return Err(crate::error::patch_path_rejected(
            "patch target is symlinked or not a regular file",
        ));
    }
    Ok(patch_path)
}

fn verify_patch_file_no_follow(path: &Path, expected: &str) -> Result<bool> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    use std::io::Read;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(crate::error::patch_path_rejected(
            "patch target is not a regular file",
        ));
    }
    if metadata.len() > MAX_PATCH_VERIFY_BYTES {
        return Err(crate::error::patch_path_rejected(
            "patch file exceeds the 64 MiB verification limit",
        ));
    }
    let mut reader = file.take(MAX_PATCH_VERIFY_BYTES + 1);
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_PATCH_VERIFY_BYTES {
            return Err(crate::error::patch_path_rejected(
                "patch file grew beyond the 64 MiB verification limit while reading",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex::encode(hasher.finalize());
    let expected = expected.strip_prefix("sha256-").unwrap_or(expected);
    Ok(actual == expected)
}

#[cfg(test)]
#[path = "test/patch.rs"]
mod tests;
