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
fn windows_batch_invocation_accepts_regular_paths_and_arguments() {
    // Spaces stay usable for common Windows installs and CLI values.
    // Vẫn cho phép khoảng trắng thường gặp trong đường dẫn và giá trị CLI.
    let args = vec!["--output".to_string(), "release bundle.zip".to_string()];
    assert!(
        super::validate_windows_batch_invocation(r"C:\Program Files\MagiCore\tool.cmd", &args)
            .is_ok()
    );
}

#[test]
fn windows_batch_invocation_rejects_cmd_expansion_and_operators() {
    // Every cmd.exe expansion/operator character must be rejected in either input.
    // Mọi ký tự expansion/toán tử của cmd.exe phải bị chặn ở cả hai đầu vào.
    for character in ['"', '%', '!', '&', '|', '<', '>', '^', '(', ')', '\n', '\r'] {
        let argument = format!("value{character}payload");
        assert!(
            super::validate_windows_batch_invocation(
                r"C:\tools\tool.cmd",
                std::slice::from_ref(&argument)
            )
            .is_err(),
            "argument containing {character:?} must be rejected"
        );

        let path = format!(r"C:\tools\bad{character}name.cmd");
        assert!(
            super::validate_windows_batch_invocation(&path, &[]).is_err(),
            "script path containing {character:?} must be rejected"
        );
    }
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
#[test]
fn process_group_scan_detects_forbidden_child_after_root_exits()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};

    let root = tempfile::tempdir()?;
    let bin = root.path().join("bin");
    fs::create_dir(&bin)?;
    let npm = bin.join("npm");
    let ready = root.path().join("npm-started");
    fs::write(
        &npm,
        "#!/bin/sh\n/bin/echo started > \"$MGC_TEST_READY\"\n/bin/sleep 5\n",
    )?;
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o700))?;

    let mut command = std::process::Command::new("/bin/sh");
    command
        .args([
            "-c",
            "npm & i=0; while [ ! -f \"$MGC_TEST_READY\" ] && [ \"$i\" -lt 100 ]; do /bin/sleep 0.01; i=$((i + 1)); done; [ -f \"$MGC_TEST_READY\" ]",
        ])
        .env("PATH", &bin)
        .env("MGC_TEST_READY", ready);
    let mut child = command.process_group(0).spawn()?;
    let root_pid = child.id();
    assert!(
        child.wait()?.success(),
        "shell root should exit successfully"
    );

    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
    );
    let group_has_forbidden_process = system.processes().iter().any(|(pid, process)| {
        super::process_group_matches(pid, root_pid)
            && super::forbidden_process_entry(pid, process, &[]).is_some()
    });
    let scan = super::find_forbidden_descendant(root_pid, &[], root.path());
    super::terminate_process_tree(root_pid, &std::collections::HashSet::from([root_pid]));
    assert!(
        group_has_forbidden_process,
        "the test npm child must remain visible in the isolated PGID"
    );
    let found = scan?.expect("forbidden npm descendant must remain visible in its process group");
    assert_eq!(found.name, "npm");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_session_prevents_descendants_from_joining_callers_process_group()
