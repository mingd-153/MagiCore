//! SBOM generation bridge for app lockfiles.
//! Tách SBOM khỏi adapter để module chính giữ đúng một trách nhiệm.

use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};
use mgc_types::MgResult;

pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}
