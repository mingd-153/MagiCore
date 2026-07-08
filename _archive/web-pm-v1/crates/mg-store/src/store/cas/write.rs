use std::fs;
use std::io::{Read, Seek, Write, BufReader};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::integrity::IntegrityHash;
use super::super::index::StoreError;

pub const STREAM_THRESHOLD: usize = 1024 * 1024; // 1MB

/// Writes data, verifies it using the SAME file handle (no TOCTOU),
/// and sets executable bit if needed. All operations use the same file handle.
pub fn write_all_verify_and_set_perms(
    mut writer: fs::File,
    dest: &Path,
    data: &[u8],
    executable: bool,
) -> Result<(), StoreError> {
    // Write data
    writer.write_all(data).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;

    // Verify content using SAME file handle (no TOCTOU)
    verify_written(&mut writer, dest, data)?;

    // Set executable bit if needed
    apply_executable_bit(&writer, dest, executable)?;

    Ok(())
}

/// Streaming write for large files - reads from source, writes to dest, verifies on the fly.
pub fn stream_write_verify_and_set_perms<R: Read>(
    mut writer: fs::File,
    dest: &Path,
    mut reader: BufReader<R>,
    executable: bool,
) -> Result<IntegrityHash, StoreError> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8192];
    let mut _total_bytes = 0usize;

    loop {
        let n = reader.read(&mut buf).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n]).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: e.to_string(),
        })?;
        hasher.update(&buf[..n]);
        _total_bytes += n;
    }

    writer.flush().map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;

    let hash_str = hex::encode(hasher.finalize());
    let hash = IntegrityHash {
        hash: hash_str.clone(),
        shard: hash_str[..2].to_string(),
        filename: hash_str.clone(),
        is_executable: executable,
    };
    
    apply_executable_bit(&writer, dest, executable)?;

    Ok(hash)
}

fn verify_written(
    writer: &mut fs::File,
    dest: &Path,
    data: &[u8],
) -> Result<(), StoreError> {
    writer.flush().map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    writer
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: e.to_string(),
        })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    let mut read_bytes = 0usize;
    loop {
        let n = writer.read(&mut buf).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        read_bytes += n;
        hasher.update(&buf[..n]);
    }
    if read_bytes != data.len() {
        return Err(StoreError::IntegrityCheck(
            "size mismatch after write".into(),
        ));
    }
    let computed = hex::encode(hasher.finalize());
    let expected = hex::encode(Sha256::digest(data));
    if computed != expected {
        return Err(StoreError::IntegrityCheck(
            "hash mismatch after write".into(),
        ));
    }
    Ok(())
}

fn apply_executable_bit(
    writer: &fs::File,
    dest: &Path,
    executable: bool,
) -> Result<(), StoreError> {
    if !executable {
        return Ok(());
    }
    set_executable_bit(writer, dest)
}

#[cfg(unix)]
fn set_executable_bit(writer: &fs::File, dest: &Path) -> Result<(), StoreError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = writer.metadata().map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    let mode = meta.permissions().mode();
    let mut perms = meta.permissions();
    perms.set_mode(mode | 0o111);
    // Use the file handle to set permissions (no TOCTOU)
    writer.set_permissions(perms).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: e.to_string(),
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable_bit(_writer: &fs::File, _dest: &Path) -> Result<(), StoreError> {
    Ok(())
}
