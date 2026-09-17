#!/usr/bin/env python3
"""Self-tests for audit_dependency_delegation.py (P0-A / T0.1).

Stdlib only — run with `python3 scripts/test_audit_dependency_delegation.py`.
Every test is hermetic except `test_repo_ledger_has_zero_violations`, which
re-runs the gate over the working tree (the gate's documented contract:
exit 0 means every delegation is measured and declared).

(Tự kiểm cho gate audit uỷ quyền: chỉ dùng stdlib — chạy bằng
`python3 scripts/test_audit_dependency_delegation.py`.)
"""

import os
import sys
import tempfile
import unittest

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
    """python3 / pip-audit / cargo-audit / govulncheck spawns are found."""

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


class MarkerDeclaration(unittest.TestCase):
    """A DELEGATED: marker flips violation to delegated-documented."""

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
        self.assertEqual(findings[0]["status"], "delegated-documented")


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
    """The committed tree keeps the gate green (measure + declare)."""

    def test_repo_ledger_has_zero_violations(self):
        code = gate.main()
        self.assertEqual(code, 0, "gate must exit 0 on the working tree")


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

    def test_real_tree_has_no_missing_lane(self):
        checked, missing = gate.check_lane_gate(self.repo_root)
        self.assertEqual(missing, [], f"lanes without gate: {missing}")
        self.assertGreater(checked, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
