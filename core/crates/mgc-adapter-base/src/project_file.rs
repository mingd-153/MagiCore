//! No-follow reads and atomic replacement for project-owned manifest files.
//! Dùng đọc không theo symlink và thay file atomic cho manifest do project quản lý.

use mgc_types::MgError;
use std::path::{Path, PathBuf};

fn error(message: impl Into<String>) -> MgError {
    MgError::Other(message.into())
}

fn reject_non_regular(path: &Path) -> Result<std::fs::Metadata, MgError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| error(format!("inspect manifest {}: {e}", path.display())))?;
    if !metadata.file_type().is_file() {
        return Err(error(format!(
            "refusing non-regular or symlink manifest: {}",
            path.display()
        )));
    }
    Ok(metadata)
}

/// Open the final path component without following a symlink/reparse point,
/// verify the opened handle is a regular file, and read from that handle.
pub fn read_regular_text(path: &Path, display_name: &str) -> Result<String, MgError> {
    use std::io::Read;

    // Refuse FIFOs and other special files before opening; O_NONBLOCK below
    // closes the replacement race between this check and the actual open.
    // Từ chối FIFO/file đặc biệt trước khi mở; O_NONBLOCK chặn race thay path.
    reject_non_regular(path)?;

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options
        .open(path)
        .map_err(|e| error(format!("open {display_name} without following links: {e}")))?;
    let metadata = file
        .metadata()
        .map_err(|e| error(format!("inspect opened {display_name}: {e}")))?;
    if !metadata.is_file() {
        return Err(error(format!("{display_name} is not a regular file")));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(error(format!(
                "refusing reparse-point manifest {display_name}"
            )));
        }
    }
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| error(format!("read {display_name}: {e}")))?;
    Ok(content)
}

/// Read a regular file below a trusted project root without traversing a
/// symlink/reparse point in any project-relative component.
/// Đọc file thường dưới project root tin cậy, không đi qua symlink/reparse.
pub fn read_project_regular_text(
    project_root: &Path,
    relative_path: &Path,
    display_name: &str,
) -> Result<String, MgError> {
    #[cfg(unix)]
    {
        read_project_regular_text_dirfd(project_root, relative_path, display_name)
    }
    #[cfg(not(unix))]
    {
        let path = checked_project_path(project_root, relative_path)?;
        read_regular_text(&path, display_name)
    }
}

/// Replace an existing regular manifest atomically from a unique staging
/// file on the same filesystem. The destination symlink (if swapped after
/// validation) is replaced as a directory entry, never followed as a target.
pub fn atomic_write_regular(path: &Path, bytes: &[u8]) -> Result<(), MgError> {
    use std::io::Write;

    let original_metadata = reject_non_regular(path)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| error(format!("invalid manifest path: {}", path.display())))?;
    let temp = unique_temp_path(parent, file_name);
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| error(format!("create manifest staging file: {e}")))?;
        file.set_permissions(original_metadata.permissions())
            .map_err(|e| error(format!("preserve manifest permissions: {e}")))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| error(format!("sync manifest staging file: {e}")))?;
        // Revalidate path immediately before atomic replacement. The OS
        // replacement primitive replaces the path entry; it does not open
        // the old file for truncation.
        reject_non_regular(path)?;
        mgc_lockfile::atomic::atomic_replace_file(&temp, path)
            .map_err(|e| error(format!("replace manifest {}: {e}", path.display())))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| error(format!("sync manifest directory: {e}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Atomically replace a regular file below a trusted project root. Every
/// existing project-relative parent is checked as a real directory first.
/// Thay file atomically dưới project root; kiểm tra từng thư mục cha hiện hữu.
pub fn atomic_write_project_regular(
    project_root: &Path,
    relative_path: &Path,
    bytes: &[u8],
) -> Result<(), MgError> {
    #[cfg(unix)]
    {
        atomic_write_project_regular_dirfd(project_root, relative_path, bytes)
    }
    #[cfg(not(unix))]
    {
        let path = checked_project_path(project_root, relative_path)?;
        atomic_write_regular(&path, bytes)
    }
}

#[cfg(unix)]
fn project_parent_dir(
    project_root: &Path,
    relative_path: &Path,
) -> Result<(std::fs::File, std::ffi::OsString), MgError> {
    use rustix::fs::{Mode, OFlags, openat};
    use std::os::fd::AsFd;
    use std::path::Component;

    let components: Vec<_> = relative_path.components().collect();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(error(format!(
            "refusing non-project-relative manifest path: {}",
            relative_path.display()
        )));
    }
    // Open the caller-selected root once and keep its handle as the authority
    // boundary. A symlink used to select the project root is intentional; all
    // components beneath the opened root are still traversed with NOFOLLOW.
    // (Mở project root một lần; symlink chọn root là chủ ý, các cấp dưới cấm.)
    let mut directory = std::fs::File::open(project_root)
        .map_err(|e| error(format!("open project root {}: {e}", project_root.display())))?;
    if !directory
        .metadata()
        .map_err(|e| {
            error(format!(
                "inspect project root {}: {e}",
                project_root.display()
            ))
        })?
        .is_dir()
    {
        return Err(error(format!(
            "project root is not a directory: {}",
            project_root.display()
        )));
    }
    for component in &components[..components.len() - 1] {
        let Component::Normal(name) = component else {
            return Err(error("refusing non-project-relative manifest path"));
        };
        let opened = openat(
            directory.as_fd(),
            *name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| {
            error(format!(
                "open project manifest directory without following links: {e}"
            ))
        })?;
        directory = std::fs::File::from(opened);
    }
    let Component::Normal(leaf) = components[components.len() - 1] else {
        return Err(error("refusing non-project-relative manifest path"));
    };
    Ok((directory, leaf.to_os_string()))
}

#[cfg(unix)]
fn read_project_regular_text_dirfd(
    project_root: &Path,
    relative_path: &Path,
    display_name: &str,
) -> Result<String, MgError> {
    use rustix::fs::{Mode, OFlags, openat};
    use std::io::Read;
    use std::os::fd::AsFd;

    let (parent, leaf) = project_parent_dir(project_root, relative_path)?;
    let opened = openat(
        parent.as_fd(),
        &leaf,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| error(format!("open {display_name} without following links: {e}")))?;
    let mut file = std::fs::File::from(opened);
    if !file
        .metadata()
        .map_err(|e| error(format!("inspect opened {display_name}: {e}")))?
        .is_file()
    {
        return Err(error(format!("{display_name} is not a regular file")));
    }
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| error(format!("read {display_name}: {e}")))?;
    Ok(content)
}

