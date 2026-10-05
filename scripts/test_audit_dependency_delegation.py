#!/usr/bin/env python3
"""Self-tests for audit_dependency_delegation.py (P0-A / T0.1).

Stdlib only — run with `python3 scripts/test_audit_dependency_delegation.py`.
Every test is hermetic except the repo-ledger contract, which re-runs the
gate over the working tree and asserts known production toolchain delegation
is still release-blocking.

(Tự kiểm cho gate audit uỷ quyền: chỉ dùng stdlib — chạy bằng
`python3 scripts/test_audit_dependency_delegation.py`.)
"""

import ast
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

    def test_cfg_test_only_helper_is_a_test_fixture_without_test_attribute(self):
        findings = scan_snippet(
            "cli/src/commands/audit/signatures.rs",
            "#[cfg(test)]\n"
            "fn verify_bundle() {\n"
            '    "npm",\n'
            "}\n",
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["op_class"], "test-fixture")
        self.assertEqual(findings[0]["status"], "allowed")

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

    def test_python_process_helper_review_binds_the_selected_executable(self):
        source = (
            "def select_shell():\n"
            "    return 'bash'\n"
        )
        function = ast.parse(source).body[0]
        fingerprint = gate._python_process_helper_fingerprint(function)
        path = "scripts/example.py"
        reviews = {
            path: {
                "select_shell": (
                    fingerprint,
                    "Selects the intended shell executable for the platform.",
                ),
            },
        }

        with mock.patch.dict(gate.PYTHON_PROCESS_HELPER_REVIEWS, reviews, clear=True):
            approved = gate.review_python_process_helpers(
                {path: source}, expected_files={path}
            )
            changed = gate.review_python_process_helpers(
                {path: source.replace("'bash'", "'sh'")}, expected_files={path}
            )
            rebound = gate.review_python_process_helpers(
                {path: source + "select_shell = lambda: 'attacker.exe'\n"},
                expected_files={path},
            )
            imported = gate.review_python_process_helpers(
                {path: source + "from attacker import select_shell\n"},
                expected_files={path},
            )
            wildcard = gate.review_python_process_helpers(
                {path: source + "from attacker import *\n"},
                expected_files={path},
            )
            namespace_write = gate.review_python_process_helpers(
                {path: source + "globals()['select_shell'] = lambda: 'attacker.exe'\n"},
                expected_files={path},
            )
            function_globals_write = gate.review_python_process_helpers(
                {
                    path: source
                    + "select_shell.__globals__['select_shell'] = "
                    + "lambda: 'attacker.exe'\n"
                },
                expected_files={path},
            )
            builtin_alias = gate.review_python_process_helpers(
                {
                    path: source
                    + "from builtins import globals as module_namespace\n"
                    + "module_namespace()['select_shell'] = lambda: 'attacker.exe'\n"
                },
                expected_files={path},
            )
            module_attribute = gate.review_python_process_helpers(
                {
                    path: source
                    + "import sys\n"
                    + "sys.modules[__name__].select_shell = lambda: 'attacker.exe'\n"
                },
                expected_files={path},
            )
            removed = gate.review_python_process_helpers({}, expected_files={path})

        self.assertEqual([item["review_status"] for item in approved], ["reviewed"])
        self.assertEqual([item["review_status"] for item in changed], ["stale-review"])
        self.assertEqual([item["review_status"] for item in rebound], ["stale-review"])
        self.assertEqual([item["review_status"] for item in imported], ["stale-review"])
        self.assertEqual([item["review_status"] for item in wildcard], ["stale-review"])
        self.assertEqual(
            [item["review_status"] for item in namespace_write], ["stale-review"]
        )
        self.assertEqual(
            [item["review_status"] for item in function_globals_write],
            ["stale-review"],
        )
        self.assertEqual(
            [item["review_status"] for item in builtin_alias], ["stale-review"]
        )
        self.assertEqual(
            [item["review_status"] for item in module_attribute], ["stale-review"]
        )
        self.assertEqual([item["review_status"] for item in removed], ["stale-review"])

    def test_python_helper_fingerprint_is_stable_across_empty_ast_type_params(self):
        source = "def select_shell():\n    return 'bash'\n"
        without_type_params = ast.parse(source).body[0]
        with_type_params = ast.parse(source).body[0]
        with_type_params.type_params = []

        self.assertEqual(
            gate._python_process_helper_fingerprint(without_type_params),
            gate._python_process_helper_fingerprint(with_type_params),
        )

        with_type_params.type_params = [ast.Name(id="Shell", ctx=ast.Load())]
        self.assertNotEqual(
            gate._python_process_helper_fingerprint(without_type_params),
            gate._python_process_helper_fingerprint(with_type_params),
        )

    def test_python_process_helper_review_binds_global_import_dependencies(self):
        source = (
            "import shutil\n"
            "from pathlib import Path\n"
            "def select_shell():\n"
            "    return shutil.which('git')\n"
        )
        function = ast.parse(source).body[2]
        fingerprint = gate._python_process_helper_fingerprint(function)
        path = "scripts/example.py"
        reviews = {
            path: {
                "select_shell": (
                    fingerprint,
                    "Selects the shell using the reviewed shutil import.",
                ),
            },
        }
        dependencies = {
            path: {
                "select_shell": {
                    "shutil": "shutil",
                    "Path": "pathlib.Path",
                },
            },
        }

        with mock.patch.dict(gate.PYTHON_PROCESS_HELPER_REVIEWS, reviews, clear=True), \
             mock.patch.dict(
                 gate.PYTHON_PROCESS_HELPER_DEPENDENCIES, dependencies, clear=True
             ):
            approved = gate.review_python_process_helpers(
                {path: source}, expected_files={path}
            )
            mutated = gate.review_python_process_helpers(
                {path: source + "shutil.which = lambda _: 'attacker.exe'\n"},
                expected_files={path},
            )
            aliased_module_mutation = gate.review_python_process_helpers(
                {
                    path: source
                    + "import shutil as alternate_shutil\n"
                    + "alternate_shutil.which = lambda _: 'attacker.exe'\n"
                },
                expected_files={path},
            )
            assigned_alias_mutation = gate.review_python_process_helpers(
                {
                    path: source
                    + "alternate_shutil = shutil\n"
                    + "alternate_shutil.which = lambda _: 'attacker.exe'\n"
                },
                expected_files={path},
            )
            class_alias_mutation = gate.review_python_process_helpers(
                {
                    path: source
                    + "from pathlib import Path as alternate_path\n"
                    + "alternate_path.resolve = lambda self: self\n"
                },
                expected_files={path},
            )
            registry_mutation_snippets = {
                "direct_index": (
                    "import sys\n"
                    "sys.modules['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "namespace_alias": (
                    "import sys\n"
                    "registry = sys.modules\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "registry_copy": (
                    "import sys\n"
                    "registry = sys.modules.copy()\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "dict_copy": (
                    "import sys\n"
                    "registry = dict(sys.modules)\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "module_index_alias": (
                    "import sys\n"
                    "shutil_module = sys.modules['shutil']\n"
                    "shutil_module.which = lambda _: 'attacker.exe'\n"
                ),
                "module_get_alias": (
                    "import sys\n"
                    "shutil_module = sys.modules.get('shutil')\n"
                    "shutil_module.which = lambda _: 'attacker.exe'\n"
                ),
                "items_copy": (
                    "import sys\n"
                    "registry = dict(sys.modules.items())\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "dict_copy_method": (
                    "import sys\n"
                    "registry = dict.copy(sys.modules)\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "mapping_unpack": (
                    "import sys\n"
                    "registry = {**sys.modules}\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "mapping_union": (
                    "import sys\n"
                    "registry = sys.modules | {}\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "inline_get": (
                    "import sys\n"
                    "sys.modules.get('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "inline_copy": (
                    "import sys\n"
                    "sys.modules.copy()['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "inline_dict": (
                    "import sys\n"
                    "dict(sys.modules)['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "dict_comprehension": (
                    "import sys\n"
                    "registry = {key: module for key, module in sys.modules.items()}\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "list_values": (
                    "import sys\n"
                    "modules = list(sys.modules.values())\n"
                    "modules[0].which = lambda _: 'attacker.exe'\n"
                ),
                "tuple_values": (
                    "import sys\n"
                    "modules = tuple(sys.modules.values())\n"
                    "modules[0].which = lambda _: 'attacker.exe'\n"
                ),
                "list_comprehension": (
                    "import sys\n"
                    "modules = [module for module in sys.modules.values()]\n"
                    "modules[0].which = lambda _: 'attacker.exe'\n"
                ),
                "dynamic_import": (
                    "__import__('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "builtin_dict_alias": (
                    "import sys\n"
                    "from builtins import dict as clone_modules\n"
                    "registry = clone_modules(sys.modules)\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "opaque_registry_wrapper": (
                    "import sys\n"
                    "from collections import ChainMap\n"
                    "registry = ChainMap(sys.modules)\n"
                    "registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "opaque_imported_registry_wrapper": (
                    "from sys import modules as registry\n"
                    "from collections import ChainMap\n"
                    "wrapped = ChainMap(registry)\n"
                    "wrapped['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "importlib_dynamic_import": (
                    "import importlib\n"
                    "importlib.import_module('shutil').which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "importlib_loader_import": (
                    "import importlib.util\n"
                    "importlib.util.find_spec('shutil').loader.load_module().which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_importlib_find_spec": (
                    "from importlib.util import find_spec as resolve_spec\n"
                    "resolve_spec('shutil').loader.load_module().which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "importlib_module_from_spec": (
                    "import importlib.util\n"
                    "spec = importlib.util.find_spec('shutil')\n"
                    "importlib.util.module_from_spec(spec).which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "importlib_reload": (
                    "import importlib\n"
                    "importlib.reload(shutil).which = lambda _: 'attacker.exe'\n"
                ),
                "aliased_dynamic_import": (
                    "from importlib import import_module as load_module\n"
                    "load_module('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "pkgutil_resolve_name": (
                    "import pkgutil\n"
                    "pkgutil.resolve_name('shutil').which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_pkgutil_resolve_name": (
                    "from pkgutil import resolve_name as resolve_module\n"
                    "resolve_module('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "pydoc_locate": (
                    "import pydoc\n"
                    "pydoc.locate('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "aliased_pydoc_locate": (
                    "from pydoc import locate as resolve_module\n"
                    "resolve_module('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "assigned_pydoc_locator": (
                    "import pydoc\n"
                    "resolve_module = pydoc.locate\n"
                    "resolve_module('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "chained_pydoc_locator": (
                    "import pydoc\n"
                    "resolve_module = pydoc.locate\n"
                    "module_lookup = resolve_module\n"
                    "module_lookup('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "assigned_builtin_import": (
                    "module_loader = __import__\n"
                    "module_loader('shutil').which = lambda _: 'attacker.exe'\n"
                ),
                "inspect_getmodule": (
                    "import inspect\n"
                    "inspect.getmodule(shutil.which).which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_inspect_getmodule": (
                    "from inspect import getmodule as owning_module\n"
                    "owning_module(shutil.which).which = lambda _: 'attacker.exe'\n"
                ),
                "inspect_getattr_static": (
                    "import inspect\n"
                    "import sys\n"
                    "inspect.getattr_static(sys, 'modules')['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_inspect_getattr_static": (
                    "from inspect import getattr_static as read_static\n"
                    "import sys\n"
                    "read_static(sys, 'modules')['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "inspect_getmembers": (
                    "import inspect\n"
                    "import sys\n"
                    "module_registry = dict(inspect.getmembers(sys))['modules']\n"
                    "module_registry['shutil'].which = lambda _: 'attacker.exe'\n"
                ),
                "gc_get_referrers": (
                    "import gc\n"
                    "namespace = next(\n"
                    "    ref for ref in gc.get_referrers(shutil.which)\n"
                    "    if isinstance(ref, dict) and ref.get('__name__') == 'shutil'\n"
                    ")\n"
                    "namespace['which'] = lambda _: 'attacker.exe'\n"
                ),
                "aliased_gc_get_referrers": (
                    "from gc import get_referrers as references\n"
                    "namespace = next(\n"
                    "    ref for ref in references(shutil.which)\n"
                    "    if isinstance(ref, dict) and ref.get('__name__') == 'shutil'\n"
                    ")\n"
                    "namespace['which'] = lambda _: 'attacker.exe'\n"
                ),
                "assigned_gc_get_referrers": (
                    "import gc\n"
                    "references = gc.get_referrers\n"
                    "namespace = next(\n"
                    "    ref for ref in references(shutil.which)\n"
                    "    if isinstance(ref, dict) and ref.get('__name__') == 'shutil'\n"
                    ")\n"
                    "namespace['which'] = lambda _: 'attacker.exe'\n"
                ),
                "sys_modules_getitem": (
                    "import sys\n"
                    "sys.modules.__getitem__('shutil').which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "sys_getattribute_modules": (
                    "import sys\n"
                    "sys.__getattribute__('modules')['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "frame_globals_import": (
                    "import sys\n"
                    "sys._getframe().f_globals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "frame_locals_import": (
                    "import sys\n"
                    "sys._getframe().f_locals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "frame_builtins_import": (
                    "import sys\n"
                    "sys._getframe().f_builtins['__import__']('shutil').which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_frame_globals_import": (
                    "from sys import _getframe as current_frame\n"
                    "current_frame().f_globals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "inspect_getargvalues_locals": (
                    "import inspect\n"
                    "import sys\n"
                    "inspect.getargvalues(sys._getframe()).locals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_inspect_getargvalues_locals": (
                    "from inspect import getargvalues as frame_values\n"
                    "import sys\n"
                    "frame_values(sys._getframe()).locals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "inspect_getclosurevars_globals": (
                    "import inspect\n"
                    "inspect.getclosurevars(select_shell).globals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_inspect_getclosurevars_globals": (
                    "from inspect import getclosurevars as closure_values\n"
                    "closure_values(select_shell).globals['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "operator_attrgetter_modules": (
                    "import operator\n"
                    "import sys\n"
                    "operator.attrgetter('modules')(sys)['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
                "aliased_operator_attrgetter_modules": (
                    "from operator import attrgetter as field\n"
                    "import sys\n"
                    "field('modules')(sys)['shutil'].which = "
                    "lambda _: 'attacker.exe'\n"
                ),
            }
            registry_mutations = {
                name: gate.review_python_process_helpers(
                    {path: source + snippet}, expected_files={path}
                )
                for name, snippet in registry_mutation_snippets.items()
            }
            rebound = gate.review_python_process_helpers(
                {path: source + "shutil = attacker\n"}, expected_files={path}
            )
            substituted = gate.review_python_process_helpers(
                {
                    path: source.replace("import shutil", "import attacker as shutil"),
                },
                expected_files={path},
            )

        self.assertEqual([item["review_status"] for item in approved], ["reviewed"])
        self.assertEqual([item["review_status"] for item in mutated], ["stale-review"])
        self.assertEqual(
            [item["review_status"] for item in aliased_module_mutation],
            ["stale-review"],
        )
        self.assertEqual(
            [item["review_status"] for item in assigned_alias_mutation],
            ["stale-review"],
        )
        self.assertEqual(
            [item["review_status"] for item in class_alias_mutation],
            ["stale-review"],
        )
        for name, result in registry_mutations.items():
            with self.subTest(registry_mutation=name):
                self.assertEqual(
                    [item["review_status"] for item in result],
                    ["stale-review"],
                )
        self.assertEqual([item["review_status"] for item in rebound], ["stale-review"])
        self.assertEqual(
            [item["review_status"] for item in substituted], ["stale-review"]
        )

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

    def test_python_process_review_matches_exact_call_ast(self):
        approved = gate.scan_python_text(
            "scripts/audit_capability_matrix.py",
            "import subprocess\n"
            "subprocess.run(\n"
            '    ["git", "status", "--porcelain", "--untracked-files=all"],\n'
            "    cwd=str(root), capture_output=True, text=True, check=False,\n"
            ")\n",
        )
        reviewed = gate.review_python_process_calls(approved, expected_files=set())
        self.assertEqual(len(reviewed), 1)
        self.assertEqual(reviewed[0]["review_status"], "reviewed")
        self.assertIn("fixed git arguments", reviewed[0]["review_reason"])

        changed = gate.scan_python_text(
            "scripts/audit_capability_matrix.py",
            "import subprocess\n"
            "subprocess.run(\n"
            '    ["git", "status", "--porcelain", "--ignored", "--untracked-files=all"],\n'
            "    cwd=str(root), capture_output=True, text=True, check=False,\n"
            ")\n",
        )
        unreviewed = gate.review_python_process_calls(changed, expected_files=set())
        self.assertEqual(unreviewed[0]["review_status"], "unreviewed")
        self.assertIsNone(unreviewed[0]["review_reason"])

    def test_removed_python_process_call_leaves_a_stale_review_blocker(self):
        stale = gate.review_python_process_calls(
            [], expected_files={"scripts/audit_capability_matrix.py"}
        )
        self.assertEqual(len(stale), 3)
        self.assertTrue(all(item["review_status"] == "stale-review" for item in stale))

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
        self.assertEqual(findings[0]["dependency_action"], "install")

    def test_finds_pm_command_after_env_unset_option(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "env -u HOME PATH=/tmp /usr/bin/npm install\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["npm"])
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_command_v_with_dynamic_argument_is_only_a_builtin_probe(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'command -v "$PM_NAME" >/dev/null 2>&1\n',
        )
        self.assertEqual(findings, [])

    def test_standalone_redirection_path_is_not_an_executable(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "if ! command -v pnpm &>/dev/null; then\n"
            "  :\n"
            "fi\n",
        )
        self.assertEqual(findings, [])

    def test_finds_pm_command_after_timeout_and_nice_wrappers(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "timeout 20 nice -n 5 cargo fetch\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["cargo"])
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_command_substitution_is_not_hidden_in_an_assignment(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'VERSION="$(npm install demo)"\n',
        )
        self.assertEqual([item["executable"] for item in findings], ["npm"])
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_shell_variables_in_arguments_do_not_mark_command_dynamic(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'echo "$HOME"\n'
            'printf "%s\\n" "$PROJECT_ROOT"\n',
        )
        self.assertEqual(findings, [])

    def test_case_selector_is_not_misread_as_a_command(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'case "$PM_NAME" in\n'
            "  npm) npm install ;;\n"
            "  mgc) mgc install ;;\n"
            "  *) exit 1 ;;\n"
            "esac\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["npm"])

    def test_shell_condition_or_and_array_arguments_are_not_process_calls(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'if [[ "${CI:-}" == "true" || "${GITHUB_ACTIONS:-}" == "true" ]]; then\n'
            '  args+=(--target "$TARGET")\n'
            "fi\n",
        )
        self.assertEqual(findings, [])

    def test_arithmetic_or_is_not_split_into_command_heads(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "if ((INLINE_TEST_COUNT > 0 || MISPLACED_TEST_COUNT > 0)); then\n"
            "  cargo fetch\n"
            "fi\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["cargo"])

    def test_arithmetic_assignment_with_division_is_not_a_command(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "MINUTES=$(((ELAPSED % 3600) / 60))\n"
            "DURATION=$(( (END - START) / 1000000 ))\n",
        )
        self.assertEqual(findings, [])

    def test_case_alternative_pattern_keeps_branch_executable_visible(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            "case $PM in\n"
            "  npm|pnpm) npm install ;;\n"
            "  *) exit 1 ;;\n"
            "esac\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["npm"])

    def test_fixed_mgc_and_pnpm_binary_aliases_are_resolved(self):
        findings = gate.scan_shell_text(
            "cli/tests/scripts/example.sh",
            'MGC_BIN="mgc"\n'
            'PNPM_BIN="$(command -v pnpm)"\n'
            '"$MGC_BIN" --version\n'
            '"$PNPM_BIN" --version\n',
        )
        self.assertEqual(
            [item["executable"] for item in findings], ["mgc", "pnpm"]
        )
        self.assertTrue(all(item["review_status"] == "observed" for item in findings))

    def test_validated_external_mgc_binary_is_recognized(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            '[[ -f "$MGC_BIN" && -x "$MGC_BIN" ]] || exit 1\n'
            '"$MGC_BIN" sign-release\n',
        )
        self.assertEqual([item["executable"] for item in findings], ["mgc"])
        self.assertEqual(findings[0]["review_status"], "observed")

    def test_dependency_mutation_requires_exact_path_and_command_fingerprint(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "benchmark/scripts/run_benchmark.sh"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()
        reviewed = [
            item
            for item in gate.scan_shell_text(rel_path, source)
            if item.get("dependency_action") == "install"
        ]
        self.assertTrue(reviewed)
        self.assertTrue(all(item["review_status"] == "reviewed" for item in reviewed))

        changed = gate.scan_shell_text(
            "benchmark/scripts/run_benchmark.sh",
            "npm install --ignore-scripts --force\n",
        )
        self.assertEqual(changed[0]["review_status"], "unreviewed")

        duplicated = gate.scan_shell_text(
            rel_path,
            source + "\nnpm install --ignore-scripts\n",
        )
        appended = [item for item in duplicated if item["line"] > len(source.splitlines())]
        self.assertEqual(len(appended), 1)
        self.assertEqual(appended[0]["review_status"], "unreviewed")

    def test_static_cargo_fetch_in_production_shell_is_blocking(self):
        findings = gate.scan_shell_text("scripts/build.sh", "cargo fetch --locked\n")
        self.assertEqual(findings[0]["executable"], "cargo")
        self.assertEqual(findings[0]["dependency_action"], "fetch")
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_package_manager_exec_and_run_commands_require_exact_review(self):
        commands = (
            "npx cowsay hi",
            "bunx cowsay hi",
            "npm exec --package=cowsay cowsay hi",
            "pnpm dlx cowsay hi",
            "yarn dlx cowsay hi",
            "bun x cowsay hi",
            "uv run task.py",
            "cargo run --bin app",
            "go run ./cmd/app",
        )
        for command in commands:
            with self.subTest(command=command):
                findings = gate.scan_shell_text("scripts/tool.sh", command + "\n")
                self.assertEqual(len(findings), 1)
                self.assertIsNotNone(findings[0]["dependency_action"])
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_alias_writes_invalidate_scanned_script_paths(self):
        root = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
        )
        launch = 'source "$SCRIPT_DIR/known.sh"\n'
        for mutation in (
            "readonly SCRIPT_DIR=/tmp\n",
            "SCRIPT_DIR[0]=/tmp\n",
            "((SCRIPT_DIR=1))\n",
            "rewrite_path() {\n  SCRIPT_DIR=/tmp\n}\n",
        ):
            with self.subTest(mutation=mutation):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    root + mutation + launch,
                    {"scripts/known.sh"},
                )
                self.assertEqual(findings[-1]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[-1]["review_status"], "unreviewed")

    def test_process_substitutions_and_dynamic_substitution_heads_are_unreviewed(self):
        process_substitution = gate.scan_shell_text(
            "scripts/harness.sh", "cat <(npm install evil)\n"
        )
        dynamic_head = gate.scan_shell_text(
            "scripts/harness.sh", 'result="$($CMD npm install evil)"\n'
        )
        self.assertTrue(any(item["executable"] == "npm" for item in process_substitution))
        self.assertTrue(any(item["review_status"] == "unreviewed" for item in process_substitution))
        self.assertTrue(any(item["executable"] == "<dynamic-shell-command>" for item in dynamic_head))
        self.assertTrue(any(item["review_status"] == "unreviewed" for item in dynamic_head))

    def test_shell_like_interpreters_with_inline_commands_are_unreviewed(self):
        for command in (
            "ksh -c 'npm install evil'\n",
            "fish -c 'npm install evil'\n",
            "csh -c 'npm install evil'\n",
            "ash -c 'npm install evil'\n",
            "busybox sh -c 'npm install evil'\n",
            "busybox ash -c 'npm install evil'\n",
        ):
            with self.subTest(command=command):
                findings = gate.scan_shell_text("scripts/harness.sh", command)
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_sudo_chroot_options_do_not_hide_the_command_head(self):
        for command in (
            "sudo -R /tmp npm install evil\n",
            "sudo --chroot /tmp npm install evil\n",
        ):
            with self.subTest(command=command):
                findings = gate.scan_shell_text("scripts/tool.sh", command)
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "npm")
                self.assertEqual(findings[0]["dependency_action"], "install")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_sudo_prompt_options_do_not_hide_the_command_head(self):
        for command in (
            "sudo -p 'password' npm install evil\n",
            "sudo --prompt 'password' npm install evil\n",
        ):
            with self.subTest(command=command):
                findings = gate.scan_shell_text("scripts/tool.sh", command)
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "npm")
                self.assertEqual(findings[0]["dependency_action"], "install")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_sudo_timeout_options_do_not_hide_the_command_head(self):
        for command in (
            "sudo -T 5 npm install evil\n",
            "sudo --command-timeout 5 npm install evil\n",
        ):
            with self.subTest(command=command):
                findings = gate.scan_shell_text("scripts/tool.sh", command)
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "npm")
                self.assertEqual(findings[0]["dependency_action"], "install")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_smoke_test_binary_calls_have_exact_shell_boundary_review(self):
        lines = (
            'if ! version_output=$("$MGC_PATH" --version 2>&1); then\n',
            'if ! help_output=$("$MGC_PATH" --help 2>&1); then\n',
            'if version_cmd=$("$MGC_PATH" version 2>&1); then\n',
        )
        findings = [
            item
            for line in lines
            for item in gate.scan_shell_text("scripts/smoke-test.sh", line)
        ]
        self.assertEqual(len(findings), 3)
        self.assertTrue(
            all(
                item["executable"] == "<reviewed-dynamic-command>"
                and item["review_status"] == "reviewed"
                and "caller-selected binary behavior is outside this inventory"
                in item["review_reason"]
                for item in findings
            )
        )
        changed = gate.scan_shell_text(
            "scripts/smoke-test.sh",
            'if ! version_output=$("$MGC_PATH" install evil 2>&1); then\n',
        )
        self.assertTrue(any(item["review_status"] == "unreviewed" for item in changed))

    def test_benchmark_install_uses_repository_manifest_not_environment_override(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        path = os.path.join(repo_root, "benchmark/scripts/run_benchmark_phased.sh")
        with open(path, "r", encoding="utf-8") as handle:
            source = handle.read()
        self.assertIn('PACKAGE_JSON="$BENCHMARK_ROOT/env/package-unified.json"', source)
        self.assertNotIn('${PACKAGE_JSON:-', source)

    def test_known_pm_in_argument_substitution_keeps_its_literal_identity(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'echo "$(npm install demo)"\n'
            'VERSION="`cargo fetch`"\n',
        )
        self.assertEqual(
            [item["executable"] for item in findings],
            ["npm", "cargo"],
        )
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

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
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

    def test_shell_interpreter_execution_is_dynamic_even_with_static_script_name(self):
        findings = gate.scan_shell_text("scripts/example.sh", "bash -c 'cargo fetch'\n")
        self.assertEqual([item["executable"] for item in findings], ["<dynamic-shell-command>"])

    def test_shell_interpreter_rejects_variable_and_untracked_script_paths(self):
        variable_path = gate.scan_shell_text(
            "scripts/harness.sh",
            'bash "$CI_HOOK"\n',
        )
        untracked_path = gate.scan_shell_text(
            "scripts/harness.sh",
            "sh /tmp/untracked-hook.sh\n",
        )
        cwd_relative_path = gate.scan_shell_text(
            "scripts/harness.sh",
            "sh runner.sh\n",
            {"scripts/runner.sh"},
        )
        for findings in (variable_path, untracked_path, cwd_relative_path):
            self.assertEqual(len(findings), 1)
            self.assertEqual(findings[0]["review_status"], "unreviewed")
            self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")

    def test_shell_interpreter_accepts_only_a_scanned_static_script(self):
        source = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            'bash "$SCRIPT_DIR/runner.sh"\n'
        )
        scanned = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace(
                'bash "$SCRIPT_DIR/runner.sh"',
                '/usr/bin/env -i PATH="$PATH" bash "$SCRIPT_DIR/runner.sh"',
            ),
            {"scripts/runner.sh": "#!/usr/bin/env bash\n"},
        )
        unscanned = gate.scan_shell_text(
            "scripts/harness.sh",
            source,
            {"scripts/other.sh"},
        )
        self.assertEqual(scanned[0]["executable"], "shell-script")
        self.assertEqual(scanned[0]["review_status"], "observed")
        self.assertEqual(unscanned[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(unscanned[0]["review_status"], "unreviewed")
        environment_root = gate.scan_shell_text(
            "scripts/harness.sh",
            'SCRIPT_DIR="$UNTRUSTED_ROOT"\n'
            'bash "$SCRIPT_DIR/runner.sh"\n',
            {"scripts/runner.sh"},
        )
        self.assertEqual(environment_root[0]["review_status"], "unreviewed")

    def test_shell_path_alias_comment_cannot_forge_a_repository_anchor(self):
        scanned_paths = {"scripts/runner.sh": "#!/usr/bin/env bash\n"}
        for executable in ('source "$SCRIPT_DIR/runner.sh"', '"$SCRIPT_DIR/runner.sh"'):
            for assignment in (
                "SCRIPT_DIR=/tmp # ${BASH_SOURCE[0]}\n",
                'SCRIPT_DIR="/tmp # ${BASH_SOURCE[0]}"\n',
            ):
                with self.subTest(executable=executable, assignment=assignment):
                    findings = gate.scan_shell_text(
                        "scripts/harness.sh",
                        assignment + executable + "\n",
                        scanned_paths,
                    )
                    self.assertEqual(len(findings), 1)
                    self.assertEqual(findings[0]["review_status"], "unreviewed")
                    self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")

    def test_bash_startup_environment_must_be_cleared_before_scanned_script(self):
        source = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '/usr/bin/env -i PATH="$PATH" bash "$SCRIPT_DIR/runner.sh"\n'
        )
        scanned = gate.scan_shell_text(
            "scripts/harness.sh", source, {"scripts/runner.sh": "#!/bin/bash\n"}
        )
        inherited_hook = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace("/usr/bin/env -i PATH=\"$PATH\" bash", "bash"),
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        command_hook = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace(
                '/usr/bin/env -i PATH="$PATH" bash "$SCRIPT_DIR/runner.sh"',
                '/usr/bin/env -i BASH_ENV=/tmp/hook.sh PATH="$PATH" bash "$SCRIPT_DIR/runner.sh"',
            ),
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        exported_function = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace(
                '/usr/bin/env -i PATH="$PATH" bash',
                "/usr/bin/env -i 'BASH_FUNC_npm%%=() { :; }' PATH=\"$PATH\" bash",
            ),
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        unset_only = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace("/usr/bin/env -i PATH=\"$PATH\" bash", "env -u BASH_ENV bash"),
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        shadowable_env = gate.scan_shell_text(
            "scripts/harness.sh",
            source.replace("/usr/bin/env -i PATH=\"$PATH\" bash", "env -i PATH=\"$PATH\" bash"),
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        self.assertEqual(scanned[0]["executable"], "shell-script")
        for findings in (inherited_hook, command_hook, exported_function, unset_only, shadowable_env):
            self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
            self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_direct_bash_script_launch_must_clear_startup_environment(self):
        text = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '"$SCRIPT_DIR/runner.sh"\n'
        )
        unguarded = gate.scan_shell_text(
            "scripts/harness.sh", text, {"scripts/runner.sh": "#!/usr/bin/env bash\n"}
        )
        guarded = gate.scan_shell_text(
            "scripts/harness.sh",
            text.replace('"$SCRIPT_DIR/runner.sh"', '/usr/bin/env -i PATH="$PATH" "$SCRIPT_DIR/runner.sh"'),
            {"scripts/runner.sh": "#!/usr/bin/env bash\n"},
        )
        self.assertEqual(unguarded[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(guarded[0]["executable"], "shell-script")

        absolute_env = gate.scan_shell_text(
            "scripts/harness.sh",
            text.replace(
                '"$SCRIPT_DIR/runner.sh"',
                '/usr/bin/env -i PATH="$PATH" "$SCRIPT_DIR/runner.sh"',
            ),
            {"scripts/runner.sh": "#!/usr/bin/env bash\n"},
        )
        self.assertEqual(absolute_env[0]["executable"], "shell-script")

        misplaced_unset = gate.scan_shell_text(
            "scripts/harness.sh",
            text.replace(
                '"$SCRIPT_DIR/runner.sh"',
                '"$SCRIPT_DIR/runner.sh" env -u BASH_ENV',
            ),
            {"scripts/runner.sh": "#!/usr/bin/env bash\n"},
        )
        self.assertEqual(misplaced_unset[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(misplaced_unset[0]["review_status"], "unreviewed")

    def test_bash_login_profiles_are_rejected_even_when_bash_env_is_cleared(self):
        source = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '/usr/bin/env -i PATH="$PATH" bash -l "$SCRIPT_DIR/runner.sh"\n'
        )
        findings = gate.scan_shell_text(
            "scripts/harness.sh", source, {"scripts/runner.sh": "#!/bin/bash\n"}
        )
        self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_bash_interactive_profiles_are_rejected_for_interpreters_and_shebangs(self):
        inline = gate.scan_shell_text(
            "scripts/harness.sh",
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '/usr/bin/env -i PATH="$PATH" HOME="$HOME" bash -i "$SCRIPT_DIR/runner.sh"\n',
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        shebang = gate.scan_shell_text(
            "scripts/harness.sh",
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '/usr/bin/env -i PATH="$PATH" HOME="$HOME" "$SCRIPT_DIR/runner.sh"\n',
            {"scripts/runner.sh": "#!/bin/bash -i\n"},
        )
        for findings in (inline, shebang):
            self.assertEqual(findings[-1]["executable"], "<dynamic-shell-command>")
            self.assertEqual(findings[-1]["review_status"], "unreviewed")

    def test_source_script_alias_is_invalidated_after_runtime_mutation(self):
        root = 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
        target = '/usr/bin/env -i PATH="$PATH" bash "$SCRIPT_DIR/runner.sh"\n'
        for mutation in (
            "read -r SCRIPT_DIR\n",
            "printf -v SCRIPT_DIR '%s' /tmp\n",
            "unset SCRIPT_DIR\n",
        ):
            with self.subTest(mutation=mutation):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    root + mutation + target,
                    {"scripts/runner.sh": "#!/bin/bash\n"},
                )
                launch = findings[-1]
                self.assertEqual(launch["executable"], "<dynamic-shell-command>")
                self.assertEqual(launch["review_status"], "unreviewed")

        prompt_read = gate.scan_shell_text(
            "scripts/harness.sh",
            root
            + 'read -p "$(echo continue?)" -n 1 -r\n'
            + target,
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        self.assertEqual(prompt_read[-1]["executable"], "shell-script")

        for mutation in (': "${SCRIPT_DIR:=/tmp}"\n', ': "${SCRIPT_DIR=/tmp}"\n'):
            with self.subTest(mutation=mutation):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    root + mutation + target,
                    {"scripts/runner.sh": "#!/bin/bash\n"},
                )
                self.assertEqual(findings[-1]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[-1]["review_status"], "unreviewed")

        for mutation in ("((SCRIPT_DIR=1))\n", "((SCRIPT_DIR++))\n", ": \"$((SCRIPT_DIR=1))\"\n"):
            with self.subTest(mutation=mutation):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    root + mutation + target,
                    {"scripts/runner.sh": "#!/bin/bash\n"},
                )
                self.assertEqual(findings[-1]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[-1]["review_status"], "unreviewed")

        read_only_arithmetic = gate.scan_shell_text(
            "scripts/harness.sh",
            root + "((SCRIPT_DIR == 1))\n" + target,
            {"scripts/runner.sh": "#!/bin/bash\n"},
        )
        self.assertEqual(read_only_arithmetic[-1]["executable"], "shell-script")

    def test_literal_script_heads_inside_command_substitutions_are_unreviewed(self):
        findings = gate.scan_shell_text(
            "scripts/harness.sh",
            'VERSION="$(./untracked-hook.sh)"\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(findings[0]["review_status"], "unreviewed")
        self.assertEqual(
            gate.scan_shell_text("scripts/harness.sh", 'VERSION="$(printf x)"\n'),
            [],
        )

    def test_literal_shell_script_command_heads_fail_closed(self):
        for command in ("./runner.sh", "../scripts/runner.sh", "/tmp/hook.bash"):
            with self.subTest(command=command):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    command + "\n",
                    {"scripts/runner.sh": "#!/usr/bin/env bash\n"},
                )
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_builtin_alias_and_dynamic_trap_actions_are_unreviewed(self):
        findings = gate.scan_shell_text(
            "scripts/aliases.sh",
            'builtin source "$CI_HOOK"\n'
            "alias deps='npm install'\n"
            "trap 'npm install' EXIT\n",
        )
        self.assertEqual(len(findings), 3)
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))
        self.assertTrue(all(item["executable"] == "<dynamic-shell-command>" for item in findings))

        delimiter = gate.scan_shell_text(
            "scripts/aliases.sh", "trap -- 'npm install evil' EXIT\n"
        )
        self.assertEqual(delimiter[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(delimiter[0]["review_status"], "unreviewed")

    def test_trap_callback_requires_a_multiline_scanned_function_body(self):
        for body in (
            'cleanup() { builtin source "$CI_HOOK"; }; trap cleanup EXIT\n',
            'cleanup() { npm install evil; }; trap cleanup EXIT\n',
        ):
            with self.subTest(body=body):
                findings = gate.scan_shell_text("scripts/harness.sh", body)
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
                self.assertEqual(findings[0]["review_status"], "unreviewed")

        safe = gate.scan_shell_text(
            "scripts/harness.sh",
            'cleanup() {\n  rm -rf "$TMP_DIR"\n}\ntrap cleanup EXIT\n',
        )
        trap = next(item for item in safe if item["executable"] == "shell-trap-static")
        self.assertEqual(trap["review_status"], "observed")

        dangerous = gate.scan_shell_text(
            "scripts/harness.sh",
            'cleanup() {\n  builtin source "$CI_HOOK"\n}\ntrap cleanup EXIT\n',
        )
        self.assertTrue(any(item["review_status"] == "unreviewed" for item in dangerous))

    def test_shell_script_aliases_require_a_scanned_non_traversing_target(self):
        source = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            'source "$SCRIPT_DIR/runner.sh"\n'
            '"$SCRIPT_DIR/runner.sh"\n'
        )
        scanned = gate.scan_shell_text(
            "scripts/harness.sh", source, {"scripts/runner.sh"}
        )
        self.assertEqual([item["executable"] for item in scanned], [
            "shell-source", "<dynamic-shell-command>"
        ])
        self.assertEqual(scanned[0]["review_status"], "observed")
        self.assertEqual(scanned[1]["review_status"], "unreviewed")

        direct_only = gate.scan_shell_text(
            "scripts/harness.sh",
            source.splitlines()[0] + '\n"$SCRIPT_DIR/runner.sh"\n',
            {"scripts/runner.sh"},
        )
        self.assertEqual(direct_only[0]["executable"], "shell-script")
        self.assertEqual(direct_only[0]["review_status"], "observed")

        for command in (
            'source "$SCRIPT_DIR/../../../../tmp/evil.sh"\n',
            '"$SCRIPT_DIR/../../../../tmp/evil.sh"\n',
            'source "$SCRIPT_DIR/untracked.sh"\n',
        ):
            with self.subTest(command=command):
                findings = gate.scan_shell_text(
                    "scripts/harness.sh",
                    source.splitlines()[0] + "\n" + command,
                    {"scripts/runner.sh"},
                )
                self.assertEqual(len(findings), 1)
                self.assertEqual(findings[0]["review_status"], "unreviewed")
                self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")

    def test_env_split_string_and_shell_startup_hooks_are_dynamic(self):
        findings = gate.scan_shell_text(
            "scripts/harness.sh",
            "env -S 'npm install --ignore-scripts'\n"
            "bash -i --rcfile \"$CI_HOOK\" \"$SCRIPT_DIR/runner.sh\"\n",
            {"scripts/runner.sh"},
        )
        self.assertEqual(len(findings), 2)
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))
        self.assertTrue(all(item["executable"] == "<dynamic-shell-command>" for item in findings))

    def test_shell_dependency_review_fingerprint_binds_location_and_guard(self):
        first = gate.scan_shell_text(
            "scripts/reviewed.sh",
            'if [ "$SAFE" = 1 ]; then\n'
            "  npm install example\n"
            "fi\n",
        )[0]["command_fingerprint"]
        moved = gate.scan_shell_text(
            "scripts/reviewed.sh",
            "echo ready\n"
            'if [ "$SAFE" = 1 ]; then\n'
            "  npm install example\n"
            "fi\n",
        )[0]["command_fingerprint"]
        changed_guard = gate.scan_shell_text(
            "scripts/reviewed.sh",
            'if [ "$SAFE" = 0 ]; then\n'
            "  npm install example\n"
            "fi\n",
        )[0]["command_fingerprint"]
        self.assertNotEqual(first, moved, "moving an invocation must invalidate its review")
        self.assertNotEqual(first, changed_guard, "changing the enclosing guard must invalidate its review")

    def test_flutter_bootstrap_dynamic_commands_need_the_exact_source_review(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "scripts/bootstrap_flutter_sdk.sh"
        source_path = os.path.join(repo_root, rel_path)
        with open(source_path, encoding="utf-8") as handle:
            source = handle.read()

        reviewed = gate.scan_shell_text(rel_path, source)
        dynamic_commands = [
            item for item in reviewed if item["executable"] == "<dynamic-shell-command>"
        ]
        self.assertEqual(len(dynamic_commands), 2)
        self.assertTrue(
            all(item["review_status"] == "reviewed" for item in dynamic_commands)
        )

        changed_source = source.replace('"$flutter_bin" --version', '"$flutter_bin" --machine')
        changed = gate.scan_shell_text(rel_path, changed_source)
        changed_dynamic_commands = [
            item for item in changed if item["executable"] == "<dynamic-shell-command>"
        ]
        self.assertEqual(len(changed_dynamic_commands), 2)
        self.assertTrue(
            all(item["review_status"] == "unreviewed" for item in changed_dynamic_commands)
        )

    def test_fixed_mgc_command_alias_is_resolved_but_environment_alias_is_not(self):
        fixed = gate.scan_shell_text(
            "cli/tests/scripts/example.sh",
            'MGC_BIN="${PROJECT_ROOT}/target/debug/mgc"\n'
            '"$MGC_BIN" --version\n',
        )
        self.assertEqual([item["executable"] for item in fixed], ["mgc"])
        self.assertEqual(fixed[0]["review_status"], "observed")

        external = gate.scan_shell_text(
            "cli/tests/scripts/example.sh",
            'MGC_BIN="$UNTRUSTED_TOOL"\n'
            '"$MGC_BIN" --version\n',
        )
        self.assertEqual([item["executable"] for item in external], ["<dynamic-shell-command>"])
        self.assertEqual(external[0]["review_status"], "unreviewed")

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

    def test_heredoc_data_is_not_scanned_as_outer_shell_commands(self):
        findings = gate.scan_shell_text(
            "scripts/generator.sh",
            "cat <<'EOF' > generated.txt\n"
            "npm install\n"
            "cargo fetch\n"
            "EOF\n",
        )
        self.assertEqual(findings, [])

    def test_expandable_heredoc_command_substitution_is_scanned(self):
        expandable = gate.scan_shell_text(
            "scripts/generator.sh",
            "cat <<EOF > generated.txt\n"
            "$(npm install evil)\n"
            "EOF\n",
        )
        quoted = gate.scan_shell_text(
            "scripts/generator.sh",
            "cat <<'EOF' > generated.txt\n"
            "$(npm install evil)\n"
            "EOF\n",
        )
        self.assertTrue(any(item["executable"] == "npm" for item in expandable))
        self.assertTrue(any(item["review_status"] == "unreviewed" for item in expandable))
        self.assertEqual(quoted, [])

    def test_multiline_heredoc_substitutions_are_scanned_but_quoted_data_is_not(self):
        for substitution in (
            "$(\nnpm install evil\n)",
            "`\nnpm install evil\n`",
        ):
            with self.subTest(substitution=substitution):
                findings = gate.scan_shell_text(
                    "scripts/generator.sh",
                    "cat <<EOF > generated.txt\n"
                    + substitution
                    + "\nEOF\n",
                )
                self.assertTrue(any(item["executable"] == "npm" for item in findings))
                self.assertTrue(any(item["review_status"] == "unreviewed" for item in findings))

        escaped_delimiter = gate.scan_shell_text(
            "scripts/generator.sh",
            "cat <<\\EOF > generated.txt\n"
            "$(npm install evil)\n"
            "EOF\n",
        )
        quoted_backtick = gate.scan_shell_text(
            "scripts/generator.sh",
            "cat <<'EOF' > generated.txt\n"
            "`npm install evil`\n"
            "EOF\n",
        )
        self.assertEqual(escaped_delimiter, [])
        self.assertEqual(quoted_backtick, [])

    def test_here_string_is_not_misread_as_an_unterminated_heredoc(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'grep -q "pattern" <<<"$output"\n'
            "cargo fetch\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["cargo"])

    def test_dynamic_argv_command_substitution_is_not_silently_dropped(self):
        findings = gate.scan_shell_text(
            "scripts/example.sh",
            'output=$("$@" 2>&1 || true)\n',
        )
        self.assertEqual([item["executable"] for item in findings], ["<dynamic-shell-command>"])

    def test_shell_interpreter_with_heredoc_remains_a_dynamic_boundary(self):
        findings = gate.scan_shell_text(
            "scripts/runner.sh",
            "bash <<'EOF'\n"
            "cargo fetch\n"
            "EOF\n",
        )
        self.assertEqual([item["executable"] for item in findings], ["<dynamic-shell-command>"])
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_extensionless_relative_script_command_is_not_skipped(self):
        findings = gate.scan_shell_text("scripts/runner.sh", "./hook\n")
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_extensionless_absolute_script_command_is_not_skipped(self):
        findings = gate.scan_shell_text("scripts/runner.sh", "/tmp/hook\n")
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["executable"], "<dynamic-shell-command>")
        self.assertEqual(findings[0]["review_status"], "unreviewed")

    def test_source_anchored_alias_requires_a_scanned_script_target(self):
        source = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            '"$SCRIPT_DIR/hook"\n'
            '"$SCRIPT_DIR/hook.SH"\n'
        )
        findings = gate.scan_shell_text(
            "scripts/runner.sh", source, {"scripts/known.sh"}
        )
        self.assertEqual(
            [item["executable"] for item in findings],
            ["<dynamic-shell-command>", "<dynamic-shell-command>"],
        )
        self.assertTrue(all(item["review_status"] == "unreviewed" for item in findings))

    def test_quoted_heredoc_text_does_not_redefine_trap_callback(self):
        source = (
            "cleanup() {\n"
            "  :\n"
            "}\n"
            "trap cleanup EXIT\n"
            "cat <<'EOF'\n"
            "cleanup() { ./evil; }\n"
            "EOF\n"
        )
        findings = gate.scan_shell_text("scripts/trap.sh", source)
        self.assertTrue(
            any(item["executable"] == "shell-trap-static" for item in findings)
        )
        self.assertFalse(
            any(item["executable"] == "<dynamic-shell-command>" for item in findings)
        )

    def test_unset_trap_function_invalidates_static_callback_review(self):
        source = (
            "cleanup() {\n"
            "  :\n"
            "}\n"
            "trap cleanup EXIT\n"
            "unset -f cleanup\n"
        )
        findings = gate.scan_shell_text("scripts/trap.sh", source)
        trap_findings = [item for item in findings if item["executable"] == "shell-trap-static"]
        self.assertEqual(len(trap_findings), 0)
        self.assertTrue(any(item["executable"] == "<dynamic-shell-command>" for item in findings))

        redefined = gate.scan_shell_text(
            "scripts/trap.sh",
            "cleanup() {\n  :\n}\n"
            "trap cleanup EXIT\n"
            "cleanup() {\n  ./later-hook\n}\n",
        )
        self.assertFalse(
            any(item["executable"] == "shell-trap-static" for item in redefined)
        )

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
    """Production routes stay visible and green — route phải được rà và thông qua."""

    def test_windows_batch_interpreter_has_an_exact_guarded_process_route(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "core/crates/mgc-exec/src/run.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()
        function_start = source.index("fn windows_batch_command(")
        function_end = source.index("\n}\n", function_start) + 2
        function_body = source[function_start:function_end]
        route = (
            rel_path,
            "windows_batch_command",
            "<dynamic:windows_system_tool_path>",
        )
        findings = [
            item for item in gate.scan_file(rel_path, abs_path)
            if (item["function"], item["tool"]) == (route[1], route[2])
        ]
        self.assertIn(route, gate.AUDITED_DIRECT_PROCESS_ROUTES)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["status"], "allowed")
        self.assertEqual(findings[0]["op_class"], "audited-executor-route")
        self.assertIn('windows_system_tool_path("cmd.exe")', function_body)
        self.assertLess(
            function_body.index("validate_windows_batch_invocation(&cmd_path, args)"),
            function_body.index('Command::new(windows_system_tool_path("cmd.exe")?)'),
        )

    def test_docker_publish_async_route_keeps_its_security_constraints(self):
        repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        rel_path = "cli/src/commands/publish.rs"
        abs_path = os.path.join(repo_root, rel_path)
        with open(abs_path, "r", encoding="utf-8") as handle:
            source = handle.read()
        function_start = source.index("async fn docker_command(")
        function_end = source.index("\n}\n", function_start) + 2
        function_body = source[function_start:function_end]
        route = (rel_path, "docker_command", "docker")
        findings = [
            item for item in gate.scan_file(rel_path, abs_path)
            if (item["function"], item["tool"]) == (route[1], route[2])
        ]
        self.assertIn(route, gate.AUDITED_DIRECT_PROCESS_ROUTES)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["status"], "allowed")
        for token in (
            'tokio::process::Command::new("docker")',
            '.args(args)',
            '.env("DOCKER_CONFIG", docker_config)',
            'Stdio::piped()',
            'stdin.write_all(secret.as_bytes()).await?',
            'tokio::time::timeout(timeout, child.wait())',
            '.kill_on_drop(true)',
        ):
            self.assertIn(token, function_body)

    def test_direct_spawn_in_a_delegated_function_does_not_inherit_its_route(self):
        findings = scan_snippet(
            "cli/src/commands/build/web_engine.rs",
            'fn build_rust_with_env() {\n'
            '    std::process::Command::new("cargo").status();\n'
            '}\n',
        )
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["tool"], "cargo")
        self.assertEqual(findings[0]["status"], "violation")
        self.assertEqual(findings[0]["op_class"], "build")

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

    def test_shell_startup_environment_blindspot_is_explicit(self):
        startup = next(
            item for item in gate.UNSCANNED_PROCESS_SURFACES
            if item["path"] == "shell interpreter startup environment"
        )
        self.assertIn("before an entrypoint script starts", startup["reason"])
        self.assertIn("invoking runner must sanitize", startup["reason"])
        self.assertIn("only child launches", startup["reason"])

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

    def test_repo_ledger_is_green_after_all_production_routes_are_reviewed(self):
        code = gate.main()
        self.assertEqual(
            code,
            0,
            "all production process spawns must be delegated or narrowly reviewed",
        )
        with open(gate.OUTPUT_PATH, "r", encoding="utf-8") as handle:
            ledger = json.load(handle)
        findings = ledger["findings"]
        self.assertEqual(ledger["schema"], "dependency-delegation-audit/11")
        self.assertEqual(ledger["summary"]["review_required"], 0)
        self.assertEqual(ledger["summary"]["violation"], 0)
        self.assertEqual(ledger["summary"]["blocking"], 0)
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
        self.assertEqual(
            ledger["python_process_unreviewed"],
            0,
            "all Python process routes must match an exact reviewed AST record",
        )
        self.assertEqual(
            ledger["python_process_reviewed"], len(ledger["python_process_calls"])
        )
        self.assertEqual(ledger["python_process_helper_unreviewed"], 0)
        self.assertEqual(ledger["python_process_helper_reviewed"], 1)
        self.assertEqual(
            [item["function"] for item in ledger["python_process_helper_reviews"]],
            ["bootstrap_bash_command"],
        )
        self.assertEqual(ledger["shell_scan_roots"], gate.SHELL_SCAN_ROOTS)
        self.assertGreater(ledger["shell_files_scanned"], 0)
        self.assertTrue(ledger["shell_process_calls"])
        self.assertGreater(ledger["javascript_files_scanned"], 0)
        self.assertGreater(ledger["powershell_files_scanned"], 0)
        self.assertIsInstance(ledger["javascript_process_calls"], list)
        self.assertIsInstance(ledger["powershell_process_calls"], list)
        self.assertTrue(
            all(
                item["review_status"] == "reviewed" and item["review_reason"]
                for item in ledger["python_process_calls"]
            )
        )
        self.assertTrue(
            all(
                item["review_status"] in {"observed", "reviewed"}
                and item["review_reason"]
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
        self.assertIsInstance(ledger["working_tree_clean"], bool)
        if ledger["working_tree_clean"]:
            self.assertEqual(ledger["dirty_paths_count"], 0)
        else:
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
        audited_routes = {
            **gate.AUDITED_EXECUTOR_ROUTES,
            **gate.AUDITED_DIRECT_PROCESS_ROUTES,
        }
        self.assertTrue(
            all(
                (
                    any(
                        segment in ("test", "tests", "bench", "benches")
                        for segment in item["file"].split("/")[:-1]
                    )
                    or item["op_class"] == "test-fixture"
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
                        ) in audited_routes
                        and item["classification_reason"]
                        == audited_routes[
                            (item["file"], item["function"], item["tool"])
                        ]
                    )
                )
                for item in allowed
            ),
            "only harnesses, test fixtures, exact doctor probes, and exact audited routes may be allowed",
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
        self.assertFalse(
            any(
                item["file"] == "core/crates/mgc-exec/src/run.rs"
                and item["tool"] == "taskkill"
                for item in findings
            ),
            "production Windows process-tree termination must use its in-process Job Object",
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
