use async_trait::async_trait;
use flate2::read::GzDecoder;
use megagate_types::error::{MegagateError, Result};
use megagate_types::package::{PackageManifest, PackageRef};
use megagate_types::store::{IntegrityInfo, PackageMetadata, PruneResult, StoreBackend};
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tar::Archive;

pub struct FsStoreBackend {
    base_path: PathBuf,
    files_path: PathBuf,
    nodes_path: PathBuf,
}

impl FsStoreBackend {
    pub fn new(base_path: PathBuf) -> Self {
        let files_path = base_path.join("v1").join("files");
        let nodes_path = base_path.join("v1").join("nodes");
        Self {
            base_path,
            files_path,
            nodes_path,
        }
    }

    fn tarball_path(&self, pkg: &PackageRef) -> PathBuf {
        self.files_path.join(format!("{}-{}.tgz", pkg.name, pkg.version))
    }

    fn integrity_path(&self, pkg: &PackageRef) -> PathBuf {
        self.files_path.join(format!("{}-{}.tgz.sha512", pkg.name, pkg.version))
    }

    fn node_path(&self, pkg: &PackageRef) -> PathBuf {
        self.nodes_path.join(&pkg.name).join(pkg.version.to_string())
    }

    fn metadata_path(&self, pkg: &PackageRef) -> PathBuf {
        self.node_path(pkg).join(".megagate-meta.json")
    }
}

