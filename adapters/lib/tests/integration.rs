#![allow(clippy::unwrap_used, unsafe_code)]
use mgc_lib_adapter::{adapter_for, check_pip_allowed, generate_sbom};
use mgc_types::adapter::PackageAdapter;
use mgc_types::capabilities::{
    ArtifactFetcher, AuditProvider, ContentStoreProvider, CoreIdent, DependencyResolver,
    LifecycleRunner, LockfileProvider, Materializer,
};
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    assert_eq!(a.language(), "ts");
}

// ── capability claims per language (Global Gate 1) ─────────────────────────

#[test]
fn ts_lane_claims_materializer_and_lifecycle_via_web_engine() {
    let dir = tmp("ts-caps");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"ts\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let caps = a.capabilities();
    assert!(caps.contains(&mgc_types::capabilities::Capability::Materializer));
    assert!(caps.contains(&mgc_types::capabilities::Capability::LifecycleRunner));
    // Claim ⇒ backed: TS rides the embedded web engine for both.
    // (Claim ⇒ có thật: TS đi trên web engine nhúng cho cả hai.)
    assert!(a.probe_materializer().is_ok());
    assert!(a.probe_lifecycle_runner().is_ok());
}

#[test]
fn toolchain_languages_do_not_claim_materializer_or_lifecycle() {
    let dir = tmp("rust-caps");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let caps = a.capabilities();
    assert!(!caps.contains(&mgc_types::capabilities::Capability::Materializer));
    assert!(!caps.contains(&mgc_types::capabilities::Capability::LifecycleRunner));
    assert!(!caps.contains(&mgc_types::capabilities::Capability::ArtifactFetcher));
    // Unclaimed ⇒ fail-closed Unsupported (trung thực).
    // (Không claim ⇒ fail-closed Unsupported.)
    let err = a.probe_materializer().unwrap_err();
    assert!(matches!(err, mgc_types::MgError::Unsupported { .. }));
    assert!(matches!(
        a.probe_artifact_fetcher(),
        Err(mgc_types::MgError::Unsupported { .. })
    ));
}

#[test]
fn java_gradle_does_not_claim_native_dependency_operations() {
    let dir = tmp("java-gradle-caps");
    std::fs::write(dir.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
    let adapter = adapter_for(&dir, None, None).unwrap().unwrap();
    let caps = adapter.capabilities();
    assert!(!caps.contains(&mgc_types::capabilities::Capability::DependencyResolver));
    assert!(!caps.contains(&mgc_types::capabilities::Capability::LockfileProvider));
    assert!(matches!(
        adapter.probe_dependency_resolver(),
        Err(mgc_types::MgError::Unsupported { .. })
    ));
}

#[test]
fn detect_python_project_via_mgc_toml() {
    let dir = tmp("py");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    assert_eq!(a.language(), "python");
}

#[test]
fn adapter_for_returns_none_for_empty_dir() {
    let dir = tmp("empty");
    assert!(adapter_for(&dir, None, None).unwrap().is_none());
}

// ── check_pip_allowed — fail-closed security ───────────────────────────────

#[test]
fn pip_allowlist_cannot_enable_external_package_manager() {
    let dir = tmp("pip-sec");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\", \"numpy\"]\n",
    )
    .unwrap();
    for name in ["requests", "numpy", "malicious-pkg"] {
        let err = check_pip_allowed(&dir, name).unwrap_err();
        assert!(err.to_string().contains("never invokes pip"));
        assert!(err.to_string().contains(name));
    }
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
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
async fn audit_lib_project_reports_scanner_truthfully() {
    let dir = tmp("audit-lib");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"audit-lib\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.219\"\nsource = \"sparse+https://index.crates.io/\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// empty lib for audit test\n").unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();

    let prior_osv_base = std::env::var_os("MGC_OSV_API_BASE");
    // A refused loopback port deterministically exercises tool/network
    // failure without binding sockets or depending on public OSV uptime.
    // (Port loopback đóng tạo lỗi mạng tất định, không cần server/mạng ngoài.)
    unsafe { std::env::set_var("MGC_OSV_API_BASE", "http://127.0.0.1:1/v1") };
    let report_result = a.audit(&dir).await;
    unsafe {
        if let Some(value) = prior_osv_base {
            std::env::set_var("MGC_OSV_API_BASE", value);
        } else {
            std::env::remove_var("MGC_OSV_API_BASE");
        }
    }
    let report = report_result.unwrap();
    // The adapter now dispatches to the REAL scanner: available+clean when
    // cargo-audit runs, honestly ToolMissing when it is not installed —
    // either way it must never fabricate findings or hide them.
    // Adapter giờ gọi scanner THẬT: available+sạch khi cargo-audit chạy,
    // trung thực ToolMissing khi không cài — không được bịa finding hay
    // giấu finding trong mọi trường hợp.
    match report.scanner_status {
        mgc_types::adapter::ScannerStatus::Available => {
            assert_eq!(report.vulnerability_count, report.vulnerabilities.len());
        }
        mgc_types::adapter::ScannerStatus::ToolMissing { .. }
        | mgc_types::adapter::ScannerStatus::Failed { .. } => {
            assert_eq!(report.vulnerability_count, 0);
        }
        other => panic!("unexpected scanner status: {other:?}"),
    }
}

