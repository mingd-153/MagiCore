//! Tool execution and installed-version readers for lib ecosystems.
//! Gom phần gọi tool native và đọc version để adapter chính không phình to.

use crate::language::LibLanguage;
use crate::manifest::{parse_cargo_manifest, parse_go_mod_manifest, parse_pyproject_manifest};
use mgc_types::{MgResult, PackageId, PackageName, Version, VersionRange};
use std::path::{Path, PathBuf};

pub(crate) fn exec_tool(root: &Path, cmd: &str, args: &[String]) -> MgResult<()> {
    // clean_env scrubs everything (no HOME/GOPATH/CARGO_HOME), so EVERY
    // toolchain spawn gets the MINIMAL env it needs — redirected into the
    // mgc shared store (never the user's home). Verified per tool by
    // runtime E2E (go/cargo/pip); a tool absent here fails closed with
    // tool-unavailable before env matters.
    // (clean_env lột sạch env — mỗi spawn toolchain nhận env tối thiểu,
    // chuyển vào store chung mgc.)
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
    if !matches!(cmd, "cargo" | "rustc" | "go" | "pip" | "pip3") {
        return Ok(Vec::new());
    }
    let globals = mgc_platform::paths::GlobalPaths::new()
        .map_err(|e| mgc_types::MgError::Other(format!("cannot resolve mgc home: {e}")))?;
    let store = globals.store;
    let mut env = Vec::new();
    let dir = |path: PathBuf| -> MgResult<String> {
        std::fs::create_dir_all(&path).map_err(|e| {
            mgc_types::MgError::Other(format!("cannot create tool dir '{}': {e}", path.display()))
        })?;
        Ok(path.display().to_string())
    };
    match cmd {
        // Cargo: CARGO_HOME redirects registry + git caches (same
        // precedent as install/shared_store.rs measured runs).
        "cargo" | "rustc" => {
            let root = dir(store.join("cargo"))?;
            env.push(("CARGO_HOME".to_string(), root));
        }
        // Go: explicit GOPATH/GOMODCACHE/GOCACHE (clean_env leaves go
        // with "module cache not found" otherwise).
        "go" => {
            let root = store.join("go");
            env.push(("GOPATH".to_string(), dir(root.join("gopath"))?));
            env.push(("GOMODCACHE".to_string(), dir(root.join("modcache"))?));
            env.push(("GOCACHE".to_string(), dir(root.join("gocache"))?));
        }
        // pip: cache dir only (config stays default; nothing is read
        // from or written to the user's home).
        "pip" | "pip3" => {
            let root = dir(store.join("pypi"))?;
            env.push(("PIP_CACHE_DIR".to_string(), root));
        }
        _ => {}
    }
    Ok(env)
}

/// Post-gate pip binary resolution: the gate already approved the pip
/// owner (`pip` ~ `pip3` alias); this picks the binary that EXISTS for
/// the real spawn — `pip` preferred, `pip3` fallback, else `pip` so a
/// missing tool surfaces a clear spawn error. Filesystem lookup ONLY
/// (no `--version` probe spawn), called strictly AFTER the gate.
/// (Resolve binary pip SAU gate: chỉ lookup filesystem.)
pub(crate) fn pip_binary() -> &'static str {
    fn on_path(bin: &str) -> bool {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(bin))
            .any(|p| p.is_file())
    }
    if on_path("pip") || !on_path("pip3") {
        "pip"
    } else {
        "pip3"
    }
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

/// Resolve the FULL Go module path for a display name by scanning go.mod
/// — display names are the last path segment, but `go get` needs the
/// whole path ("text" → "golang.org/x/text"). Falls back to the bare
/// name when go.mod does not mention it (the go toolchain will error
/// honestly on an unknown path).
/// Tra path module Go ĐẦY ĐỦ cho một tên hiển thị bằng cách quét
/// go.mod — tên hiển thị là đoạn cuối path, nhưng `go get` cần cả path
/// ("text" → "golang.org/x/text"). Rơi về tên trần khi go.mod không nhắc
/// (go toolchain sẽ lỗi trung thực với path lạ).
pub(crate) fn go_module_path(project_root: &Path, name: &PackageName) -> String {
    let Ok(content) = std::fs::read_to_string(project_root.join("go.mod")) else {
        return name.as_str().to_string();
    };
    for line in content.lines() {
        let no_comment = line.split("//").next().unwrap_or("").trim();
        let candidate = no_comment
            .strip_prefix("require ")
            .map(|rest| rest.trim())
            .unwrap_or_else(|| no_comment)
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        if let Some((path, _version)) = candidate.split_once(' ')
            && path.rsplit('/').next() == Some(name.as_str())
        {
            return path.to_string();
        }
    }
    name.as_str().to_string()
}

pub(crate) fn version_from_manifest(
    root: &Path,
    name: &PackageName,
    language: LibLanguage,
) -> Option<Version> {
    let manifest = match language {
        LibLanguage::Rust => parse_cargo_manifest(root).ok()?,
        LibLanguage::Python => parse_pyproject_manifest(root).ok()?,
        // Go manifest versions come from go.mod directly (same parser as
        // the audit path — one source of truth).
        // Version manifest Go đọc thẳng từ go.mod (cùng parser với đường
        // audit — một nguồn chân lý).
        LibLanguage::Go => parse_go_mod_manifest(root).ok()?,
        // Java/.NET lifecycle add/remove is not wired (P2 audit parity
        // scope) — no manifest version to resolve yet.
        // Add/remove lifecycle Java/.NET chưa nối (scope parity audit
        // P2) — chưa có version manifest để resolve.
        LibLanguage::Java | LibLanguage::DotNet => return None,
        LibLanguage::Ts => return None,
    };
    manifest
        .find_dep(name.as_str())
        .and_then(|d| d.range.satisfying_version())
}
