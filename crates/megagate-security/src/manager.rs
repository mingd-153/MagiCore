use crate::typosquat::TyposquatDetector;
use crate::slopsquat::SlopsquatDetector;
use crate::minimum_age::MinimumReleaseAge;
use crate::approve_builds::ApproveBuilds;
use crate::lockdown::LockdownManager;
use crate::provenance::ProvenanceVerifier;
use crate::sbom::SBOMGenerator;
use megagate_types::error::{MegagateError, Result};
use megagate_types::lockfile::LockfileV1;
use megagate_types::package::LockedPackage;
use megagate_types::config::MegagateConfig;
use std::sync::Arc;

pub struct SecurityManager {
    pub typosquat: Arc<TyposquatDetector>,
    pub slopsquat: Arc<SlopsquatDetector>,
    pub minimum_age: Arc<MinimumReleaseAge>,
    pub approve_builds: Arc<ApproveBuilds>,
    pub lockdown: Arc<LockdownManager>,
    pub provenance: Arc<ProvenanceVerifier>,
    pub sbom: Arc<SBOMGenerator>,
    pub minimum_release_age_hours: u32,
}

impl SecurityManager {
    pub fn new(config: &MegagateConfig) -> Self {
        Self {
            typosquat: Arc::new(TyposquatDetector::new(Vec::new())),
            slopsquat: Arc::new(SlopsquatDetector::new()),
            minimum_age: Arc::new(MinimumReleaseAge::new(config.minimum_release_age_hours)),
            approve_builds: Arc::new(ApproveBuilds::new()),
            lockdown: Arc::new(LockdownManager::new()),
            provenance: Arc::new(ProvenanceVerifier::default()),
            sbom: Arc::new(SBOMGenerator::new()),
            minimum_release_age_hours: config.minimum_release_age_hours,
        }
    }

    pub async fn check_package(&self, pkg: &LockedPackage, _registry: &str) -> Result<()> {
        if let Some(matches) = self.typosquat.check(&pkg.name).ok() {
            if matches.iter().any(|m| m.confidence > 0.8) {
                return Err(MegagateError::SecurityViolation(
                    format!("Typosquat detected for {}", pkg.name)
                ));
            }
        }

        if self.minimum_release_age_hours > 0 {
            self.minimum_age.check(pkg)?;
        }

        Ok(())
    }

    pub fn generate_sbom(&self, lockfile: &LockfileV1) -> Result<crate::sbom::SBOM> {
        self.sbom.generate(lockfile)
    }

    pub fn check_licenses(&self, lockfile: &LockfileV1, allowed: &[String]) -> Result<crate::sbom::LicenseReport> {
        self.sbom.check_licenses(lockfile, allowed)
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new(&MegagateConfig::default())
    }
}