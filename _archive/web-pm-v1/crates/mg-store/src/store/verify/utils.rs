use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::store::index::StoreError;

pub fn verify_file_integrity(path: &Path, expected_hash: &str) -> Result<bool, StoreError> {
    let meta = fs::metadata(path).map_err(|e| StoreError::Io {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;
    if !meta.is_file() {
        return Ok(false);
    }

    let mut file = fs::File::open(path).map_err(|e| StoreError::Io {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = file.read(&mut buf).map_err(|e| StoreError::Io {
            path: path.to_path_buf(),
            msg: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let computed = hex::encode(hasher.finalize());
    Ok(computed == expected_hash)
}
