//! Language detection for mgc-lib-adapter.
//! Nhận diện ngôn ngữ lib từ mgc.toml và manifest ecosystem chuẩn.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibLanguage {
    Ts,
    Rust,
    Python,
    Go,
    Java,
    DotNet,
}

type ManifestProbe = fn(&Path) -> Option<String>;

pub(crate) fn detect_language(root: &Path) -> Option<LibLanguage> {
    let mgc_toml = root.join("mgc.toml");
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(content) = std::fs::read_to_string(&mgc_toml)
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem")
            .and_then(|e| e.as_str())
            .is_some_and(|eco| eco != "lib")
            && v.get("lib").is_none()
        {
            return None;
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
                "go" => Some(LibLanguage::Go),
                "java" | "kotlin" => Some(LibLanguage::Java),
                "dotnet" | "csharp" | "cs" => Some(LibLanguage::DotNet),
                _ => None,
            };
        }
    }
    if root.join("package.json").exists() {
        return Some(LibLanguage::Ts);
    }
    if root.join("Cargo.toml").exists() {
        return Some(LibLanguage::Rust);
    }
    if root.join("go.mod").exists() {
        return Some(LibLanguage::Go);
    }
    // Java/Kotlin: the gradle verification metadata (lockfile) is the
    // audit source — prefer it over the plain build file.
    // Java/Kotlin: metadata verification gradle (lockfile) là nguồn
    // audit — ưu tiên trước build file thường.
    if root
        .join("gradle")
        .join("verification-metadata.xml")
        .is_file()
        || root.join("build.gradle").is_file()
        || root.join("build.gradle.kts").is_file()
    {
        return Some(LibLanguage::Java);
    }
    // .NET: packages.lock.json is the lockfile the audit reads.
    // .NET: packages.lock.json là lockfile audit đọc.
    if root.join("packages.lock.json").is_file() {
        return Some(LibLanguage::DotNet);
    }
    if root.join("pyproject.toml").exists() {
        return Some(LibLanguage::Python);
    }
    None
}

pub(crate) fn manifest_is_lib(root: &Path) -> bool {
    // let-chain edition 2024 — gộp điều kiện theo clippy 1.98.
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml"))
        && let Ok(v) = toml::from_str::<toml::Value>(&content)
    {
        if v.get("ecosystem").and_then(|e| e.as_str()) == Some("lib") {
            return true;
        }
        if v.get("lib").is_some() {
            return true;
        }
    }
    let probes: [(&Path, ManifestProbe); 3] = [
        (&root.join("package.json"), probe_package_json),
        (&root.join("Cargo.toml"), probe_cargo_toml),
        (&root.join("pyproject.toml"), probe_pyproject),
    ];
    for (path, probe) in probes {
        if path.exists()
            && let Some(eco) = probe(path)
            && eco == "lib"
        {
            return true;
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
