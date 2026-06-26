use megagate_types::error::{MegagateError, Result};
use megagate_types::package::PackageRef;
use megagate_types::store::{IntegrityInfo, StoreBackend};
use ring::digest::{Context, SHA512};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Extractor {
    store: Arc<dyn StoreBackend>,
}

impl Extractor {
    pub fn new(store: Arc<dyn StoreBackend>) -> Self {
        Self { store }
    }

    pub async fn extract_tarball(&self, pkg: &PackageRef, data: &[u8]) -> Result<IntegrityInfo> {
        let mut sha512 = Context::new(&SHA512);
        let mut sha256 = Sha256::new();
        let size = data.len() as u64;

        sha512.update(data);
        sha256.update(data);

        let integrity = format!("sha512-{}", hex::encode(sha512.finish().as_ref()));

        let extract_path = self.store.get_path(pkg).await?;
        tokio::fs::create_dir_all(&extract_path).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        let tarball_path = extract_path.join("package.tgz");
        tokio::fs::write(&tarball_path, data).await
            .map_err(|e| MegagateError::IoError(e.to_string()))?;

        Ok(IntegrityInfo { integrity, size })
    }

    pub async fn verify_integrity(&self, pkg: &PackageRef, _expected: &str) -> Result<bool> {
        Ok(self.store.verify_integrity(pkg).await.unwrap_or(false))
    }
}

pub async fn extract_tarball_bytes(data: &[u8], dest: &PathBuf) -> Result<(String, u64)> {
    let mut sha512 = Context::new(&SHA512);
    let size = data.len() as u64;
    sha512.update(data);

    tokio::fs::create_dir_all(dest).await
        .map_err(|e| MegagateError::IoError(e.to_string()))?;

    let tarball_path = dest.join("package.tgz");
    tokio::fs::write(&tarball_path, data).await
        .map_err(|e| MegagateError::IoError(e.to_string()))?;

    let integrity = format!("sha512-{}", hex::encode(sha512.finish().as_ref()));
    Ok((integrity, size))
}