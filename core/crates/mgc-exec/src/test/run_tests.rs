//! Tests for process runner isolation — kiểm tra biên cô lập của process runner.

#[cfg(unix)]
use super::{ShadowPath, find_forbidden_descendant_with_program, inspect_forbidden_process_table};
use super::{
    ensure_process_inspection_available, is_path_env_key, process_inspection_unavailable_error,
    reject_external_dependency_resolution,
};
#[cfg(unix)]
use std::fs;

#[test]
fn path_environment_key_uses_host_case_semantics() {
    assert!(is_path_env_key("PATH"));
    #[cfg(windows)]
    assert!(is_path_env_key("Path"));
    #[cfg(not(windows))]
    assert!(!is_path_env_key("Path"));
}

#[test]
fn native_windows_path_lookup_prefers_pe_before_shims_across_path_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let local_bin = root.path().join("local-bin");
    let system_bin = root.path().join("system-bin");
    std::fs::create_dir_all(&local_bin)?;
    std::fs::create_dir_all(&system_bin)?;
    std::fs::write(local_bin.join("tool.cmd"), "local shim")?;
    std::fs::write(local_bin.join("tool"), "extensionless script")?;
    std::fs::write(system_bin.join("tool.exe"), "PE executable")?;
    let search_path = std::env::join_paths([&local_bin, &system_bin])?;

    let resolved = super::find_windows_command("tool", Some(&search_path));

    assert_eq!(resolved, Some(system_bin.join("tool.exe").into_os_string()));
    Ok(())
}

#[test]
fn native_windows_path_lookup_prefers_cmd_over_bat_and_extensionless_script()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("tool"), "extensionless script")?;
    std::fs::write(root.path().join("tool.bat"), "batch shim")?;
    std::fs::write(root.path().join("tool.cmd"), "command shim")?;
    let search_path = std::env::join_paths([root.path()])?;

    let resolved = super::find_windows_command("tool", Some(&search_path));

    assert_eq!(
        resolved,
        Some(root.path().join("tool.cmd").into_os_string())
    );
    Ok(())
}

#[test]
fn native_windows_path_lookup_does_not_search_a_different_path_entry()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let unselected_bin = root.path().join("unselected-bin");
    let selected_bin = root.path().join("selected-bin");
    std::fs::create_dir_all(&unselected_bin)?;
    std::fs::create_dir_all(&selected_bin)?;
    std::fs::write(unselected_bin.join("tool.exe"), "unselected executable")?;
    let search_path = std::env::join_paths([&selected_bin])?;

    let resolved = super::find_windows_command("tool", Some(&search_path));

    assert_eq!(resolved, None);
    Ok(())
}

#[test]
#[cfg(unix)]
fn shadow_directory_creation_refuses_a_preexisting_symlink()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let target = root.path().join("attacker-owned");
    fs::create_dir(&target)?;
    let protected_file = target.join("npm");
    fs::write(&protected_file, "do not overwrite")?;
    let link = root.path().join("shadow-link");
    std::os::unix::fs::symlink(&target, &link)?;

    let result = ShadowPath::create_at(link, &[]);

    assert!(
        result.is_err(),
        "existing symlink must not be adopted as a shim directory"
    );
    assert_eq!(fs::read_to_string(protected_file)?, "do not overwrite");
    Ok(())
}

