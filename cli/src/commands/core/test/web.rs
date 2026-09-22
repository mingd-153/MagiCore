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
fn compat_tool_mapping_is_hard_per_member_kind() {
    // Map cứng member → tool: mỗi loại chỉ mở đúng tool sẽ spawn.
    // (Hard mapping — each member kind opens exactly its spawn tool.)
    use super::required_compat_tool;
    for (file, tool) in [
        ("go.mod", "go"),
        ("requirements.txt", "pip"),
        ("Cargo.toml", "cargo"),
        ("pom.xml", "mvn"),
        ("composer.json", "composer"),
        ("artisan", "composer"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(file), "x").unwrap();
        assert_eq!(required_compat_tool(dir.path()), Some(tool), "file {file}");
    }
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(required_compat_tool(empty.path()), None);
}

#[test]
fn compat_gate_opens_only_the_matching_tool() {
    // Một flag hợp lệ không bao giờ mở mọi nhánh: cargo chỉ mở Cargo,
    // sai tool hoặc Native đều fail-closed.
    // (One flag never opens every branch.)
    use super::compat_install_target;
    use crate::commands::compat::CompatMode;
    let cargo_member = tempfile::tempdir().unwrap();
    std::fs::write(cargo_member.path().join("Cargo.toml"), "[package]\n").unwrap();
    // Wrong tool → denied (would spawn `cargo fetch`, not go).
    let err = compat_install_target(cargo_member.path(), &CompatMode::Explicit("go".to_string()))
        .expect_err("cargo member with --compat-runtime=go must fail closed");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("--compat-runtime=cargo"),
        "must name the required tool: {msg}"
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
fn compat_decision_allows_match_and_pip3_alias() {
    // Decision thuần (không spawn): đúng tool → Ok; pip3 mở nhánh pip
    // (alias đã ghi nhận); sai tool/Native → Err.
    // (Pure decision: match → Ok; pip3 ⇒ pip alias; else Err.)
    use super::compat_gate_decision;
    use crate::commands::compat::CompatMode;
    let pip_member = tempfile::tempdir().unwrap();
    std::fs::write(pip_member.path().join("requirements.txt"), "six==1.17.0\n").unwrap();
    assert_eq!(
        compat_gate_decision(pip_member.path(), &CompatMode::Explicit("pip".to_string())).unwrap(),
        "pip"
    );
    assert_eq!(
        compat_gate_decision(pip_member.path(), &CompatMode::Explicit("pip3".to_string())).unwrap(),
        "pip",
        "documented pip3 alias must open the pip branch"
    );
    compat_gate_decision(pip_member.path(), &CompatMode::Explicit("uv".to_string()))
        .expect_err("uv must not open the pip branch");
    compat_gate_decision(pip_member.path(), &CompatMode::Native)
        .expect_err("native must stay closed");
}
