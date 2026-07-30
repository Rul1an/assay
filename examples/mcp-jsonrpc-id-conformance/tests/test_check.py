import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[1]
MODULE_PATH = ROOT / "check.py"


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

    def test_each_vector_has_the_declared_classification(self):
        expected = {
            "shared-string-id-control": (True, True),
            "mcp-error-with-omitted-id": (True, False),
            "jsonrpc-error-with-null-id": (False, True),
        }
        for vector_path in sorted((ROOT / "vectors").glob("*.json")):
            vector = json.loads(vector_path.read_text(encoding="utf-8"))
            observed = self.m.evaluate_message(vector["message"])
            self.assertEqual(
                (observed["mcp"], observed["jsonrpc"]),
                expected[vector["id"]],
                vector["id"],
            )

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
            try:
                os.symlink(copied / "missing", copied / "broken-link")
                os.symlink(target, copied / "directory-link")
            except OSError as exc:
                self.skipTest(f"symlink creation is unavailable: {exc}")
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
            try:
                os.symlink(external, checksum)
            except OSError as exc:
                self.skipTest(f"symlink creation is unavailable: {exc}")
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
            name: {"sha256": self.m.sha256_bytes(subject)}
            for name, subject in subjects.items()
        }
        self.m.verify_source_digests(records, subjects)
        subjects["jsonrpc_spec"] += b"\n"
        with self.assertRaisesRegex(self.m.SubjectError, "digest"):
            self.m.verify_source_digests(records, subjects)

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
        proc = subprocess.run(
            [sys.executable, str(MODULE_PATH), "reproduce", "--root", str(ROOT)],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(json.loads(proc.stdout)["status"], "contradiction")


if __name__ == "__main__":
    unittest.main()
