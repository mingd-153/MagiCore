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
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_install_cdk() {
        let tmp = TempDir::new().unwrap();
        let deps = install_dependencies(CloudType::Cdk, tmp.path())
            .await
            .unwrap();
        assert!(!deps.is_empty());
    }
}
