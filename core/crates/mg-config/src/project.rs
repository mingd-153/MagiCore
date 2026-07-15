/// Project-level configuration (.megagate/project.toml)
///
/// Stores which ecosystem/core this project belongs to,
/// along with optional per-core settings.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Project config saved by `mg init` and read by all other commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name
    pub name: String,
    /// Project version
    pub version: String,
    /// Ecosystem / core type (web, game, ai, cloud, iot, app, lib)
    pub ecosystem: String,
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>, ecosystem: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            ecosystem: ecosystem.into(),
        }
    }

    /// Load from project root (.megagate/project.toml)
    pub fn load(project_root: &Path) -> Result<Option<Self>, anyhow::Error> {
        let path = project_root.join(".megagate").join("project.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(toml::from_str(&content)?))
    }

    /// Save to project root (.megagate/project.toml)
    pub fn save(&self, project_root: &Path) -> Result<(), anyhow::Error> {
        let dir = project_root.join(".megagate");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("project.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Detect ecosystem from project files (fallback if no config)
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

    /// Get the project root by looking for .megagate/ or package.json.
    ///
    /// - `.megagate/project.toml` is checked in CWD and ALL parent directories
    ///   (monorepo support).
    /// - `package.json` is checked in CWD ONLY (no parent walking — prevents detecting
    ///   unrelated parent projects).
    pub fn find_project_root(from: &Path) -> Option<PathBuf> {
        // CWD: check both .megagate/ and package.json
        if from.join(".megagate").join("project.toml").exists()
            || from.join("package.json").exists()
        {
            return Some(from.to_path_buf());
        }

        // Parent dirs: only .megagate/project.toml (user may be in a subdirectory)
        let mut current = from.parent();
        while let Some(dir) = current {
            if dir.join(".megagate").join("project.toml").exists() {
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
        let mg = dir.join(".megagate");
        std::fs::create_dir_all(&mg).unwrap();
        std::fs::write(mg.join("project.toml"), "[[[invalid").unwrap();
        assert!(ProjectConfig::load(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_directory_and_file() {
        let dir = temp_test_dir("save-dir");
        let cfg = ProjectConfig::new("save-test", "lib");
        cfg.save(&dir).unwrap();
        let path = dir.join(".megagate").join("project.toml");
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
    fn find_project_root_dot_megagate_in_cwd() {
        let dir = temp_test_dir("root-cwd-dot");
        std::fs::create_dir_all(dir.join(".megagate")).unwrap();
        std::fs::write(dir.join(".megagate").join("project.toml"), "").unwrap();
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
    fn find_project_root_ignores_parent_package_json_without_project_config() {
        let root = temp_test_dir("parent-package-json");
        let child = root.join("apps").join("frontend");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();

        assert_eq!(ProjectConfig::find_project_root(&child), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_project_root_accepts_parent_project_toml() {
        let root = temp_test_dir("parent-project-toml");
        let child = root.join("apps").join("frontend");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir_all(root.join(".megagate")).unwrap();
        std::fs::write(root.join(".megagate").join("project.toml"), "").unwrap();

        assert_eq!(ProjectConfig::find_project_root(&child), Some(root.clone()));
        let _ = std::fs::remove_dir_all(root);
    }
}
