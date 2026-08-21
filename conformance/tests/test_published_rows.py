#!/usr/bin/env python3
"""Behavioral contracts for typed published adequacy rows."""

from __future__ import annotations

import json
import hashlib
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/adequacy"))

import check_published_numbers as checker  # noqa: E402
import measure_all  # noqa: E402
import published_rows  # noqa: E402


TOOL_COMMIT = "13048989c84ab6b4e0281f9514ea45fb79a2d8b4"
MEASURED_COMMIT = "7c68894a2065ca8a273ddfd36824f28e3bfdc168"


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def manifest_bytes(*, runner: str = "module", implementation: str = "implementation.py") -> bytes:
    return (json.dumps({
        "schema": "corpus-adequacy.manifest.v0",
        "runner": runner,
        "tool_pin": {"commit": TOOL_COMMIT, "tool": "producer"},
        "implementation": implementation,
        "mutants": {"rules": []},
    }, indent=2, sort_keys=True) + "\n").encode()


def report(*, runner: str = "module", control_status: str = "killed") -> dict:
    return {
        "schema": "corpus-adequacy.report.v0",
        "runner": runner,
        "killed": 2,
        "survived": 1,
        "silent": 1,
        "equivalent": 1,
        "unexercised_out_of_scope": 3,
        "known_holes": 1,
        "unproved": 2,
        "declared_total": 11,
        "score_percent": 50.0,
        "adequate": False,
        "diagnostic_channel_declared": True,
        "control_status": control_status,
        "manifest_sha256": "",
        "tool_commit": TOOL_COMMIT,
        "tool_source_state": "exact",
        "tool_content_sha256": "sha256:" + "a" * 64,
        "tool_version": "0.1.0",
        "mutants": [{"verdict": "control-killed"}],
        "failures": [],
    }


def report_bytes(value: dict) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def projected(temp: Path, *, value: dict | None = None, runner: str = "module",
              implementation: str = "implementation.py",
              subject: dict | None = None) -> tuple[dict, bytes]:
    manifest = temp / "sample.manifest.json"
    temp.mkdir(parents=True, exist_ok=True)
    raw_manifest = manifest_bytes(runner=runner, implementation=implementation)
    manifest.write_bytes(raw_manifest)
    if not implementation.startswith("../"):
        (temp / implementation).write_text("# measured\n", encoding="utf-8")
    value = dict(value or report(runner=runner))
    if not value.get("manifest_sha256"):
        value["manifest_sha256"] = sha256(raw_manifest)
    raw_report = report_bytes(value)
    row = published_rows.project_report(
        manifest,
        value,
        raw_report,
        corpus="sample",
        manifest="conformance/adequacy/sample.manifest.json",
        measured_commit=MEASURED_COMMIT,
        depends_on=(
            ["conformance/adequacy/sample.manifest.json"]
            if implementation.startswith("../") else
            ["conformance/adequacy/implementation.py",
             "conformance/adequacy/sample.manifest.json"]
        ),
        subject=subject or {"kind": "in_tree"},
    )
    return row, raw_report


class WriterUsesCanonicalRows(unittest.TestCase):
    def test_duplicate_corpus_ids_are_rejected_before_dict_projection(self):
        document = {
            "schema": measure_all.SCHEMA,
            "corpora": [{"corpus": "same"}, {"corpus": "same"}],
        }
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            previous = measure_all.RESULTS
            measure_all.RESULTS = path
            try:
                with self.assertRaisesRegex(ValueError, "duplicate.*same"):
                    measure_all.read_existing()
            finally:
                measure_all.RESULTS = previous

    def test_writer_forwards_the_exact_producer_bytes(self):
        encoded = b'{"not":"re-serialized"}\n'
        with tempfile.TemporaryDirectory() as raw:
            manifest = Path(raw) / "sample.manifest.json"
            manifest.write_bytes(manifest_bytes())
            with mock.patch.object(measure_all, "rel", return_value="sample.manifest.json"), \
                    mock.patch.object(measure_all, "measured_at", return_value={
                        "measured_at": {"commit": MEASURED_COMMIT,
                                        "depends_on": ["sample.manifest.json"]}}), \
                    mock.patch.object(measure_all, "subject",
                                      return_value={"subject": {"kind": "in_tree"}}), \
                    mock.patch.object(published_rows, "project_report", return_value={}) as project:
                measure_all.row(manifest, report(), encoded)
        self.assertIs(project.call_args.args[2], encoded)

    def test_writer_calls_the_canonical_loader(self):
        sentinel = published_rows.LoadedResults({"corpora": []}, ())
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            path.write_text('{"corpora": []}', encoding="utf-8")
            with mock.patch.object(measure_all, "RESULTS", path), mock.patch.object(
                published_rows, "load_results", return_value=sentinel
            ) as loader:
                self.assertEqual(measure_all.read_existing(), {})
                loader.assert_called_once_with(path)


