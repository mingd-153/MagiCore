#!/usr/bin/env python3
"""Negative and positive tests for the Phase 0 coverage contract."""

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate_coverage_manifest.py")
SPEC = importlib.util.spec_from_file_location("coverage_validator", SCRIPT)
validator = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(validator)


def lane(**overrides):
    value = {
        "core": "web",
        "language": "javascript",
        "framework": "vite",
        "operation": "install",
        "target": "linux-x86_64-gnu",
        "dependency_owner": "unverified",
        "status": "unverified",
        "evidence": [],
    }
    value.update(overrides)
    return value


def manifest(*lanes):
    return {"schema_version": 1, "manifest_kind": "coverage-skeleton", "lanes": list(lanes)}


class CoverageManifestContract(unittest.TestCase):
    def test_checked_in_fixture_passes(self):
        validator.validate(validator.load_json(validator.FIXTURE_PATH))

    def test_json_schema_enums_match_the_executable_validator(self):
        validator.validate_schema_contract(validator.load_json(validator.SCHEMA_PATH))

    def test_rejects_duplicate_operation_lane(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            validator.validate(manifest(lane(), lane()))

    def test_target_is_mandatory_per_lane(self):
        missing_target = lane()
        del missing_target["target"]
        with self.assertRaisesRegex(ValueError, "missing required field.*target"):
            validator.validate(manifest(missing_target))

    def test_boolean_is_not_accepted_as_schema_version_integer(self):
        with self.assertRaisesRegex(ValueError, "unsupported schema_version"):
            validator.validate({"schema_version": True, "manifest_kind": "coverage-skeleton", "lanes": []})

    def test_rejects_delegated_lane_labeled_supported(self):
        with self.assertRaisesRegex(ValueError, "conflicts"):
            validator.validate(manifest(lane(dependency_owner="delegated", status="supported")))

    def test_supported_requires_release_artifact_provenance(self):
        local_evidence = [{"kind": "integration", "commit": "a" * 40, "result": "pass"}]
        with self.assertRaisesRegex(ValueError, "requires passing release-artifact"):
            validator.validate(manifest(lane(dependency_owner="mgc-native", status="supported", evidence=local_evidence)))

    def test_supported_release_evidence_requires_digest_and_run(self):
        incomplete = [{"kind": "release-artifact", "commit": "a" * 40, "result": "pass"}]
        with self.assertRaisesRegex(ValueError, "requires passing release-artifact"):
            validator.validate(manifest(lane(dependency_owner="mgc-native", status="supported", evidence=incomplete)))

    def test_supported_accepts_complete_release_provenance_shape(self):
        evidence = [{
            "kind": "release-artifact",
            "commit": "a" * 40,
            "result": "pass",
            "artifact_sha256": "b" * 64,
            "workflow_run": "https://github.com/example/project/actions/runs/123",
        }]
        validator.validate(manifest(lane(dependency_owner="mgc-native", status="supported", evidence=evidence)))

    def test_rejects_unknown_field(self):
        invalid = lane()
        invalid["compatibility_only"] = True
        with self.assertRaisesRegex(ValueError, "unknown field"):
            validator.validate(manifest(invalid))

    def test_rejects_wrong_json_types_without_traceback(self):
        invalid = lane(core=[])
        with self.assertRaisesRegex(ValueError, "unsupported core"):
            validator.validate(manifest(invalid))


if __name__ == "__main__":
    unittest.main(verbosity=2)
