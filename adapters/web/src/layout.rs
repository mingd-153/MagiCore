use mgc_types::{MgError, MgResult};
use std::path::Path;

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(not(unix))]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    // Windows: directory symlinks need Developer Mode or elevation. Try the
    // real symlink first, then fall back to a junction point (no privilege
    // required, resolved by the filesystem like a symlink for our purposes).
    // Windows: symlink cần Developer Mode/elevation — thử symlink trước,
    // fail thì fallback junction point (không cần quyền, CI runner dùng được).
    match std::os::windows::fs::symlink_dir(original, link) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            // mklink /J must run inside cmd.exe (it is a shell builtin).
            let status = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &link.to_string_lossy(),
                    &original.to_string_lossy(),
                ])
                .status()
                .map_err(|e| {
                    // Report the ORIGINAL symlink error, not the fallback spawn error.
                    std::io::Error::new(first_err.kind(), format!("{first_err}"))
                })?;
            if status.success() {
                Ok(())
            } else {
                // Surface the original symlink error — junction also failed.
                Err(std::io::Error::new(
                    first_err.kind(),
                    format!(
                        "symlink_dir failed on Windows (symlink: {first_err}; mklink /J exit {:?})",
                        status.code()
                    ),
                ))
            }
        }
    }
}

/// Clear the read-only attribute before deletion. npm tarballs store files
/// as 0444; on Windows that maps to the read-only attribute and
/// remove_dir_all/remove_file fail with ACCESS_DENIED (os error 5).
/// Failure is best-effort — the remove below surfaces the real error.
/// Xóa thuộc tính read-only trước khi delete: tarball npm lưu 0444, trên
/// Windows thành read-only khiến remove_* bị ACCESS_DENIED. Best-effort —
/// lệnh remove phía sau sẽ trả lỗi thật nếu vẫn fail.
fn clear_readonly_recursively(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_real_dir = std::fs::symlink_metadata(&path)
            .is_ok_and(|m| m.file_type().is_dir() && !m.file_type().is_symlink());
        if is_real_dir {
            clear_readonly_recursively(&path);
        }
        clear_readonly_file(&path);
    }
    clear_readonly_file(root);
}

fn clear_readonly_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut mode = meta.permissions().mode();
            if mode & 0o222 == 0 {
                mode |= 0o200; // owner write
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
    }
}

pub fn create_symlink(target: &Path, link: &Path) -> MgResult<()> {
    if let Ok(metadata) = link.symlink_metadata() {
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if metadata.file_type().is_symlink()
            && let Ok(existing_target) = std::fs::read_link(link)
            && existing_target == target
        {
            return Ok(());
        }
        if metadata.file_type().is_symlink() {
            // Directory symlinks AND junction points both report as symlinks
            // through symlink_metadata. Windows refuses remove_file on them
            // with ACCESS_DENIED — remove_dir (non-recursive) is the correct
            // primitive for both, and it never crosses into the target.
            // Symlink dir & junction đều hiện là symlink qua symlink_metadata.
            // Windows remove_file trả ACCESS_DENIED — remove_dir (không đệ
            // quy) là primitive đúng cho cả hai và không chạm vào target.
            std::fs::remove_dir(link).map_err(|err| {
                MgError::Other(format!(
                    "failed to remove existing symlink/junction '{}': {}",
                    link.display(),
                    err
                ))
            })?;
        } else if metadata.file_type().is_dir() {
            clear_readonly_recursively(link);
            std::fs::remove_dir_all(link).map_err(|err| {
                MgError::Other(format!(
                    "failed to remove existing directory link '{}': {}",
                    link.display(),
                    err
                ))
            })?;
        } else {
            clear_readonly_file(link);
            std::fs::remove_file(link).map_err(|err| {
                MgError::Other(format!(
                    "failed to remove existing file link '{}' -> '{}': {}",
                    link.display(),
                    target.display(),
                    err
                ))
            })?;
        }
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            MgError::Other(format!(
                "failed to create parent dir '{}' for link '{}': {}",
                parent.display(),
                link.display(),
                err
            ))
        })?;
    }

    // Attempt symlink, fallback to copy if failed (Windows fallback)
    if let Err(e) = symlink_dir(target, link) {
        #[cfg(not(unix))]
        {
            if let Err(e2) = crate::install::link_tree::hardlink_tree(target, link) {
                return Err(mgc_types::MgError::Other(format!(
                    "failed to create symlink (or fallback hardlink tree) from {} to {}: {} (fallback error: {})",
                    target.display(),
                    link.display(),
                    e,
                    e2
                )));
            }
        }
        #[cfg(unix)]
        {
            return Err(mgc_types::MgError::Other(format!(
                "failed to create symlink from {} to {}: {}",
                target.display(),
                link.display(),
                e
            )));
        }
    }
    Ok(())
}
