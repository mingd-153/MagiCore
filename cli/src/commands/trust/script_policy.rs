//! Serialize lifecycle-script trust policy writes with dependency installs.
//! Tuần tự hóa thay đổi trust policy với install dependency.

use anyhow::{Context, Result};
use mgc_lockfile::project_lock::ProjectWriteLock;
use mgc_store::{Database, Layout};
use std::path::Path;

/// A user decision for package lifecycle scripts — Quyết định chạy lifecycle scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPolicy {
    Approved,
    Denied,
}

impl ScriptPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Denied => "denied",
        }
    }
}

/// Persist a script decision under the same project lock used by install.
/// Ghi quyết định script dưới cùng project lock mà install sử dụng.
pub fn set_policy(root: &Path, package: &str, policy: ScriptPolicy) -> Result<()> {
    if package.trim().is_empty() {
        return Err(crate::error::trust_policy_package_empty());
    }
    let _lock = acquire_policy_mutation_lock(root)?;
    let layout = policy_layout(root);
    ensure_policy_layout(&layout)?;
    let db_path = layout.db_path();
    let db = Database::open(&db_path)
        .context(crate::error::trust_policy_database_open_failed(&db_path))?;
    db.upsert_trust_policy(package, policy.as_str())
        .map_err(|error| crate::error::trust_policy_database_write_failed(&db_path, &error))
}

/// Prune decisions under the project lock so package rows cannot change mid-query.
/// Dọn quyết định dưới project lock để package rows không đổi giữa truy vấn.
pub fn prune_stale_policies(root: &Path) -> Result<usize> {
    let _lock = acquire_policy_mutation_lock(root)?;
    let layout = policy_layout(root);
    ensure_policy_layout(&layout)?;
    let db_path = layout.db_path();
    let db = Database::open(&db_path)
        .context(crate::error::trust_policy_database_open_failed(&db_path))?;
    db.prune_trust_policies()
        .map_err(|error| crate::error::trust_policy_database_write_failed(&db_path, &error))
}

fn policy_layout(root: &Path) -> Layout {
    Layout::new(root.join(".magicore").join("cache").join("web"))
}

fn ensure_policy_layout(layout: &Layout) -> Result<()> {
    for directory in [layout.root().parent(), Some(layout.root())]
        .into_iter()
        .flatten()
    {
        if mgc_lockfile::project_lock::path_is_link_or_reparse(directory) {
            return Err(crate::error::trust_policy_path_invalid(directory));
        }
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(crate::error::trust_policy_path_invalid(directory)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(directory)
                    .map_err(|_| crate::error::trust_policy_path_invalid(directory))?;
            }
            Err(_) => return Err(crate::error::trust_policy_path_invalid(directory)),
        }
    }
    let db_path = layout.db_path();
    if mgc_lockfile::project_lock::path_is_link_or_reparse(&db_path) {
        return Err(crate::error::trust_policy_path_invalid(&db_path));
    }
    match std::fs::symlink_metadata(&db_path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(crate::error::trust_policy_path_invalid(&db_path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(crate::error::trust_policy_path_invalid(&db_path)),
    }
    Ok(())
}

fn acquire_policy_mutation_lock(root: &Path) -> Result<ProjectWriteLock> {
    let lock = ProjectWriteLock::acquire(
        root,
        crate::commands::core::shared::writer_lock_timeout(root),
    )
    .map_err(|error| crate::error::trust_policy_lock_failed(root, &error))?;
    crate::commands::core::shared::ensure_no_pending_remove_journal(root, &lock)?;
    Ok(lock)
}

#[cfg(test)]
#[path = "test/script_policy.rs"]
mod tests;