class BoundedInput(unittest.TestCase):
    def test_exact_limit_is_accepted_and_limit_plus_one_is_rejected(self):
        exact = b'{"corpora":[]}'
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            path.write_bytes(exact)
            self.assertEqual(published_rows.load_results(path, limit=len(exact)).rows, ())
            path.write_bytes(exact + b" ")
            with self.assertRaisesRegex(ValueError, "exceeds"):
                published_rows.load_results(path, limit=len(exact))

    def test_malformed_json_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            path.write_text("{", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "results JSON"):
                published_rows.load_results(path)

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks unavailable")
    def test_symlink_input_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            target = root / "target.json"
            target.write_text('{"corpora":[]}', encoding="utf-8")
            link = root / "results.json"
            link.symlink_to(target)
            with self.assertRaises(ValueError):
                published_rows.load_results(link)

    def test_nonregular_input_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "regular file"):
                published_rows.load_results(Path(raw))

    @unittest.skipUnless(hasattr(os, "mkfifo"), "FIFOs unavailable")
    def test_fifo_is_rejected_without_waiting_for_a_writer(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            os.mkfifo(path)
            started = time.monotonic()
            with self.assertRaisesRegex(ValueError, "regular file"):
                published_rows.load_results(path)
            self.assertLess(time.monotonic() - started, 1.0)


class CurrentReportProjection(unittest.TestCase):
    def test_all_portable_fields_and_exact_report_address_are_projected(self):
        with tempfile.TemporaryDirectory() as raw:
            row, encoded = projected(Path(raw))
        self.assertEqual(row["runner"], "module")
        self.assertEqual(row["silent"], 1)
        self.assertEqual(row["out_of_scope"], 3)
        self.assertEqual(row["known_holes"], 1)
        self.assertEqual(row["unproved"], 2)
        self.assertTrue(row["diagnostic_channel_declared"])
        self.assertEqual(row["control_status"], "killed")
        self.assertEqual(row["tool_source_state"], "exact")
        self.assertEqual(row["tool_content_sha256"], "sha256:" + "a" * 64)
        self.assertEqual(row["report_sha256"], sha256(encoded))
        self.assertEqual(row["report_ref"], "#/reports/%s" % sha256(encoded))

    def test_control_status_is_not_reconstructed_from_mutant_rows(self):
        value = report(control_status="survived")
        value["mutants"] = [{"verdict": "control-killed"}]
        with tempfile.TemporaryDirectory() as raw:
            row, _ = projected(Path(raw), value=value)
        self.assertEqual(row["control_status"], "survived")
        self.assertEqual(row["control"], "SURVIVED")

    def test_error_envelope_is_not_a_successful_report(self):
        value = report()
        value["schema"] = "corpus-adequacy.error.v0"
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "report.v0"):
                projected(Path(raw), value=value)

    def test_bool_is_not_an_integer_count(self):
        value = report()
        value["killed"] = True
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "killed"):
                projected(Path(raw), value=value)

    def test_control_only_report_may_have_no_score(self):
        value = report()
        value["score_percent"] = None
        with tempfile.TemporaryDirectory() as raw:
            row, _ = projected(Path(raw), value=value)
        self.assertIsNone(row["score_percent"])

    def test_dirty_and_unresolved_current_identity_fail_closed(self):
        for state in ("dirty", "unresolved"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as raw:
                value = report()
                value["tool_source_state"] = state
                value["tool_commit"] = None
                with self.assertRaisesRegex(ValueError, state):
                    projected(Path(raw), value=value)

    def test_runner_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "runner"):
                projected(Path(raw), value=report(runner="batch"), runner="module")

    def test_manifest_digest_mismatch_is_rejected(self):
        value = report()
        value["manifest_sha256"] = "sha256:" + "0" * 64
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "manifest_sha256"):
                projected(Path(raw), value=value)


