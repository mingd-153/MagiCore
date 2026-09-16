//! Safe tar.gz extraction shared by the crates and pub materializers.
//! Giải nén tar.gz an toàn dùng chung cho materializer crates và pub.
//!
//! Mirrors mgc-fetcher's extraction safety contract: entries are validated
//! before unpacking so a malicious archive cannot write outside `dest` or
//! create links/special files (path traversal + symlink escape are blocked).
//! Phản chiếu hợp đồng an toàn của mgc-fetcher: entry được validate trước
//! khi giải nén để archive độc hại không thể ghi ra ngoài `dest` hay tạo
//! link/special file (chặn path traversal + symlink escape).

use mgc_types::{MgError, MgResult};
use std::path::{Component, Path, PathBuf};

/// Extract a gzip-compressed tarball (held in memory) into `dest`.
/// Giải nén tarball gzip (nằm trong bộ nhớ) vào `dest`.
pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> MgResult<()> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);

    std::fs::create_dir_all(dest)?;
    let dest_root = dest.canonicalize()?;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel_path = sanitize_archive_path(entry.path()?.as_ref())?;
        let target = dest_root.join(rel_path);

        if !target.starts_with(&dest_root) {
            return Err(MgError::Other(format!(
                "tar entry escapes destination: {}",
                target.display()
            )));
        }

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(MgError::Other(format!(
                "tar links are not allowed: {}",
                target.display()
            )));
        }

        if entry_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }

        // pax metadata entries are archive bookkeeping, not package files.
        // (Entry metadata pax là bookkeeping archive, không phải file package.)
        if matches!(entry_type.as_byte(), b'g' | b'x') {
            continue;
        }

        if !entry_type.is_file() {
            return Err(MgError::Other(format!(
                "unsupported tar entry type for {}",
                target.display()
            )));
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }

    Ok(())
}

/// Reject any path component that would escape the destination root.
/// Từ chối mọi thành phần path có thể thoát khỏi gốc đích.
fn sanitize_archive_path(path: &Path) -> MgResult<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(MgError::Other(format!(
                    "unsafe tar entry path: {}",
                    path.display()
                )));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(MgError::Other("empty tar entry path".to_string()));
    }
    Ok(clean)
}