-> Result<(), Box<dyn std::error::Error>> {
    const TARGET_GROUP_ENV: &str = "MGC_EXEC_TEST_TARGET_GROUP";
    const ATTEMPT_JOIN_ENV: &str = "MGC_EXEC_TEST_ATTEMPT_JOIN";

    if let Ok(target_group) = std::env::var(TARGET_GROUP_ENV) {
        if std::env::var_os(ATTEMPT_JOIN_ENV).is_some() {
            // A descendant in the isolated session must not join the caller's process group.
            // Descendant trong session cô lập không được nhập process group của caller.
            // SAFETY: this reads the test-only group id passed by the parent test.
            // AN TOÀN: đây là group id chỉ dành cho test do test cha truyền vào.
            #[allow(unsafe_code)]
            let result = unsafe {
                libc::setpgid(
                    0,
                    target_group.parse().expect("valid caller process-group id"),
                )
            };
            assert_eq!(result, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EPERM)
            );
            return Ok(());
        }

        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "run::tests::unix_session_prevents_descendants_from_joining_callers_process_group",
                "--nocapture",
            ])
            .env(ATTEMPT_JOIN_ENV, "1")
            .output()?;
        assert!(
            output.status.success(),
            "nested descendant failed to verify session isolation: stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }

    // SAFETY: getpgrp only reads the current test process's group id.
    // AN TOÀN: getpgrp chỉ đọc group id của process test hiện tại.
    #[allow(unsafe_code)]
    let caller_group = unsafe { libc::getpgrp() };
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "run::tests::unix_session_prevents_descendants_from_joining_callers_process_group",
            "--nocapture",
        ])
        .env(TARGET_GROUP_ENV, caller_group.to_string());
    super::configure_process_isolation(&mut command)?;
    let output = command.output()?;
    assert!(
        output.status.success(),
        "isolated test session failed to prevent process-group escape: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(unix)]
fn process_table_with_child(command: &str, command_line: &str) -> std::process::Output {
    process_table_with_raw_child(command.as_bytes(), command_line.as_bytes())
}

#[cfg(unix)]
fn process_table_with_raw_child(command: &[u8], command_line: &[u8]) -> std::process::Output {
    use std::os::unix::process::ExitStatusExt;
    let mut output = std::process::Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
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

#[cfg(unix)]
#[test]
fn process_signal_plan_avoids_duplicate_signal_to_root_group() {
    let observed = std::collections::HashSet::from([4242, 4300]);
    let live = std::collections::HashSet::from([4242, 4300]);

    let plan = super::process_signal_plan(4242, &observed, &live, Some(4242), false);

    assert_eq!(plan.process_groups, live);
    assert!(!plan.signal_root_directly);
}

#[cfg(unix)]
#[test]
fn process_signal_plan_skips_reaped_root_and_recycled_groups() {
    let observed = std::collections::HashSet::from([4242, 4300, 4400]);
    let live = std::collections::HashSet::from([4300]);

    let plan = super::process_signal_plan(4242, &observed, &live, None, true);

    assert_eq!(plan.process_groups, live);
    assert!(!plan.signal_root_directly);
}

#[cfg(unix)]
#[test]
fn process_signal_plan_directly_targets_live_root_when_its_group_is_unverifiable() {
    let observed = std::collections::HashSet::from([4242, 4300]);
    let live = std::collections::HashSet::from([4300]);

    let plan = super::process_signal_plan(4242, &observed, &live, Some(4500), false);

    assert_eq!(plan.process_groups, live);
    assert!(plan.signal_root_directly);
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

/// Verify the Windows inspector sees a real forbidden descendant executable.
/// Xác minh inspector Windows thấy executable descendant bị cấm thật.
#[cfg(windows)]
#[test]
fn windows_native_inspector_detects_forbidden_descendant() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempfile::tempdir()?;
    let forbidden = directory.path().join("npm.exe");
    std::fs::copy(
        std::env::var_os("COMSPEC").ok_or("Windows must expose COMSPEC")?,
        &forbidden,
    )?;
    let mut child = std::process::Command::new(&forbidden)
        .args(["/D", "/C", "ping -n 10 127.0.0.1 > NUL"])
        .spawn()?;
    let mut ancestry = super::WindowsProcessAncestry::default();
    let found = super::find_forbidden_descendant_with_timeout(
        std::process::id(),
        &[],
        directory.path(),
        std::time::Instant::now(),
        None,
        &mut ancestry,
        || Ok(false),
    );
    let taskkill = super::windows_system_tool_path("taskkill.exe")?;
    let _ = std::process::Command::new(taskkill)
        .args(["/F", "/T", "/PID", &child.id().to_string()])
        .status();
    let _ = child.kill();
    let _ = child.wait();
    let super::WindowsProcessScan::Forbidden(found) = found? else {
        return Err("native Windows inspector must detect npm.exe".into());
    };
    assert_eq!(found.name, "npm");
    Ok(())
}

#[test]
fn monitor_recognizes_runtime_package_manager_entrypoints() {
    for (entry, expected) in [
        ("npm-cli.js", "npm"),
        ("pnpm-cli.cjs", "pnpm"),
        ("yarn-cli.mjs", "yarn"),
        ("bun-cli.js", "bun"),
    ] {
        let command = format!("node.exe C:/project/node_modules/{entry} install");
        assert_eq!(
            super::forbidden_process_name("node.exe", &command, &[]).as_deref(),
            Some(expected)
        );
        assert!(super::forbidden_process_name("node.exe", &command, &[expected]).is_none());
    }
}

#[test]
fn command_line_is_required_when_missing_for_any_child_image() {
    for process_name in [
        "node.exe",
        "nodejs.exe",
        "C:\\Program Files\\Python\\python.exe",
        "python3.14.exe",
        "C:\\Windows\\System32\\cmd.exe",
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        "pwsh.exe",
        "bash.exe",
        "conhost.exe",
        "esbuild.exe",
        "git.exe",
        "unknown-helper.exe",
        "",
    ] {
        assert!(
            super::validate_monitored_child_command_line(process_name, &[], 7).is_err(),
            "every child image must remain fail-closed without its command line: {process_name}"
        );
        assert!(
            super::validate_monitored_child_command_line(
                process_name,
                &[std::ffi::OsString::from(process_name)],
                7
            )
            .is_ok(),
            "a readable command line is sufficient metadata for {process_name}"
        );
    }

    assert_eq!(
        super::forbidden_process_name("npm.exe", "", &[]).as_deref(),
        Some("npm"),
        "a forbidden executable must still be rejected when its command line is empty"
    );
}

#[test]
fn windows_job_scan_ignores_global_processes_outside_job_membership()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;

    let snapshot =
        |pid, parent_pid, image_name: &str, command: &[&str]| super::WindowsProcessSnapshot {
            pid,
            parent_pid,
            creation_time: Some(pid as u64),
            image_name: image_name.to_owned(),
            command: command.iter().map(OsString::from).collect(),
        };
    let processes = [
        snapshot(100, None, "runner.exe", &["runner.exe"]),
        snapshot(101, Some(100), "npm.exe", &["npm.exe", "install"]),
        snapshot(102, Some(100), "LsaIso.exe", &[]),
    ];

    let scanned = super::inspect_windows_job_processes(100, &[100, 101], &processes, &[])?;

    assert_eq!(scanned.descendants, [(101, Some("npm".to_owned()))]);
    Ok(())
}

#[test]
fn windows_job_scan_fails_closed_for_unreadable_job_members()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;

    let processes = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(100),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 102,
            parent_pid: Some(100),
            creation_time: Some(102),
            image_name: "LsaIso.exe".to_owned(),
            command: Vec::new(),
        },
    ];

    let error = super::inspect_windows_job_processes(100, &[100, 102], &processes, &[])?;

    assert_eq!(
        error.missing_command_line,
        Some((102, "LsaIso.exe".to_owned()))
    );
    assert!(matches!(
        super::windows_job_scan_from_inspection(error),
        super::WindowsProcessScan::MissingCommandLine { pid: 102, .. }
    ));
    Ok(())
}

