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
