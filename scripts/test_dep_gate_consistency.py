#!/usr/bin/env python3
"""Self-tests for the T0.4 binary<->matrix consistency check (stdlib only).

Run: python3 scripts/test_dep_gate_consistency.py
(Kiểm tra cổng nhất quán binary<->matrix — chỉ stdlib.)
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lifecycle_capability_matrix import (
    DEPENDENCY_OWNER_VOCABULARY,
    LANES,
    check_dep_gate_consistency,
)


def lanes(rows):
    """Build minimal lane dicts from (core, language, dependency_owner)."""
    return [
        {"core": core, "language": language, "dependency_owner": owner}
        for core, language, owner in rows
    ]


def ownership(install, languages=None):
    """Binary-table entry shape: core-level install owner plus optional
    per-language install overrides (mirrors the validator's projection of
    `mgc capabilities --json`)."""
    return {"install": install, "languages": languages or {}}


# Mirror of the REAL `mgc capabilities --json` dependency_ownership
# projection ({core: {"install": owner, "languages": {gate_lang: owner}}}).
# P0#2 truth: an UNDECLARED ecosystem fails closed, so every core-level
# base row is "unsupported" except the single-ecosystem lanes the binary
# can still name... none: even web/ai/game/iot/clo lanes must detect
# their ecosystem (web "js", ai "python", game "bevy", iot framework,
# clo "terraform"), so ALL bases are unsupported and every supported
# cell lives under its gate-ecosystem key.
BINARY = {
    "web": ownership(
        "unsupported",
        {"js": "mgc-native", "ts": "mgc-native", "javascript": "mgc-native"},
    ),
    "lib": ownership(
        "unsupported",
        {
            "ts": "mgc-native",
            "rust": "mgc-native",
            "python": "mgc-native",
            "go": "mgc-native",
            "java": "mgc-native",
            "dotnet": "mgc-native",
        },
    ),
    "ai": ownership("unsupported", {"python": "delegated"}),
    "app": ownership(
        "unsupported",
        {
            "flutter": "delegated",
            "kotlin": "delegated",
            "swift": "delegated",
            "objc": "delegated",
            "rn": "unsupported",
        },
    ),
    "game": ownership("unsupported", {"bevy": "delegated"}),
    "iot": ownership(
        "unsupported",
        {
            "esp32-rust": "delegated",
            "platformio": "delegated",
            "zephyr": "delegated",
        },
    ),
    "clo": ownership("unsupported", {"terraform": "delegated"}),
    "cicd": ownership("unsupported"),
    "hardware": ownership("unsupported"),
}


class ConsistencyDirections(unittest.TestCase):
    def test_agreement_passes(self):
        rows = [
            ("web", "javascript", "mgc-native"),
            ("ai", "python", "delegated"),
            ("hardware", "benchmark", "scaffold-only"),
            ("cicd", "github-actions", "unsupported"),
        ]
        self.assertEqual(check_dep_gate_consistency(BINARY, lanes(rows)), [])

    def test_laundering_fails(self):
        # A lane claiming native the binary denies is laundering.
        rows = [("app", "flutter", "mgc-native")]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertEqual(len(violations), 1)
        self.assertIn("app/flutter", violations[0])

    def test_stale_downgrade_fails(self):
        # A lane denying native the binary proves is stale.
        rows = [("web", "javascript", "delegated")]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertEqual(len(violations), 1)

    def test_missing_core_fails(self):
        rows = [("unknown", "x", "unsupported")]
        violations = check_dep_gate_consistency(BINARY, lanes(rows))
        self.assertEqual(len(violations), 1)


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
        # Mirrors the C0 firewall table (dep_gate::owner_for, install op,
        # language-unaware branch) — update BOTH sides together.
        violations = check_dep_gate_consistency(BINARY, LANES)
        self.assertEqual(violations, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
