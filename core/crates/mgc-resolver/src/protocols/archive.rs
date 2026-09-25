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
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static EXTRACT_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_TAR_ENTRY_UNCOMPRESSED: u64 = 256 * 1024 * 1024;
const MAX_TAR_TOTAL_UNCOMPRESSED: u64 = 1024 * 1024 * 1024;

/// Extract a gzip-compressed tarball (held in memory) into `dest`.
/// Giải nén tarball gzip (nằm trong bộ nhớ) vào `dest`.
pub fn extract_tar_gz(bytes: &[u8], dest: &Path) -> MgResult<()> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);

    let dest_root = ensure_extract_root(dest)?;
    let mut total_uncompressed = 0_u64;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel_path = sanitize_archive_path(entry.path()?.as_ref())?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(MgError::Other(format!(
                "tar links are not allowed: {}",
                rel_path.display()
            )));
        }
        let target = if entry_type.is_dir() {
            ensure_extract_directory(&dest_root, &rel_path)?
        } else {
            ensure_extract_file_target(&dest_root, &rel_path)?
        };

        if !target.starts_with(&dest_root) {
            return Err(MgError::Other(format!(
                "tar entry escapes destination: {}",
                target.display()
            )));
        }

        if entry_type.is_dir() {
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

        let size = entry.size();
        if size > MAX_TAR_ENTRY_UNCOMPRESSED {
            return Err(MgError::Other(format!(
                "tar entry '{}' exceeds the per-entry size limit",
                rel_path.display()
            )));
        }
        total_uncompressed = total_uncompressed
            .checked_add(size)
            .ok_or_else(|| MgError::Other("tar total uncompressed size overflow".to_string()))?;
        if total_uncompressed > MAX_TAR_TOTAL_UNCOMPRESSED {
            return Err(MgError::Other(
                "tar total uncompressed size exceeds the safety limit".to_string(),
            ));
        }

        let mut contents = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut contents)?;
        write_extracted_file(&target, &contents)?;
    }

    Ok(())
}

pub(crate) fn ensure_extract_root(dest: &Path) -> MgResult<PathBuf> {
    match std::fs::symlink_metadata(dest) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(MgError::Integrity(format!(
                "refusing linked or non-directory archive extraction root: {}",
                dest.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dest)?;
        }
        Err(error) => return Err(error.into()),
    }
    let metadata = std::fs::symlink_metadata(dest)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(MgError::Integrity(format!(
            "archive extraction root changed to a link or non-directory: {}",
            dest.display()
        )));
    }
    Ok(dest.canonicalize()?)
}