#[cfg(unix)]
fn atomic_write_project_regular_dirfd(
    project_root: &Path,
    relative_path: &Path,
    bytes: &[u8],
) -> Result<(), MgError> {
    use rustix::fs::{Mode, OFlags, openat, renameat, unlinkat};
    use std::io::Write;
    use std::os::fd::AsFd;

    let (parent, leaf) = project_parent_dir(project_root, relative_path)?;
    let existing = openat(
        parent.as_fd(),
        &leaf,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| {
        error(format!(
            "open existing project manifest without following links: {e}"
        ))
    })?;
    let existing = std::fs::File::from(existing);
    let metadata = existing
        .metadata()
        .map_err(|e| error(format!("inspect existing project manifest: {e}")))?;
    if !metadata.is_file() {
        return Err(error("refusing to replace a non-regular project manifest"));
    }

    let temp = std::ffi::OsString::from(format!(
        ".{}.mgc-tmp-{}-{}",
        leaf.to_string_lossy(),
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let staged = openat(
        parent.as_fd(),
        &temp,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|e| error(format!("create project manifest staging file: {e}")))?;
    let result = (|| {
        let mut file = std::fs::File::from(staged);
        file.set_permissions(metadata.permissions())
            .map_err(|e| error(format!("preserve project manifest permissions: {e}")))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| error(format!("sync project manifest staging file: {e}")))?;
        // renameat is anchored to the already-open parent directory; a path
        // swap cannot redirect the write outside this directory.
        renameat(parent.as_fd(), &temp, parent.as_fd(), &leaf)
            .map_err(|e| error(format!("publish project manifest atomically: {e}")))?;
        parent
            .sync_all()
            .map_err(|e| error(format!("sync project manifest directory: {e}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = unlinkat(parent.as_fd(), &temp, rustix::fs::AtFlags::empty());
    }
    result
}

#[cfg(not(unix))]
fn checked_project_path(project_root: &Path, relative_path: &Path) -> Result<PathBuf, MgError> {
    use std::path::Component;

    if relative_path.as_os_str().is_empty()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(error(format!(
            "refusing non-project-relative manifest path: {}",
            relative_path.display()
        )));
    }
    // The caller chooses the project root; canonicalizing that one boundary
    // avoids rejecting system aliases such as macOS /var -> /private/var.
    let root = std::fs::canonicalize(project_root).map_err(|e| {
        error(format!(
            "resolve project root {}: {e}",
            project_root.display()
        ))
    })?;
    let mut current = root.clone();
    let components: Vec<_> = relative_path.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(error(format!(
                "refusing non-project-relative manifest path: {}",
                relative_path.display()
            )));
        };
        current.push(name);
        let is_leaf = index + 1 == components.len();
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                let file_type = metadata.file_type();
                if file_type.is_symlink() {
                    return Err(error(format!(
                        "refusing symlinked project manifest component: {}",
                        current.display()
                    )));
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
                    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err(error(format!(
                            "refusing reparse-point project manifest component: {}",
                            current.display()
                        )));
                    }
                }
                if !is_leaf && !file_type.is_dir() {
                    return Err(error(format!(
                        "project manifest parent is not a directory: {}",
                        current.display()
                    )));
                }
            }
            Err(e) => {
                return Err(error(format!(
                    "inspect project manifest component {}: {e}",
                    current.display()
                )));
            }
        }
    }
    if !current.starts_with(&root) {
        return Err(error(format!(
            "project manifest escaped root: {}",
            relative_path.display()
        )));
    }
    Ok(current)
}

fn unique_temp_path(parent: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    parent.join(format!(
        ".{}.mgc-tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}