#[tokio::test]
async fn python_manifest_roundtrip_preserves_project_table() {
    let dir = tmp("py-manifest");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"py-lib\"\nversion = \"0.1.0\"\ndependencies = [\"requests>=2.32.3\"]\n\n[tool.magicore]\ncore = \"lib\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let manifest = a.parse_manifest(&dir).await.unwrap();
    assert_eq!(manifest.name, "py-lib");
    assert!(manifest.find_dep("requests").is_some());
}

#[tokio::test]
async fn direct_python_adapter_mutations_fail_closed_outside_mutation_gateway() {
    let dir = tmp("py-update-all");
    std::fs::write(
        dir.join("mgc.toml"),
        "ecosystem = \"lib\"\n\n[lib]\nlanguage = \"python\"\npip_allowed_packages = [\"requests\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("pyproject.toml"), "[project]\nname = \"py-lib\"\n").unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let before = std::fs::read(dir.join("pyproject.toml")).unwrap();
    let name = PackageName::new("requests").unwrap();
    let range = VersionRange::parse("^2.32.3").unwrap();
    let add_error = a
        .add(&dir, &name, Some(&range), Default::default())
        .await
        .expect_err("direct adapter add must not bypass the CLI mutation gateway");
    assert!(
        add_error
            .to_string()
            .contains("direct adapter mutation is disabled")
    );
    let remove_error = a
        .remove(&dir, &name)
        .await
        .expect_err("direct adapter remove must not bypass the CLI mutation gateway");
    assert!(
        remove_error
            .to_string()
            .contains("direct adapter mutation is disabled")
    );
    let update_error = a
        .update(&dir, None)
        .await
        .expect_err("direct adapter update must not bypass the CLI mutation gateway");
    assert!(
        update_error
            .to_string()
            .contains("direct adapter mutation is disabled")
    );
    assert_eq!(std::fs::read(dir.join("pyproject.toml")).unwrap(), before);
    assert!(!dir.join("mgc.lock").exists());
}

#[tokio::test]
async fn rust_list_does_not_confuse_lock_pins_with_installed_packages() {
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let error = a.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            capability: "list installed packages",
            ..
        }
    ));
}

#[tokio::test]
async fn rust_list_is_unsupported_even_when_manifest_has_no_dependencies() {
    let dir = tmp("rust-list-empty");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"rust-list-empty\"\nversion = \"0.1.0\"\n\n[package.metadata.magicore]\ncore = \"lib\"\n",
    )
    .unwrap();
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let error = a.list(&dir).await.unwrap_err();
    assert!(matches!(
        error,
        mgc_types::MgError::Unsupported {
            capability: "list installed packages",
            ..
        }
    ));
}

#[tokio::test]
async fn python_list_does_not_claim_venv_packages_without_mgc_lock() {
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
    let installed = a.list(&dir).await.unwrap();
    assert!(installed.is_empty());
}

#[tokio::test]
async fn python_foreign_lock_keeps_native_mutation_and_install_unclaimed() {
    use mgc_types::capabilities::Capability;

    let dir = tmp("python-foreign-lock");
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"py-lib\"\ndependencies = [\"requests>=2.32\"]\n\n[tool.poetry.dependencies]\nrequests = \"^2.32\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("poetry.lock"), "# foreign lock\n").unwrap();
    let adapter = mgc_lib_adapter::adapter_for_language(
        mgc_lib_adapter::LibLanguage::Python,
        &dir,
        None,
        None,
    )
    .unwrap();

    assert!(!adapter.manifest_owned());
    assert!(
        !adapter
            .capabilities()
            .contains(&Capability::DependencyResolver)
    );
    assert!(matches!(
        adapter.probe_dependency_resolver(),
        Err(mgc_types::MgError::Unsupported { .. })
    ));
    assert!(matches!(
        adapter.probe_lockfile_provider(),
        Err(mgc_types::MgError::Unsupported { .. })
    ));
    assert!(matches!(
        adapter.probe_content_store(),
        Err(mgc_types::MgError::Unsupported { .. })
    ));
    assert!(matches!(
        adapter.parse_manifest(&dir).await,
        Err(mgc_types::MgError::Unsupported { .. })
    ));
    assert!(matches!(
        adapter.list(&dir).await,
        Err(mgc_types::MgError::Unsupported { .. })
    ));
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
    let a = adapter_for(&dir, None, None).unwrap().unwrap();
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
