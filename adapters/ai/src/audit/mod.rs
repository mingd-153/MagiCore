//! Model security audit cho AI adapter.
//! Scan pickle files, safetensors, malicious patterns.

use mgc_types::{MgError, MgResult};
use std::path::Path;

pub mod scanner;

use scanner::{scan_pickle, scan_safetensors, scan_weights};

/// Audit severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Audit finding
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub category: String,
    pub message: String,
    pub file_path: Option<String>,
}

impl Finding {
    pub fn critical(category: &str, message: &str) -> Self {
        Finding {
            severity: Severity::Critical,
            category: category.to_string(),
            message: message.to_string(),
            file_path: None,
        }
    }

    pub fn high(category: &str, message: &str) -> Self {
        Finding {
            severity: Severity::High,
            category: category.to_string(),
            message: message.to_string(),
            file_path: None,
        }
    }

    pub fn medium(category: &str, message: &str) -> Self {
        Finding {
            severity: Severity::Medium,
            category: category.to_string(),
            message: message.to_string(),
            file_path: None,
        }
    }

    pub fn with_file(mut self, path: &str) -> Self {
        self.file_path = Some(path.to_string());
        self
    }
}

/// Audit report
#[derive(Debug, Clone)]
pub struct AuditReport {
    pub model_path: String,
    pub findings: Vec<Finding>,
    pub scanned_files: usize,
    pub passed: bool,
}

impl AuditReport {
    pub fn new(model_path: &str) -> Self {
        AuditReport {
            model_path: model_path.to_string(),
            findings: vec![],
            scanned_files: 0,
            passed: true,
        }
    }

    pub fn add_finding(&mut self, finding: Finding) {
        if finding.severity >= Severity::High {
            self.passed = false;
        }
        self.findings.push(finding);
    }

    pub fn critical_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Critical)
            .count()
    }

    pub fn high_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::High)
            .count()
    }
}

/// Audit model directory or file
pub async fn audit_model(path: &Path) -> MgResult<AuditReport> {
    if !path.exists() {
        return Err(MgError::Other(format!(
            "Model path not found: {}",
            path.display()
        )));
    }

    let mut report = AuditReport::new(&path.to_string_lossy());

    if path.is_file() {
        audit_file(path, &mut report)?;
    } else if path.is_dir() {
        audit_directory(path, &mut report)?;
    }

    Ok(report)
}

fn audit_file(path: &Path, report: &mut AuditReport) -> MgResult<()> {
    report.scanned_files += 1;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "pkl" | "pickle" => {
            let findings = scan_pickle(path)?;
            for f in findings {
                report.add_finding(f);
            }
        }
        "safetensors" => {
            let findings = scan_safetensors(path)?;
            for f in findings {
                report.add_finding(f);
            }
        }
        "pt" | "pth" | "bin" => {
            let findings = scan_weights(path)?;
            for f in findings {
                report.add_finding(f);
            }
        }
        _ => {
            // Unknown format - skip
        }
    }

    Ok(())
}

fn audit_directory(dir: &Path, report: &mut AuditReport) -> MgResult<()> {
    let entries = std::fs::read_dir(dir)?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_file() {
            audit_file(&path, report)?;
        } else if path.is_dir() {
            audit_directory(&path, report)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "test/audit_tests.rs"]
mod tests;
