#!/usr/bin/env python3
"""Validate the Phase 0 coverage-manifest skeleton without third-party deps.

This validator deliberately checks structure and anti-laundering invariants;
it does not generate capability claims or treat the fixture as evidence.
"""

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCHEMA_PATH = ROOT / "ci/evidence/coverage-manifest.schema.json"
FIXTURE_PATH = ROOT / "ci/evidence/coverage-manifest.fixture.json"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
CORES = {"web", "ai", "app", "lib", "game", "iot", "clo", "cicd", "hardware"}
OPERATIONS = {
    "detect", "resolve", "lock", "fetch", "verify", "store", "materialize",
    "install", "add", "remove", "update", "list", "frozen-install",
    "offline-reinstall", "audit", "build", "test", "run", "dev", "publish",
}
OWNERS = {"mgc-native", "delegated", "scaffold-only", "unsupported", "unverified"}
STATUSES = {"native-experimental", "supported", "delegated", "scaffold-only", "unsupported", "unverified"}
EVIDENCE_KINDS = {"unit", "integration", "release-artifact", "clean-machine", "benchmark"}
EVIDENCE_RESULTS = {"pass", "fail", "unverified"}


def fail(message: str) -> None:
    raise ValueError(message)


def require_object(value, where: str, required: set[str], allowed: set[str]) -> dict:
    if not isinstance(value, dict):
        fail(f"{where}: expected object")
    missing = sorted(required - value.keys())
    unknown = sorted(value.keys() - allowed)
    if missing:
        fail(f"{where}: missing required field(s): {', '.join(missing)}")
    if unknown:
        fail(f"{where}: unknown field(s): {', '.join(unknown)}")
    return value


def validate(document: object) -> None:
    root_fields = {"schema_version", "manifest_kind", "lanes"}
    root = require_object(document, "manifest", root_fields, root_fields)
    if type(root["schema_version"]) is not int or root["schema_version"] != 1 or root["manifest_kind"] != "coverage-skeleton":
        fail("manifest: unsupported schema_version or manifest_kind")
    if not isinstance(root["lanes"], list):
        fail("manifest.lanes: expected array")

    lane_fields = {
        "core", "language", "framework", "operation", "target", "dependency_owner", "status", "evidence"
    }
    evidence_fields = {"kind", "commit", "result", "artifact_sha256", "workflow_run"}
    evidence_required = {"kind", "commit", "result"}
    seen = set()

    for index, raw_lane in enumerate(root["lanes"]):
        where = f"manifest.lanes[{index}]"
        lane = require_object(raw_lane, where, lane_fields, lane_fields)
        for field in ("language", "framework"):
            if not isinstance(lane[field], str) or not lane[field].strip():
                fail(f"{where}.{field}: must be a non-empty string")
        if not isinstance(lane["target"], str) or not lane["target"].strip():
            fail(f"{where}.target: must be a non-empty string")
        if not isinstance(lane["core"], str) or lane["core"] not in CORES:
            fail(f"{where}.core: unsupported core {lane['core']!r}")
        if not isinstance(lane["operation"], str) or lane["operation"] not in OPERATIONS:
            fail(f"{where}.operation: unsupported operation {lane['operation']!r}")
        if (
            not isinstance(lane["dependency_owner"], str)
            or lane["dependency_owner"] not in OWNERS
            or not isinstance(lane["status"], str)
            or lane["status"] not in STATUSES
        ):
            fail(f"{where}: unknown dependency_owner or status")

        identity = tuple(lane.get(key, "") for key in ("core", "language", "framework", "operation", "target"))
        if identity in seen:
            fail(f"{where}: duplicate core/language/framework/operation/target lane")
        seen.add(identity)

        owner, status = lane["dependency_owner"], lane["status"]
        compatible = {
            "native-experimental": owner == "mgc-native",
            "supported": owner == "mgc-native",
            "delegated": owner == "delegated",
            "scaffold-only": owner == "scaffold-only",
            "unsupported": owner == "unsupported",
            "unverified": owner == "unverified",
        }
        if not compatible[status]:
            fail(f"{where}: status {status!r} conflicts with dependency_owner {owner!r}")

        evidence = lane["evidence"]
        if not isinstance(evidence, list):
            fail(f"{where}.evidence: expected array")
        release_proof = False
        for evidence_index, raw_item in enumerate(evidence):
            ewhere = f"{where}.evidence[{evidence_index}]"
            item = require_object(raw_item, ewhere, evidence_required, evidence_fields)
            if (
                not isinstance(item["kind"], str)
                or item["kind"] not in EVIDENCE_KINDS
                or not isinstance(item["result"], str)
                or item["result"] not in EVIDENCE_RESULTS
            ):
                fail(f"{ewhere}: invalid kind or result")
            if not isinstance(item["commit"], str) or not HEX40.fullmatch(item["commit"]):
                fail(f"{ewhere}.commit: expected lowercase 40-hex commit")
            if "artifact_sha256" in item and (
                not isinstance(item["artifact_sha256"], str) or not HEX64.fullmatch(item["artifact_sha256"])
            ):
                fail(f"{ewhere}.artifact_sha256: expected lowercase 64-hex digest")
            if "workflow_run" in item and (
                not isinstance(item["workflow_run"], str) or not item["workflow_run"].strip()
            ):
                fail(f"{ewhere}.workflow_run: expected non-empty string")
            if (
                item["kind"] in {"release-artifact", "clean-machine"}
                and item["result"] == "pass"
                and HEX64.fullmatch(item.get("artifact_sha256", ""))
                and item.get("workflow_run")
            ):
                release_proof = True
        if status == "supported" and not release_proof:
            fail(f"{where}: supported requires passing release-artifact/clean-machine evidence, digest, and workflow_run")


