// SBOM generation for core-web — exports lockfile data as SBOM JSON.
// Sinh SBOM cho core-web — giữ logic xuất báo cáo khỏi adapter chính.
use mgc_lockfile::Lockfile;
use mgc_sbom::{SbomGenerator, SbomOptions};
use mgc_types::MgResult;

pub fn generate_sbom(lockfile: &Lockfile, options: SbomOptions) -> MgResult<String> {
    let generator = SbomGenerator::new(options);
    generator
        .generate_json(lockfile)
        .map_err(|e| mgc_types::MgError::Other(format!("SBOM generation failed: {e}")))
}
