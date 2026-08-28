//! Cloud deployment via mgc-exec passthrough.

use crate::cloud_type::CloudType;
use mgc_exec::run::{run as mgc_run, ExecOptions};
use mgc_types::{MgError, MgResult};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DeployResult {
    pub dry_run: bool,
    pub changes: Vec<String>,
    pub duration_ms: u64,
}

pub async fn deploy(framework: CloudType, root: &Path, dry_run: bool) -> MgResult<DeployResult> {
    let started = std::time::Instant::now();
    
    // Real commands per framework (placeholder args - production needs specifics)
    let (tool, args): (&str, Vec<&str>) = match framework {
        // AWS CDK: "cdk deploy" (actual needs --app, --stack-name, etc.)
        CloudType::Cdk => ("cdk", vec!["deploy"]),
        // Pulumi: "pulumi up" (actual needs --stack, --yes for non-interactive, etc.)
        CloudType::Pulumi => ("pulumi", vec!["up"]),
        // Terraform: "terraform apply" (actual needs -auto-approve, -var-file, etc.)
        CloudType::Terraform => ("terraform", vec!["apply"]),
        // Cloudflare Workers: "wrangler deploy"
        CloudType::Cloudflare => ("wrangler", vec!["deploy"]),
    };

    let summary = format!("{} {}", tool, args.join(" "));
    
    if dry_run {
        return Ok(DeployResult {
            dry_run: true,
            changes: vec![format!("{} (dry_run)", summary)],
            duration_ms: 0,
        });
    }

    let opts = ExecOptions {
        cwd: Some(root.to_path_buf()),
        ..Default::default()
    };
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let report = mgc_run(tool, &owned, &opts)
        .map_err(|e| MgError::Other(format!("{summary}: {e}")))?;
    
    if report.exit_code != 0 {
        return Err(MgError::Other(format!(
            "{} exited with {}: {}",
            summary,
            report.exit_code,
            report.stderr_tail.trim()
        )));
    }

    Ok(DeployResult {
        dry_run: false,
        changes: vec![format!("{} (deployed)", summary)],
        duration_ms: started.elapsed().as_millis() as u64,
    })
}


#[cfg(test)]
#[path = "test/mod_test.rs"]
mod tests;
