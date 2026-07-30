#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("verify-mcp-upstream-reference.py")
SPEC = importlib.util.spec_from_file_location("mcp_upstream_reference", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class UpstreamReferenceValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.results = Path(self.temp.name)
        self.paths: dict[str, Path] = {}
        for scenario, check_ids in MODULE.EXPECTED.items():
            prefix = "" if scenario.startswith("sep-2322-client") else "server-"
            path = self.results / f"{prefix}{scenario}-2026-07-31" / "checks.json"
            path.parent.mkdir(parents=True)
            path.write_text(
                json.dumps(
                    [
                        {"id": check_id, "status": "SUCCESS"}
                        for check_id in sorted(check_ids)
                    ]
                    + [{"id": "wire-schema-observation", "status": "INFO"}]
                ),
                encoding="utf-8",
            )
            self.paths[scenario] = path

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_result_type_checks(self, status: str, *, duplicate: bool = False) -> None:
        checks = [
            {"id": "sep-2322-result-type-included", "status": status},
            {"id": "wire-schema-valid", "status": "SUCCESS"},
        ]
        if duplicate:
            checks.insert(1, checks[0].copy())
        self.paths["input-required-result-result-type"].write_text(
            json.dumps(checks),
            encoding="utf-8",
        )

    def test_clean_focused_result_set_is_accepted(self) -> None:
        summary = MODULE.validate(self.results)
        self.assertEqual(set(summary), set(MODULE.EXPECTED))

    def test_missing_scenario_is_refused(self) -> None:
        self.paths["input-required-result-result-type"].unlink()
        with self.assertRaisesRegex(MODULE.ValidationError, "exactly one"):
            MODULE.validate(self.results)

    def test_duplicate_scenario_output_is_refused(self) -> None:
        source = self.paths["input-required-result-result-type"]
        duplicate = (
            self.results
            / "server-input-required-result-result-type-later"
            / "checks.json"
        )
        duplicate.parent.mkdir()
        duplicate.write_bytes(source.read_bytes())
        with self.assertRaisesRegex(MODULE.ValidationError, "found 2"):
            MODULE.validate(self.results)

    def test_missing_named_check_is_refused(self) -> None:
        path = self.paths["input-required-result-result-type"]
        path.write_text("[]", encoding="utf-8")
        with self.assertRaisesRegex(MODULE.ValidationError, "exactly once"):
            MODULE.validate(self.results)

    def test_missing_wire_schema_check_is_refused(self) -> None:
        path = self.paths["input-required-result-result-type"]
        path.write_text(
            json.dumps(
                [{"id": "sep-2322-result-type-included", "status": "SUCCESS"}]
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(MODULE.ValidationError, "wire-schema-valid"):
            MODULE.validate(self.results)

    def test_failure_status_is_refused(self) -> None:
        self.write_result_type_checks("FAILURE")
        with self.assertRaisesRegex(MODULE.ValidationError, "non-success"):
            MODULE.validate(self.results)

    def test_warning_status_is_refused(self) -> None:
        self.write_result_type_checks("WARNING")
        with self.assertRaisesRegex(MODULE.ValidationError, "non-success"):
            MODULE.validate(self.results)

    def test_expected_info_status_is_refused(self) -> None:
        self.write_result_type_checks("INFO")
        with self.assertRaisesRegex(MODULE.ValidationError, "must be SUCCESS"):
            MODULE.validate(self.results)

    def test_expected_skipped_status_is_refused(self) -> None:
        self.write_result_type_checks("SKIPPED")
        with self.assertRaisesRegex(MODULE.ValidationError, "non-success"):
            MODULE.validate(self.results)

    def test_duplicate_named_check_is_refused(self) -> None:
        self.write_result_type_checks("SUCCESS", duplicate=True)
        with self.assertRaisesRegex(MODULE.ValidationError, "exactly once"):
            MODULE.validate(self.results)

    def test_malformed_json_is_refused(self) -> None:
        self.paths["input-required-result-result-type"].write_text(
            "{", encoding="utf-8"
        )
        with self.assertRaisesRegex(MODULE.ValidationError, "unreadable"):
            MODULE.validate(self.results)

    def test_unexpected_result_file_is_refused(self) -> None:
        path = self.results / "unrelated-scenario" / "checks.json"
        path.parent.mkdir()
        path.write_text("[]", encoding="utf-8")
        with self.assertRaisesRegex(MODULE.ValidationError, "unexpected"):
            MODULE.validate(self.results)


if __name__ == "__main__":
    unittest.main()
