use mgc_types::MgResult;
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

pub fn create_symlink(target: &Path, link: &Path) -> MgResult<()> {
    if let Ok(metadata) = link.symlink_metadata() {
        // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
        if metadata.file_type().is_symlink()
            && let Ok(existing_target) = std::fs::read_link(link)
            && existing_target == target
        {
            return Ok(());
        }
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(link)?;
        } else {
            std::fs::remove_file(link)?;
        }
    }
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
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
