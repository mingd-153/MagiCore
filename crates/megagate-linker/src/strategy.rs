use async_trait::async_trait;
use megagate_types::package::PackageRef;
use megagate_types::error::{MegagateError, Result};
use std::path::PathBuf;

#[async_trait]
pub trait LinkStrategy: Send + Sync {
    async fn link(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()>;
    async fn unlink(&self, target: &PathBuf) -> Result<()>;
    fn name(&self) -> &'static str;
}

pub struct HardlinkStrategy;

#[async_trait]
impl LinkStrategy for HardlinkStrategy {
    async fn link(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()> {
        let source = self.get_store_path(pkg)?;
        eprintln!("DEBUG: HardlinkStrategy linking {} -> {}", source.display(), target.display());
        eprintln!("DEBUG: source exists: {}", std::path::Path::new(&source).exists());
        tokio::fs::hard_link(&source, target).await
            .map_err(|e| {
                eprintln!("DEBUG: hard_link error: {}", e);
                MegagateError::IoError(e.to_string())
            })
    }

    async fn unlink(&self, target: &PathBuf) -> Result<()> {
        tokio::fs::remove_file(target).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "hardlink"
    }
}

impl HardlinkStrategy {
    fn get_store_path(&self, pkg: &PackageRef) -> Result<PathBuf> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".megagate")
            .join("store")
            .join("v1")
            .join("nodes")
            .join(&pkg.name)
            .join(pkg.version.to_string());
        eprintln!("DEBUG: HardlinkStrategy get_store_path for {}@{}: {}", pkg.name, pkg.version, path.display());
        Ok(path)
    }
}

pub struct SymlinkStrategy;

#[async_trait]
impl LinkStrategy for SymlinkStrategy {
    async fn link(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()> {
        let source = self.get_store_path(pkg)?;
        eprintln!("DEBUG: SymlinkStrategy linking {} -> {}", source.display(), target.display());
        eprintln!("DEBUG: source exists: {}", std::path::Path::new(&source).exists());
        tokio::fs::symlink(&source, target).await
            .map_err(|e| {
                eprintln!("DEBUG: symlink error: {}", e);
                MegagateError::IoError(e.to_string())
            })
    }

    async fn unlink(&self, target: &PathBuf) -> Result<()> {
        tokio::fs::remove_file(target).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "symlink"
    }
}

impl SymlinkStrategy {
    fn get_store_path(&self, pkg: &PackageRef) -> Result<PathBuf> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".megagate")
            .join("store")
            .join("v1")
            .join("nodes")
            .join(&pkg.name)
            .join(pkg.version.to_string());
        eprintln!("DEBUG: SymlinkStrategy get_store_path for {}@{}: {}", pkg.name, pkg.version, path.display());
        Ok(path)
    }
}

pub struct CopyStrategy;

#[async_trait]
impl LinkStrategy for CopyStrategy {
    async fn link(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()> {
        let source = self.get_store_path(pkg)?;
        tokio::task::block_in_place(|| {
            copy_dir_all(&source, target).map_err(|e| MegagateError::IoError(e.to_string()))
        })
    }

    async fn unlink(&self, target: &PathBuf) -> Result<()> {
        tokio::fs::remove_dir_all(target).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    fn name(&self) -> &'static str {
        "copy"
    }
}

impl CopyStrategy {
    fn get_store_path(&self, pkg: &PackageRef) -> Result<PathBuf> {
        Ok(dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".megagate")
            .join("store")
            .join("v1")
            .join("nodes")
            .join(&pkg.name)
            .join(pkg.version.to_string()))
    }
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    let mut stack = vec![(src.clone(), dst.clone())];
    while let Some((src_path, dst_path)) = stack.pop() {
        std::fs::create_dir_all(&dst_path)?;
        for entry in std::fs::read_dir(&src_path)? {
            let entry = entry?;
            let src_entry = entry.path();
            let dst_entry = dst_path.join(entry.file_name());
            if src_entry.is_dir() {
                stack.push((src_entry, dst_entry));
            } else {
                std::fs::copy(&src_entry, &dst_entry)?;
            }
        }
    }
    Ok(())
}