#[test]
fn windows_job_member_capture_fails_closed_when_membership_cannot_be_verified()
-> Result<(), Box<dyn std::error::Error>> {
    let error = match super::capture_windows_job_members(100, &[100, 101], |_| {
        Err::<(), _>(anyhow::anyhow!(
            "PID from the Job snapshot no longer belongs to the Job"
        ))
    }) {
        Ok(_) => panic!("an unverifiable Job Object PID must fail closed"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("no longer belongs to the Job"));
    Ok(())
}

#[test]
fn windows_job_snapshot_scans_a_captured_child_that_exits_after_membership_check()
-> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;

    let captured = super::capture_windows_job_members(100, &[100, 101], |_| Ok(()))?;
    let captured_pids = captured
        .iter()
        .map(|(process_id, _)| *process_id)
        .collect::<Vec<_>>();
    let process_snapshot = [super::WindowsProcessSnapshot {
        pid: 101,
        parent_pid: Some(100),
        creation_time: Some(101),
        image_name: "npm.exe".to_owned(),
        command: vec![OsString::from("npm.exe"), OsString::from("install")],
    }];

    // Captured identity evidence remains authoritative after the process leaves the job.
    // Bằng chứng danh tính đã chụp vẫn có hiệu lực sau khi process rời Job.
    let inspection =
        super::inspect_windows_job_processes(100, &captured_pids, &process_snapshot, &[])?;
    assert_eq!(inspection.descendants, [(101, Some("npm".to_owned()))]);
    Ok(())
}

