//! Cloud deployment (dry-run P1).

use crate::cloud_type::CloudType;
use mgc_types::MgResult;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DeployResult {
    pub dry_run: bool,
    pub changes: Vec<String>,
}

pub async fn deploy(framework: CloudType, _root: &Path, dry_run: bool) -> MgResult<DeployResult> {
    let changes = match framework {
        CloudType::Cdk => vec!["cdk synth output".to_string()],
        CloudType::Pulumi => vec!["pulumi preview".to_string()],
        CloudType::Terraform => vec!["terraform plan".to_string()],
        CloudType::Cloudflare => vec!["wrangler publish --dry-run".to_string()],
    };

    Ok(DeployResult { dry_run, changes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_deploy_dry_run() {
        let tmp = TempDir::new().unwrap();
        let result = deploy(CloudType::Terraform, tmp.path(), true)
            .await
            .unwrap();
        assert!(result.dry_run);
    }
}
