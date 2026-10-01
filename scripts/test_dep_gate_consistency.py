#!/usr/bin/env python3
"""Self-tests for the T0.4 binary<->matrix consistency check (stdlib only).

Run: python3 scripts/test_dep_gate_consistency.py
(Kiểm tra cổng nhất quán binary<->matrix — chỉ stdlib.)
"""

import os
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lifecycle_capability_matrix import (
    DEPENDENCY_OWNER_VOCABULARY,
    ALL_DEPENDENCY_OPERATIONS,
    LANES,
    check_dep_gate_consistency,
    validate_dep_gate_consistency,
)


def lanes(rows):
    """Build lane dicts from (core, language, dependency_owner, operation owners)."""
    return [
        {
            "core": core,
            "language": language,
            "dependency_owner": owner,
            "owner_by_operation": {
                operation: {
                    "mgc-native": "mgc",
                    "unsupported": "unsupported-no-runner",
                    "scaffold-only": "scaffold-only",
                }.get(value, value)
                for operation, value in operation_owners.items()
            },
        }
        for core, language, owner, operation_owners in rows
    ]


def cells(native=(), unsupported=(), scaffold=()):
    """Create the complete owner_by_operation table returned by the binary."""
    out = {operation: "unsupported" for operation in ALL_DEPENDENCY_OPERATIONS}
    for operation in native:
        out[operation] = "mgc-native"
    for operation in scaffold:
        out[operation] = "scaffold-only"
    for operation in unsupported:
        out[operation] = "unsupported"
    return out


NATIVE_ALL = cells(native=ALL_DEPENDENCY_OPERATIONS)
NATIVE_WITHOUT_GC = cells(
    native=set(ALL_DEPENDENCY_OPERATIONS) - {"gc"}, unsupported={"gc"}
)
NATIVE_WITHOUT_GC_OFFLINE = cells(
    native=set(ALL_DEPENDENCY_OPERATIONS) - {"gc", "offline-reinstall"},
    unsupported={"gc", "offline-reinstall"},
)
NATIVE_WITHOUT_LIST_GC_OFFLINE = cells(
    native=set(ALL_DEPENDENCY_OPERATIONS) - {"list", "gc", "offline-reinstall"},
    unsupported={"list", "gc", "offline-reinstall"},
)
NATIVE_MUTATION_ONLY = cells(
    native={"install", "add", "remove", "update", "resolve", "lock", "fetch", "verify", "store", "materialize", "frozen-install"},
    unsupported={"list", "gc", "offline-reinstall"},
)
NATIVE_INSTALL_ONLY = cells(
    native={"install", "resolve", "lock", "fetch", "verify", "store", "materialize", "frozen-install"},
    unsupported={"add", "remove", "update", "list", "gc", "offline-reinstall"},
)
NO_OPERATIONS = cells()


# Mirror of the REAL `mgc capabilities --json` dependency_ownership
# projection ({core: {"install": owner, "languages": {gate_lang: owner}}}).
# P0#2 truth: an UNDECLARED ecosystem fails closed, so every core-level
# base row is "unsupported" except the single-ecosystem lanes the binary
# can still name... none: even web/ai/game/iot/clo lanes must detect
# their ecosystem (web "js", ai "python", game "bevy", iot framework,
# clo "terraform"), so ALL bases are unsupported and every supported
# cell lives under its gate-ecosystem key.
BINARY = {
    "web": {
        "operations": NO_OPERATIONS,
        "languages": {"js": NATIVE_ALL, "ts": NATIVE_ALL},
        "frameworks": {
            "vanilla": NATIVE_ALL,
            "ts": NATIVE_ALL,
            "node": NATIVE_ALL,
        },
    },
    "lib": {
        "operations": NO_OPERATIONS,
        "languages": {
        "ts": NATIVE_WITHOUT_GC,
            "rust": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "python": NATIVE_WITHOUT_GC,
            "go": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "java": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "dotnet": NATIVE_WITHOUT_LIST_GC_OFFLINE,
        },
        "frameworks": {
            "ts": NATIVE_WITHOUT_GC,
            "rust": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "python": NATIVE_WITHOUT_GC,
            "go": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "java": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "dotnet": NATIVE_WITHOUT_LIST_GC_OFFLINE,
        },
    },
    "ai": {"operations": NO_OPERATIONS, "languages": {"python": NATIVE_WITHOUT_GC_OFFLINE}},
    "app": {
        "operations": NO_OPERATIONS,
        "languages": {
            "flutter": NATIVE_WITHOUT_LIST_GC_OFFLINE,
            "swift": NATIVE_INSTALL_ONLY,
            "objc": NO_OPERATIONS,
            "rn": NO_OPERATIONS,
        },
    },
    "game": {"operations": NO_OPERATIONS, "languages": {"bevy": NATIVE_MUTATION_ONLY}},
    "iot": {"operations": NO_OPERATIONS, "languages": {"esp32-rust": NATIVE_MUTATION_ONLY}},
    "clo": {
        "operations": NO_OPERATIONS,
        "languages": {"terraform": NO_OPERATIONS},
        "frameworks": {
            "cdk": NATIVE_WITHOUT_GC,
            "pulumi": NATIVE_WITHOUT_GC,
        },
    },
    "cicd": {"operations": NO_OPERATIONS},
    "hardware": {
        "operations": cells(scaffold={"list"}),
    },
}


