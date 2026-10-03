//! Tests for process runner isolation — kiểm tra biên cô lập của process runner.

#[cfg(unix)]
use super::{
    ShadowPath, find_forbidden_descendant_with_program, inspect_forbidden_process_table,
    is_shadow_npm_version_probe, npm_probe_command_line_matches_shim_path,
};
use super::{
    ensure_process_inspection_available, is_path_env_key, process_inspection_unavailable_error,
    reject_external_dependency_resolution,
};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

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
    let shadow_dir = root.path().join("mgc-exec-shadow-path-test");
    fs::create_dir(&shadow_dir).expect("create executor shim directory");

    let result = find_forbidden_descendant_with_program(4242, &[], &shadow_dir, &missing_inspector);

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
    let shadow_dir = tempfile::tempdir().expect("create npm shim directory");

    let result = inspect_forbidden_process_table(4242, output, &[], shadow_dir.path());

    assert!(
        result.is_err(),
        "a process table that omits the live root cannot prove its descendants clean"
    );
}

#[cfg(unix)]
fn process_table_with_child(command: &str, command_line: &str) -> std::process::Output {
    process_table_with_raw_child(command.as_bytes(), command_line.as_bytes())
}

#[cfg(unix)]
fn process_table_with_raw_child(command: &[u8], command_line: &[u8]) -> std::process::Output {
    let mut output = std::process::Command::new("/usr/bin/true")
        .output()
        .expect("run true to create process-table test output");
    output.stdout = b"4242 1 cargo /usr/bin/cargo\n4243 4242 ".to_vec();
    output.stdout.extend_from_slice(command);
    output.stdout.push(b' ');
    output.stdout.extend_from_slice(command_line);
    output.stdout.push(b'\n');
    output
}