#[test]
fn dependency_guard_blocks_uncovered_package_manager_subcommands() {
    let cases = [
        ("cargo", vec!["package"]),
        ("cargo", vec!["publish"]),
        ("cargo", vec!["tree"]),
        ("cargo", vec!["info", "serde"]),
        ("cargo", vec!["generate-lockfile"]),
        ("cargo", vec!["upgrade"]),
        ("go", vec!["list", "-m", "all"]),
        ("dotnet", vec!["tool", "install", "example.tool"]),
        ("dotnet", vec!["workload", "install", "wasm-tools"]),
        ("dotnet", vec!["nuget", "push", "package.nupkg"]),
        ("dotnet", vec!["msbuild", "-t:Restore"]),
        ("dotnet", vec!["build", "--no-restore", "-t:Restore"]),
        (
            "dotnet",
            vec!["build", "--no-restore", "-target:Build;Restore"],
        ),
        ("python3", vec!["-m", "pip._internal", "install", "example"]),
        ("python", vec!["-m", "ensurepip"]),
    ];

    for (program, args) in cases {
        let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert!(
            reject_external_dependency_resolution(program, &args, &[]).is_err(),
            "{program} {args:?} must not perform package-manager work through mgc-exec"
        );
    }
}

#[test]
fn process_inspection_unavailable_error_is_explicit() {
    let error = process_inspection_unavailable_error();

    assert!(
        error
            .to_string()
            .contains("native process-tree inspection is unavailable"),
        "unsupported process inspection must provide a fail-closed diagnostic"
    );
}

#[test]
fn process_inspection_policy_rejects_unsupported_monitored_mode() {
    assert!(ensure_process_inspection_available(false, false).is_ok());
    assert!(ensure_process_inspection_available(true, true).is_ok());
    assert!(ensure_process_inspection_available(true, false).is_err());
}

#[cfg(unix)]
#[test]
fn unavailable_process_inspector_is_an_error_not_a_clean_process_tree() {
    let root = tempfile::tempdir().expect("create process-inspector test directory");
    let missing_inspector = root.path().join("missing-ps");

    let result = find_forbidden_descendant_with_program(4242, &[], &missing_inspector);

    assert!(
        result.is_err(),
        "failure to inspect child processes must not be interpreted as no forbidden child"
    );
}

#[cfg(unix)]
#[test]
fn incomplete_process_table_is_an_error_not_a_clean_process_tree() {
    let output = std::process::Command::new("/usr/bin/true")
        .output()
        .expect("run true to produce an empty process table");

    let result = inspect_forbidden_process_table(4242, output, &[]);

    assert!(
        result.is_err(),
        "a process table that omits the live root cannot prove its descendants clean"
    );
}

#[test]
fn cargo_compile_is_allowed_only_with_locked_offline_flags() {
    assert!(reject_external_dependency_resolution("cargo", &["build".into()], &[]).is_err());
    assert!(
        reject_external_dependency_resolution(
            "cargo",
            &["build".into(), "--locked".into(), "--offline".into()],
            &[]
        )
        .is_ok()
    );
}

#[test]
fn rustc_is_limited_to_the_read_only_host_target_probe() {
    assert!(
        crate::allowlist::check_tool_with_scope(
            "rustc",
            crate::allowlist::ExecutionScope::BuildRunner,
            None,
        )
        .is_ok()
    );
    assert!(reject_external_dependency_resolution("rustc", &["-vV".into()], &[]).is_ok());
    assert!(reject_external_dependency_resolution("rustc", &["--version".into()], &[]).is_err());
    assert!(
        reject_external_dependency_resolution(
            "rustc",
            &["-vV".into(), "--crate-name".into(), "injected".into()],
            &[]
        )
        .is_err()
    );
}

#[test]
fn dotnet_run_requires_no_restore_at_the_execution_boundary() {
    assert!(reject_external_dependency_resolution("dotnet", &["run".into()], &[]).is_err());
    assert!(
        reject_external_dependency_resolution(
            "dotnet",
            &["run".into(), "--no-restore".into()],
            &[]
        )
        .is_ok()
    );
}

#[test]
fn go_guard_checks_the_effective_goproxy_value() {
    let args = vec!["build".to_string(), "-mod=readonly".to_string()];
    let env = vec![
        ("GOPROXY".to_string(), "off".to_string()),
        ("GOPROXY".to_string(), "https://proxy.example".to_string()),
    ];

    assert!(reject_external_dependency_resolution("go", &args, &env).is_err());
}
