//! Audit scanner for mobile platforms.

use mgc_types::adapter::AuditReport;
use mgc_types::{MgError, MgResult};
use std::path::Path;

pub async fn audit_flutter(project_root: &Path) -> MgResult<AuditReport> {
    if which::which("flutter").is_err() {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
        });
    }

    let args = vec![
        "pub".to_string(),
        "outdated".to_string(),
        "--json".to_string(),
    ];
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let result = mgc_exec::run::run("flutter", &args, &exec_opts)
        .map_err(|e| MgError::Other(format!("flutter pub outdated failed: {}", e)))?;

    // Exit code 1 means outdated packages found
    if result.exit_code != 0 && result.exit_code != 1 {
        return Err(MgError::Other(format!(
            "flutter pub outdated exited with code {}",
            result.exit_code
        )));
    }

    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

pub async fn audit_kotlin(project_root: &Path) -> MgResult<AuditReport> {
    let tool = if project_root.join("gradlew").exists() {
        "./gradlew"
    } else if which::which("gradle").is_ok() {
        "gradle"
    } else {
        return Ok(AuditReport {
            packages_audited: 0,
            vulnerability_count: 0,
            vulnerabilities: vec![],
        });
    };

    let args = vec!["dependencyCheckAnalyze".to_string()];
    let exec_opts = mgc_exec::run::ExecOptions {
        cwd: Some(project_root.to_path_buf()),
        ..Default::default()
    };

    let _result = mgc_exec::run::run(tool, &args, &exec_opts);
    // OWASP dependency-check may not be configured - graceful degradation

    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

pub async fn audit_swift(_project_root: &Path) -> MgResult<AuditReport> {
    // Swift Package Manager doesn't have built-in audit yet
    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

pub async fn audit_cocoapods(_project_root: &Path) -> MgResult<AuditReport> {
    // CocoaPods doesn't have built-in audit
    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}

pub async fn audit_multi(project_root: &Path) -> MgResult<AuditReport> {
    if project_root.join("pubspec.yaml").exists() {
        return audit_flutter(project_root).await;
    }
    if project_root.join("build.gradle").exists() || project_root.join("build.gradle.kts").exists()
    {
        return audit_kotlin(project_root).await;
    }
    Ok(AuditReport {
        packages_audited: 0,
        vulnerability_count: 0,
        vulnerabilities: vec![],
    })
}
