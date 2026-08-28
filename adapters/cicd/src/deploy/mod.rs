//! Deployment targets (dry-run P1).

use mgc_types::MgResult;

use mgc_exec::run::{run as mgc_run, ExecOptions};

#[derive(Debug, Clone)]
pub enum DeployTarget {
    Aws,
    Cloudflare,
    Gcp,
}

pub async fn deploy(target: DeployTarget, dry_run: bool) -> MgResult<String> {
    // Real CLI commands per service (placeholder args - production needs specifics)
    let (tool, args): (&str, Vec<&str>) = match target {
        // AWS CLI: placeholder "cloudformation deploy" (actual needs --stack-name, --template-file, etc.)
        DeployTarget::Aws => ("aws", vec!["cloudformation", "deploy"]),
        // Wrangler: "wrangler deploy" (formerly "wrangler publish" in v2, v3 uses "deploy")
        DeployTarget::Cloudflare => ("wrangler", vec!["deploy"]),
        // Gcloud: placeholder "app deploy" (actual needs --project, app.yaml, etc.)
        DeployTarget::Gcp => ("gcloud", vec!["app", "deploy"]),
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
#[path = "test/mod_test.rs"]
mod tests;
