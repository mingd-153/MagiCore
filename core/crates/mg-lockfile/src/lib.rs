pub mod serialization;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolutionMeta {
    pub state: String,
    pub store: String,
    pub package_count: usize,
}

impl Default for ResolutionMeta {
    fn default() -> Self {
        Self {
            state: "pending".to_string(),
            store: "megagate".to_string(),
            package_count: 0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockPackage {
    pub name: String,
    pub version: String,
    pub integrity: Option<String>,
    #[serde(default)]
    pub direct: bool,
    #[serde(default)]
    pub dev: bool,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceLock {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub package_count: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub core: String,
    pub mode: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub resolution: ResolutionMeta,
    #[serde(rename = "workspace", default)]
    pub workspaces: Vec<WorkspaceLock>,
    #[serde(rename = "package", default)]
    pub packages: Vec<LockPackage>,
}

impl Lockfile {
    pub fn new(core: impl Into<String>, mode: impl Into<String>) -> Self {
        Self {
            version: 1,
            core: core.into(),
            mode: mode.into(),
            frameworks: vec![],
            resolution: ResolutionMeta::default(),
            workspaces: vec![],
            packages: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization;

    #[test]
    fn test_resolution_meta_default() {
        let meta = ResolutionMeta::default();
        assert_eq!(meta.state, "pending");
        assert_eq!(meta.store, "megagate");
        assert_eq!(meta.package_count, 0);
    }

    #[test]
    fn test_lockfile_new_defaults() {
        let lock = Lockfile::new("test-core", "test-mode");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.core, "test-core");
        assert_eq!(lock.mode, "test-mode");
        assert!(lock.frameworks.is_empty());
        assert_eq!(lock.resolution.state, "pending");
        assert!(lock.workspaces.is_empty());
        assert!(lock.packages.is_empty());
    }

    #[test]
    fn test_json_roundtrip() {
        let mut lock = Lockfile::new("backend", "api");
        lock.frameworks = vec!["actix-web".to_string()];
        lock.resolution = ResolutionMeta {
            state: "locked".to_string(),
            store: "megagate".to_string(),
            package_count: 2,
        };
        lock.packages.push(LockPackage {
            name: "serde".to_string(),
            version: "1.0.200".to_string(),
            integrity: None,
            direct: true,
            dev: false,
            dependencies: vec![],
        });
        lock.packages.push(LockPackage {
            name: "tokio".to_string(),
            version: "1.38.0".to_string(),
            integrity: Some("sha256-abc".to_string()),
            direct: false,
            dev: true,
            dependencies: vec!["bytes@1.6.0".to_string()],
        });

        let json = serialization::to_json(&lock).unwrap();
        let parsed: Lockfile = serialization::from_json(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.core, "backend");
        assert_eq!(parsed.mode, "api");
        assert_eq!(parsed.frameworks, vec!["actix-web"]);
        assert_eq!(parsed.resolution.package_count, 2);
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].name, "serde");
        assert_eq!(parsed.packages[0].integrity, None);
        assert!(parsed.packages[0].direct);
        assert!(!parsed.packages[0].dev);
        assert!(parsed.packages[0].dependencies.is_empty());
        assert_eq!(parsed.packages[1].name, "tokio");
        assert_eq!(parsed.packages[1].integrity, Some("sha256-abc".to_string()));
        assert!(!parsed.packages[1].direct);
        assert!(parsed.packages[1].dev);
    }

    #[test]
    fn test_json_empty_lists() {
        let lock = Lockfile::new("cli", "rust");
        let json = serialization::to_json(&lock).unwrap();
        let parsed: Lockfile = serialization::from_json(&json).unwrap();
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert!(parsed.frameworks.is_empty());
    }

    #[test]
    fn test_toml_roundtrip() {
        let mut lock = Lockfile::new("web", "frontend");
        lock.frameworks = vec!["react-vite".to_string()];
        lock.resolution = ResolutionMeta {
            state: "locked".to_string(),
            store: "megagate".to_string(),
            package_count: 1,
        };
        lock.packages.push(LockPackage {
            name: "tailwindcss".to_string(),
            version: "4.3.2".to_string(),
            integrity: Some("sha512-test".to_string()),
            direct: true,
            dev: false,
            dependencies: vec!["@tailwindcss/node@4.3.2".to_string()],
        });

        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();

        assert_eq!(parsed.core, "web");
        assert_eq!(parsed.mode, "frontend");
        assert_eq!(parsed.frameworks, vec!["react-vite"]);
        assert_eq!(parsed.resolution.package_count, 1);
        assert!(parsed.workspaces.is_empty());
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "tailwindcss");
        assert_eq!(parsed.packages[0].version, "4.3.2");
        assert_eq!(
            parsed.packages[0].integrity,
            Some("sha512-test".to_string())
        );
        assert!(parsed.packages[0].direct);
        assert!(!parsed.packages[0].dev);
    }

    #[test]
    fn test_toml_empty_roundtrip() {
        let lock = Lockfile::new("empty", "test");
        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();
        assert_eq!(parsed.core, "empty");
        assert_eq!(parsed.mode, "test");
        assert!(parsed.packages.is_empty());
        assert!(parsed.workspaces.is_empty());
    }

    #[test]
    fn test_toml_with_workspaces() {
        let mut lock = Lockfile::new("monorepo", "fullstack");
        lock.workspaces.push(WorkspaceLock {
            path: "packages/web".to_string(),
            name: "web".to_string(),
            mode: "frontend".to_string(),
            frameworks: vec!["react".to_string()],
            package_count: 5,
        });
        lock.workspaces.push(WorkspaceLock {
            path: "packages/api".to_string(),
            name: "api".to_string(),
            mode: "backend".to_string(),
            frameworks: vec![],
            package_count: 3,
        });

        let toml = serialization::to_toml(&lock).unwrap();
        let parsed: Lockfile = serialization::from_toml(&toml).unwrap();
        assert_eq!(parsed.workspaces.len(), 2);
        assert_eq!(parsed.workspaces[0].path, "packages/web");
        assert_eq!(parsed.workspaces[0].name, "web");
        assert_eq!(parsed.workspaces[0].mode, "frontend");
        assert_eq!(parsed.workspaces[0].frameworks, vec!["react"]);
        assert_eq!(parsed.workspaces[0].package_count, 5);
        assert_eq!(parsed.workspaces[1].path, "packages/api");
        assert_eq!(parsed.workspaces[1].name, "api");
        assert_eq!(parsed.workspaces[1].package_count, 3);
    }

    #[test]
    fn test_workspace_lock_defaults() {
        let ws = WorkspaceLock {
            path: "libs/util".to_string(),
            ..Default::default()
        };
        assert_eq!(ws.path, "libs/util");
        assert!(ws.name.is_empty());
        assert!(ws.mode.is_empty());
        assert!(ws.frameworks.is_empty());
        assert_eq!(ws.package_count, 0);
    }

    #[test]
    fn test_lock_package_defaults() {
        let pkg = LockPackage {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        };
        assert_eq!(pkg.integrity, None);
        assert!(!pkg.direct);
        assert!(!pkg.dev);
        assert!(pkg.dependencies.is_empty());
    }

    #[test]
    fn test_malformed_json() {
        let result = serialization::from_json::<Lockfile>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_toml() {
        let result = serialization::from_toml::<Lockfile>("[[[invalid toml]]]");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_fields_toml() {
        let minimal = r#"version = 1
core = "minimal"
mode = "test""#;
        let parsed: Lockfile = serialization::from_toml(minimal).unwrap();
        assert_eq!(parsed.core, "minimal");
        assert_eq!(parsed.mode, "test");
        assert_eq!(parsed.version, 1);
        assert!(parsed.frameworks.is_empty());
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert_eq!(parsed.resolution.state, "pending");
    }

    #[test]
    fn test_missing_fields_json() {
        let minimal = r#"{"version": 1, "core": "minimal", "mode": "test"}"#;
        let parsed: Lockfile = serialization::from_json(minimal).unwrap();
        assert_eq!(parsed.core, "minimal");
        assert_eq!(parsed.mode, "test");
        assert_eq!(parsed.version, 1);
        assert!(parsed.frameworks.is_empty());
        assert!(parsed.workspaces.is_empty());
        assert!(parsed.packages.is_empty());
        assert_eq!(parsed.resolution.state, "pending");
    }

    #[test]
    fn test_json_preserves_integrity_none() {
        let pkg = LockPackage {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
        };
        let json = serialization::to_json(&pkg).unwrap();
        let parsed: LockPackage = serialization::from_json(&json).unwrap();
        assert_eq!(parsed.integrity, None);
    }
}
