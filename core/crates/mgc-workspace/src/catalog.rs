//! Catalogs protocol parser and resolution (PNPM 11 / Bun parity).
//!
//! Allows defining centralized dependency versions in `magicore.workspace.toml`
//! under `[catalogs.default]` or named catalogs `[catalogs.<name>]`.
//!
//! Sub-packages can reference them in `package.json` as:
//! `"react": "catalog:"` or `"lodash": "catalog:utilities"`.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct WorkspaceCatalogs {
    #[serde(default)]
    pub default: HashMap<String, String>,
    #[serde(flatten)]
    pub named: HashMap<String, HashMap<String, String>>,
}

/// Load catalogs from `magicore.workspace.toml` if present.
pub fn load_workspace_catalogs(project_root: &Path) -> anyhow::Result<Option<WorkspaceCatalogs>> {
    let ws_path = project_root.join("magicore.workspace.toml");
    if !ws_path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&ws_path)?;
    #[derive(serde::Deserialize)]
    struct Config {
        #[serde(default)]
        catalogs: Option<WorkspaceCatalogs>,
    }
    let parsed: Config = toml::from_str(&contents)?;
    Ok(parsed.catalogs)
}

/// Resolve a `catalog:` specifier to its actual version string.
/// - `"catalog:"` or `"catalog:default"` -> looks up in `catalogs.default`
/// - `"catalog:name"` -> looks up in `catalogs.name`
pub fn resolve_catalog_specifier(
    package_name: &str,
    specifier: &str,
    catalogs: &WorkspaceCatalogs,
) -> Option<String> {
    if !specifier.starts_with("catalog:") {
        return None;
    }
    let catalog_name = specifier.strip_prefix("catalog:").unwrap().trim();
    if catalog_name.is_empty() || catalog_name == "default" {
        catalogs.default.get(package_name).cloned()
    } else {
        catalogs
            .named
            .get(catalog_name)
            .and_then(|cat| cat.get(package_name))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_default_catalog() {
        let mut catalogs = WorkspaceCatalogs::default();
        catalogs
            .default
            .insert("react".to_string(), "^19.0.0".to_string());

        assert_eq!(
            resolve_catalog_specifier("react", "catalog:", &catalogs),
            Some("^19.0.0".to_string())
        );
        assert_eq!(
            resolve_catalog_specifier("react", "catalog:default", &catalogs),
            Some("^19.0.0".to_string())
        );
        assert_eq!(
            resolve_catalog_specifier("unknown", "catalog:", &catalogs),
            None
        );
    }

    #[test]
    fn test_resolve_named_catalog() {
        let mut catalogs = WorkspaceCatalogs::default();
        let mut utils = HashMap::new();
        utils.insert("lodash".to_string(), "^4.17.21".to_string());
        catalogs.named.insert("utilities".to_string(), utils);

        assert_eq!(
            resolve_catalog_specifier("lodash", "catalog:utilities", &catalogs),
            Some("^4.17.21".to_string())
        );
    }
}
