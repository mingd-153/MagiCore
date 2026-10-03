#!/usr/bin/env python3
"""Self-tests for audit_dependency_delegation.py (P0-A / T0.1).

Stdlib only — run with `python3 scripts/test_audit_dependency_delegation.py`.
Every test is hermetic except the repo-ledger contract, which re-runs the
gate over the working tree and asserts known production toolchain delegation
is still release-blocking.

(Tự kiểm cho gate audit uỷ quyền: chỉ dùng stdlib — chạy bằng
`python3 scripts/test_audit_dependency_delegation.py`.)
"""

import json
import os
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(
    0, os.path.dirname(os.path.abspath(__file__)),
)
import audit_dependency_delegation as gate


def scan_snippet(rel_path, snippet):
    """Run scan_file over an in-memory snippet via a temp file.
    (Chạy scan_file trên đoạn mã trong bộ nhớ qua file tạm.)"""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".rs", delete=False, encoding="utf-8"
    ) as handle:
        handle.write(snippet)
        tmp = handle.name
    try:
        return gate.scan_file(rel_path, tmp)
    finally:
        os.unlink(tmp)


class NewToolDetection(unittest.TestCase):
    """New package-manager and scanner spawns remain visible to the gate."""

    def test_web_layout_does_not_spawn_a_shell_for_windows_junctions(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "adapters/web/src/layout.rs"
        abs_path = os.path.join(repo_root, rel_path)
        findings = gate.scan_file(rel_path, abs_path)
        shell_spawns = [item for item in findings if item["tool"] == "cmd"]
        self.assertEqual(
            shell_spawns,
            [],
            "project paths must never be interpolated into cmd.exe command text",
        )

    def test_python3_quantize_spawn_is_violation_without_marker(self):
        findings = scan_snippet(
            "cli/src/commands/model/mod.rs",
            'fn quantize(path: &str) -> Result<()> {\n'
            '    let status = std::process::Command::new("python3")\n'
            '        .args(["-m", "x"])\n'
            '        .status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "python3")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["op_class"], "quantize")

    def test_pip_audit_spawn_is_violation_without_marker(self):
        findings = scan_snippet(
            "core/crates/mgc-audit/src/scanners/mod.rs",
            'pub async fn audit_python(project_root: &Path) -> MgResult<AuditReport> {\n'
            '    let result = mgc_exec::run::run("pip-audit", &args, &exec_opts)?;\n'
            '    Ok(result)\n'
            '}\n',
        )
        tools = [f["tool"] for f in findings]
        self.assertIn("pip-audit", tools)
        for finding in findings:
            if finding["tool"] == "pip-audit":
                self.assertEqual(finding["status"], "violation")

    def test_cargo_audit_which_probe_is_found(self):
        findings = scan_snippet(
            "core/crates/mgc-audit/src/scanners/mod.rs",
            'pub async fn audit_rust(project_root: &Path) -> MgResult<AuditReport> {\n'
            '    if which::which("cargo-audit").is_err() {\n'
            '        return Ok(AuditReport::tool_missing("cargo-audit"));\n'
            '    }\n'
            '    Ok(AuditReport::clean(0))\n'
            '}\n',
        )
        self.assertTrue(
            any(f["tool"] == "cargo-audit" for f in findings),
            "which-probe of cargo-audit must be measured",
        )

    def test_govulncheck_spawn_is_found(self):
        findings = scan_snippet(
            "core/crates/mgc-audit/src/scanners/govulncheck.rs",
            'pub async fn audit_go(project_root: &Path) -> MgResult<AuditReport> {\n'
            '    let result = mgc_exec::run::run("govulncheck", &args, &exec_opts)?;\n'
            '    Ok(result)\n'
            '}\n',
        )
        self.assertTrue(
            any(f["tool"] == "govulncheck" for f in findings),
            "govulncheck spawn must be measured",
        )

    def test_composer_and_pub_package_manager_spawns_are_found(self):
        findings = scan_snippet(
            "adapters/lib/src/install/mod.rs",
            'pub async fn run_install() -> Result<()> {\n'
            '    mgc_exec::run::run("composer", &["install".to_string()], &opts)?;\n'
            '    mgc_exec::run::run("pub", &["add".to_string()], &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual({finding["tool"] for finding in findings}, {"composer", "pub"})
        self.assertTrue(all(finding["status"] == "violation" for finding in findings))

    def test_git_fetch_spawn_is_dependency_delegation(self):
        findings = scan_snippet(
            "core/crates/mgc-resolver/src/protocols/swift.rs",
            'async fn resolve_git() -> Result<()> {\n'
            '    let report = mgc_exec::run::run("git", &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "git")
        self.assertEqual(findings[0]["status"], "violation")

    def test_unreadable_rust_source_is_reported_not_silently_skipped(self):
        findings = gate.scan_file(
            "core/crates/mgc-exec/src/missing.rs",
            "/definitely/not/a/real/magicore-source.rs",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["evidence_kind"], "scan_error")
        self.assertEqual(findings[0]["status"], "violation")

    def test_dynamic_native_install_wrapper_is_forbidden(self):
        findings = scan_snippet(
            "cli/src/commands/core/web.rs",
            'fn install_member(root: &Path) -> Result<()> {\n'
            '    run_native_install(root, program, &args)\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "run_native_install")
        self.assertEqual(findings[0]["op_class"], "install")
        self.assertEqual(findings[0]["status"], "violation")

    def test_tool_selector_return_values_are_not_process_spawns(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn python_cmd() -> &\'static str {\n'
            '    if !tool_unavailable("python") {\n'
            '        "python"\n'
            '    } else if !tool_unavailable("python3") {\n'
            '        "python3"\n'
            '    } else {\n'
            '        "python"\n'
            '    }\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_internal_test_script_sentinel_is_not_an_executable(self):
        findings = scan_snippet(
            "cli/src/commands/test.rs",
            'fn detect_test_script() {\n'
            '    Some(("mgc-internal-run-script".to_string(), vec![]));\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_multiline_process_tool_argument_is_still_found(self):
        findings = scan_snippet(
            "adapters/lib/src/install/mod.rs",
            'fn install_deps() -> Result<()> {\n'
            '    mgc_exec::run::run(\n'
            '        "cargo",\n'
            '        &args,\n'
            '        &opts,\n'
            '    )?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "cargo")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["line"], 3)

    def test_production_dev_command_cannot_delegate_to_external_runtime(self):
        findings = scan_snippet(
            "cli/src/commands/core/install/app.rs",
            'fn dev_command() {\n'
            '    let command = InstallCommand { tool: "flutter".to_string() };\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "dev")
        self.assertEqual(findings[0]["status"], "review-required")
        self.assertTrue(findings[0]["blocking"])

    def test_production_build_command_cannot_delegate_to_build_system(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_app() -> Result<()> {\n'
            '    std::process::Command::new("gradle").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "build")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["evidence_kind"], "spawn_or_wrapper_call")

    def test_unlisted_executable_literal_in_build_is_still_a_blocker(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_app() -> Result<()> {\n'
            '    std::process::Command::new("ninja").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "ninja")
        self.assertEqual(findings[0]["op_class"], "build")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["evidence_kind"], "spawn_or_wrapper_call")

    def test_unlisted_tool_descriptor_in_build_is_still_a_blocker(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_app() {\n'
            '    let command = BuildCommand { tool: "ninja".to_string() };\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "ninja")
        self.assertEqual(findings[0]["evidence_kind"], "command_descriptor")
        self.assertEqual(findings[0]["status"], "review-required")
        self.assertTrue(findings[0]["blocking"])

    def test_program_comparison_without_process_call_is_not_a_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/core/web.rs",
            'fn native_install_env(project_root: &Path, program: &str) {\n'
            '    if program == "composer" { configure_cache(project_root); }\n'
            '    if program == "go" { configure_cache(project_root); }\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_command_selection_branch_is_not_a_second_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/dev.rs",
            'fn run(cmd: &str) {\n'
            '    if cmd == "godot" { show_tool_error(); }\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_process_helper_declaration_is_not_a_spawn(self):
        findings = scan_snippet(
            "adapters/iot/src/flash/mod.rs",
            'fn exec_tool(tool: &str, args: &[&str]) -> Result<()> {\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_dynamic_exec_tool_is_a_blocker(self):
        findings = scan_snippet(
            "adapters/cloud/src/deploy/mod.rs",
            'fn deploy_with_tool(program: &str) -> Result<()> {\n'
            '    exec_tool(program, &args, root)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "<dynamic:program>")
        self.assertEqual(findings[0]["status"], "violation")

    def test_dynamic_mgc_run_is_a_blocker(self):
        findings = scan_snippet(
            "adapters/cloud/src/deploy/mod.rs",
            'fn deploy_with_tool(tool: &str) -> Result<()> {\n'
            '    mgc_run(tool, &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "<dynamic:tool>")
        self.assertEqual(findings[0]["status"], "violation")

    def test_dynamic_executable_in_build_is_a_blocker(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_rust_with_env(program: &str) -> Result<()> {\n'
            '    mgc_exec::prelude::run_inherited(program, &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "<dynamic:program>")
        self.assertEqual(findings[0]["op_class"], "build")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["evidence_kind"], "dynamic_spawn_call")

    def test_dynamic_command_new_in_unclassified_production_path_is_blocker(self):
        findings = scan_snippet(
            "cli/src/commands/hooks.rs",
            'fn run_hook(program: &str) -> Result<()> {\n'
            '    std::process::Command::new(program).status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "<dynamic:program>")
        self.assertEqual(findings[0]["status"], "violation")

    def test_mgc_run_dispatcher_is_not_misidentified_as_process_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/core/dev/cicd.rs",
            'async fn run_test_step() -> Result<()> {\n'
            '    crate::commands::run::run("test".to_string(), args, core, None).await?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_other_cli_run_functions_are_not_process_spawn_patterns(self):
        findings = scan_snippet(
            "cli/src/commands/add.rs",
            'async fn dispatch_add(package: String) -> Result<()> {\n'
            '    crate::commands::core::add::run(package, args).await?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_dynamic_mgc_exec_run_is_a_blocker(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_with_selected_tool(program: &str) -> Result<()> {\n'
            '    mgc_exec::prelude::run(program, &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "<dynamic:program>")
        self.assertEqual(findings[0]["op_class"], "build")
        self.assertEqual(findings[0]["status"], "violation")

    def test_mgc_exec_run_inherited_literal_is_detected(self):
        findings = scan_snippet(
            "cli/src/commands/build.rs",
            'fn build_runtime() -> Result<()> {\n'
            '    mgc_exec::prelude::run_inherited("node", &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "node")
        self.assertEqual(findings[0]["evidence_kind"], "spawn_or_wrapper_call")
        self.assertEqual(findings[0]["op_class"], "build")
        self.assertEqual(findings[0]["status"], "violation")

    def test_production_test_command_cannot_delegate_to_package_manager(self):
        findings = scan_snippet(
            "cli/src/commands/core/dev/cicd.rs",
            'fn run_test_step() -> Result<()> {\n'
            '    std::process::Command::new("cargo").arg("test").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "test")
        self.assertEqual(findings[0]["status"], "violation")

    def test_windows_path_lookup_process_spawn_is_not_exempt(self):
        findings = scan_snippet(
            "core/crates/mgc-exec/src/run.rs",
            'fn resolve_windows_shim(cmd: &str) {\n'
            '    Command::new("where.exe").arg(cmd).output();\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["status"], "violation")
        self.assertTrue(findings[0]["blocking"])

    def test_process_tree_os_tools_are_reviewed_but_still_blocking(self):
        cases = (
            ("terminate_process_tree", "taskkill", "Windows process-tree termination"),
            ("find_forbidden_descendant", "ps", "Unix process-table inspection"),
        )
        for function, executable, reason in cases:
            with self.subTest(function=function):
                findings = scan_snippet(
                    "core/crates/mgc-exec/src/run.rs",
                    f'fn {function}() {{\n'
                    f'    Command::new("{executable}").status();\n'
                    '}\n',
                )
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["op_class"], "platform-process-boundary")
                self.assertEqual(findings[0]["status"], "review-required")
                self.assertTrue(findings[0]["blocking"])
                self.assertIn(reason, findings[0]["classification_reason"])

    def test_os_tool_allowlist_does_not_allow_package_manager_in_same_helper(self):
        findings = scan_snippet(
            "core/crates/mgc-exec/src/run.rs",
            'fn terminate_process_tree() {\n'
            '    Command::new("taskkill").status();\n'
            '    Command::new("cargo").arg("fetch").status();\n'
            '}\n',
        )
        self.assertEqual(len(findings), 2)
        by_tool = {finding["tool"]: finding for finding in findings}
        self.assertEqual(by_tool["taskkill"]["status"], "review-required")
        self.assertEqual(by_tool["cargo"]["status"], "violation")
        self.assertTrue(all(finding["blocking"] for finding in findings))

    def test_test_prefix_directory_does_not_waive_production_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/core/testing/build.rs",
            'fn build_app() -> Result<()> {\n'
            '    std::process::Command::new("gradle").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["status"], "violation")

    def test_production_spawn_in_exact_test_named_source_directory_is_not_exempt(self):
        findings = scan_snippet(
            "cli/src/commands/test/runtime.rs",
            'pub fn install_package() -> Result<()> {\n'
            '    std::process::Command::new("cargo").arg("fetch").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "install")
        self.assertEqual(findings[0]["status"], "violation")

    def test_source_subdirectory_named_tests_is_not_a_harness_exemption(self):
        findings = scan_snippet(
            "core/crates/mgc-store/src/tests/runtime.rs",
            'pub fn install_package() -> Result<()> {\n'
            '    std::process::Command::new("cargo").arg("fetch").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "install")
        self.assertEqual(findings[0]["status"], "violation")

    def test_external_integration_harness_remains_exempt(self):
        findings = scan_snippet(
            "core/crates/mgc-store/tests/runtime.rs",
            'fn fixture_installs_dependency() -> Result<()> {\n'
            '    std::process::Command::new("cargo").arg("fetch").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "tests")
        self.assertEqual(findings[0]["status"], "allowed")

    def test_test_attribute_is_a_narrow_exemption_inside_source_tree(self):
        findings = scan_snippet(
            "cli/src/commands/test/runtime_test.rs",
            '#[tokio::test]\n'
            'async fn test_external_fixture_tool() -> Result<()> {\n'
            '    std::process::Command::new("cargo").arg("fetch").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "test-fixture")
        self.assertEqual(findings[0]["status"], "allowed")

    def test_dev_path_does_not_waive_dependency_named_function(self):
        findings = scan_snippet(
            "cli/src/commands/core/dev/app.rs",
            'fn install_dependencies() -> Result<()> {\n'
            '    let status = std::process::Command::new("gradle")\n'
            '        .args(["dependencies"])\n'
            '        .status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "install")
        self.assertEqual(
            findings[0]["status"],
            "violation",
            "a dev path must not turn a dependency lifecycle spawn into an allowed runtime call",
        )

    def test_dev_path_does_not_waive_unclassified_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/core/dev/app.rs",
            'fn prepare_project() -> Result<()> {\n'
            '    std::process::Command::new("gradle").status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "unclassified")
        self.assertEqual(findings[0]["status"], "violation")


class ScanBoundaryCoverage(unittest.TestCase):
    """Every dependency lane and claimed core-owned tool flow is scanned."""

    def test_complete_cli_command_tree_is_in_dependency_scan_roots(self):
        for root in ("adapters", "cli", "core", "tests", "tools"):
            self.assertIn(
                root,
                gate.SCAN_ROOTS,
                f"the process-boundary gate must scan first-party Rust root {root}",
            )
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        scanned = {
            os.path.relpath(path, repo_root).replace(os.sep, "/")
            for path in gate._iter_rust_files()
        }
        self.assertIn("cli/src/main.rs", scanned)
        self.assertIn("cli/src/commands/python_runtime.rs", scanned)
        self.assertIn("cli/tests/native_engine_spawn_audit.rs", scanned)
        self.assertIn("core/crates/mgc-exec/src/run.rs", scanned)
        self.assertIn("core/crates/mgc-resolver/src/protocols/swift.rs", scanned)
        self.assertIn("tools/mgc-dist/src/main.rs", scanned)
        python_scanned = {
            os.path.relpath(path, repo_root).replace(os.sep, "/")
            for path in gate._iter_python_files()
        }
        for root in (
            ".",
        ):
            self.assertIn(root, gate.PYTHON_SCAN_ROOTS)
        self.assertIn("scripts/lifecycle_capability_matrix.py", python_scanned)
        self.assertIn("benchmark/scripts/bench_v2.py", python_scanned)
        shell_scanned = {
            os.path.relpath(path, repo_root).replace(os.sep, "/")
            for path in gate._iter_shell_files()
        }
        self.assertIn("CHECK_PROGRESS.sh", shell_scanned)
        self.assertIn("reproduce.sh", shell_scanned)
        self.assertFalse(any("gitnexus" in path.lower() for path in shell_scanned))
        self.assertFalse(any("gitnexus" in path.lower() for path in python_scanned))
        self.assertIn("tools/mgc-mcp/mcp_server.py", python_scanned)
        shell_scanned = {
            os.path.relpath(path, repo_root).replace(os.sep, "/")
            for path in gate._iter_shell_files()
        }
        self.assertIn("scripts/install-from-gh.sh", shell_scanned)
        self.assertIn("cli/tests/scripts/runtime_full_e2e.sh", shell_scanned)

    def test_project_kind_check_is_not_a_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/core/install/clo.rs",
            'fn install() -> Result<()> {\n'
            '    let cloud_kind = "terraform";\n'
            '    if cloud_kind == "terraform" { return Err(err()); }\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_deployment_tool_is_not_dependency_delegation(self):
        findings = scan_snippet(
            "adapters/cloud/src/deploy/mod.rs",
            'pub async fn deploy() {\n'
            '    mgc_run("terraform", &["apply"], &opts);\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "deploy")
        self.assertEqual(findings[0]["status"], "violation")


class PythonProcessInventory(unittest.TestCase):
    """Python process APIs are inventoried without treating prose as execution."""

    def test_import_aliases_and_os_shell_calls_are_reported(self):
        findings = gate.scan_python_text(
            "scripts/example.py",
            "import subprocess as sp\n"
            "from os import system as shell\n"
            "def run():\n"
            "    sp.run(['cargo', 'fetch'])\n"
            "    shell('pip install demo')\n",
        )
        self.assertEqual(
            [(item["line"], item["api"], item["executable"]) for item in findings],
            [
                (4, "subprocess.run", "cargo"),
                (5, "os.system", "<shell-command>"),
            ],
        )
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

    def test_subprocess_shell_command_is_not_guessed_as_executable(self):
        findings = gate.scan_python_text(
            "scripts/example.py",
            "import subprocess\n"
            "subprocess.run('go test ./...', shell=True)\n",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<shell-command>")

    def test_comments_and_non_execution_strings_are_ignored(self):
        findings = gate.scan_python_text(
            "scripts/example.py",
            "# subprocess.run(['cargo', 'fetch'])\n"
            "message = \"os.system('pip install demo')\"\n",
        )
        self.assertEqual(findings, [])

    def test_dynamic_process_target_is_not_dropped(self):
        findings = gate.scan_python_text(
            "tools/tool.py",
            "import subprocess\n"
            "def invoke(argv):\n"
            "    subprocess.Popen(argv)\n",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<dynamic>")
        self.assertEqual(findings[0]["api"], "subprocess.Popen")

    def test_invalid_python_is_reported_as_unparsed_not_clean(self):
        findings = gate.scan_python_text("scripts/broken.py", "def broken(:\n")
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<unparsed-python>")
        self.assertIsNotNone(findings[0]["parse_error"])

    def test_process_module_star_import_is_unreviewed(self):
        findings = gate.scan_python_text(
            "scripts/star.py",
            "from subprocess import *\n"
            "def invoke():\n"
            "    Popen(['cargo', 'fetch'])\n",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["api"], "<wildcard-import:subprocess>")
        self.assertEqual(findings[0]["executable"], "<dynamic>")


class ShellProcessInventory(unittest.TestCase):
    """Direct shell command heads are inventoried without matching prose."""

    def test_finds_pm_commands_after_env_sudo_and_absolute_path(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "sudo env PIP_DISABLE_PIP_VERSION_CHECK=1 /usr/local/bin/pip install demo\n",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["line"], 1)
        self.assertEqual(findings[0]["executable"], "pip")
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_finds_pm_command_after_env_unset_option(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "env -u HOME PATH=/tmp /usr/bin/npm install\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["npm"])

    def test_finds_pm_command_after_timeout_and_nice_wrappers(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "timeout 20 nice -n 5 cargo fetch\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["cargo"])

    def test_command_substitution_is_not_hidden_in_an_assignment(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'VERSION="$(npm install demo)"\n',
        )
        self.assertEqual([item["executable"] for item in findings], ["<dynamic-shell-command>"])

    def test_shell_variables_in_arguments_do_not_mark_command_dynamic(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'echo "$HOME"\n'
            'printf "%s\\n" "$PROJECT_ROOT"\n',
        )
        self.assertEqual(findings, [])

    def test_known_pm_in_argument_substitution_is_still_a_blocker(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'echo "$(npm install demo)"\n'
            'VERSION="`cargo fetch`"\n',
        )
        self.assertEqual(
            [item["executable"] for item in findings],
            ["<dynamic-shell-command>", "<dynamic-shell-command>"],
        )

    def test_eval_and_source_are_dynamic_execution_boundaries(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'eval "$TOOL install"\n'
            'source "$PROJECT_HOOK"\n',
        )
        self.assertEqual(
            [item["executable"] for item in findings],
            ["<dynamic-shell-command>", "<dynamic-shell-command>"],
        )

    def test_shell_interpreter_execution_is_dynamic_even_with_static_script_name(self):
        findings = gate.scan_shell_text("scripts/example.sh", "bash -c 'cargo fetch'\n")
        self.assertEqual([item["executable"] for item in findings], ["<dynamic-shell-command>"])

    def test_finds_pipeline_and_continued_package_manager_commands(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "echo prepare | cargo fetch \\\n  --locked\n"
            "npm ci\n",
        )
        self.assertEqual(
            [(item["line"], item["executable"]) for item in findings],
            [(1, "cargo"), (3, "npm")],
        )

    def test_ignores_comments_and_echoed_command_names(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "# cargo fetch is intentionally not executed\n"
            "echo 'npm install is a displayed example'\n",
        )
        self.assertEqual(findings, [])

    def test_multiline_single_quoted_program_is_not_misread_as_unclosed(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "awk 'BEGIN { print \"npm install\" }\n"
            "END { print \"cargo fetch\" }'\n",
        )
        self.assertEqual(findings, [])

    def test_malformed_shell_quoting_is_unparsed_not_clean(self):
        findings = gate.scan_shell_text("scripts/broken.sh", "cargo fetch 'unterminated\n")
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<unparsed-shell>")
        self.assertIsNotNone(findings[0]["parse_error"])

    def test_trailing_shell_continuation_is_unparsed_not_clean(self):
        findings = gate.scan_shell_text("scripts/broken.sh", "cargo fetch \\\n")
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<unparsed-shell>")

class JavaScriptProcessInventory(unittest.TestCase):
    """JavaScript runtime process APIs are visible to the static gate."""

    def test_detects_child_process_bun_and_deno_apis(self):
        findings = gate.scan_javascript_text(
            "cli/tooling.js",
            "import { spawn as launch } from 'node:child_process';\n"
            "launch('npm', ['install']);\n"
            "Bun.spawn(['cargo', 'fetch']);\n"
            "new Deno.Command('deno', { args: ['install'] });\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [(2, "child_process.spawn"), (3, "Bun.spawn"), (4, "Deno.Command")],
        )
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

    def test_detects_commonjs_destructuring_and_inline_require(self):
        findings = gate.scan_javascript_text(
            "scripts/commonjs.cjs",
            "const { execFile: launch } = require('child_process');\n"
            "launch('npm', ['install']);\n"
            "require('child_process').spawn('cargo', ['fetch']);\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [(2, "child_process.execFile"), (3, "child_process.spawn")],
        )

    def test_detects_esm_namespace_alias(self):
        findings = gate.scan_javascript_text(
            "scripts/namespace.mjs",
            "import * as cp from 'node:child_process';\n"
            "cp.execFileSync('npm', ['install']);\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [(2, "child_process.execFileSync")],
        )

    def test_detects_computed_optional_and_dynamic_import_aliases(self):
        findings = gate.scan_javascript_text(
            "scripts/indirect-process.js",
            "const cp = require('child_process');\n"
            "const launch = cp['spawn'];\n"
            "launch('npm', ['install']);\n"
            "cp?.['execFileSync']('cargo', ['fetch']);\n"
            "const dynamicCp = await import('node:child_process');\n"
            "dynamicCp.spawnSync('pnpm', ['install']);\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [
                (3, "child_process.spawn"),
                (4, "child_process.execFileSync"),
                (6, "child_process.spawnSync"),
            ],
        )

    def test_detects_dynamic_import_destructure_and_namespace_reassignment(self):
        findings = gate.scan_javascript_text(
            "scripts/dynamic-process.mjs",
            "const { exec: launch } = await import('node:child_process');\n"
            "launch('npm', ['install']);\n"
            "const cp = await import('node:child_process');\n"
            "const child = cp;\n"
            "child.spawn('cargo', ['fetch']);\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [(2, "child_process.exec"), (5, "child_process.spawn")],
        )

    def test_computed_process_examples_inside_strings_are_not_findings(self):
        findings = gate.scan_javascript_text(
            "scripts/process-docs.js",
            "// cp['spawn']('npm') is an example\n"
            'const docs = "const run = cp[\'exec\']; run(\\\'npm\\\')";\n',
        )
        self.assertEqual(findings, [])

    def test_ignores_comments_and_string_examples(self):
        findings = gate.scan_javascript_text(
            "cli/example.js",
            "// spawn('npm')\n"
            "const docs = \"Bun.spawn(['npm']) child_process.exec('npm install')\";\n",
        )
        self.assertEqual(findings, [])


class PowerShellProcessInventory(unittest.TestCase):
    """PowerShell process boundaries are not hidden from the gate."""

    def test_detects_process_apis_and_native_package_commands(self):
        findings = gate.scan_powershell_text(
            "scripts/setup.ps1",
            "Start-Process npm -ArgumentList 'install'\n"
            "& $PackageManager install\n"
            "cargo fetch\n",
        )
        self.assertEqual(
            [(item["line"], item["api"]) for item in findings],
            [(1, "Start-Process"), (2, "call-operator"), (3, "native-command")],
        )
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

    def test_ignores_comments_and_quoted_examples(self):
        findings = gate.scan_powershell_text(
            "scripts/example.ps1",
            "# Start-Process npm\n"
            "# saps npm install\n"
            "$example = 'iex cargo fetch'\n"
            "$example = 'cargo fetch'\n",
        )
        self.assertEqual(findings, [])

    def test_finds_quoted_executable_invoked_with_call_operator(self):
        findings = gate.scan_powershell_text(
            "scripts/quoted.ps1",
            "& 'npm' install\n",
        )
        self.assertEqual([(item["api"], item["executable"]) for item in findings], [("call-operator", "npm")])

    def test_detects_builtin_process_aliases(self):
        findings = gate.scan_powershell_text(
            "scripts/aliases.ps1",
            "start npm install\n"
            "saps cargo -ArgumentList 'fetch'\n"
            "iex $commandText\n"
            "icm $hostName { go mod download }\n"
            "sajb { pnpm install }\n",
        )
        self.assertEqual(
            [(item["line"], item["api"], item["executable"]) for item in findings],
            [
                (1, "process-alias", "Start-Process"),
                (2, "process-alias", "Start-Process"),
                (3, "process-alias", "Invoke-Expression"),
                (4, "native-command", "go"),
                (4, "process-alias", "Invoke-Command"),
                (5, "native-command", "pnpm"),
                (5, "process-alias", "Start-Job"),
            ],
        )


class MarkerDeclaration(unittest.TestCase):
    """A DELEGATED marker documents debt but never waives the strict gate."""

    def test_marker_in_doc_block_declares_delegation(self):
        findings = scan_snippet(
            "cli/src/commands/model/mod.rs",
            '/// Quantize models.\n'
            '///\n'
            '/// DELEGATED: runs inside the external python package.\n'
            'fn quantize(path: &str) -> Result<()> {\n'
            '    let status = std::process::Command::new("python3")\n'
            '        .status()?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(
            findings[0]["status"],
            "violation",
            "documenting a dependency-manager spawn must not make it pass",
        )
        self.assertTrue(findings[0]["declared_delegation"])


class MessageMacroExclusion(unittest.TestCase):
    """Bare literals inside message macros are prose, not spawns."""

    def test_tool_name_inside_format_is_not_a_finding(self):
        findings = scan_snippet(
            "adapters/web/src/audit.rs",
            'pub async fn run_audit(project_root: &Path) -> MgResult<AuditReport> {\n'
            '    return Err(MgError::Other(format!(\n'
            '        "rival lockfile(s) detected — run `mgc import {}",\n'
            '        if rival.iter().any(|r| r.contains("deno")) {\n'
            '            "deno"\n'
            '        } else {\n'
            '            "bun"\n'
            '        }\n'
            '    )));\n'
            '}\n',
        )
        tools = [f["tool"] for f in findings]
        self.assertNotIn("deno", tools)
        self.assertNotIn("bun", tools)

    def test_escaped_quote_inside_format_does_not_break_span(self):
        spans = gate._message_spans(
            'format!("say \\"hi\\" {}",\n    "deno"\n)'
        )
        self.assertEqual(len(spans), 1)
        self.assertTrue(gate._inside_spans(spans, 30))

    def test_bare_table_literal_outside_message_still_flagged(self):
        findings = scan_snippet(
            "cli/src/commands/core/install/app.rs",
            'fn tool_command() -> Vec<(&str, Vec<String>)> {\n'
            '    vec![\n'
            '        ("uv", vec!["sync".to_string()]),\n'
            '    ]\n'
            '}\n',
        )
        self.assertTrue(
            any(f["tool"] == "uv" for f in findings),
            "tool-table literals outside messages must stay measured",
        )

    def test_call_shaped_spawn_inside_message_still_flagged(self):
        # A call-shaped spawn can never hide behind a message macro: the
        # call patterns stay active inside message spans by design.
        findings = scan_snippet(
            "adapters/lib/src/install/mod.rs",
            'pub async fn run_install() -> MgResult<()> {\n'
            '    let msg = format!("about to run {}", "cargo");\n'
            '    let _ = mgc_exec::run::run("cargo", &args, &opts)?;\n'
            '    Ok(())\n'
            '}\n',
        )
        self.assertTrue(
            any(f["tool"] == "cargo" for f in findings),
            "call-shaped spawns must match even near message macros",
        )


class ConstArrayExclusion(unittest.TestCase):
    """Top-level deny-list arrays name tools without spawning them."""

    def test_top_level_deny_list_literals_are_not_findings(self):
        findings = scan_snippet(
            "core/crates/mgc-config/src/hooks.rs",
            "const DENY: &[&str] = &[\n"
            '    "npm", "npx",\n'
            '    "cargo",\n'
            "];\n"
            "\n"
            "pub fn run_hooks() -> Result<()> {\n"
            "    Ok(())\n"
            "}\n",
        )
        self.assertEqual(findings, [])

    def test_sql_queries_are_not_misclassified_as_process_command_tables(self):
        findings = scan_snippet(
            "core/crates/mgc-store/src/database.rs",
            'fn inspect_db(conn: &Connection) {\n'
            '    conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0));\n'
            '    conn.query_row("SELECT COUNT(*) FROM package_files", [], |row| row.get(0));\n'
            '}\n',
        )
        self.assertEqual(findings, [])

    def test_public_const_tool_inventory_is_not_a_spawn(self):
        findings = scan_snippet(
            "cli/src/commands/dep_gate.rs",
            "pub const SPLIT_LANGUAGES: &[&str] = &[\n"
            '    "python",\n'
            '    "go",\n'
            "];\n",
        )
        self.assertEqual(findings, [])

    def test_spawn_next_to_deny_list_still_flagged(self):
        findings = scan_snippet(
            "adapters/lib/src/install/mod.rs",
            "const DENY: &[&str] = &[\n"
            '    "npm",\n'
            "];\n"
            "\n"
            "pub async fn run_install() -> MgResult<()> {\n"
            '    let _ = mgc_exec::run::run("cargo", &args, &opts)?;\n'
            "    Ok(())\n"
            "}\n",
        )
        self.assertTrue(
            any(f["tool"] == "cargo" for f in findings),
            "call-shaped spawns stay measured beside deny lists",
        )


class RepoLedgerContract(unittest.TestCase):
    """Production toolchain delegation stays a visible blocking finding."""

    def test_mgc_dist_routes_cargo_and_rustc_through_scoped_executor(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "tools/mgc-dist/src/main.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()
        self.assertNotIn("std::process::Command", source)
        self.assertIn('mgc_exec::run::run_inherited("cargo"', source)
        self.assertIn('mgc_exec::run::run("rustc"', source)
        self.assertIn('"--locked"', source)
        self.assertIn('"--offline"', source)
        findings = gate.scan_file(rel_path, abs_path)
        tool_findings = [item for item in findings if item["tool"] in {"cargo", "rustc"}]
        self.assertEqual({item["tool"] for item in tool_findings}, {"cargo", "rustc"})
        self.assertTrue(
            all(
                item["status"] == "allowed"
                and item["op_class"] == "audited-executor-route"
                and item["classification_reason"]
                for item in tool_findings
            ),
            f"mgc-dist routes must stay visible with an explicit executor classification: {tool_findings}",
        )

    def test_locked_build_and_device_control_executor_routes_are_exactly_scoped(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        cases = (
            (
                "cli/src/commands/build.rs",
                "build_lib",
                "node",
                (
                    "ExecutionScope::BuildRunner",
                    'run_inherited("node"',
                    'node_bin_args(root, "tsc"',
                ),
            ),
            (
                "cli/src/commands/build.rs",
                "build_cloud",
                "node",
                (
                    "ExecutionScope::BuildRunner",
                    'run_inherited("node"',
                    'node_bin_args(root, "cdk"',
                ),
            ),
            (
                "cli/src/commands/build/web_engine.rs",
                "build_rust_with_env",
                "cargo",
                ("ExecutionScope::BuildRunner", '"--locked"', '"--offline"'),
            ),
            (
                "cli/src/commands/core/dev/app.rs",
                "run_xcrun",
                "xcrun",
                ("ExecutionScope::DeviceControl", 'run("xcrun"'),
            ),
            (
                "cli/src/commands/core/dev/app.rs",
                "run_android_device_command",
                "adb",
                ("ExecutionScope::DeviceControl", 'run("adb"'),
            ),
        )
        for rel_path, function, tool, required_tokens in cases:
            with self.subTest(function=function, tool=tool):
                abs_path = os.path.join(repo_root, rel_path)
                with open(abs_path, "r", encoding="utf-8") as handle:
                    source = handle.read()
                start = source.index(f"fn {function}(")
                end = source.index("\n}", start) + 2
                function_body = source[start:end]
                self.assertTrue(
                    all(token in function_body for token in required_tokens),
                    f"{rel_path}:{function} must enforce its executor boundary",
                )
                findings = gate.scan_file(rel_path, abs_path)
                route = (rel_path, function, tool)
                route_findings = [
                    item for item in findings
                    if (item["function"], item["tool"]) == (function, tool)
                ]
                self.assertIn(route, gate.AUDITED_EXECUTOR_ROUTES)
                self.assertEqual(len(route_findings), 1)
                self.assertEqual(route_findings[0]["status"], "allowed")
                self.assertEqual(route_findings[0]["op_class"], "audited-executor-route")
                self.assertEqual(
                    route_findings[0]["classification_reason"],
                    gate.AUDITED_EXECUTOR_ROUTES[route],
                )

    def test_web_lifecycle_executor_is_scoped_to_policy_approved_install(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "adapters/web/src/lifecycle.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            lifecycle = handle.read()
        route = (rel_path, "run_script", "<dynamic:invocation>")
        findings = gate.scan_file(rel_path, abs_path)
        route_findings = [
            item for item in findings
            if (item["function"], item["tool"]) == ("run_script", "<dynamic:invocation>")
        ]
        self.assertIn(route, gate.AUDITED_EXECUTOR_ROUTES)
        self.assertEqual(len(route_findings), 1)
        self.assertEqual(route_findings[0]["status"], "allowed")
        self.assertEqual(route_findings[0]["op_class"], "audited-executor-route")
        self.assertIn("parse_script_invocation(script)", lifecycle)
        self.assertIn("ExecutionScope::Install", lifecycle)
        self.assertIn("#[cfg(test)]\n    pub(crate) fn run_scripts(", lifecycle)

        install_path = os.path.join(repo_root, "adapters/web/src/install/mod.rs")
        with open(install_path, "r", encoding="utf-8") as handle:
            install = handle.read()
        policy_position = install.index("match decide_lifecycle_scripts(")
        runner_position = install.index("LifecycleRunner::run_scripts_with_snapshot(")
        self.assertLess(install.index("load_trust_policies(&layout)"), policy_position)
        self.assertLess(policy_position, runner_position)

        production_callers = []
        for root, _, filenames in os.walk(os.path.join(repo_root, "adapters/web/src")):
            for filename in filenames:
                if not filename.endswith(".rs") or "/test/" in (root + "/"):
                    continue
                path = os.path.join(root, filename)
                with open(path, "r", encoding="utf-8") as handle:
                    if "LifecycleRunner::run_scripts_with_snapshot(" in handle.read():
                        production_callers.append(os.path.relpath(path, repo_root).replace(os.sep, "/"))
        self.assertEqual(production_callers, ["adapters/web/src/install/mod.rs"])

    def test_publish_lifecycle_executor_is_scoped_to_explicit_opt_in(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "cli/src/commands/publish.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()

        route = (rel_path, "run_lifecycle", "<dynamic:invocation>")
        route_findings = [
            item for item in gate.scan_file(rel_path, abs_path)
            if (item["function"], item["tool"]) == ("run_lifecycle", "<dynamic:invocation>")
        ]
        self.assertIn(route, gate.AUDITED_EXECUTOR_ROUTES)
        self.assertEqual(len(route_findings), 1)
        self.assertEqual(route_findings[0]["status"], "allowed")
        self.assertEqual(route_findings[0]["op_class"], "audited-executor-route")

        function_start = source.index("fn run_lifecycle(")
        function_end = source.index("\n}\n", function_start) + 2
        runner = source[function_start:function_end]
        self.assertIn("reject_forbidden_pm_script(cmd)", runner)
        self.assertIn("parse_script_invocation(cmd)", runner)
        self.assertIn("ExecutionScope::Install", runner)
        self.assertIn("cwd: Some(project_root.to_path_buf())", runner)

        publish_start = source.index("async fn publish_project(")
        publish_end = source.index("\n}\n", publish_start) + 2
        publish = source[publish_start:publish_end]
        gate_position = publish.index("publish_lifecycle_decision(")
        for hook in ("prepublishOnly", "prepublish", "prepare"):
            hook_position = publish.index(f'run_lifecycle(&pkg_json, "{hook}"')
            self.assertLess(gate_position, hook_position)

    def test_publish_git_checks_are_closed_and_project_scoped(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "cli/src/commands/publish.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()

        route = (rel_path, "run_git_capture", "git")
        findings = [
            item for item in gate.scan_file(rel_path, abs_path)
            if (item["function"], item["tool"]) == ("run_git_capture", "git")
        ]
        self.assertIn(route, gate.AUDITED_EXECUTOR_ROUTES)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["status"], "allowed")
        self.assertEqual(findings[0]["op_class"], "audited-executor-route")

        publish_start = source.index("async fn publish_project(")
        publish_end = source.index("\n}\n", publish_start) + 2
        publish = source[publish_start:publish_end]
        git_checks_start = source.index("fn git_checks(")
        git_checks_end = source.index("\n}\n", git_checks_start) + 2
        git_checks = source[git_checks_start:git_checks_end]
        self.assertIn(
            "git_checks(args.publish_branch.as_deref(), project_root)",
            publish,
        )
        self.assertIn("GitReadQuery::ValidateBranch", git_checks)
        self.assertIn("publish_branch.is_some()", git_checks)
        self.assertIn("GitReadQuery::CommitsBehind(branch.clone())", git_checks)

        query_start = source.index("enum GitReadQuery")
        query_end = source.index("impl GitReadQuery", query_start)
        queries = source[query_start:query_end]
        for variant in (
            "WorkingTreeStatus",
            "CurrentBranch",
            "ValidateBranch",
            "UpstreamRef",
            "CommitsBehind",
        ):
            self.assertIn(variant, queries)
        argv_start = query_end
        argv_end = source.index("\n}\n", argv_start) + 2
        self.assertIn("--end-of-options", source[argv_start:argv_end])

        runner_start = source.index("fn run_git_capture(")
        runner_end = source.index("\n}\n", runner_start) + 2
        runner = source[runner_start:runner_end]
        self.assertIn("git_exec_options(project_root)", runner)
        self.assertIn('run("git", &args, &opts)', runner)
        self.assertNotIn('Command::new("git")', source)

    def test_workflow_blindspot_distinguishes_pinned_refs_from_unreviewed_commands(self):
        workflow = next(
            item for item in gate.UNSCANNED_PROCESS_SURFACES
            if item["path"] == ".github/workflows"
        )
        self.assertIn("immutable action-reference syntax has a dedicated CI gate", workflow["reason"])
        self.assertIn("command provisioning", workflow["reason"])
        self.assertIn("upstream SHA provenance", workflow["reason"])

    def test_indirect_javascript_blindspot_remains_explicit(self):
        javascript = next(
            item for item in gate.UNSCANNED_PROCESS_SURFACES
            if "JavaScript" in item["path"]
        )
        self.assertIn("arbitrary alias/data-flow", javascript["reason"])
        self.assertIn("template-literal expressions", javascript["reason"])
        self.assertIn("reflective or module-loader wrappers", javascript["reason"])
        self.assertIn("common built-in process aliases", javascript["reason"])
        self.assertIn("user-defined aliases/data-flow", javascript["reason"])

    def test_repo_ledger_fails_while_product_workflows_spawn_external_tools(self):
        code = gate.main()
        self.assertEqual(
            code,
            1,
            "build/dev/test/deploy/flash tool spawns must remain release blockers",
        )
        with open(gate.OUTPUT_PATH, "r", encoding="utf-8") as handle:
            ledger = json.load(handle)
        findings = ledger["findings"]
        self.assertEqual(ledger["schema"], "dependency-delegation-audit/10")
        self.assertGreater(ledger["summary"]["review_required"], 0)
        self.assertGreater(ledger["summary"]["blocking"], 0)
        self.assertEqual(
            ledger["scan_languages"],
            [
                "Rust process boundaries",
                "Python stdlib process APIs",
                "Shell command/process surfaces (static inventory)",
                "JavaScript/TypeScript process APIs (lexical inventory)",
                "PowerShell process APIs (lexical inventory)",
            ],
        )
        self.assertEqual(ledger["python_scan_roots"], gate.PYTHON_SCAN_ROOTS)
        self.assertGreater(ledger["python_files_scanned"], 0)
        self.assertFalse(ledger["coverage_complete"])
        self.assertTrue(ledger["unscanned_process_surfaces"])
        self.assertTrue(ledger["python_process_calls"])
        self.assertEqual(ledger["shell_scan_roots"], gate.SHELL_SCAN_ROOTS)
        self.assertGreater(ledger["shell_files_scanned"], 0)
        self.assertTrue(ledger["shell_process_calls"])
        self.assertGreater(ledger["javascript_files_scanned"], 0)
        self.assertGreater(ledger["powershell_files_scanned"], 0)
        self.assertIsInstance(ledger["javascript_process_calls"], list)
        self.assertIsInstance(ledger["powershell_process_calls"], list)
        self.assertTrue(
            all(
                item["review_status"] == "unreviewed"
                for item in ledger["python_process_calls"]
            )
        )
        self.assertTrue(
            all(
                item["review_status"] == "unreviewed"
                for item in ledger["shell_process_calls"]
            )
        )
        self.assertTrue(
            all(
                item["review_status"] == "unreviewed"
                for item in ledger["javascript_process_calls"]
            )
        )
        self.assertTrue(
            all(
                item["review_status"] == "unreviewed"
                for item in ledger["powershell_process_calls"]
            )
        )
        self.assertIsInstance(ledger["dirty_paths_count"], int)
        self.assertGreater(ledger["dirty_paths_count"], 0)
        self.assertTrue(
            all(
                item["evidence_kind"]
                in {
                    "command_descriptor",
                    "dynamic_spawn_call",
                    "spawn_or_wrapper_call",
                }
                for item in findings
            )
        )
        self.assertTrue(findings, "current product tree must expose the known debt")
        allowed = [item for item in findings if item["status"] == "allowed"]
        self.assertTrue(
            all(
                (
                    any(
                        segment in ("test", "tests", "bench", "benches")
                        for segment in item["file"].split("/")[:-1]
                    )
                    or (
                        item["file"] == "cli/src/commands/doctor.rs"
                        and (item["function"], item["tool"])
                        in {
                            ("tool_version", "<dynamic:bin>"),
                            ("fs_avail", "df"),
                        }
                    )
                    or (
                        (
                            item["file"],
                            item["function"],
                            item["tool"],
                        ) in gate.AUDITED_EXECUTOR_ROUTES
                        and item["classification_reason"]
                        == gate.AUDITED_EXECUTOR_ROUTES[
                            (item["file"], item["function"], item["tool"])
                        ]
                    )
                )
                for item in allowed
            ),
            "only harnesses, exact doctor probes, and exact audited executor routes may be allowed",
        )
        self.assertTrue(
            any(
                item["file"] == "cli/src/commands/build.rs"
                and item["tool"] == "gradle"
                for item in findings
            ),
            "the Java/Android build delegation must not be hidden by the ledger",
        )
        self.assertFalse(
            any(
                item["file"] == "core/crates/mgc-exec/src/run.rs"
                and item["tool"] == "where.exe"
                for item in findings
            ),
            "Windows PATH resolution must stay in-process instead of spawning where.exe",
        )
        self.assertTrue(
            any(
                item["file"] == "core/crates/mgc-exec/src/run.rs"
                and item["tool"] == "taskkill"
                and item["status"] == "review-required"
                for item in findings
            ),
            "Windows process-tree termination must remain a blocking reviewed boundary",
        )
        self.assertTrue(
            any(
                item["file"] == "tools/mgc-dist/src/main.rs"
                and item["tool"] == "cargo"
                for item in findings
            ),
            "the release packaging build invocation must be inventoried",
        )
        self.assertTrue(
            any(
                item["file"] == "tools/mgc-dist/src/main.rs"
                and item["tool"] == "rustc"
                and item["function"] == "detect_host_target"
                for item in findings
            ),
            "the read-only host-target query must be inventoried",
        )


class LaneGateCoverage(unittest.TestCase):
    """Every dependency lane passes through dep_gate:: or GATE-EXEMPT."""

    def setUp(self):
        self.repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    def test_lane_with_gate_call_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            lane_dir = os.path.join(tmp, "cli", "src", "commands", "core", "install")
            os.makedirs(lane_dir)
            with open(os.path.join(lane_dir, "ai.rs"), "w", encoding="utf-8") as handle:
                handle.write(
                    "pub async fn install() -> Result<()> {\n"
                    "    crate::commands::dep_gate::gate(\n"
                    "        \"ai\",\n"
                    "        crate::commands::dep_gate::DepOp::Install,\n"
                    "        Some(tool),\n"
                    "        &compat,\n"
                    "        None,\n"
                    "    )?;\n"
                    "    Ok(())\n"
                    "}\n"
                )
            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = [
                os.path.relpath(
                    os.path.join(tmp, "cli", "src", "commands", "core", "install"), tmp
                )
            ]
            try:
                checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots
        self.assertEqual(missing, [])
        self.assertEqual(checked, 1)

    def test_lane_without_gate_or_exempt_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            lane_dir = os.path.join(tmp, "cli", "src", "commands", "core", "add")
            os.makedirs(lane_dir)
            with open(os.path.join(lane_dir, "ai.rs"), "w", encoding="utf-8") as handle:
                handle.write(
                    "pub async fn add() -> Result<()> {\n"
                    "    shared::ai_run_tool(&root, tool, &args)?;\n"
                    "    Ok(())\n"
                    "}\n"
                )
            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = [
                os.path.relpath(
                    os.path.join(tmp, "cli", "src", "commands", "core", "add"), tmp
                )
            ]
            try:
                checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots
        self.assertEqual(len(missing), 1)
        self.assertTrue(missing[0].endswith("ai.rs"))

    def test_pure_router_without_functions_is_exempt(self):
        with tempfile.TemporaryDirectory() as tmp:
            lane_dir = os.path.join(tmp, "cli", "src", "commands", "core", "list")
            os.makedirs(lane_dir)
            with open(os.path.join(lane_dir, "mod.rs"), "w", encoding="utf-8") as handle:
                handle.write("pub mod ai;\npub mod web;\n")
            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = [
                os.path.relpath(
                    os.path.join(tmp, "cli", "src", "commands", "core", "list"), tmp
                )
            ]
            try:
                checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots
        self.assertEqual(missing, [])

    def test_lane_gate_fails_closed_when_source_cannot_be_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            lane_dir = os.path.join(tmp, "cli", "src", "commands", "core", "install")
            os.makedirs(lane_dir)
            source_path = os.path.join(lane_dir, "ai.rs")
            with open(source_path, "w", encoding="utf-8") as handle:
                handle.write("pub async fn install() -> Result<()> { Ok(()) }\n")

            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = [os.path.relpath(lane_dir, tmp)]
            real_open = open

            def deny_source_read(path, *args, **kwargs):
                if os.fspath(path) == source_path:
                    raise PermissionError("fixture unreadable source")
                return real_open(path, *args, **kwargs)

            try:
                with mock.patch("builtins.open", side_effect=deny_source_read):
                    checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots

        self.assertEqual(checked, 1)
        self.assertEqual(
            missing,
            [f"<scan-error:unreadable> {os.path.relpath(source_path, tmp)}"],
        )

    def test_lane_gate_fails_closed_when_root_is_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = ["cli/src/commands/core/install"]
            try:
                checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots

        self.assertEqual(checked, 1)
        self.assertEqual(
            missing,
            ["<scan-error:missing-root> cli/src/commands/core/install"],
        )

    def test_lane_gate_fails_closed_on_directory_walk_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            lane_dir = os.path.join(tmp, "cli", "src", "commands", "core", "install")
            os.makedirs(lane_dir)
            roots = gate.LANE_GATE_ROOTS
            gate.LANE_GATE_ROOTS = [os.path.relpath(lane_dir, tmp)]

            def failed_walk(_path, onerror=None):
                error = PermissionError("fixture unreadable directory")
                error.filename = os.path.join(lane_dir, "blocked")
                onerror(error)
                return iter(())

            try:
                with mock.patch.object(gate.os, "walk", side_effect=failed_walk):
                    checked, missing = gate.check_lane_gate(tmp)
            finally:
                gate.LANE_GATE_ROOTS = roots

        self.assertEqual(checked, 1)
        self.assertEqual(
            missing,
            [
                "<scan-error:walk> "
                + os.path.join("cli", "src", "commands", "core", "install", "blocked")
            ],
        )

    def test_real_tree_has_no_missing_lane(self):
        checked, missing = gate.check_lane_gate(self.repo_root)
        self.assertEqual(missing, [], f"lanes without gate: {missing}")
        self.assertGreater(checked, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
