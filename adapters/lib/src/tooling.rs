//! Tool execution and installed-version readers for lib ecosystems.
//! Gom phần gọi tool native và đọc version để adapter chính không phình to.

use crate::language::LibLanguage;
use crate::manifest::{parse_cargo_manifest, parse_pyproject_manifest};
use mgc_types::{MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::{Path, PathBuf};

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

pub fn check_pip_allowed(root: &Path, name: &str) -> MgResult<()> {
    let allowed = read_pip_allowlist(root);
    if allowed.iter().any(|a| a == name) {
        return Ok(());
    }
    Err(mgc_types::MgError::Other(format!(
        "pip '{}' is not in [lib].pip_allowed_packages (mgc.toml). Fail-closed — add the package there to allow pip install/uninstall.",
        name
    )))
}

fn read_pip_allowlist(root: &Path) -> Vec<String> {
    let mgc_toml = root.join("mgc.toml");
    let Ok(content) = std::fs::read_to_string(&mgc_toml) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    v.get("lib")
        .and_then(|l| l.get("pip_allowed_packages"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn cargo_lock_versions(root: &Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(root.join("Cargo.lock")) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    v.get("package")
        .and_then(|p| p.as_array())
        .map(|pkgs| {
            pkgs.iter()
                .filter_map(|p| {
                    let name = p.get("name")?.as_str()?.to_string();
                    let version = p.get("version")?.as_str()?.to_string();
                    Some((name, version))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn dist_info_versions(root: &Path) -> Vec<(String, String)> {
    let candidates: [PathBuf; 4] = [
        root.join("venv").join("lib"),
        root.join(".venv").join("lib"),
        root.join("lib"),
        root.join("site-packages"),
    ];
    let mut out = Vec::new();
    for base in candidates {
        if !base.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_dist_infos(&path, &mut out);
                }
            }
        }
    }
    out
}

fn collect_dist_infos(dir: &Path, out: &mut Vec<(String, String)>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".dist-info") {
                if let Some((pkg, version)) = parse_dist_metadata(&path.join("METADATA")) {
                    out.push((pkg, version));
                }
            } else if path.is_dir() {
                collect_dist_infos(&path, out);
            }
        }
    }
}

fn parse_dist_metadata(path: &Path) -> Option<(String, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Name:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Version:") {
            version = Some(rest.trim().to_string());
        }
        if name.is_some() && version.is_some() {
            break;
        }
    }
    Some((name?, version?))
}

pub(crate) fn placeholder_id(name: &PackageName, range: Option<&VersionRange>) -> PackageId {
    let version = range
        .and_then(|r| r.satisfying_version())
        .unwrap_or_else(|| Version::new(0, 1, 0));
    PackageId::new(name.clone(), version)
}

pub(crate) fn version_from_manifest(
    root: &Path,
    name: &PackageName,
    language: LibLanguage,
) -> Option<Version> {
    let manifest = match language {
        LibLanguage::Rust => parse_cargo_manifest(root).ok()?,
        LibLanguage::Python => parse_pyproject_manifest(root).ok()?,
        LibLanguage::Ts => return None,
    };
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
}
