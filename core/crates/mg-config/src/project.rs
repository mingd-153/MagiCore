/// Project-level configuration (mg.toml)
///
/// Stores ecosystem/core, scaffold settings, and per-core configuration.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectExecutionConfig {
    #[serde(default = "default_execution_architecture")]
    pub architecture: String,
    #[serde(default = "default_execution_lane")]
    pub lane: String,
    #[serde(default = "default_execution_compatibility_layer")]
    pub compatibility_layer: String,
    #[serde(default)]
    pub native_targets: Vec<String>,
}

fn default_execution_architecture() -> String {
    "rust-first".to_string()
}

fn default_execution_lane() -> String {
    "compatibility-shell".to_string()
}

fn default_execution_compatibility_layer() -> String {
    "js".to_string()
}

impl Default for ProjectExecutionConfig {
    fn default() -> Self {
        Self {
            architecture: default_execution_architecture(),
            lane: default_execution_lane(),
            compatibility_layer: default_execution_compatibility_layer(),
            native_targets: vec![],
        }
    }
}

fn default_execution_for(ecosystem: &str, features: &[String]) -> ProjectExecutionConfig {
    let has_ts = features.iter().any(|feature| {
        let value = feature.trim().to_ascii_lowercase();
        value == "ts" || value == "typescript"
    });

    match ecosystem {
        "web" => ProjectExecutionConfig {
            architecture: "rust-first".to_string(),
            lane: "compatibility-shell".to_string(),
            compatibility_layer: if has_ts { "ts" } else { "js" }.to_string(),
            native_targets: vec![
                "frontend-executable".to_string(),
                "backend-executable".to_string(),
                "wasm-bridge".to_string(),
            ],
        },
        "game" | "app" | "lib" => ProjectExecutionConfig {
            architecture: "native-first".to_string(),
            lane: "native-ready".to_string(),
            compatibility_layer: "none".to_string(),
            native_targets: vec!["binary".to_string()],
        },
        _ => ProjectExecutionConfig {
            architecture: "rust-first".to_string(),
            lane: "compatibility-shell".to_string(),
            compatibility_layer: "none".to_string(),
            native_targets: vec![],
        },
    }
}

