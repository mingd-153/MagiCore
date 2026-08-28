//! SBOM generation bridge for IoT lockfiles.
//! Giữ SBOM tách khỏi adapter để dễ maintain.

use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};
use mgc_types::MgResult;

pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}