#[async_trait]
impl StoreBackend for FsStoreBackend {
    async fn init(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.files_path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        tokio::fs::create_dir_all(&self.nodes_path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        Ok(())
    }

    async fn get_path(&self, pkg: &PackageRef) -> Result<PathBuf> {
        Ok(self.node_path(pkg))
    }

    async fn write_tarball_bytes(&self, pkg: &PackageRef, data: &[u8]) -> Result<IntegrityInfo> {
        let tarball_path = self.tarball_path(pkg);

        let mut hasher = Sha512::new();
        hasher.update(data);
        let size = data.len() as u64;
        let integrity = format!("sha512-{}", hex::encode(hasher.finalize()));

        eprintln!("DEBUG: Writing tarball for {}@{} to {}", pkg.name, pkg.version, tarball_path.display());
        tokio::fs::write(&tarball_path, data).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        eprintln!("DEBUG: Wrote tarball, size: {}", size);

        let integrity_path = self.integrity_path(pkg);
        tokio::fs::write(&integrity_path, &integrity).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        Ok(IntegrityInfo { integrity, size })
    }

    async fn exists(&self, pkg: &PackageRef) -> Result<bool> {
        let tarball_path = self.tarball_path(pkg);
        let exists = tarball_path.exists();
        eprintln!("DEBUG: exists check for {}@{}: {} at {}", pkg.name, pkg.version, exists, self.tarball_path(pkg).display());
        Ok(exists)
    }

    async fn is_extracted(&self, pkg: &PackageRef) -> Result<bool> {
        let node_path = self.node_path(pkg);
        let exists = node_path.exists() && node_path.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
        eprintln!("DEBUG: is_extracted check for {}@{}: {} at {}", pkg.name, pkg.version, exists, node_path.display());
        Ok(exists)
    }

    async fn extract_tarball(&self, pkg: &PackageRef) -> Result<()> {
        let tarball_path = self.tarball_path(pkg);
        let node_path = self.node_path(pkg);
        
        eprintln!("DEBUG: Extracting tarball for {}@{} to {}", pkg.name, pkg.version, node_path.display());
        
        if !tarball_path.exists() {
            eprintln!("DEBUG: Tarball not found at {}", tarball_path.display());
            return Err(MegagateError::IoError("Tarball not found".to_string()));
        }
        
        fs::create_dir_all(&node_path)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        
        // Extract tarball using synchronous tar crate with gzip decompression
        let file = fs::File::open(&tarball_path)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        archive.unpack(&node_path)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        
        eprintln!("DEBUG: Unpacked to {}, node_path exists: {}", node_path.display(), node_path.exists());
        
        // The npm tarball extracts to a "package" subdirectory, move contents up
        let package_dir = node_path.join("package");
        eprintln!("DEBUG: package_dir exists: {}", package_dir.exists());
        if package_dir.exists() {
            let entries = fs::read_dir(&package_dir)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            
            for entry in entries {
                let entry = entry.map_err(|e| MegagateError::IoError(e.to_string()))?;
                let dest = self.node_path(pkg).join(entry.file_name());
                fs::rename(entry.path(), dest)
                    .map_err(|e| MegagateError::IoError(e.to_string()))?;
            }
            
            fs::remove_dir_all(&package_dir)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
        }
        
        Ok(())
    }

    async fn read_tarball_bytes(&self, pkg: &PackageRef) -> Result<Vec<u8>> {
        let path = self.tarball_path(pkg);
        tokio::fs::read(&path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn write_manifest(&self, pkg: &PackageRef, manifest: &PackageManifest) -> Result<()> {
        let node_path = self.node_path(pkg);
        tokio::fs::create_dir_all(&node_path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        let manifest_path = node_path.join("package.json");
        let content = serde_json::to_string_pretty(manifest)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        tokio::fs::write(&manifest_path, content).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn read_manifest(&self, pkg: &PackageRef) -> Result<Option<PackageManifest>> {
        let manifest_path = self.node_path(pkg).join("package.json");
        if manifest_path.exists() {
            let content = tokio::fs::read_to_string(&manifest_path).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            let manifest: PackageManifest = serde_json::from_str(&content)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    async fn write_metadata(&self, pkg: &PackageRef, meta: &PackageMetadata) -> Result<()> {
        let metadata_path = self.metadata_path(pkg);
        tokio::fs::create_dir_all(metadata_path.parent().unwrap()).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        let content = serde_json::to_string_pretty(meta)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        tokio::fs::write(&metadata_path, content).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn read_metadata(&self, pkg: &PackageRef) -> Result<Option<PackageMetadata>> {
        let metadata_path = self.metadata_path(pkg);
        if metadata_path.exists() {
            let content = tokio::fs::read_to_string(&metadata_path).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            let meta: PackageMetadata = serde_json::from_str(&content)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            Ok(Some(meta))
        } else {
            Ok(None)
        }
    }

    async fn create_hardlink(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()> {
        let source = self.node_path(pkg);
        tokio::fs::hard_link(&source, target).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn create_symlink(&self, pkg: &PackageRef, target: &PathBuf) -> Result<()> {
        let source = self.node_path(pkg);
        tokio::fs::symlink(&source, target).await
            .map_err(|e| MegagateError::IoError(e.to_string()))
    }

    async fn remove(&self, pkg: &PackageRef) -> Result<()> {
        let tarball = self.tarball_path(pkg);
        if tarball.exists() {
            tokio::fs::remove_file(tarball).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
        }
        let node = self.node_path(pkg);
        if node.exists() {
            tokio::fs::remove_dir_all(node).await
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
        }
        Ok(())
    }

    async fn prune(&self, referenced: HashMap<String, PackageRef>) -> Result<PruneResult> {
        let mut removed = 0u64;
        let mut freed_bytes = 0u64;

        let mut files_dir = tokio::fs::read_dir(&self.files_path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        while let Some(entry) = files_dir.next_entry().await
            .map_err(|e| MegagateError::IoError(e.to_string()))? {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".tgz") {
                let pkg_key = name.strip_suffix(".tgz").unwrap_or(&name);
                if !referenced.contains_key(pkg_key) {
                if let Ok(meta) = entry.metadata().await {
                    freed_bytes += meta.len();
                }
                tokio::fs::remove_file(entry.path()).await
                        .map_err(|e| MegagateError::IoError(e.to_string()))?;
                    removed += 1;
                }
            }
        }

        Ok(PruneResult {
            removed: removed as usize,
            freed_bytes,
        })
    }

    async fn verify_integrity(&self, pkg: &PackageRef) -> Result<bool> {
        let integrity_path = self.integrity_path(pkg);
        let tarball_path = self.tarball_path(pkg);

        if !integrity_path.exists() || !tarball_path.exists() {
            return Ok(false);
        }

        let expected = fs::read_to_string(&integrity_path)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        let mut file = fs::File::open(&tarball_path)
            .map_err(|e| MegagateError::IoError(e.to_string()))?;
        let mut hasher = Sha512::new();
        let mut buffer = [0u8; 8192];

        loop {
            let n = file.read(&mut buffer)
                .map_err(|e| MegagateError::IoError(e.to_string()))?;
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
        }

        let actual = format!("sha512-{}", hex::encode(hasher.finalize()));
        Ok(expected.trim() == actual)
    }
}