/// Project config saved by `mg init` and read by all commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name
    pub name: String,
    /// Project version
    #[serde(default = "default_version")]
    pub version: String,
    /// Ecosystem / core type (web, game, ai, cloud, iot, app, lib)
    pub ecosystem: String,
    /// Mode (frontend, backend, fullstack, monorepo) — web core only
    #[serde(default)]
    pub mode: String,
    /// Frameworks used (e.g. ["react-vite"] or ["node", "express"])
    #[serde(default)]
    pub frameworks: Vec<String>,
    /// Template path used during scaffold
    #[serde(default)]
    pub template: String,
    /// Features selected (e.g. ["ts", "tailwind"])
    #[serde(default)]
    pub features: Vec<String>,
    /// Execution strategy / runtime lane metadata
    #[serde(default)]
    pub execution: ProjectExecutionConfig,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>, ecosystem: impl Into<String>) -> Self {
        let ecosystem = ecosystem.into();
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            ecosystem: ecosystem.clone(),
            mode: String::new(),
            frameworks: vec![],
            template: String::new(),
            features: vec![],
            execution: default_execution_for(&ecosystem, &[]),
        }
    }

    pub fn from_scaffold(
        name: impl Into<String>,
        ecosystem: impl Into<String>,
        mode: impl Into<String>,
        frameworks: Vec<String>,
        template: impl Into<String>,
        features: Vec<String>,
    ) -> Self {
        let ecosystem = ecosystem.into();
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            ecosystem: ecosystem.clone(),
            mode: mode.into(),
            frameworks,
            template: template.into(),
            execution: default_execution_for(&ecosystem, &features),
            features,
        }
    }

    /// Load from project root (mg.toml)
    pub fn load(project_root: &Path) -> Result<Option<Self>, anyhow::Error> {
        let path = project_root.join("mg.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    /// Save to project root (mg.toml)
    pub fn save(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        let path = project_root.join("mg.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Detect ecosystem from project files (fallback if no mg.toml)
    pub fn auto_detect(project_root: &Path) -> Option<String> {
        if project_root.join("package.json").exists() {
            return Some("web".to_string());
        }
        if project_root.join("Cargo.toml").exists() {
            return Some("lib".to_string());
        }
        if project_root.join("pyproject.toml").exists() {
            return Some("ai".to_string());
        }
        None
    }

    /// Find project root by looking for mg.toml, package.json, or Cargo.toml.
    ///
    /// - `mg.toml` checked in CWD and ALL parent directories (monorepo support).
    /// - `package.json` / `Cargo.toml` checked in CWD ONLY.
    pub fn find_project_root(from: &Path) -> Option<PathBuf> {
        if from.join("mg.toml").exists()
            || from.join("package.json").exists()
            || from.join("Cargo.toml").exists()
        {
            return Some(from.to_path_buf());
        }

        let mut current = from.parent();
        while let Some(dir) = current {
            if dir.join("mg.toml").exists() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectConfig;
    use std::path::PathBuf;

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "megagate-mg-config-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn new_sets_version_and_fields() {
        let cfg = ProjectConfig::new("my-proj", "web");
        assert_eq!(cfg.name, "my-proj");
        assert_eq!(cfg.version, "0.1.0");
        assert_eq!(cfg.ecosystem, "web");
        assert!(cfg.mode.is_empty());
        assert!(cfg.frameworks.is_empty());
        assert_eq!(cfg.execution.architecture, "rust-first");
        assert_eq!(cfg.execution.lane, "compatibility-shell");
        assert_eq!(cfg.execution.compatibility_layer, "js");
    }

    #[test]
    fn from_scaffold_sets_all_fields() {
        let cfg = ProjectConfig::from_scaffold(
            "my-app",
            "web",
            "frontend",
            vec!["react-vite".to_string()],
            "templates/web/frontend/react-vite",
            vec!["ts".to_string(), "tailwind".to_string()],
        );
        assert_eq!(cfg.name, "my-app");
        assert_eq!(cfg.ecosystem, "web");
        assert_eq!(cfg.mode, "frontend");
        assert_eq!(cfg.frameworks, vec!["react-vite"]);
        assert_eq!(cfg.template, "templates/web/frontend/react-vite");
        assert_eq!(cfg.features, vec!["ts", "tailwind"]);
        assert_eq!(cfg.execution.architecture, "rust-first");
        assert_eq!(cfg.execution.lane, "compatibility-shell");
        assert_eq!(cfg.execution.compatibility_layer, "ts");
        assert!(cfg.execution.native_targets.contains(&"frontend-executable".to_string()));
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = temp_test_dir("load-missing");
        assert!(ProjectConfig::load(&dir).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_valid_file() {
        let dir = temp_test_dir("load-valid");
        let cfg = ProjectConfig::new("test", "web");
        cfg.save(&dir).unwrap();
        let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.ecosystem, "web");
        assert_eq!(loaded.version, "0.1.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_invalid_toml_returns_err() {
        let dir = temp_test_dir("load-invalid");
        std::fs::write(dir.join("mg.toml"), "[[[invalid").unwrap();
        assert!(ProjectConfig::load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_file_at_root() {
        let dir = temp_test_dir("save-dir");
        let cfg = ProjectConfig::new("save-test", "lib");
        cfg.save(&dir).unwrap();
        let path = dir.join("mg.toml");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("save-test"));
        assert!(content.contains("lib"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_roundtrip() {
        let dir = temp_test_dir("save-roundtrip");
        let cfg = ProjectConfig::new("roundtrip", "ai");
        cfg.save(&dir).unwrap();
        let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
        assert_eq!(loaded.name, "roundtrip");
        assert_eq!(loaded.ecosystem, "ai");
        assert_eq!(loaded.version, "0.1.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_roundtrip_with_scaffold_fields() {
        let dir = temp_test_dir("save-scaffold");
        let cfg = ProjectConfig::from_scaffold(
            "roundtrip",
            "web",
            "frontend",
            vec!["react-vite".to_string()],
            "templates/web/frontend/react-vite",
            vec!["ts".to_string()],
        );
        cfg.save(&dir).unwrap();
        let loaded = ProjectConfig::load(&dir).unwrap().unwrap();
        assert_eq!(loaded.name, "roundtrip");
        assert_eq!(loaded.ecosystem, "web");
        assert_eq!(loaded.mode, "frontend");
        assert_eq!(loaded.frameworks, vec!["react-vite"]);
        assert_eq!(loaded.template, "templates/web/frontend/react-vite");
        assert_eq!(loaded.features, vec!["ts"]);
        assert_eq!(loaded.execution.compatibility_layer, "ts");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_detect_package_json_returns_web() {
        let dir = temp_test_dir("auto-web");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(ProjectConfig::auto_detect(&dir), Some("web".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_detect_cargo_toml_returns_lib() {
        let dir = temp_test_dir("auto-lib");
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        assert_eq!(ProjectConfig::auto_detect(&dir), Some("lib".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_detect_pyproject_toml_returns_ai() {
        let dir = temp_test_dir("auto-ai");
        std::fs::write(dir.join("pyproject.toml"), "").unwrap();
        assert_eq!(ProjectConfig::auto_detect(&dir), Some("ai".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_detect_no_manifest_returns_none() {
        let dir = temp_test_dir("auto-none");
        assert_eq!(ProjectConfig::auto_detect(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_project_root_mg_toml_in_cwd() {
        let dir = temp_test_dir("root-cwd-mg");
        std::fs::write(dir.join("mg.toml"), "").unwrap();
        assert_eq!(ProjectConfig::find_project_root(&dir), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_project_root_package_json_in_cwd() {
        let dir = temp_test_dir("root-cwd-pkg");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert_eq!(ProjectConfig::find_project_root(&dir), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_project_root_no_match_returns_none() {
        let dir = temp_test_dir("root-none");
        assert_eq!(ProjectConfig::find_project_root(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_project_root_ignores_parent_package_json_without_mg_toml() {
        let root = temp_test_dir("parent-package-json");
        let child = root.join("apps").join("frontend");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();

        assert_eq!(ProjectConfig::find_project_root(&child), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_project_root_accepts_parent_mg_toml() {
        let root = temp_test_dir("parent-mg-toml");
        let child = root.join("apps").join("frontend");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("mg.toml"), "").unwrap();

        assert_eq!(ProjectConfig::find_project_root(&child), Some(root.clone()));
        let _ = std::fs::remove_dir_all(root);
    }
}