class ConsistencyDirections(unittest.TestCase):
    def test_cloud_framework_lane_uses_per_framework_binary_truth(self):
        row = lanes([("clo", "cdk", "mgc-native", NATIVE_WITHOUT_GC)])[0]
        row["framework_id"] = "cdk"
        self.assertEqual(check_dep_gate_consistency(BINARY, [row]), [])

    def test_missing_framework_binary_truth_fails_closed(self):
        row = lanes([("clo", "cdk", "mgc-native", NATIVE_WITHOUT_GC)])[0]
        row["framework_id"] = "missing-framework"
        violations = check_dep_gate_consistency(BINARY, [row])
        self.assertTrue(any("missing from binary qualification" in item for item in violations))

    def test_gc_is_not_inferred_from_install_ownership(self):
        for lane in LANES:
            expected = (
                "mgc" if (lane["core"], lane["language"]) in {
                    ("web", "javascript"), ("web", "vanilla"),
                    ("web", "ts"), ("web", "node"),
                }
                else "unsupported-no-runner"
            )
            self.assertEqual(
                lane["owner_by_operation"]["gc"],
                expected,
                f"{lane['core']}/{lane['language']} GC ownership must be explicit",
            )

    def test_offline_reinstall_is_not_inferred_from_install_ownership(self):
        supported = {
            ("web", "javascript"),
            ("web", "vanilla"),
            ("web", "ts"),
            ("web", "node"),
            ("web", "typescript"),
            ("lib", "typescript"),
            ("lib", "python"),
            ("clo", "cdk"),
            ("clo", "pulumi"),
        }
        for lane in LANES:
            expected = "mgc" if (lane["core"], lane["language"]) in supported else "unsupported-no-runner"
            self.assertEqual(
                lane["owner_by_operation"]["offline-reinstall"],
                expected,
                f"{lane['core']}/{lane['language']} offline ownership must be explicit",
            )

    def test_agreement_passes(self):
        rows = [
            ("web", "javascript", "mgc-native", NATIVE_ALL),
            ("ai", "python", "mgc-native", NATIVE_WITHOUT_GC_OFFLINE),
            ("hardware", "benchmark", "scaffold-only", cells(scaffold={"list"})),
            ("cicd", "github-actions", "unsupported", NO_OPERATIONS),
        ]
        self.assertEqual(check_dep_gate_consistency(BINARY, lanes(rows)), [])

    def test_laundering_fails(self):
        # A lane claiming native the binary denies is laundering.
        rows = [("app", "kotlin", "mgc-native", NO_OPERATIONS)]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertTrue(any("app/kotlin" in violation for violation in violations))

    def test_stale_downgrade_fails(self):
        # A lane denying native the binary proves is stale.
        rows = [("web", "javascript", "delegated", NATIVE_ALL)]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertTrue(violations)

    def test_missing_core_fails(self):
        rows = [("unknown", "x", "unsupported", NO_OPERATIONS)]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertEqual(len(violations), 1)

    def test_missing_operation_owner_fails_closed(self):
        row = lanes([("web", "javascript", "mgc-native", NATIVE_ALL)])[0]
        del row["owner_by_operation"]["list"]
        violations = check_dep_gate_consistency(BINARY, [row])
        self.assertTrue(any("matrix operation owner missing 'list'" in item for item in violations))

    def test_external_package_manager_cannot_be_labeled_native(self):
        owners = dict(NATIVE_ALL)
        owners["install"] = "cargo"
        row = lanes([("web", "javascript", "mgc-native", owners)])[0]
        violations = check_dep_gate_consistency(BINARY, [row])
        self.assertTrue(any("operation 'install'" in item for item in violations))


