//! Scaffold provenance — ghi lại nguồn gốc scaffold để track supply chain

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Scaffold provenance metadata - ghi vào project generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldProvenance {
    /// Template/scaffold name (e.g., "nextjs", "fastapi")
    pub template: String,
    /// Core kind (web, ai, app, lib)
    pub core: String,
    /// Version/tag resolved (e.g., "15.5.0", "latest")
    pub version: String,
    /// Registry URL used
    pub registry: Option<String>,
    /// Timestamp ISO 8601
    pub generated_at: String,
    /// mgc CLI version
    pub mgc_version: String,
    /// Layers used
    pub layers: Vec<String>,
}

impl ScaffoldProvenance {
    /// Create new provenance record
    pub fn new(
        template: String,
        core: String,
        version: String,
        registry: Option<String>,
        layers: Vec<String>,
    ) -> Self {
        Self {
            template,
            core,
            version,
            registry,
            generated_at: chrono::Utc::now().to_rfc3339(),
            mgc_version: env!("CARGO_PKG_VERSION").to_string(),
            layers,
        }
    }

    /// Write provenance to project directory
    pub fn write(&self, project_dir: &Path) -> Result<()> {
        let provenance_path = project_dir.join(".mgc-provenance.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(provenance_path, json)?;
        Ok(())
    }

    /// Read provenance from project directory
    pub fn read(project_dir: &Path) -> Result<Self> {
        let provenance_path = project_dir.join(".mgc-provenance.json");
        let content = std::fs::read_to_string(provenance_path)?;
        let provenance: Self = serde_json::from_str(&content)?;
        Ok(provenance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_provenance_write_read() {
        let dir = TempDir::new().unwrap();
        let prov = ScaffoldProvenance::new(
            "nextjs".to_string(),
            "web".to_string(),
            "15.5.0".to_string(),
            Some("https://registry.npmjs.org".to_string()),
            vec!["web/frontend/nextjs".to_string()],
        );

        prov.write(dir.path()).unwrap();

        let read = ScaffoldProvenance::read(dir.path()).unwrap();
        assert_eq!(read.template, "nextjs");
        assert_eq!(read.version, "15.5.0");
        assert_eq!(read.core, "web");
    }

    #[test]
    fn test_provenance_includes_timestamp() {
        let prov = ScaffoldProvenance::new(
            "fastapi".to_string(),
            "ai".to_string(),
            "0.115.0".to_string(),
            None,
            vec![],
        );
        assert!(!prov.generated_at.is_empty());
        assert!(prov.generated_at.contains('T')); // ISO 8601
    }

    #[test]
    fn test_provenance_includes_mgc_version() {
        let prov = ScaffoldProvenance::new(
            "flutter".to_string(),
            "app".to_string(),
            "stable".to_string(),
            None,
            vec![],
        );
        assert!(!prov.mgc_version.is_empty());
    }
}
