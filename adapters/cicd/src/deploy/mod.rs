//! Deployment targets (dry-run P1).

use mgc_types::MgResult;

use mgc_exec::run::{run as mgc_run, ExecOptions};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum DeployTarget {
    Aws,
    Cloudflare,
    Gcp,
}

pub async fn deploy(target: DeployTarget, dry_run: bool) -> MgResult<String> {
    // (tool, args) theo allowlist mgc-exec — dry_run chỉ in lệnh, không chạy
    let (tool, args): (&str, Vec<&str>) = match target {
        DeployTarget::Aws => ("aws", vec!["deploy"]),
        DeployTarget::Cloudflare => ("wrangler", vec!["publish"]),
        DeployTarget::Gcp => ("gcloud", vec!["deploy"]),
    };
    let summary = format!("{} {}", tool, args.join(" "));

    if dry_run {
        return Ok(format!("{} (dry_run: true)", summary));
    }

    let opts = ExecOptions {
        cwd: std::env::current_dir().ok(),
        ..Default::default()
    };
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let report = mgc_run(tool, &owned, &opts)
        .map_err(|e| mgc_types::MgError::Other(format!("{summary}: {e}")))?;
    if report.exit_code != 0 {
        return Err(mgc_types::MgError::Other(format!(
            "{} exited with {}: {}",
            summary,
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }
    Ok(format!("{} (deployed)", summary))
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