class MissingBinaryPolicy(unittest.TestCase):
    def test_missing_binary_fails_by_default(self):
        env = {"MGC_BIN": "", "CI": "", "MGC_ALLOW_MISSING_DEP_GATE_BINARY": ""}
        with patch.dict(os.environ, env):
            self.assertEqual(validate_dep_gate_consistency("/nonexistent/mgc"), 1)

    def test_static_only_requires_explicit_local_opt_in(self):
        env = {"MGC_BIN": "", "CI": "", "MGC_ALLOW_MISSING_DEP_GATE_BINARY": "1"}
        with patch.dict(os.environ, env):
            self.assertEqual(validate_dep_gate_consistency("/nonexistent/mgc"), 0)

    def test_ci_never_accepts_missing_binary_opt_in(self):
        env = {"MGC_BIN": "", "CI": "true", "MGC_ALLOW_MISSING_DEP_GATE_BINARY": "1"}
        with patch.dict(os.environ, env):
            self.assertEqual(validate_dep_gate_consistency("/nonexistent/mgc"), 1)


class RealLanesShape(unittest.TestCase):
    def test_every_lane_has_closed_vocabulary_owner(self):
        self.assertGreater(len(LANES), 0)
        for lane in LANES:
            self.assertIn(
                lane.get("dependency_owner"),
                DEPENDENCY_OWNER_VOCABULARY,
                f"{lane['core']}/{lane['language']} owner must be closed-vocabulary",
            )

    def test_real_lanes_agree_with_binary_table(self):
        # Mirrors every DepOp::ALL cell, not just Install. Update both the
        # binary fixture and lane projection when the source ownership table changes.
        violations = check_dep_gate_consistency(BINARY, LANES)
        self.assertEqual(violations, [])

    def test_every_lane_declares_all_dependency_operations(self):
        expected = set(ALL_DEPENDENCY_OPERATIONS)
        for lane in LANES:
            with self.subTest(core=lane["core"], language=lane["language"]):
                self.assertEqual(set(lane.get("owner_by_operation", {})), expected)

    def test_current_source_truth_corrections_are_not_stale(self):
        by_key = {(lane["core"], lane["language"]): lane for lane in LANES}
        self.assertEqual(by_key[("app", "swift")]["dependency_owner"], "mgc-native")
        self.assertEqual(by_key[("app", "objc")]["dependency_owner"], "unsupported")
        self.assertEqual(by_key[("game", "rust")]["dependency_owner"], "mgc-native")
        self.assertEqual(by_key[("iot", "rust")]["dependency_owner"], "mgc-native")
        self.assertEqual(by_key[("clo", "terraform")]["dependency_owner"], "unsupported")

    def test_recovery_probe_is_not_inferred_from_native_install_owner(self):
        lanes_by_key = {(lane["core"], lane["language"]): lane for lane in LANES}
        self.assertTrue(lanes_by_key[("web", "javascript")].get("recovery_probe"))
        self.assertTrue(lanes_by_key[("lib", "typescript")].get("recovery_probe"))
        for key in [("lib", "python"), ("ai", "python"), ("app", "flutter")]:
            self.assertFalse(
                lanes_by_key[key].get("recovery_probe", False),
                f"{key} must not inherit the web store recovery probe",
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
