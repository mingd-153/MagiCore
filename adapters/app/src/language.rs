//! App language detection for mgc-app-adapter.
//! Nhận diện core app qua mgc.toml và marker file nền tảng.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLanguage {
    Flutter,
    Kotlin,
    Swift,
    ReactNative,
    ObjC,
    Multi,
}

impl AppLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppLanguage::Flutter => "flutter",
            AppLanguage::Kotlin => "kotlin",
            AppLanguage::Swift => "swift",
            AppLanguage::ReactNative => "react-native",
            AppLanguage::ObjC => "objc",
            AppLanguage::Multi => "multi",
        }
    }
}

pub fn detect_language(root: &Path) -> Option<AppLanguage> {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(p) = v
                .get("app")
                .and_then(|c| c.get("language"))
                .and_then(|p| p.as_str())
            {
                return match p {
                    "flutter" => Some(AppLanguage::Flutter),
                    "kotlin" => Some(AppLanguage::Kotlin),
                    "swift" => Some(AppLanguage::Swift),
                    "multi" => Some(AppLanguage::Multi),
                    _ => None,
                };
            }
        }
    }
    if root.join("pubspec.yaml").exists() {
        return Some(AppLanguage::Flutter);
    }
    if root.join("build.gradle.kts").exists() || root.join("build.gradle").exists() {
        return Some(AppLanguage::Kotlin);
    }
    if root.join("Package.swift").exists() {
        return Some(AppLanguage::Swift);
    }
    if let Ok(content) = std::fs::read_to_string(root.join("package.json")) {
        if content.contains("\"react-native\"") {
            return Some(AppLanguage::ReactNative);
        }
    }
    if root.join("ObjcBridge.h").exists() && root.join("ObjcBridge.m").exists() {
        return Some(AppLanguage::ObjC);
    }
    None
}

pub(crate) fn manifest_is_app(root: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(root.join("mgc.toml")) {
        if let Ok(v) = toml::from_str::<toml::Value>(&content) {
            if let Some(eco) = v.get("ecosystem").and_then(|e| e.as_str()) {
                if eco == "app" {
                    return true;
                }
            }
            if v.get("app").is_some() {
                return true;
            }
        }
    }
    detect_language(root).is_some()
}