#[cfg(unix)]
#[test]
fn process_table_accepts_linux_npm_comm_with_its_interpreter_command_line() {
    let root = tempfile::tempdir().expect("create process-table test directory");
    let shadow = ShadowPath::create_at(root.path().join("executor-shim"), &[])
        .expect("create authenticated executor shims");
    let command_line = format!("/bin/sh {} --version", shadow.path().join("npm").display());
    let output = process_table_with_child("npm", &command_line);

    let result = inspect_forbidden_process_table(4242, output, &[], shadow.path());

    assert!(
        matches!(result, Ok(None)),
        "an exact executor shim version probe should be allowed: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn process_table_preserves_repeated_spaces_in_the_executor_shim_path() {
    let root = tempfile::tempdir().expect("create process-table test directory");
    let shadow = ShadowPath::create_at(root.path().join("executor  shim"), &[])
        .expect("create authenticated executor shims");
    let command_line = format!("/bin/sh {} --version", shadow.path().join("npm").display());
    let output = process_table_with_child("sh", &command_line);

    let result = inspect_forbidden_process_table(4242, output, &[], shadow.path());

    assert!(
        matches!(result, Ok(None)),
        "process inspection must preserve the exact shim path: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn process_table_rejects_a_real_npm_command_with_a_fake_shim_suffix() {
    let root = tempfile::tempdir().expect("create process-table test directory");
    let shadow = ShadowPath::create_at(root.path().join("executor-shim"), &[])
        .expect("create authenticated executor shims");
    let command_line = format!(
        "/bin/sh /real/path/npm --version {} --version",
        shadow.path().join("npm").display()
    );
    let output = process_table_with_child("npm", &command_line);

    let result = inspect_forbidden_process_table(4242, output, &[], shadow.path());

    assert!(
        matches!(result, Ok(Some(_))),
        "a real npm invocation must remain forbidden: {result:?}"
    );
}

#[test]
#[cfg(unix)]
fn npm_version_probe_requires_exact_executor_shim_path_and_arguments() {
    let root = tempfile::tempdir().expect("create process-shim test directory");
    let shadow = ShadowPath::create_at(root.path().join("mgc-exec shadow path test"), &[])
        .expect("create authenticated executor shims");
    let shim = shadow.path().join("npm");
    let unquoted_sh_probe = format!("/bin/sh {} --version", shim.display());
    let quoted_sh_probe = format!("/bin/sh \"{}\" --version", shim.display());
    let impostor_then_shim = format!(
        "/bin/sh /real/path/npm --version {} --version",
        shim.display()
    );

    assert!(is_shadow_npm_version_probe(
        b"sh",
        unquoted_sh_probe.as_bytes(),
        shadow.path()
    ));
    assert!(is_shadow_npm_version_probe(
        b"sh",
        quoted_sh_probe.as_bytes(),
        shadow.path()
    ));
    assert!(is_shadow_npm_version_probe(
        b"npm",
        format!("{} --version", shim.display()).as_bytes(),
        shadow.path()
    ));
    assert!(!is_shadow_npm_version_probe(
        b"sh",
        impostor_then_shim.as_bytes(),
        shadow.path()
    ));
    assert!(!is_shadow_npm_version_probe(
        b"sh",
        format!("/bin/sh {} install", shim.display()).as_bytes(),
        shadow.path()
    ));
    assert!(!is_shadow_npm_version_probe(
        b"sh",
        format!("/bin/sh {} --version install", shim.display()).as_bytes(),
        shadow.path()
    ));
    assert!(!is_shadow_npm_version_probe(
        b"npm",
        b"npm --version",
        shadow.path()
    ));
    assert!(!is_shadow_npm_version_probe(
        b"node",
        format!("node {} --version", shim.display()).as_bytes(),
        shadow.path()
    ));
}

#[cfg(unix)]
#[test]
fn process_table_rejects_a_modified_npm_probe_shim() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("create process-table test directory");
    let shadow = ShadowPath::create_at(root.path().join("executor-shim"), &[])
        .expect("create authenticated executor shims");
    let mut directory_permissions = fs::metadata(shadow.path())
        .expect("read sealed executor directory permissions")
        .permissions();
    directory_permissions.set_mode(0o700);
    fs::set_permissions(shadow.path(), directory_permissions)
        .expect("temporarily make executor directory writable for tampering test");
    let shim_path = shadow.path().join("npm");
    let mut shim_permissions = fs::metadata(&shim_path)
        .expect("read sealed shim permissions")
        .permissions();
    shim_permissions.set_mode(0o600);
    fs::set_permissions(&shim_path, shim_permissions)
        .expect("temporarily make npm shim writable for tampering test");
    fs::write(&shim_path, "#!/bin/sh\nnpm install\n")
        .expect("replace npm shim with a forbidden command");
    let mut shim_permissions = fs::metadata(&shim_path)
        .expect("read modified shim permissions")
        .permissions();
    shim_permissions.set_mode(0o500);
    fs::set_permissions(&shim_path, shim_permissions).expect("reseal modified npm shim");
    let mut directory_permissions = fs::metadata(shadow.path())
        .expect("read writable executor directory permissions")
        .permissions();
    directory_permissions.set_mode(0o500);
    fs::set_permissions(shadow.path(), directory_permissions)
        .expect("reseal modified executor directory");

    let command_line = format!("/bin/sh {} --version", shim_path.display());
    let output = process_table_with_child("sh", &command_line);
    let result = inspect_forbidden_process_table(4242, output, &[], shadow.path());

    assert!(
        matches!(result, Ok(Some(_))),
        "a modified executor shim must no longer qualify for the read-only probe: {result:?}"
    );
}

#[cfg(unix)]
#[test]
fn process_table_rejects_a_different_invalid_utf8_shim_path() {
    let root = tempfile::tempdir().expect("create process-table test directory");
    let shadow = ShadowPath::create_at(root.path().join("executor-shim"), &[])
        .expect("create authenticated executor shims");
    let shim_path = shadow.path().join("npm");
    let impostor_line = b"/bin/sh /tmp/executor-\xff/npm --version".to_vec();
    assert!(
        !impostor_line
            .windows(shim_path.as_os_str().as_bytes().len())
            .any(|window| window == shim_path.as_os_str().as_bytes()),
        "the synthetic invalid path must differ from the real shim path"
    );
    let output = process_table_with_raw_child(b"sh", &impostor_line);
    let impostor_result = inspect_forbidden_process_table(4242, output, &[], shadow.path());
    assert!(
        matches!(impostor_result, Ok(Some(_))),
        "a different invalid-UTF-8 path must not authenticate the real shim: {impostor_result:?}"
    );
}

#[cfg(unix)]
#[test]
fn npm_probe_path_comparison_keeps_invalid_unix_bytes_distinct() {
    let expected = b"/tmp/executor-\xff/npm";
    let exact = b"/bin/sh /tmp/executor-\xff/npm --version";
    let different = b"/bin/sh /tmp/executor-\xfe/npm --version";

    assert!(npm_probe_command_line_matches_shim_path(
        "sh", exact, expected
    ));
    assert!(!npm_probe_command_line_matches_shim_path(
        "sh", different, expected
    ));
}

#[cfg(windows)]
#[test]
fn windows_npm_shim_never_interpolates_caller_arguments() {
    let root = tempfile::tempdir().expect("create Windows shim test directory");
    super::write_blocker(root.path(), "npm").expect("write Windows npm blocker");
    let content =
        std::fs::read_to_string(root.path().join("npm.cmd")).expect("read Windows npm blocker");

    assert!(content.contains("MagiCore blocked forbidden package manager: npm"));
    assert!(
        !content.contains("%*"),
        "caller arguments must never be expanded"
    );
    assert!(
        !content.contains("%1"),
        "individual caller arguments must not be expanded"
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
