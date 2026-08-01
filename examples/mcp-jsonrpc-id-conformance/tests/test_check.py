import contextlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest
from unittest import mock
from pathlib import Path


ROOT = Path(__file__).parents[1]
MODULE_PATH = ROOT / "check.py"
MAX_CLI_OUTPUT_BYTES = 64 << 10


class _BoundedTextBuffer(io.StringIO):
    def __init__(self) -> None:
        super().__init__()
        self._bytes_written = 0

    def write(self, text: str) -> int:
        encoded_bytes = len(text.encode("utf-8"))
        if self._bytes_written + encoded_bytes > MAX_CLI_OUTPUT_BYTES:
            raise AssertionError("CLI output exceeds test limit")
        written = super().write(text)
        self._bytes_written += encoded_bytes
        return written


def _load_module():
    spec = importlib.util.spec_from_file_location("mcp_jsonrpc_id_check", MODULE_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _mcp_schema(*, require_id: bool = False, allow_null: bool = False) -> bytes:
    request_id_types = ["string", "integer"]
    if allow_null:
        request_id_types.append("null")
    required = ["error", "jsonrpc"]
    if require_id:
        required.append("id")
    return json.dumps(
        {
            "$defs": {
                "RequestId": {"type": request_id_types},
                "JSONRPCErrorResponse": {
                    "properties": {
                        "error": {"$ref": "#/$defs/Error"},
                        "id": {"$ref": "#/$defs/RequestId"},
                        "jsonrpc": {"const": "2.0"},
                    },
                    "required": required,
                    "type": "object",
                },
            }
        }
    ).encode()


def _mcp_typescript(*, require_id: bool = False, allow_null: bool = False) -> bytes:
    optional = "" if require_id else "?"
    request_id = "string | number | null" if allow_null else "string | number"
    return f"""
export type RequestId = {request_id};
export interface JSONRPCErrorResponse {{
  jsonrpc: typeof JSONRPC_VERSION;
  id{optional}: RequestId;
  error: Error;
}}
""".encode()


JSONRPC_SUBJECT = b"""
<html><body>
<h2>Response object</h2>
<dl>
<dt>id</dt>
<dd>This member is REQUIRED.<br>
It MUST be the same as the value of the id member in the Request Object.<br>
If there was an error in detecting the id in the Request object
(e.g. Parse error/Invalid Request),
it MUST be Null.</dd>
</dl>
</body></html>
"""

MCP_OVERVIEW_SUBJECT = b"""
<html><body>
<p>All messages between MCP clients and servers MUST follow the
JSON-RPC 2.0 specification.</p>
</body></html>
"""


class McpJsonRpcIdConformanceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.m = _load_module()

    def test_pinned_pack_reproduces_the_two_incompatible_arms(self):
        report = self.m.reproduce(ROOT)
        self.assertEqual(report["status"], "contradiction")
        self.assertEqual(
            report["summary"],
            {
                "both_valid": 1,
                "mcp_only": 1,
                "jsonrpc_only": 1,
                "neither_valid": 0,
            },
        )

    def test_committed_subjects_reproduce_the_pinned_constraints_offline(self):
        report = self.m.verify_committed_subjects(ROOT)

        self.assertEqual(report["mode"], "verify-committed")
        self.assertEqual(report["status"], "contradiction")
        self.assertEqual(report["constraints"], self.m.PINNED_CONSTRAINTS)

    def test_committed_subject_digest_is_independent_of_pack_checksums(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            relative = "subjects/mcp-overview.html"
            subject = copied / relative
            subject.write_bytes(subject.read_bytes() + b"\n")
            sums = copied / "SHA256SUMS"
            lines = sums.read_text(encoding="utf-8").splitlines()
            updated = [
                f"{self.m.sha256_bytes(subject.read_bytes())}  {relative}"
                if line.endswith(f"  {relative}")
                else line
                for line in lines
            ]
            sums.write_text("\n".join(updated) + "\n", encoding="utf-8")

            with self.assertRaisesRegex(self.m.SubjectError, "digest"):
                self.m.verify_committed_subjects(copied)

    def test_live_drift_reports_missing_input_as_operationally_unavailable(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = {
                name: Path(tmp) / name
                for name in (
                    "mcp_schema_typescript",
                    "mcp_schema_json",
                    "mcp_overview",
                    "jsonrpc_spec",
                )
            }

            report = self.m.observe_live_subject_paths(ROOT, paths)

        self.assertEqual(report["operational"]["status"], "unavailable")
        self.assertEqual(report["content"]["status"], "unknown")
        self.assertEqual(report["semantic"]["status"], "unknown")
        self.assertCountEqual(report["operational"]["unavailable"], paths)

    def test_live_byte_drift_is_distinct_from_semantic_drift(self):
        subjects = {
            "mcp_schema_typescript": _mcp_typescript(),
            "mcp_schema_json": _mcp_schema(),
            "mcp_overview": MCP_OVERVIEW_SUBJECT,
            "jsonrpc_spec": JSONRPC_SUBJECT,
        }
        records = {
            name: {"upstream_sha256": self.m.sha256_bytes(subject)}
            for name, subject in subjects.items()
        }
        subjects["mcp_overview"] = MCP_OVERVIEW_SUBJECT.replace(
            b"<p>", b"<p>\n"
        )

        report = self.m.classify_live_subjects(records, subjects)

        self.assertEqual(report["operational"]["status"], "available")
        self.assertEqual(report["content"]["status"], "changed")
        self.assertEqual(report["content"]["changed"], ["mcp_overview"])
        self.assertEqual(report["semantic"]["status"], "contradiction")

    def test_live_semantic_drift_is_reported_without_failing_open(self):
        baseline = {
            "mcp_schema_typescript": _mcp_typescript(),
            "mcp_schema_json": _mcp_schema(),
            "mcp_overview": MCP_OVERVIEW_SUBJECT,
            "jsonrpc_spec": JSONRPC_SUBJECT,
        }
        records = {
            name: {"upstream_sha256": self.m.sha256_bytes(subject)}
            for name, subject in baseline.items()
        }
        changed = dict(baseline)
        changed["mcp_schema_typescript"] = _mcp_typescript(
            require_id=True, allow_null=True
        )
        changed["mcp_schema_json"] = _mcp_schema(
            require_id=True, allow_null=True
        )

        report = self.m.classify_live_subjects(records, changed)

        self.assertEqual(report["operational"]["status"], "available")
        self.assertEqual(report["content"]["status"], "changed")
        self.assertEqual(report["semantic"]["status"], "not_reproduced")

    def test_live_oversize_is_an_operational_refusal_not_semantic_cleanliness(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            paths = {
                "mcp_schema_typescript": directory / "mcp-schema.ts",
                "mcp_schema_json": directory / "mcp-schema.json",
                "mcp_overview": directory / "mcp-overview.html",
                "jsonrpc_spec": directory / "jsonrpc-spec.html",
            }
            paths["mcp_schema_typescript"].write_bytes(_mcp_typescript())
            paths["mcp_schema_json"].write_bytes(_mcp_schema())
            paths["mcp_overview"].write_bytes(MCP_OVERVIEW_SUBJECT)
            paths["jsonrpc_spec"].write_bytes(
                b"x" * (self.m.MAX_SUBJECT_BYTES + 1)
            )

            report = self.m.observe_live_subject_paths(ROOT, paths)

        self.assertEqual(report["operational"]["status"], "unavailable")
        self.assertEqual(report["operational"]["unavailable"], ["jsonrpc_spec"])
        self.assertEqual(report["content"]["status"], "unknown")
        self.assertEqual(report["semantic"]["status"], "unknown")

    def test_live_drift_does_not_hide_an_invalid_local_pack(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            (copied / "README.md").write_text("tampered", encoding="utf-8")
            subjects = copied / "subjects"
            stdout = _BoundedTextBuffer()

            with contextlib.redirect_stdout(stdout):
                returncode = self.m.main(
                    [
                        "live-drift",
                        "--root",
                        str(copied),
                        "--mcp-typescript",
                        str(subjects / "mcp-schema.ts"),
                        "--mcp-schema",
                        str(subjects / "mcp-schema.json"),
                        "--mcp-overview",
                        str(subjects / "mcp-overview.html"),
                        "--jsonrpc-spec",
                        str(subjects / "jsonrpc-spec.html"),
                    ]
                )

        self.assertEqual(returncode, 3)
        self.assertEqual(json.loads(stdout.getvalue())["error"], "PackError")

    def test_live_drift_does_not_catch_an_unexpected_checker_failure(self):
        with mock.patch.object(
            self.m,
            "observe_live_subject_paths",
            side_effect=RuntimeError("checker defect"),
        ):
            with self.assertRaisesRegex(RuntimeError, "checker defect"):
                self.m.main(
                    [
                        "live-drift",
                        "--mcp-typescript",
                        "unused",
                        "--mcp-schema",
                        "unused",
                        "--mcp-overview",
                        "unused",
                        "--jsonrpc-spec",
                        "unused",
                    ]
                )

    def test_each_vector_has_the_declared_classification(self):
        expected = {
            "shared-string-id-control": (True, True),
            "mcp-error-with-omitted-id": (True, False),
            "jsonrpc-error-with-null-id": (False, True),
        }
        self.m.validate_checksums(ROOT)
        provenance = self.m._validate_provenance(ROOT)
        observed_ids = []
        for relative in sorted(provenance["vectors"]):
            vector = self.m._load_json(ROOT / relative)
            observed_ids.append(vector["id"])
            observed = self.m.evaluate_message(vector["message"])
            self.assertEqual(
                (observed["mcp"], observed["jsonrpc"]),
                expected[vector["id"]],
                vector["id"],
            )
        self.assertCountEqual(observed_ids, expected)

    def test_vector_digest_mutation_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            vector = copied / "vectors" / "shared-string-id-control.json"
            vector.write_text(
                vector.read_text(encoding="utf-8") + "\n", encoding="utf-8"
            )
            with self.assertRaisesRegex(self.m.PackError, "digest"):
                self.m.reproduce(copied)

    def test_sha256sums_cover_every_public_pack_file(self):
        self.m.validate_checksums(ROOT)

    def test_unlisted_public_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            (copied / "unlisted.txt").write_text("not bound\n", encoding="utf-8")
            with self.assertRaisesRegex(self.m.PackError, "file set"):
                self.m.validate_checksums(copied)

    def test_pack_inventory_rejects_too_many_entries(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            extras = copied / "extras"
            extras.mkdir()
            for index in range(self.m.MAX_PACK_ENTRIES + 1):
                (extras / f"{index:04}.txt").touch()
            with self.assertRaisesRegex(self.m.PackError, "entry limit"):
                self.m.validate_checksums(copied)

    def test_pack_inventory_rejects_excessive_depth(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            nested = copied
            for index in range(self.m.MAX_PACK_DEPTH + 1):
                nested /= f"d{index}"
                nested.mkdir()
            with self.assertRaisesRegex(self.m.PackError, "depth limit"):
                self.m.validate_checksums(copied)

    def test_pack_inventory_rejects_broken_and_directory_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            target = copied / "target"
            target.mkdir()
            os.symlink(copied / "missing", copied / "broken-link")
            os.symlink(target, copied / "directory-link")
            with self.assertRaisesRegex(self.m.PackError, "symbolic links"):
                self.m.validate_checksums(copied)

    def test_pack_inventory_rejects_symlinked_checksum_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            checksum = copied / "SHA256SUMS"
            external = Path(tmp) / "external-sha256sums"
            external.write_bytes(checksum.read_bytes())
            checksum.unlink()
            os.symlink(external, checksum)
            with self.assertRaisesRegex(self.m.PackError, "symbolic links"):
                self.m.validate_checksums(copied)

    def test_reassessment_extracts_the_pinned_contradiction_from_subjects(self):
        report = self.m.reassess_subjects(
            _mcp_typescript(),
            _mcp_schema(),
            MCP_OVERVIEW_SUBJECT,
            JSONRPC_SUBJECT,
        )
        self.assertEqual(report["status"], "contradiction")
        self.assertTrue(report["constraints"]["mcp_requires_jsonrpc_2"])
        self.assertFalse(report["constraints"]["mcp_error_id_required"])
        self.assertFalse(report["constraints"]["mcp_error_id_allows_null"])
        self.assertTrue(report["constraints"]["jsonrpc_response_id_required"])
        self.assertTrue(report["constraints"]["jsonrpc_unknown_id_must_be_null"])

    def test_reassessment_can_report_that_a_later_subject_no_longer_reproduces_it(self):
        report = self.m.reassess_subjects(
            _mcp_typescript(require_id=True, allow_null=True),
            _mcp_schema(require_id=True, allow_null=True),
            MCP_OVERVIEW_SUBJECT,
            JSONRPC_SUBJECT,
        )
        self.assertEqual(report["status"], "not_reproduced")

    def test_jsonrpc_response_id_clauses_must_share_the_id_definition(self):
        subject = b"""
        <html><body>
        <dl><dt>id</dt><dd>
        This member is no longer REQUIRED. An unknown request id may be omitted.
        </dd></dl>
        <p>Historical wording: id This member is REQUIRED.</p>
        <dl><dt>unrelated</dt><dd>
        If there was an error in detecting the id in the Request object,
        it MUST be Null.
        </dd></dl>
        </body></html>
        """
        with self.assertRaisesRegex(self.m.SubjectError, "response-id clauses"):
            self.m.reassess_subjects(
                _mcp_typescript(),
                _mcp_schema(),
                MCP_OVERVIEW_SUBJECT,
                subject,
            )

    def test_jsonrpc_response_id_clauses_must_be_in_the_response_section(self):
        subject = b"""
        <html><body>
        <h2>Historical wording, not normative</h2>
        <dl><dt>id</dt><dd>
        This member is REQUIRED.
        It MUST be the same as the value of the id member in the Request Object.
        If there was an error in detecting the id in the Request object
        (e.g. Parse error/Invalid Request), it MUST be Null.
        </dd></dl>
        <h2>5 Response object</h2>
        <dl><dt>id</dt><dd>
        This member is optional. An unknown request id may be omitted.
        </dd></dl>
        </body></html>
        """
        with self.assertRaisesRegex(self.m.SubjectError, "response-id clauses"):
            self.m.reassess_subjects(
                _mcp_typescript(),
                _mcp_schema(),
                MCP_OVERVIEW_SUBJECT,
                subject,
            )

    def test_source_digest_binding_rejects_changed_bytes(self):
        subjects = {
            "mcp_schema_typescript": _mcp_typescript(),
            "mcp_schema_json": _mcp_schema(),
            "mcp_overview": MCP_OVERVIEW_SUBJECT,
            "jsonrpc_spec": JSONRPC_SUBJECT,
        }
        records = {
            name: {"upstream_sha256": self.m.sha256_bytes(subject)}
            for name, subject in subjects.items()
        }
        self.m.verify_source_digests(records, subjects)
        subjects["jsonrpc_spec"] += b"\n"
        with self.assertRaisesRegex(self.m.SubjectError, "digest"):
            self.m.verify_source_digests(records, subjects)

    def test_committed_subject_paths_must_be_unique(self):
        with tempfile.TemporaryDirectory() as tmp:
            copied = Path(tmp) / "pack"
            shutil.copytree(ROOT, copied)
            provenance_path = copied / "PROVENANCE.json"
            provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
            provenance["sources"]["jsonrpc_spec"]["subject"] = dict(
                provenance["sources"]["mcp_overview"]["subject"]
            )
            provenance_path.write_text(json.dumps(provenance), encoding="utf-8")

            with self.assertRaisesRegex(self.m.PackError, "unique"):
                self.m._validate_provenance(copied)

    def test_bounded_reader_accepts_exact_limit_and_rejects_limit_plus_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            subject = Path(tmp) / "subject"
            subject.write_bytes(b"x" * 8)
            self.assertEqual(
                self.m.read_bounded(subject, 8, self.m.SubjectError),
                b"x" * 8,
            )
            subject.write_bytes(b"x" * 9)
            with self.assertRaisesRegex(self.m.SubjectError, "size limit"):
                self.m.read_bounded(subject, 8, self.m.SubjectError)

    def test_disagreement_between_source_and_generated_schema_fails_closed(self):
        with self.assertRaises(self.m.SubjectError):
            self.m.reassess_subjects(
                _mcp_typescript(require_id=True, allow_null=True),
                _mcp_schema(),
                MCP_OVERVIEW_SUBJECT,
                JSONRPC_SUBJECT,
            )

    def test_missing_mcp_jsonrpc_requirement_is_not_inferred(self):
        with self.assertRaises(self.m.SubjectError):
            self.m.reassess_subjects(
                _mcp_typescript(),
                _mcp_schema(),
                b"<html><body>No universal protocol statement.</body></html>",
                JSONRPC_SUBJECT,
            )

    def test_unrecognized_subject_fails_instead_of_guessing(self):
        with self.assertRaises(self.m.SubjectError):
            self.m.reassess_subjects(
                b"not a TypeScript schema",
                b"{}",
                b"<html>different MCP document</html>",
                b"<html>different document</html>",
            )

    def test_cli_reproduce_is_machine_readable(self):
        stdout = _BoundedTextBuffer()
        stderr = _BoundedTextBuffer()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            returncode = self.m.main(["reproduce", "--root", str(ROOT)])
        self.assertEqual(returncode, 0, stderr.getvalue())
        self.assertEqual(json.loads(stdout.getvalue())["status"], "contradiction")


if __name__ == "__main__":
    unittest.main()
