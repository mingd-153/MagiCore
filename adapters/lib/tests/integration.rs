#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::{adapter_for, check_pip_allowed};
use mgc_types::adapter::PackageAdapter;
use std::path::PathBuf;

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mgc-lib-itg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

// ── adapter_for — detect languages ─────────────────────────────────────────

#[test]
fn detect_rust_project_via_cargo_toml_with_magicore_metadata() {
    let dir = tmp("rust");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"demo-lib\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    assert_eq!(a.language(), "rust");
}

#[test]
fn detect_ts_project_via_mgc_toml() {
    let dir = tmp("ts");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"ts\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    assert_eq!(a.language(), "ts");
}

#[test]
fn detect_python_project_via_mgc_toml() {
    let dir = tmp("py");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    assert_eq!(a.language(), "python");
}

#[test]
fn adapter_for_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(adapter_for(&dir, None, None).is_none());
}

// ── check_pip_allowed — fail-closed security ───────────────────────────────

#[test]
fn pip_allowlist_rejects_unlisted_package() {
    let dir = tmp("pip-sec");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\", \"numpy\"]\n",
    )
    .unwrap();
    assert!(check_pip_allowed(&dir, "requests").is_ok());
    assert!(check_pip_allowed(&dir, "numpy").is_ok());
    let err = check_pip_allowed(&dir, "malicious-pkg").unwrap_err();
    assert!(err.to_string().contains("malicious-pkg"));
}

// ── PackageAdapter trait ───────────────────────────────────────────────────

#[test]
fn adapter_name_and_ecosystem() {
    let dir = tmp("name-eco");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"rust\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    assert_eq!(a.name(), "lib");
    assert_eq!(format!("{:?}", a.ecosystem()), "Lib");
}

#[tokio::test]
async fn rust_manifest_roundtrip() {
    let dir = tmp("manifest-rt");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"roundtrip\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert_eq!(manifest.name, "roundtrip");
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dependencies[0].name.as_str(), "serde");
}

#[tokio::test]
async fn audit_returns_clean_for_lib_project() {
    let dir = tmp("audit-lib");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"audit-lib\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let report = a.audit(&dir).await.unwrap();
    assert_eq!(report.vulnerabilities.len(), 0);
}
