//! IoT tooling helpers for native command passthrough.
//! Gom phần gọi tool và version placeholder để adapter chính gọn hơn.

use mgc_types::{Ecosystem, MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::Path;

pub(crate) fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    // clean_env scrubs everything (no HOME), so pio/west/cargo spawns get
    // the MINIMAL env they need — redirected into the mgc shared store
    // (never the user's home). Same precedent as lib toolchain_env.
    // (clean_env lột sạch env — spawn nhận env tối thiểu vào store mgc.)
    let opts = mgc_exec::prelude::ExecOptions {
        cwd: Some(root.to_path_buf()),
        log_path: Some(root.join(".magicore").join("exec.log")),
        clean_env: true,
        env: toolchain_env(cmd)?,
        ..Default::default()
    };
    mgc_exec::prelude::run(cmd, args, &opts)
        .map_err(|e| mgc_types::MgError::Other(e.to_string()))?;
    Ok(())
}

/// Minimal child env per toolchain, rooted at the mgc shared store.
/// (Env tối thiểu mỗi toolchain, gốc tại store chung mgc.)
fn toolchain_env(cmd: &str) -> MgResult<Vec<(String, String)>> {
    if !matches!(cmd, "cargo" | "rustc" | "pio" | "platformio" | "west") {
        return Ok(Vec::new());
    }
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|e| mgc_types::MgError::Other(format!("cannot resolve mgc home: {e}")))?;
    let store = globals.store;
    let mut env = Vec::new();
    let dir = |path: std::path::PathBuf| -> MgResult<String> {
        std::fs::create_dir_all(&path).map_err(|e| {
            mgc_types::MgError::Other(format!("cannot create tool dir '{}': {e}", path.display()))
        })?;
        Ok(path.display().to_string())
    };
    match cmd {
        "cargo" | "rustc" => {
            let root = dir(store.join("cargo"))?;
            env.push(("CARGO_HOME".to_string(), root));
        }
        // PlatformIO keeps cores/packages/cache under its core dir —
        // redirect the whole home so no user state is touched.
        // (PlatformIO giữ core/package/cache dưới core dir của nó.)
        "pio" | "platformio" => {
            let root = dir(store.join("pio"))?;
            env.push(("PLATFORMIO_CORE_DIR".to_string(), root));
        }
        // west is workspace-local (`.west/` + manifest); nothing to
        // redirect — clean env stands as-is.
        // (west cục bộ theo workspace — giữ nguyên clean env.)
        _ => {}
    }
    Ok(env)
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