#[test]
fn windows_job_scan_fails_closed_for_captured_child_missing_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let captured = super::capture_windows_job_members(100, &[100, 101], |_| Ok(()))?;
    let captured_pids = captured
        .iter()
        .map(|(process_id, _)| *process_id)
        .collect::<Vec<_>>();
    let error = match super::inspect_windows_job_processes(100, &captured_pids, &[], &[]) {
        Ok(_) => panic!("a captured Job Object member without metadata must fail closed"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("cannot inspect captured Windows job child PID 101")
    );
    Ok(())
}

#[test]
fn windows_metadata_retry_rewalks_new_descendants() {
    use std::collections::VecDeque;
    use std::ffi::OsString;

    let snapshot = |pid, parent_pid, creation_time, image_name: &str, command: &[&str]| {
        super::WindowsProcessSnapshot {
            pid,
            parent_pid,
            creation_time: Some(creation_time),
            image_name: image_name.to_owned(),
            command: command.iter().map(OsString::from).collect(),
        }
    };
    let snapshots = VecDeque::from([
        vec![
            snapshot(100, None, 1, "runner.exe", &["runner.exe"]),
            snapshot(101, Some(100), 2, "node.exe", &[]),
        ],
        vec![
            snapshot(100, None, 1, "runner.exe", &["runner.exe"]),
            snapshot(102, Some(101), 3, "npm.exe", &["npm.exe", "install"]),
        ],
    ]);
    let mut snapshots = snapshots;
    let mut inspections = 0;
    let mut ancestry = super::WindowsProcessAncestry::default();

    let found = super::retry_windows_process_scan(
        std::time::Instant::now(),
        None,
        || Ok(false),
        || {
            inspections += 1;
            let snapshot = snapshots.pop_front().expect("retry must inspect again");
            super::scan_windows_process_snapshot(100, &mut ancestry, &snapshot, &[], |_| Ok(None))
        },
    )
    .expect("process snapshot inspection should succeed");

    assert_eq!(inspections, 2, "the retry must refresh and rescan the tree");
    let super::WindowsProcessScan::Forbidden(found) = found else {
        panic!("npm below an exited launcher must still be rejected");
    };
    assert_eq!(found.pid, 102);
    assert_eq!(found.name, "npm");
}

#[test]
fn windows_process_scan_keeps_ancestry_across_clean_snapshots() {
    use std::ffi::OsString;

    let mut ancestry = super::WindowsProcessAncestry::default();
    let first_snapshot = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(1),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 101,
            parent_pid: Some(100),
            creation_time: Some(2),
            image_name: "node.exe".to_owned(),
            command: vec![OsString::from("node.exe"), OsString::from("script.js")],
        },
    ];
    assert!(matches!(
        super::scan_windows_process_snapshot(100, &mut ancestry, &first_snapshot, &[], |_| {
            Ok(None)
        })
        .expect("initial tree scan should succeed"),
        super::WindowsProcessScan::Clean
    ));

    let second_snapshot = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(1),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 102,
            parent_pid: Some(101),
            creation_time: Some(3),
            image_name: "npm.exe".to_owned(),
            command: vec![OsString::from("npm.exe"), OsString::from("install")],
        },
    ];
    let result =
        super::scan_windows_process_snapshot(100, &mut ancestry, &second_snapshot, &[], |_| {
            Ok(None)
        })
        .expect("detached descendant scan should succeed");

    let super::WindowsProcessScan::Forbidden(found) = result else {
        panic!("npm below a previously observed launcher must still be rejected");
    };
    assert_eq!(found.pid, 102);
    assert_eq!(found.name, "npm");
}

