use megagate_types::error::Result;
use megagate_types::lockfile::LockfileV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOM {
    pub bom_format: String,
    pub spec_version: String,
    pub serial_number: String,
    pub version: u32,
    pub metadata: SBOMMetadata,
    pub components: Vec<SBOMComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMMetadata {
    pub timestamp: String,
    pub tool: SBOMTool,
    pub component: SBOMComponentRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMTool {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMComponentRef {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMComponent {
    pub name: String,
    pub version: String,
    pub supplier: Option<String>,
    pub author: Option<String>,
    pub licenses: Vec<SBOMLicense>,
    pub copyright: Option<String>,
    pub purl: String,
    pub hashes: Vec<SBOMHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMLicense {
    pub license: SBOLicenseInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOLicenseInfo {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SBOMHash {
    pub alg: String,
    pub content: String,
}

pub struct SBOMGenerator;

impl SBOMGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, lockfile: &LockfileV1) -> Result<SBOM> {
        let mut components = Vec::new();

        for (_, pkg) in &lockfile.packages {
            let licenses = pkg.provenance.as_ref()
                .and_then(|p| p.repository_url.as_ref())
                .map(|_| vec![SBOMLicense {
                    license: SBOLicenseInfo {
                        id: "MIT".to_string(),
                        name: Some("MIT License".to_string()),
                    },
                }])
                .unwrap_or_else(|| vec![SBOMLicense {
                    license: SBOLicenseInfo {
                        id: "UNKNOWN".to_string(),
                        name: None,
                    },
                }]);

            components.push(SBOMComponent {
                name: pkg.name.clone(),
                version: pkg.version.to_string(),
                supplier: pkg.provenance.as_ref().and_then(|p| p.repository_url.clone()),
                author: None,
                licenses,
                copyright: None,
                purl: format!("pkg:npm/{}@{}", pkg.name, pkg.version),
                hashes: vec![SBOMHash {
                    alg: "SHA-512".to_string(),
                    content: pkg.integrity.strip_prefix("sha512-").unwrap_or(&pkg.integrity).to_string(),
                }],
            });
        }

        Ok(SBOM {
            bom_format: "CycloneDX".to_string(),
            spec_version: "1.5".to_string(),
            serial_number: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
            version: 1,
            metadata: SBOMMetadata {
                timestamp: chrono::Utc::now().to_rfc3339(),
                tool: SBOMTool {
                    name: "megagate".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                component: SBOMComponentRef {
                    name: "project".to_string(),
                    version: "1.0.0".to_string(),
                },
            },
            components,
        })
    }

    pub fn check_licenses(&self, lockfile: &LockfileV1, allowed: &[String]) -> Result<LicenseReport> {
        let mut violations = Vec::new();
        let mut allowed_count = 0;
        let mut total = 0;

        for (_, pkg) in &lockfile.packages {
            total += 1;
            let is_allowed = pkg.provenance.as_ref()
                .and_then(|p| p.repository_url.as_ref())
                .map(|_| allowed.contains(&"MIT".to_string()))
                .unwrap_or(false);

            if is_allowed {
                allowed_count += 1;
            } else {
                violations.push(LicenseViolation {
                    package: format!("{}@{}", pkg.name, pkg.version),
                    license: "UNKNOWN".to_string(),
                    allowed: false,
                });
            }
        }

        Ok(LicenseReport {
            total_packages: total,
            allowed_packages: allowed_count,
            violations,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseReport {
    pub total_packages: usize,
    pub allowed_packages: usize,
    pub violations: Vec<LicenseViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseViolation {
    pub package: String,
    pub license: String,
    pub allowed: bool,
}

impl Default for SBOMGenerator {
    fn default() -> Self {
        Self::new()
    }
}