/// Create each parent directory one component at a time and reject any
/// symlink/reparse-like component already present below the extraction root.
/// This prevents a valid archive path such as `nested/file` from following a
/// pre-planted `nested -> outside` link. This is defense against pre-existing
/// links; directory-handle/openat hardening for a concurrent same-user path
/// swap remains a separate platform-specific layer.
/// Tạo parent từng thành phần và từ chối symlink có sẵn dưới extraction root.
pub(crate) fn ensure_extract_file_target(root: &Path, relative: &Path) -> MgResult<PathBuf> {
    let (parent, filename) = split_safe_relative(relative)?;
    let mut current = root.to_path_buf();
    for component in parent {
        current.push(component);
        ensure_directory_component(&current)?;
    }
    current.push(filename);
    match std::fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(MgError::Integrity(format!(
                "refusing archive file target symlink: {}",
                current.display()
            )));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(MgError::Integrity(format!(
                "refusing non-file archive target: {}",
                current.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(current)
}

/// Ensure an archive directory is a real directory, never a symlink.
/// Đảm bảo thư mục archive là thư mục thật, không phải symlink.
pub(crate) fn ensure_extract_directory(root: &Path, relative: &Path) -> MgResult<PathBuf> {
    let (parent, filename) = split_safe_relative(relative)?;
    let mut current = root.to_path_buf();
    for component in parent {
        current.push(component);
        ensure_directory_component(&current)?;
    }
    current.push(filename);
    ensure_directory_component(&current)?;
    Ok(current)
}

fn split_safe_relative(relative: &Path) -> MgResult<(Vec<std::ffi::OsString>, std::ffi::OsString)> {
    let components: Vec<_> = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => Ok(part.to_os_string()),
            _ => Err(MgError::Integrity(format!(
                "unsafe archive path component: {}",
                relative.display()
            ))),
        })
        .collect::<MgResult<_>>()?;
    let (filename, parent) = components
        .split_last()
        .ok_or_else(|| MgError::Integrity("empty archive relative path".to_string()))?;
    Ok((parent.to_vec(), filename.clone()))
}

fn ensure_directory_component(path: &Path) -> MgResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(MgError::Integrity(format!(
                "refusing linked or non-directory archive parent: {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::create_dir(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure_directory_component(path)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Write extracted bytes via a unique same-directory temp and rename so the
/// final path is not opened through a swapped symlink. Windows uses an
/// atomic replace primitive rather than remove-then-rename.
/// Ghi byte qua temp duy nhất cùng thư mục rồi rename để không mở file đích
/// qua symlink bị thay giữa chừng.
pub(crate) fn write_extracted_file(path: &Path, contents: &[u8]) -> MgResult<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .ok_or_else(|| MgError::Integrity("archive target has no parent directory".to_string()))?;
    let stem = path
        .file_name()
        .ok_or_else(|| MgError::Integrity("archive target has no filename".to_string()))?;
    let mut temp_path = None;
    let mut temp_file = None;
    for _ in 0..32 {
        let sequence = EXTRACT_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.mgc-extract-{}-{sequence}.tmp",
            stem.to_string_lossy(),
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let temp_path = temp_path.ok_or_else(|| {
        MgError::Other("could not allocate unique archive extraction temp".to_string())
    })?;
    let result = (|| {
        let mut file = temp_file.expect("temp path and file are created together");
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        atomic_publish_extracted_file(&temp_path, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(not(windows))]
fn atomic_publish_extracted_file(temp_path: &Path, path: &Path) -> MgResult<()> {
    std::fs::rename(temp_path, path)?;
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn atomic_publish_extracted_file(temp_path: &Path, path: &Path) -> MgResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from: Vec<u16> = temp_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both nul-terminated path buffers remain alive for the synchronous call.
    // (AN TOÀN: hai buffer đường dẫn kết thúc nul còn sống trong lời gọi đồng bộ.)
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn tar_gz_file(path: &str, bytes: &[u8]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, Cursor::new(bytes))
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn tar_extraction_refuses_preexisting_symlink_parent() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("nested")).unwrap();
        let archive = tar_gz_file("nested/pwned.txt", b"outside");

        let result = extract_tar_gz(&archive, root.path());

        assert!(result.is_err(), "extractor must refuse a linked parent");
        assert!(
            !outside.path().join("pwned.txt").exists(),
            "tar data must never be written through a pre-existing symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tar_extraction_refuses_symlink_root() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let linked_root = parent.path().join("linked-root");
        symlink(outside.path(), &linked_root).unwrap();
        let archive = tar_gz_file("pwned.txt", b"outside");

        assert!(extract_tar_gz(&archive, &linked_root).is_err());
        assert!(!outside.path().join("pwned.txt").exists());
    }

    #[test]
    fn extracted_files_replace_regular_files_without_leaking_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("file.txt");
        std::fs::write(&dest, b"old").unwrap();

        write_extracted_file(&dest, b"new").unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "successful publish leaves no extraction temp"
        );
    }

    #[test]
    fn failed_extracted_file_publish_preserves_destination_and_cleans_temp() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("existing-dir");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("keep.txt"), b"original").unwrap();

        assert!(write_extracted_file(&dest, b"replacement").is_err());

        assert_eq!(std::fs::read(dest.join("keep.txt")).unwrap(), b"original");
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            1,
            "failed publish must not leave an extraction temp"
        );
    }
}
