//! `mgc create-<core>` — router: core detect → file con (v5: LỆNH = folder, CORE = file).
//!
//! T5: provider = registry starter kit (create-mgc-<core>) → fallback local template → wizard.
//! T9a: sau scaffold, tự ghi `.mgc.core` marker tại project folder.
//!
//! Security: `validate_project_name` chặn path traversal/absolute tại CLI
//! boundary — mọi create-<core> đều đi qua đây trước khi chạm filesystem.

use anyhow::Result;
use mgc_config::project::ProjectConfig;
use std::path::{Path, PathBuf};

/// Fail-closed CLI input gate — a project name from the user must be a
/// single normal path segment; separators, "..", absolute paths, drive
/// prefixes, and home prefixes are rejected.
/// Cổng chặn tại CLI boundary — tên project từ user phải là 1 path segment
/// đơn lẻ; chặn separator, "..", absolute path, drive prefix, "~".
pub fn validate_project_name(project_name: &str) -> Result<()> {
    let valid = !project_name.is_empty()
        && !project_name.contains('/')
        && !project_name.contains('\\')
        && !project_name.contains("..")
        && !project_name.starts_with('~')
        && !project_name.contains(':')
        && Path::new(project_name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    if valid {
        Ok(())
    } else {
        Err(crate::error::invalid_project_name(project_name))
    }
}

#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "cicd")]
pub mod cicd;
#[cfg(feature = "clo")]
pub mod clo;
#[cfg(feature = "game")]
pub mod game;
#[cfg(feature = "hardware")]
pub mod hardware;
#[cfg(feature = "iot")]
pub mod iot;
#[cfg(feature = "lib")]
pub mod library;
pub mod web;

// Router `run(core, ...)` đã bị loại: dispatch live (dispatch/core/create.rs) gọi
// thẳng từng create::<core>::run và ghi .mgc.core marker qua init/wizard path —
// router trung gian này trùng lắp 100% và không còn caller.
// Removed legacy router: live dispatch calls each create subcommand directly.

pub(crate) fn save_scaffold_metadata(
    project_dir: &Path,
    config: &crate::wizard::engine::ScaffoldConfig,
) -> Result<()> {
    let name = crate::scaffold::processor::Scaffolder::display_name(project_dir);
    let template = config.template_dir.to_str().unwrap_or("").to_string();
    let project = ProjectConfig::from_scaffold(
        name,
        &config.core,
        &config.sub_type,
        config.frameworks.clone(),
        template,
        config.features.clone(),
    );
    project.save(project_dir)?;
    Ok(())
}

pub(crate) fn scaffold_and_save_metadata(
    config: &crate::wizard::engine::ScaffoldConfig,
) -> Result<PathBuf> {
    let project_dir = crate::scaffold::processor::Scaffolder::scaffold(config)?;
    save_scaffold_metadata(&project_dir, config)?;
    Ok(project_dir)
}

#[cfg(test)]
#[path = "../../../test/create_security_test.rs"]
mod tests;
