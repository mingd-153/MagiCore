//! SBOM generation bridge for lib lockfiles.
//! Giữ SBOM ở module riêng để nhánh adapter không bị lẫn trách nhiệm release.

use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};
use mgc_types::MgResult;

pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}
