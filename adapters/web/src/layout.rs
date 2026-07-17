use mg_types::MgResult;
use std::path::Path;

#[cfg(unix)]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(not(unix))]
fn symlink_dir(original: &Path, link: &Path) -> std::io::Result<()> {
    // For Windows, try junction point or directory symlink.
    // If not elevated or Developer Mode is off, it might fail.
    // For this prototype, we'll try symlink_dir
    std::os::windows::fs::symlink_dir(original, link)
}

pub fn create_symlink(target: &Path, link: &Path) -> MgResult<()> {
    if link.exists() {
        let _ = std::fs::remove_dir_all(link);
    } else if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Attempt symlink, fallback to copy if failed (Windows fallback)
    if let Err(e) = symlink_dir(target, link) {
        #[cfg(not(unix))]
        {
            if let Err(e2) = crate::hardlink_tree(target, link) {
                return Err(mg_types::MgError::Other(format!(
                    "failed to create symlink (or fallback hardlink tree) from {} to {}: {} (fallback error: {})",
                    target.display(), link.display(), e, e2
                )));
            }
        }
        #[cfg(unix)]
        {
            return Err(mg_types::MgError::Other(format!(
                "failed to create symlink from {} to {}: {}",
                target.display(),
                link.display(),
                e
            )));
        }
    }
    Ok(())
}
