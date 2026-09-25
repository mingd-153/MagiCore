#[test]
fn v3_web_lockfile_is_written_with_current_schema_version() {
    // Deliberate v3 bump (Phase 1): web writes target the v3 schema.
    // Nâng lên v3 có chủ đích (Phase 1): web ghi ra nhắm schema v3.
    let lock = mgc_lockfile::Lockfile::new();
    let encoded = mgc_lockfile::serialization::to_toml(&lock).unwrap();
    assert!(encoded.contains("version = \"3\""));
    let decoded: mgc_lockfile::Lockfile = mgc_lockfile::serialization::from_toml(&encoded).unwrap();
    assert_eq!(decoded.version, "3");
}

/// Vite scripts launch MgDevServer (never the vite binary), so building
/// the launch must NOT require node_modules/.bin/vite to exist — React
/// runs straight through MGC with zero Vite dependence.
/// (Launch vite không cần binary vite — React chạy thẳng qua MGC.)
#[test]
fn vite_dev_launch_needs_no_vite_binary() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "x", "scripts": {"dev": "vite --port 4315"}}"#,
    )
    .unwrap();
    assert!(
        !dir.path().join("node_modules").exists(),
        "fixture must have no node_modules"
    );
    let launch = super::build_dev_launch(
        dir.path(),
        "dev",
        None,
        Some(4315),
        &crate::commands::compat::CompatMode::Native,
    )
    .expect("vite dev launch must build without the binary");
    assert!(
        launch.program.to_string_lossy().ends_with("vite"),
        "vite program routes to MgDevServer downstream"
    );
}

#[test]
fn non_native_monorepo_members_are_classified_without_tool_mapping() {
    // Detect unsupported member ecosystems for accurate fail-closed errors.
    // Nhận diện ecosystem member chưa được hỗ trợ để báo lỗi chính xác.
    use super::non_native_member_ecosystem;
    for (file, tool) in [
        ("go.mod", "go"),
        ("requirements.txt", "python"),
        ("Cargo.toml", "rust"),
        ("pom.xml", "java"),
        ("composer.json", "php"),
        ("artisan", "php"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(file), "x").unwrap();
        assert_eq!(
            non_native_member_ecosystem(dir.path()),
            Some(tool),
            "file {file}"
        );
    }
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(non_native_member_ecosystem(empty.path()), None);
}

#[test]
fn monorepo_install_rejects_external_package_manager_fallbacks() {
    // No compatibility flag may enable a provider package manager.
    // Không cờ compatibility nào được bật package manager bên ngoài.
    use super::compat_install_target;
    use crate::commands::compat::CompatMode;
    let cargo_member = tempfile::tempdir().unwrap();
    std::fs::write(cargo_member.path().join("Cargo.toml"), "[package]\n").unwrap();
    // Even an exact opt-in is denied and no provider command is run.
    let err = compat_install_target(
        cargo_member.path(),
        &CompatMode::Explicit("cargo".to_string()),
    )
    .expect_err("cargo member with exact compat opt-in must fail closed");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("does not yet own the complete native dependency lifecycle"),
        "must explain native lifecycle is missing: {msg}"
    );
    // Native (no opt-in) → denied.
    compat_install_target(cargo_member.path(), &CompatMode::Native)
        .expect_err("native must fail closed");
    // Unknown member kind → denied even WITH an opt-in.
    let empty = tempfile::tempdir().unwrap();
    compat_install_target(empty.path(), &CompatMode::Explicit("cargo".to_string()))
        .expect_err("unknown member must fail closed");
}

#[test]
fn compat_decision_rejects_every_external_package_manager() {
    // No opt-in may authorize an external package-manager install.
    // Không cờ opt-in nào được cấp quyền cài bằng package manager ngoài.
    use super::compat_gate_decision;
    for manifest in [
        "go.mod",
        "requirements.txt",
        "Cargo.toml",
        "pom.xml",
        "composer.json",
    ] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join(manifest), "fixture").unwrap();
        let error = compat_gate_decision(project.path())
            .expect_err("compat mode must not authorize a provider package manager");
        assert!(format!("{error:#}").contains("native"));
    }
}
