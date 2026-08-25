#![allow(clippy::unwrap_used)]
use mgc_lib_adapter::{adapter_for, check_pip_allowed, generate_sbom};
use mgc_types::adapter::PackageAdapter;
use mgc_types::{DependencySpec, PackageName, VersionRange};
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
async fn rust_write_manifest_preserves_magicore_metadata() {
    let dir = tmp("rust-write");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let mut manifest = a.parse_manifest(&dir).await.unwrap();
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("serde").unwrap(),
            VersionRange::parse("1").unwrap(),
        ),
        false,
        false,
        false,
    );
    a.write_manifest(&dir, &manifest).await.unwrap();
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert!(content.contains("magicore"));
    assert!(content.contains("serde"));
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

#[tokio::test]
async fn python_manifest_roundtrip_preserves_project_table() {
    let dir = tmp("py-manifest");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"py-lib\"\nversion = \"0.1.0\"\ndependencies = [\"requests>=2.32.3\"]\n\n[tool.magicore]\ncore = \"lib\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert_eq!(manifest.name, "py-lib");
    assert!(manifest.find_dep("requests").is_some());
}

#[tokio::test]
async fn python_update_all_fails_closed() {
    let dir = tmp("py-update-all");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("pyproject.toml"), "[project]\nname = \"py-lib\"\n").unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let err = a.update(&dir, None).await.unwrap_err();
    assert!(err.to_string().contains("update-all"));
}

#[tokio::test]
async fn rust_list_reads_cargo_lock_versions() {
    let dir = tmp("rust-list");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"rust-list\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.lock"),
        "[[package]]\nname = \"serde\"\nversion = \"1.0.219\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let installed = a.list(&dir).await.unwrap();
    assert_eq!(installed[0].id.version().to_string(), "1.0.219");
}

#[tokio::test]
async fn python_list_reads_dist_info_versions() {
    let dir = tmp("py-dist-info");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"py-lib\"\ndependencies = [\"requests>=2.32.3\"]\n\n[tool.magicore]\ncore = \"lib\"\n",
    )
    .unwrap();
    let metadata = dir
        .join(".venv")
        .join("lib")
        .join("python3.11")
        .join("site-packages")
        .join("requests-2.32.3.dist-info");
    std::fs::create_dir_all(&metadata).unwrap();
    std::fs::write(
        metadata.join("METADATA"),
        "Metadata-Version: 2.1\nName: requests\nVersion: 2.32.3\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let installed = a.list(&dir).await.unwrap();
    assert_eq!(installed[0].id.version().to_string(), "2.32.3");
}

#[tokio::test]
async fn ts_delegate_ignores_workspace_protocol_dependencies() {
    let dir = tmp("ts-workspace");
    std::fs::write(
        dir.join("package.json"),
        serde_json::json!({
            "name": "frontend",
            "version": "0.1.0",
            "dependencies": {
                "@core/shared": "workspace:*",
                "react": "^18.2.0"
            }
        })
        .to_string(),
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert!(manifest.find_dep("react").is_some());
    assert!(manifest.find_dep("@core/shared").is_none());
}

#[test]
fn generate_sbom_uses_lockfile_v2_fixture() {
    let mut lockfile = mgc_lockfile::Lockfile::new();
    lockfile.add_package(mgc_lockfile::Package::new(
        "test-pkg".to_string(),
        "1.0.0".to_string(),
        "https://example.com/test.tgz".to_string(),
        "blake3:test123".to_string(),
    ));
    let json = generate_sbom(&lockfile, mgc_sbom::SbomOptions::default()).unwrap();
    assert!(json.contains("CycloneDX"));
    assert!(json.contains("test-pkg"));
    assert!(json.contains("1.0.0"));
}