def load_json(path: Path):
    try:
        with path.open(encoding="utf-8") as stream:
            return json.load(stream)
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{path}: {error}")


def validate_schema_contract(schema: object) -> None:
    """Catch drift between the checked-in JSON Schema and stdlib validator."""
    if (
        not isinstance(schema, dict)
        or schema.get("type") != "object"
        or schema.get("additionalProperties") is not False
    ):
        fail("schema: root must be a JSON Schema object")
    defs = schema.get("$defs")
    if not isinstance(defs, dict) or not isinstance(defs.get("lane"), dict) or not isinstance(defs.get("evidence"), dict):
        fail("schema: missing lane/evidence definitions")
    lane_props = defs["lane"].get("properties", {})
    evidence_props = defs["evidence"].get("properties", {})
    root_props = schema.get("properties", {})
    if set(root_props) != {"schema_version", "manifest_kind", "lanes"}:
        fail("schema: root properties differ from validator")
    if set(lane_props) != {
        "core", "language", "framework", "operation", "target", "dependency_owner", "status", "evidence"
    }:
        fail("schema: lane properties differ from validator")
    if set(evidence_props) != {"kind", "commit", "result", "artifact_sha256", "workflow_run"}:
        fail("schema: evidence properties differ from validator")
    if set(schema.get("required", [])) != {"schema_version", "manifest_kind", "lanes"}:
        fail("schema: root required fields differ from validator")
    if set(defs["lane"].get("required", [])) != {
        "core", "language", "framework", "operation", "target", "dependency_owner", "status", "evidence"
    }:
        fail("schema: lane required fields differ from validator")
    if set(defs["evidence"].get("required", [])) != {"kind", "commit", "result"}:
        fail("schema: evidence required fields differ from validator")
    if root_props.get("schema_version", {}).get("const") != 1 or root_props.get("manifest_kind", {}).get("const") != "coverage-skeleton":
        fail("schema: root constants differ from validator")
    expected_enums = {
        "core": CORES,
        "operation": OPERATIONS,
        "dependency_owner": OWNERS,
        "status": STATUSES,
    }
    for field, expected in expected_enums.items():
        actual = lane_props.get(field, {}).get("enum")
        if not isinstance(actual, list) or set(actual) != expected:
            fail(f"schema: lane.{field} enum differs from validator")
    for field, expected in (("kind", EVIDENCE_KINDS), ("result", EVIDENCE_RESULTS)):
        actual = evidence_props.get(field, {}).get("enum")
        if not isinstance(actual, list) or set(actual) != expected:
            fail(f"schema: evidence.{field} enum differs from validator")
    if defs["lane"].get("additionalProperties") is not False or defs["evidence"].get("additionalProperties") is not False:
        fail("schema: lane and evidence objects must reject unknown fields")


def main(argv: list[str]) -> int:
    schema_path = Path(argv[1]) if len(argv) > 1 else SCHEMA_PATH
    manifest_path = Path(argv[2]) if len(argv) > 2 else FIXTURE_PATH
    # Ensure both checked-in artifacts remain valid JSON. Structural validation
    # mirrors the deliberately supported subset of JSON Schema in stdlib.
    validate_schema_contract(load_json(schema_path))
    validate(load_json(manifest_path))
    print(f"coverage manifest contract PASS: {manifest_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except ValueError as error:
        print(f"coverage manifest contract FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
