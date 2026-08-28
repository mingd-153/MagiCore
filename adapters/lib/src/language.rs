//! Language detection for mgc-lib-adapter.
//! Nhận diện ngôn ngữ lib từ mgc.toml và manifest ecosystem chuẩn.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibLanguage {
    Ts,
    Rust,
    Python,
}

type ManifestProbe = fn(&Path) -> Option<String>;

pub(crate) fn detect_language(root: &Path) -> Option<LibLanguage> {
    let mgc_toml = root.join("mgc.toml");
    if let Ok(content) = std::fs::read_to_string(&mgc_toml) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco != "lib" && v.get("lib").is_none() {
                    return None;
                }
            }
            if let Some(lang) = v
                .get("lib")
                .and_then(|l| l.get("language"))
                .and_then(|l| l.as_str())
            {
                return match lang {
                    "ts" | "typescript" => Some(LibLanguage::Ts),
                    "rust" => Some(LibLanguage::Rust),
                    "python" => Some(LibLanguage::Python),
                    _ => None,
                };
            }
        }
    }
    if root.join("package.json").exists() {
        return Some(LibLanguage::Ts);
    }
    if root.join("Cargo.toml").exists() {
        return Some(LibLanguage::Rust);
    }
    if root.join("pyproject.toml").exists() {
        return Some(LibLanguage::Python);
    }
    None
}

pub(crate) fn manifest_is_lib(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "lib" {
                    return true;
                }
            }
            if v.get("lib").is_some() {
                return true;
            }
        }
    }
    let probes: [(&Path, ManifestProbe); 3] = [
        (&root.join("package.json"), probe_package_json),
        (&root.join("Cargo.toml"), probe_cargo_toml),
        (&root.join("pyproject.toml"), probe_pyproject),
    ];
    for (path, probe) in probes {
        if path.exists() {
            if let Some(eco) = probe(path) {
                if eco == "lib" {
                    return true;
                }
            }
        }
    }
    false
}

fn probe_package_json(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("magicore")
        .and_then(|m| m.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_cargo_toml(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

fn probe_pyproject(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: toml::Value = toml::from_str(&content).ok()?;
    v.get("tool")
        .and_then(|t| t.get("magicore"))
        .and_then(|mgc| mgc.get("core"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}
