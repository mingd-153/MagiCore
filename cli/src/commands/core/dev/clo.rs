//! `mgc dev`/`mgc deploy` clo — cloud tooling (terraform/cdk/pulumi) — tách từ core/clo.rs (v5).

use anyhow::Result;
use std::path::{Path, PathBuf};

fn project_root() -> Result<PathBuf> {
    super::super::shared::core_project_root("clo")
}

/// Cloud type từ mgc.toml `[cloud] type` hoặc manifest probe — dùng cho dev/deploy.
pub fn cloud_type(root: &Path) -> anyhow::Result<String> {
    // adapter_for is Result<Option<_>> now (P0-4): `?` carries the typed
    // registry-URL error, Ok(None) still means "no cloud project here".
    // (adapter_for là Result<Option<_>> (P0-4): `?` mang lỗi registry-URL
    // typed, Ok(None) vẫn nghĩa là "không phải project cloud".)
    let adapter = mgc_cloud_adapter::adapter_for(root)?
        .ok_or_else(|| crate::error::no_framework_detected("cloud", root))?;
    Ok(adapter.cloud_type().to_string())
}

pub async fn dev(dry_run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let _ = (dry_run, &root);
    native_dev_command(&kind)?;
    Ok(())
}

/// `mgc deploy` — mặc định dry-run (in lệnh deploy theo type, KHÔNG chạy);
/// chạy thật chỉ với `--run` (spec §4: deploy = hành động ghi cloud).
pub async fn deploy(run: bool) -> Result<()> {
    let root = project_root()?;
    let kind = cloud_type(&root)?;
    let _ = (run, &root);
    native_deploy_command(&kind)?;
    Ok(())
}

fn native_dev_command(kind: &str) -> Result<()> {
    Err(crate::error::dev_cloud_not_implemented(kind))
}

fn native_deploy_command(kind: &str) -> Result<()> {
    Err(crate::error::deploy_not_implemented(kind))
}

#[cfg(test)]
#[path = "test/clo.rs"]
mod tests;
