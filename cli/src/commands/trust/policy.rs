//! Trust policy enforcement for CI/CD
//! Thực thi policy trust cho CI/CD

use anyhow::Result;
use mg_lockfile::{verify_lockfile, VerificationStatus};
use std::path::Path;

/// Policy mode — Chế độ policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    /// Strict: require signed lockfile (fail on unsigned/tampered)
    /// Nghiêm ngặt: yêu cầu lockfile đã ký (fail nếu chưa ký/tampered)
    Strict,
    
    /// Warn: warn on unsigned, fail on tampered
    /// Cảnh báo: cảnh báo nếu chưa ký, fail nếu tampered
    Warn,
    
    /// Audit: log only, never fail
    /// Audit: chỉ log, không fail
    Audit,
}

impl PolicyMode {
    /// Parse from environment variable — Parse từ biến môi trường
    pub fn from_env() -> Self {
        // R5.1 FIX (AUDIT VÒNG 2): Default to strict in CI, warn in dev
        let default_mode = if is_ci_environment() {
            Self::Strict  // ✅ Stricter default in CI (require signed)
        } else {
            Self::Warn    // ✅ Relaxed default in dev (allow unsigned)
        };
        
        match std::env::var("MG_TRUST_POLICY").as_deref() {
            Ok("strict") => Self::Strict,
            Ok("warn") => Self::Warn,
            Ok("audit") => Self::Audit,
            _ => default_mode,  // ✅ CI-aware default
        }
    }
}

/// Enforce trust policy — Thực thi trust policy
pub fn enforce_policy(lockfile_path: &Path, mode: PolicyMode) -> Result<()> {
    if !lockfile_path.exists() {
        return Ok(()); // No lockfile = nothing to enforce
    }
    
    let status = verify_lockfile(lockfile_path)?;
    
    match (mode, &status) {
        // Strict mode: fail on unsigned or invalid
        (PolicyMode::Strict, VerificationStatus::Unsigned) => {
            anyhow::bail!(
                "POLICY VIOLATION (strict): Lockfile not signed. Run 'mg trust sign' or set MG_TRUST_POLICY=warn"
            );
        }
        (PolicyMode::Strict, VerificationStatus::Tampered(msg)) => {
            anyhow::bail!("POLICY VIOLATION (strict): Lockfile tampered: {}", msg);
        }
        (PolicyMode::Strict, VerificationStatus::InvalidSignature(msg)) => {
            anyhow::bail!("POLICY VIOLATION (strict): Invalid signature: {}", msg);
        }
        (PolicyMode::Strict, VerificationStatus::Valid) => {
            eprintln!("✓ Trust policy: Lockfile signature valid (strict mode)");
        }
        
        // Warn mode: warn on unsigned, fail on tampered
        (PolicyMode::Warn, VerificationStatus::Unsigned) => {
            eprintln!("⚠ Trust policy: Lockfile not signed (warn mode)");
        }
        (PolicyMode::Warn, VerificationStatus::Tampered(msg)) => {
            anyhow::bail!("POLICY VIOLATION (warn): Lockfile tampered: {}", msg);
        }
        (PolicyMode::Warn, VerificationStatus::InvalidSignature(msg)) => {
            anyhow::bail!("POLICY VIOLATION (warn): Invalid signature: {}", msg);
        }
        (PolicyMode::Warn, VerificationStatus::Valid) => {
            eprintln!("✓ Trust policy: Lockfile signature valid (warn mode)");
        }
        
        // Audit mode: log only, never fail
        (PolicyMode::Audit, status) => {
            eprintln!("ℹ Trust policy audit: {:?}", status);
        }
    }
    
    Ok(())
}

/// Check if CI environment — Kiểm tra môi trường CI
pub fn is_ci_environment() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
        || std::env::var("CIRCLECI").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
        || std::env::var("TRAVIS").is_ok()
}

/// Auto-detect and enforce policy in CI — Tự động detect và enforce policy trong CI
pub fn auto_enforce_in_ci(lockfile_path: &Path) -> Result<()> {
    if !is_ci_environment() {
        return Ok(()); // Skip policy in dev environment
    }
    
    let mode = PolicyMode::from_env();
    eprintln!("🔒 CI detected, enforcing trust policy: {:?}", mode);
    enforce_policy(lockfile_path, mode)
}
