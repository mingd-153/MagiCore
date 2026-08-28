//! Cloud infrastructure dependency installation.

use crate::cloud_type::CloudType;
use mgc_types::MgResult;
use std::path::Path;

pub async fn install_dependencies(
    framework: CloudType,
    _project_root: &Path,
) -> MgResult<Vec<String>> {
    match framework {
        CloudType::Cdk | CloudType::Pulumi => {
            // Full npm-format resolver (like web core)
            Ok(vec!["aws-cdk-lib@2.0.0".to_string()])
        }
        CloudType::Terraform | CloudType::Cloudflare => {
            // Exec passthrough: terraform init
            Ok(vec![])
        }
    }
}


#[cfg(test)]
#[path = "../test/mod_test.rs"]
mod tests;
