//! Binary lockfile serialization (mgpm.lockb)
//!
//! Fast load/dump with bincode + custom header

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use bincode;

use super::{Lockfile, LOCKFILE_MAGIC, LOCKFILE_VERSION};
use crate::LockfileError;

#[allow(dead_code)]
const BINARY_HEADER_SIZE: usize = 16;

pub fn write_binary(lockfile: &Lockfile, path: &Path) -> Result<(), LockfileError> {
    let file = File::create(path)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    let mut writer = BufWriter::new(file);

    writer.write_all(LOCKFILE_MAGIC)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    let version_bytes = LOCKFILE_VERSION.to_le_bytes();
    writer.write_all(&version_bytes)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    let reserved: [u8; 8] = [0; 8];
    writer.write_all(&reserved)
        .map_err(|e| LockfileError::Io(e.to_string()))?;

    let encoded: Vec<u8> = bincode::serialize(lockfile)
        .map_err(|e| LockfileError::Serialization(e.to_string()))?;
    
    let len_bytes = (encoded.len() as u64).to_le_bytes();
    writer.write_all(&len_bytes)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    writer.write_all(&encoded)
        .map_err(|e| LockfileError::Io(e.to_string()))?;

    writer.flush()
        .map_err(|e| LockfileError::Io(e.to_string()))?;

    Ok(())
}

pub fn read_binary(path: &Path) -> Result<Lockfile, LockfileError> {
    let file = File::open(path)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    if &magic != LOCKFILE_MAGIC {
        return Err(LockfileError::InvalidMagic);
    }

    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    let version = u32::from_le_bytes(version_bytes);
    if version != LOCKFILE_VERSION {
        return Err(LockfileError::VersionMismatch { 
            found: version, 
            expected: LOCKFILE_VERSION 
        });
    }

    let mut reserved = [0u8; 8];
    reader.read_exact(&mut reserved)
        .map_err(|e| LockfileError::Io(e.to_string()))?;

    let mut len_bytes = [0u8; 8];
    reader.read_exact(&mut len_bytes)
        .map_err(|e| LockfileError::Io(e.to_string()))?;
    
    let len = u64::from_le_bytes(len_bytes) as usize;

    let mut encoded = vec![0u8; len];
    reader.read_exact(&mut encoded)
        .map_err(|e| LockfileError::Io(e.to_string()))?;

    let lockfile: Lockfile = bincode::deserialize(&encoded)
        .map_err(|e| LockfileError::Deserialization(e.to_string()))?;

    Ok(lockfile)
}

#[derive(Debug, thiserror::Error)]
pub enum BinaryLockfileError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("deserialization error: {0}")]
    Deserialization(String),
    #[error("invalid magic number")]
    InvalidMagic,
    #[error("version mismatch: found {found}, expected {expected}")]
    VersionMismatch { found: u32, expected: u32 },
}

impl From<BinaryLockfileError> for crate::LockfileError {
    fn from(e: BinaryLockfileError) -> Self {
        match e {
            BinaryLockfileError::Io(s) => crate::LockfileError::Io(s),
            BinaryLockfileError::Serialization(s) => crate::LockfileError::Serialization(s),
            BinaryLockfileError::Deserialization(s) => crate::LockfileError::Deserialization(s),
            BinaryLockfileError::InvalidMagic => crate::LockfileError::InvalidMagic,
            BinaryLockfileError::VersionMismatch { found, expected } => {
                crate::LockfileError::VersionMismatch { found, expected }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mgpm.lockb");
        
        let mut lock = Lockfile::new(1, "npm");
        lock.add_package(super::super::LockfilePackage {
            id: "react@18.0.0".to_string(),
            name: "react".to_string(),
            version: "18.0.0".to_string(),
            resolution: crate::PackageResolution {
                r#type: "registry".to_string(),
                url: "https://registry.npmjs.org/react/-/react-18.0.0.tgz".to_string(),
                registry: Some("npm".to_string()),
            },
            integrity: Some("sha512-...".to_string()),
        });
        
        write_binary(&lock, &path).unwrap();
        let loaded = read_binary(&path).unwrap();
        
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].name, "react");
        assert_eq!(loaded.packages[0].version, "18.0.0");
    }
}