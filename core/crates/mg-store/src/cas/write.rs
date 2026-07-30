use std::fs;
use std::io::{Read, Write};
use std::path::Path;


use super::integrity::IntegrityHash;
use super::store::StoreError;

/// Files larger than this threshold will be streamed through a buffered reader
/// instead of being fully loaded into memory.
pub const STREAM_THRESHOLD: usize = 1024 * 1024; // 1 MB

/// Write data to file and set permissions.
///
/// CAS entries are reproducible cache artifacts, so we avoid per-file fsync and
/// read-back verification here. The content hash is already computed from the
/// source bytes before writing, and failed/corrupt cache entries can be rebuilt.
pub fn write_all_verify_and_set_perms(
    mut writer: fs::File,
    dest: &Path,
    data: &[u8],
    executable: bool,
) -> Result<IntegrityHash, StoreError> {
    let expected = IntegrityHash::from_bytes(data, executable);

    writer.write_all(data).map_err(|e| StoreError::Io {
        path: dest.to_path_buf(),
        msg: format!("write failed: {e}"),
    })?;

    set_permissions(dest, executable)?;

    Ok(expected)
}

/// Stream data from reader to writer while computing the content hash.
pub fn stream_write_verify_and_set_perms(
    mut writer: fs::File,
    dest: &Path,
    mut reader: impl Read,
    executable: bool,
) -> Result<IntegrityHash, StoreError> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = reader.read(&mut buf).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("read failed: {e}"),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n]).map_err(|e| StoreError::Io {
            path: dest.to_path_buf(),
            msg: format!("write failed: {e}"),
        })?;
    }

    let hash = hasher.finalize().to_hex().to_string();
    let integrity = IntegrityHash { hash, executable };

    set_permissions(dest, executable)?;

    Ok(integrity)
}

fn set_permissions(path: &Path, executable: bool) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| {
            StoreError::Io {
                path: path.to_path_buf(),
                msg: format!("set permissions failed: {e}"),
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_all_verify() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let writer = fs::File::create(&path).unwrap();
        let result = write_all_verify_and_set_perms(writer, &path, b"hello world", false);
        assert!(result.is_ok());
        assert!(path.exists());
    }
}
