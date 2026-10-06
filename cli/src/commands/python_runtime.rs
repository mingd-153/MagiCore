//! Environment wiring for MagiCore-owned Python packages.
//! Kết nối môi trường cho package Python do MagiCore sở hữu.

use anyhow::Result;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Build a child `PYTHONPATH` from verified MGC materializations and,
/// optionally, the caller's existing `PYTHONPATH`.
/// Tạo `PYTHONPATH` cho tiến trình con từ materialization đã kiểm của MGC.
pub fn native_python_path_value(
    project_root: &Path,
    preserve_existing: bool,
) -> Result<Option<OsString>> {
    #[cfg(feature = "lib")]
    let entries = mgc_lib_adapter::install::native_python_path_entries(project_root)?;
    #[cfg(not(feature = "lib"))]
    let entries: Vec<PathBuf> = {
        let _ = project_root;
        Vec::new()
    };

    if entries.is_empty() {
        return Ok(None);
    }

    let existing = preserve_existing
        .then(|| std::env::var_os("PYTHONPATH"))
        .flatten();
    Ok(Some(join_python_paths(entries, existing)?))
}

/// Add all environment controls needed for verified MGC Python packages.
/// Disable bytecode writes so subsequent launches never see unrecorded
/// `__pycache__` files in the immutable shared materialization.
/// (Thêm env cho package Python đã kiểm; tắt ghi bytecode để tránh file
/// `__pycache__` ngoài RECORD trong store dùng chung.)
pub fn extend_native_python_env(
    env: &mut Vec<(String, String)>,
    project_root: &Path,
    preserve_existing: bool,
) -> Result<()> {
    if let Some(path) = native_python_path_value(project_root, preserve_existing)? {
        env.push(("PYTHONPATH".to_string(), python_path_env_value(path)?));
        env.push(("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string()));
    }
    Ok(())
}

/// Convert a native path value without silently corrupting non-Unicode paths.
/// Chuyển đường dẫn mà không âm thầm làm hỏng path không phải Unicode.
pub fn python_path_env_value(path: OsString) -> Result<String> {
    path.into_string()
        .map_err(|_| crate::error::python_path_not_unicode())
}

fn join_python_paths(mut managed: Vec<PathBuf>, existing: Option<OsString>) -> Result<OsString> {
    if let Some(existing) = existing {
        managed.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(managed).map_err(Into::into)
}

#[cfg(test)]
#[path = "test/python_runtime.rs"]
mod tests;