#[test]
fn windows_process_scan_fails_closed_on_child_below_reused_pid() {
    use std::ffi::OsString;

    let snapshot = |pid, parent_pid, creation_time, image_name: &str, command: &[&str]| {
        super::WindowsProcessSnapshot {
            pid,
            parent_pid,
            creation_time: Some(creation_time),
            image_name: image_name.to_owned(),
            command: command.iter().map(OsString::from).collect(),
        }
    };
    let first_snapshot = vec![
        snapshot(100, None, 1, "runner.exe", &["runner.exe"]),
        snapshot(101, Some(100), 2, "node.exe", &["node.exe", "script.js"]),
    ];
    let second_snapshot = vec![
        snapshot(100, None, 1, "runner.exe", &["runner.exe"]),
        snapshot(101, Some(999), 10, "unrelated.exe", &["unrelated.exe"]),
        snapshot(102, Some(101), 11, "npm.exe", &["npm.exe", "install"]),
    ];
    let mut ancestry = super::WindowsProcessAncestry::default();
    assert!(matches!(
        super::scan_windows_process_snapshot(100, &mut ancestry, &first_snapshot, &[], |_| Ok(
            None
        ),)
        .expect("initial ancestry scan should succeed"),
        super::WindowsProcessScan::Clean
    ));

    let result = super::retry_windows_process_scan(
        std::time::Instant::now(),
        None,
        || Ok(false),
        || {
            super::scan_windows_process_snapshot(100, &mut ancestry, &second_snapshot, &[], |_| {
                Ok(None)
            })
        },
    );

    let error = match result {
        Ok(_) => panic!("a child below a reused PID must fail closed"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("ambiguous after PID reuse"), "{error}");
    assert!(!error.contains("forbidden child process"), "{error}");
}

#[test]
fn windows_process_scan_ignores_unrelated_process_after_pid_reuse() {
    use std::ffi::OsString;

    let mut ancestry = super::WindowsProcessAncestry::default();
    let first_snapshot = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(1),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 101,
            parent_pid: Some(100),
            creation_time: Some(2),
            image_name: "node.exe".to_owned(),
            command: vec![OsString::from("node.exe"), OsString::from("script.js")],
        },
    ];
    super::scan_windows_process_snapshot(100, &mut ancestry, &first_snapshot, &[], |_| Ok(None))
        .expect("initial ancestry scan should succeed");

    let second_snapshot = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(1),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 101,
            parent_pid: Some(999),
            creation_time: Some(10),
            image_name: "npm.exe".to_owned(),
            command: vec![OsString::from("npm.exe"), OsString::from("install")],
        },
    ];

    assert!(matches!(
        super::scan_windows_process_snapshot(100, &mut ancestry, &second_snapshot, &[], |_| Ok(
            None
        ),)
        .expect("unrelated PID reuse without descendants should remain clean"),
        super::WindowsProcessScan::Clean
    ));
}

#[test]
fn windows_process_scan_retries_transient_creation_time_unavailability() {
    use std::cell::Cell;
    use std::ffi::OsString;

    let snapshot = [
        super::WindowsProcessSnapshot {
            pid: 100,
            parent_pid: None,
            creation_time: Some(1),
            image_name: "runner.exe".to_owned(),
            command: vec![OsString::from("runner.exe")],
        },
        super::WindowsProcessSnapshot {
            pid: 101,
            parent_pid: Some(100),
            creation_time: None,
            image_name: "node.exe".to_owned(),
            command: vec![OsString::from("node.exe"), OsString::from("script.js")],
        },
    ];
    let mut ancestry = super::WindowsProcessAncestry::default();
    let inspections = Cell::new(0);
    let identity_queries = Cell::new(0);

    let result = super::retry_windows_process_scan(
        std::time::Instant::now(),
        None,
        || Ok(false),
        || {
            inspections.set(inspections.get() + 1);
            super::scan_windows_process_snapshot(100, &mut ancestry, &snapshot, &[], |_| {
                let query = identity_queries.get() + 1;
                identity_queries.set(query);
                Ok((query > 1).then_some(2))
            })
        },
    )
    .expect("transient process creation-time lookup should recover");

    assert!(matches!(result, super::WindowsProcessScan::Clean));
    assert_eq!(
        inspections.get(),
        2,
        "retry must refresh the full process tree"
    );
    assert_eq!(
        identity_queries.get(),
        2,
        "retry must query process identity again"
    );
}