class CurrentResultsDocument(unittest.TestCase):
    def write_current(self, root: Path) -> tuple[Path, dict, bytes]:
        adequacy = root / "conformance/adequacy"
        row, encoded = projected(adequacy)
        document = {
            "schema": measure_all.SCHEMA,
            "row_contract": published_rows.ROW_CONTRACT,
            "reports": {sha256(encoded): encoded.decode("utf-8")},
            "unmeasured": [],
            "corpora": [row],
        }
        path = adequacy / "results.json"
        path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return path, document, encoded

    def test_report_hash_is_over_exact_stored_producer_bytes(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, encoded = self.write_current(Path(raw))
            old = sha256(encoded)
            document["reports"][old] = document["reports"][old] + " "
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "report_sha256"):
                published_rows.load_results(path)

    def test_current_row_missing_a_portable_field_is_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            del document["corpora"][0]["silent"]
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "silent"):
                published_rows.load_results(path)

    def test_current_rows_cannot_be_downgraded_to_legacy(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            del document["reports"]
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "downgraded"):
                published_rows.load_results(path)

    def test_removing_all_current_markers_cannot_downgrade_the_document(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            del document["reports"]
            del document["row_contract"]
            for field in ("report_sha256", "report_ref"):
                del document["corpora"][0][field]
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "downgraded"):
                published_rows.load_results(path)

    def test_dependencies_must_be_complete_for_the_addressed_manifest(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            document["corpora"][0]["measured_at"]["depends_on"] = [
                "conformance/adequacy/sample.manifest.json"]
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "complete dependency"):
                published_rows.load_results(path)

    def test_external_manifest_cannot_be_relabelled_in_tree(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            adequacy = root / "conformance/adequacy"
            external_subject = {
                "kind": "out_of_tree",
                "repos": [{"repository": "example/external", "commit": "b" * 40,
                           "dirty": False, "measured": ["../../../outside/file.py"]}],
            }
            row, encoded = projected(
                adequacy, implementation="../../../outside/file.py", subject=external_subject)
            row["subject"] = {"kind": "in_tree"}
            document = {
                "schema": measure_all.SCHEMA,
                "row_contract": published_rows.ROW_CONTRACT,
                "reports": {sha256(encoded): encoded.decode("utf-8")},
                "unmeasured": [],
                "corpora": [row],
            }
            path = adequacy / "results.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "out_of_tree"):
                published_rows.load_results(path)

    def test_external_subject_requires_a_typed_nonempty_repository_list(self):
        invalid_repos = ([], {}, ["not-an-object"])
        for repos in invalid_repos:
            with self.subTest(repos=repos), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                adequacy = root / "conformance/adequacy"
                row, encoded = projected(
                    adequacy,
                    implementation="../../../outside/file.py",
                    subject={"kind": "out_of_tree", "repos": repos},
                )
                document = {
                    "schema": measure_all.SCHEMA,
                    "row_contract": published_rows.ROW_CONTRACT,
                    "reports": {sha256(encoded): encoded.decode("utf-8")},
                    "unmeasured": [],
                    "corpora": [row],
                }
                path = adequacy / "results.json"
                path.write_text(json.dumps(document), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "repos"):
                    published_rows.load_results(path)

    def test_control_display_must_match_the_producer_status(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            document["corpora"][0]["control"] = "SURVIVED"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "control"):
                published_rows.load_results(path)

    def test_unaddressed_report_bytes_are_rejected(self):
        with tempfile.TemporaryDirectory() as raw:
            path, document, _ = self.write_current(Path(raw))
            document["reports"]["sha256:" + "f" * 64] = "{}\n"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "exactly the addressed"):
                published_rows.load_results(path)

    def test_empty_and_hostile_dependencies_are_rejected(self):
        for dependencies in ([], [""], [":(exclude)README.md"], ["../README.md"]):
            with self.subTest(dependencies=dependencies), tempfile.TemporaryDirectory() as raw:
                path, document, _ = self.write_current(Path(raw))
                document["corpora"][0]["measured_at"]["depends_on"] = dependencies
                path.write_text(json.dumps(document), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "depends_on"):
                    published_rows.load_results(path)

    def test_truly_legacy_row_may_lack_additive_report_fields(self):
        legacy = {
            "schema": "assay.conformance.adequacy.results.v0",
            "corpora": [{"corpus": "legacy", "manifest": "legacy.manifest.json"}],
        }
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "results.json"
            path.write_text(json.dumps(legacy), encoding="utf-8")
            loaded = published_rows.load_results(path)
        self.assertEqual(loaded.rows[0]["corpus"], "legacy")


class CheckerUsesCanonicalRows(unittest.TestCase):
    def test_checker_calls_the_canonical_loader(self):
        with mock.patch.object(
            published_rows, "load_results", side_effect=ValueError("canonical sentinel")
        ) as loader:
            findings = checker.check()
        loader.assert_called_once_with(checker.RESULTS)
        self.assertTrue(any("canonical sentinel" in finding for finding in findings), findings)

    def test_checker_names_unmeasured_rows_without_discarding_loaded_rows(self):
        current = published_rows.load_results(checker.RESULTS)
        document = dict(current.document, unmeasured=["missing"])
        loaded = published_rows.LoadedResults(document, current.rows)
        with mock.patch.object(published_rows, "load_results", return_value=loaded):
            findings = checker.check()
        self.assertTrue(any("missing was not measured" in finding for finding in findings))


if __name__ == "__main__":
    unittest.main(verbosity=2)
