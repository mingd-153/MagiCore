//! IoT tooling helpers for native command passthrough.
//! Gom phần gọi tool và version placeholder để adapter chính gọn hơn.

use mgc_types::{Ecosystem, MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::Path;

pub(crate) fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        ..Default::default()
    };
    mgc_exec::prelude::run(cmd, args, &opts)
        .map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    Ok(())
}

pub(crate) fn placeholder_id(name: &PackageName, range: Option<&VersionRange>) -> PackageId {
    let version = range
        .and_then(|r| r.satisfying_version())
        .unwrap_or_else(|| Version::new(0, 1, 0));
    PackageId::new(name.clone(), version)
}

pub(crate) fn cargo_dep_version(root: &Path, name: &PackageName) -> Option<Version> {
    let manifest = mgc_adapter_base::cargo_manifest::parse_manifest(root, Ecosystem::Iot).ok()?;
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
}
