//! processors/<core>.rs — scaffold files per core (v5: `scaffold/ → processors/`).
//! Web scaffold giữ tại processor.rs (web templates/contract engine).

pub mod ai;
pub mod app;
pub mod cicd;
pub mod clo;
pub mod game;
pub mod iot;
pub mod lib;

use std::path::Path;

use anyhow::Result;

/// Ghi file — atomic-ish: tạo parent dirs trước khi write.
pub(crate) fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub(crate) fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
