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
    }
}
