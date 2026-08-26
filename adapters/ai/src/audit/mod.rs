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
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }

    #[test]
    fn test_finding_builders() {
        let f = Finding::critical("pickle", "Dangerous import");
        assert_eq!(f.severity, Severity::Critical);
        assert_eq!(f.category, "pickle");

        let f2 = Finding::high("weights", "Large file").with_file("model.bin");
        assert_eq!(f2.file_path, Some("model.bin".to_string()));
    }

    #[test]
    fn test_audit_report_passed() {
        let mut report = AuditReport::new("/tmp/model");
        assert!(report.passed);

        report.add_finding(Finding::medium("test", "warning"));
        assert!(report.passed);

        report.add_finding(Finding::high("test", "error"));
        assert!(!report.passed);
    }

    #[test]
    fn test_audit_report_counts() {
        let mut report = AuditReport::new("/tmp/model");
        report.add_finding(Finding::critical("a", "1"));
        report.add_finding(Finding::high("b", "2"));
        report.add_finding(Finding::high("c", "3"));
        report.add_finding(Finding::medium("d", "4"));

        assert_eq!(report.critical_count(), 1);
        assert_eq!(report.high_count(), 2);
    }

    #[tokio::test]
    async fn test_audit_model_missing() {
        let result = audit_model(Path::new("/nonexistent")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_audit_single_file() {
        let tmp = tmp();
        let model = tmp.path().join("model.safetensors");
        std::fs::write(&model, b"fake safetensors").unwrap();

        let report = audit_model(&model).await.unwrap();
        assert_eq!(report.scanned_files, 1);
    }

    #[tokio::test]
    async fn test_audit_directory() {
        let tmp = tmp();
        std::fs::write(tmp.path().join("model1.bin"), b"fake").unwrap();
        std::fs::write(tmp.path().join("model2.pkl"), b"fake").unwrap();
        std::fs::write(tmp.path().join("readme.txt"), b"docs").unwrap();

        let report = audit_model(tmp.path()).await.unwrap();
        // Should scan .bin, .pkl (readme.txt skipped)
        assert!(report.scanned_files >= 2);
    }
}