#[test]
fn windows_metadata_retry_stops_when_child_exits() {
    use std::cell::Cell;

    let running_checks = Cell::new(0);
    let inspections = Cell::new(0);
    let result = super::retry_windows_process_scan(
        std::time::Instant::now(),
        None,
        || {
            let check = running_checks.get() + 1;
            running_checks.set(check);
            Ok(check >= 3)
        },
        || {
            inspections.set(inspections.get() + 1);
            Ok(super::WindowsProcessScan::MissingCommandLine {
                pid: 101,
                image_name: "node.exe".to_owned(),
            })
        },
    )
    .expect("finished child should end retries cleanly");

    assert!(matches!(result, super::WindowsProcessScan::ChildExited));
    assert_eq!(inspections.get(), 1, "do not refresh after the child exits");
}

#[test]
fn windows_metadata_retry_observes_child_exit_after_clean_scan() {
    use std::cell::Cell;

    let running_checks = Cell::new(0);
    let result = super::retry_windows_process_scan(
        std::time::Instant::now(),
        None,
        || {
            let check = running_checks.get() + 1;
            running_checks.set(check);
            Ok(check == 2)
        },
        || Ok(super::WindowsProcessScan::Clean),
    )
    .expect("child exit during a clean scan should be observed");

    assert!(matches!(result, super::WindowsProcessScan::ChildExited));
}

#[test]
fn windows_metadata_retry_respects_command_deadline() {
    use std::cell::Cell;

    let inspections = Cell::new(0);
    let result = super::retry_windows_process_scan(
        std::time::Instant::now(),
        Some(std::time::Duration::from_millis(1)),
        || Ok(false),
        || {
            inspections.set(inspections.get() + 1);
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(super::WindowsProcessScan::MissingCommandLine {
                pid: 101,
                image_name: "node.exe".to_owned(),
            })
        },
    )
    .expect("slow snapshot must stop retries at the command deadline");

    assert!(matches!(result, super::WindowsProcessScan::DeadlineReached));
    assert_eq!(
        inspections.get(),
        1,
        "do not start another scan after timeout"
    );
}

#[cfg(windows)]
#[test]
fn root_exit_completion_inspects_and_terminates_a_forbidden_job_child()
-> Result<(), Box<dyn std::error::Error>> {
    use super::{ExecOutcome, ProcessTreeGuard, finish_monitored_root_exit};
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;
    use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

    let temp = tempfile::tempdir()?;
    let comspec = std::env::var_os("COMSPEC").ok_or("Windows must expose COMSPEC")?;
    let forbidden_exe = temp.path().join("npm.exe");
    std::fs::copy(&comspec, &forbidden_exe)?;

    let guard = ProcessTreeGuard::new()?;
    let mut forbidden_command = Command::new(&forbidden_exe);
    forbidden_command
        .args(["/D", "/S", "/C", "ping -n 30 127.0.0.1 > NUL"])
        .creation_flags(CREATE_SUSPENDED);
    let forbidden_child = forbidden_command.spawn()?;
    guard.activate(&forbidden_child)?;
    std::thread::sleep(Duration::from_millis(150));

    let mut root_command = Command::new(comspec);
    root_command
        .args(["/D", "/S", "/C", "exit /b 0"])
        .creation_flags(CREATE_SUSPENDED);
    let mut root = root_command.spawn()?;
    guard.activate(&root)?;
    assert!(
        root.wait()?.success(),
        "test root command must exit successfully"
    );

    let result = finish_monitored_root_exit(&mut root, guard, &[], |child, _| {
        (
            ExecOutcome {
                status: child
                    .wait()
                    .expect("root process status must remain readable"),
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            true,
        )
    });
    let error = match result {
        Ok(_) => return Err("root-exit completion accepted a forbidden child".into()),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("forbidden package manager 'npm' spawned"),
        "root-exit helper did not inspect the surviving job child: {error}"
    );
    Ok(())
}
