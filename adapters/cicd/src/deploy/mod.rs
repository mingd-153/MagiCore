//! Deployment targets (dry-run P1).

use mgc_types::MgResult;

#[derive(Debug, Clone)]
pub enum DeployTarget {
    Aws,
    Cloudflare,
    Gcp,
}

pub async fn deploy(target: DeployTarget, dry_run: bool) -> MgResult<String> {
    let cmd = match target {
        DeployTarget::Aws => "aws deploy (stub)",
        DeployTarget::Cloudflare => "wrangler publish",
        DeployTarget::Gcp => "gcloud deploy",
    };

    Ok(format!("{} (dry_run: {})", cmd, dry_run))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deploy_dry_run() {
        let result = deploy(DeployTarget::Aws, true).await.unwrap();
        assert!(result.contains("dry_run: true"));
    }
}
