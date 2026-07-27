#!/usr/bin/env python3
"""Contract tests for the clean-room conformance activation kit."""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import shlex
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import time
import unittest
from pathlib import Path
from unittest import mock

CORPUS_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = CORPUS_DIR.parents[1]
BUILD_SCRIPT = CORPUS_DIR / "scripts" / "build_clean_room_pack.py"
SCORE_SCRIPT = CORPUS_DIR / "scripts" / "score_candidate.py"
VALIDATE_SCRIPT = CORPUS_DIR / "scripts" / "validate_run_record.py"
SOURCE_COMMIT = "4e9bdfcc4bef83e6935ab9b916b39adf89d4cd01"
IMPLEMENTATION_COMMIT = "1" * 40


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [sys.executable, *args],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and result.returncode != 0:
        raise AssertionError(
            f"command failed ({result.returncode}): {args}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def read_archive(path: Path) -> dict[str, bytes]:
    with tarfile.open(path, "r:gz") as archive:
        return {
            member.name: archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile()
        }


def read_bundle(bundle: bytes) -> tuple[dict, list[dict], bytes]:
    with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
        members = {member.name: member for member in archive.getmembers()}
        manifest = json.load(archive.extractfile(members["manifest.json"]))
        events_bytes = archive.extractfile(members["events.ndjson"]).read()
    return manifest, [json.loads(line) for line in events_bytes.splitlines()], events_bytes


class CleanRoomPackTests(unittest.TestCase):
    def build(self, output: Path) -> None:
        run(
            str(BUILD_SCRIPT),
            "--repo-root",
            str(REPO_ROOT),
            "--source-commit",
            SOURCE_COMMIT,
            "--output",
            str(output),
        )

    def test_pack_is_deterministic_opaque_and_inputs_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.tar.gz"
            second = Path(tmp) / "second.tar.gz"
            self.build(first)
            self.build(second)

            self.assertEqual(first.read_bytes(), second.read_bytes())
            files = read_archive(first)
            names = sorted(files)
            self.assertIn("privileged-mcp-action-v0/spec.md", names)
            self.assertIn("privileged-mcp-action-v0/descriptor.json", names)
            self.assertIn("privileged-mcp-action-v0/cases.json", names)
            self.assertIn("privileged-mcp-action-v0/README.md", names)

            case_names = [
                name
                for name in names
                if name.startswith("privileged-mcp-action-v0/cases/")
            ]
            self.assertEqual(len(case_names), 14)
            self.assertTrue(
                all(
                    Path(name).name.startswith("case-")
                    and Path(name).name.endswith(".bundle.tar.gz")
                    for name in case_names
                )
            )

            joined_names = "\n".join(names)
            for forbidden in (
                "MANIFEST.json",
                "gen_vectors.py",
                "crates/",
                "ok-",
                "bad-",
            ):
                self.assertNotIn(forbidden, joined_names)

            cases = json.loads(files["privileged-mcp-action-v0/cases.json"])
            self.assertEqual(
                cases["source_corpus_digest"],
                "sha256:cb58ce91863f52e0568742b977f0642158453ec11bbcd25821f9171dccd03342",
            )
            self.assertRegex(cases["rendered_set_digest"], r"^sha256:[0-9a-f]{64}$")
            self.assertEqual(cases["declared_source_commit"], SOURCE_COMMIT)
            self.assertEqual(cases["case_count"], 14)
            self.assertNotIn("expected", json.dumps(cases))
            self.assertNotIn("description", json.dumps(cases))

            manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
            source_hashes = {vector["sha256"] for vector in manifest["vectors"]}
            packed_hashes = {
                sha256(files[f"privileged-mcp-action-v0/{case['file']}"])
                for case in cases["cases"]
            }
            self.assertTrue(packed_hashes.isdisjoint(source_hashes))

            for case in cases["cases"]:
                bundle = files[f"privileged-mcp-action-v0/{case['file']}"]
                with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
                    self.assertEqual(
                        [member.name for member in archive.getmembers()],
                        ["manifest.json", "events.ndjson"],
                    )
                    inner = b"".join(
                        archive.extractfile(member).read()
                        for member in archive.getmembers()
                        if member.isfile()
                    )
                self.assertNotIn(b"pmav0-ok-", inner)
                self.assertNotIn(b"pmav0-bad-", inner)

            public_inputs = b"\n".join(
                data for name, data in files.items() if "/cases/" not in name
            )
            for forbidden in (
                b"gen_vectors.py",
                b"first_failure_informative",
                b"ok-005",
                b"bad-105",
                b"bad-108",
            ):
                self.assertNotIn(forbidden, public_inputs)

    def test_archive_metadata_is_normalized(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "pack.tar.gz"
            self.build(output)
            with gzip.GzipFile(fileobj=io.BytesIO(output.read_bytes())) as stream:
                tar_bytes = stream.read()
            with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
                for member in archive.getmembers():
                    self.assertEqual(member.mtime, 0)
                    self.assertEqual(member.uid, 0)
                    self.assertEqual(member.gid, 0)
                    self.assertEqual(member.uname, "")
                    self.assertEqual(member.gname, "")
            files = read_archive(output)
            for name, bundle in files.items():
                if "/cases/" not in name:
                    continue
                with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
                    for member in archive.getmembers():
                        self.assertEqual(member.mtime, 0)
                        self.assertEqual(member.uid, 0)
                        self.assertEqual(member.gid, 0)
                        self.assertEqual(member.uname, "")
                        self.assertEqual(member.gname, "")

    def test_rendering_changes_only_stream_identity_and_preserves_integrity_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "pack.tar.gz"
            self.build(output)
            files = read_archive(output)
            cases = json.loads(files["privileged-mcp-action-v0/cases.json"])["cases"]
            manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
            vectors = sorted(manifest["vectors"], key=lambda vector: vector["sha256"])

            for case, vector in zip(cases, vectors, strict=True):
                source = (CORPUS_DIR / vector["file"]).read_bytes()
                rendered = files[f"privileged-mcp-action-v0/{case['file']}"]
                source_manifest, source_events, source_event_bytes = read_bundle(source)
                rendered_manifest, rendered_events, rendered_event_bytes = read_bundle(rendered)

                source_clean = (
                    source_manifest["files"]["events.ndjson"]["sha256"]
                    == sha256(source_event_bytes)
                    and source_manifest["files"]["events.ndjson"]["bytes"]
                    == len(source_event_bytes)
                )
                rendered_clean = (
                    rendered_manifest["files"]["events.ndjson"]["sha256"]
                    == sha256(rendered_event_bytes)
                    and rendered_manifest["files"]["events.ndjson"]["bytes"]
                    == len(rendered_event_bytes)
                )
                self.assertEqual(rendered_clean, source_clean, case["id"])

                self.assertEqual(len(rendered_events), len(source_events))
                for original, opaque in zip(source_events, rendered_events, strict=True):
                    original = dict(original)
                    opaque = dict(opaque)
                    original.pop("id")
                    original.pop("assayrunid")
                    opaque.pop("id")
                    opaque.pop("assayrunid")
                    self.assertEqual(opaque, original, case["id"])

                source_manifest = dict(source_manifest)
                rendered_manifest = dict(rendered_manifest)
                source_manifest.pop("run_id")
                rendered_manifest.pop("run_id")
                source_manifest["files"] = dict(source_manifest["files"])
                rendered_manifest["files"] = dict(rendered_manifest["files"])
                source_manifest["files"].pop("events.ndjson")
                rendered_manifest["files"].pop("events.ndjson")
                self.assertEqual(rendered_manifest, source_manifest, case["id"])

    def test_source_bundle_rejects_surplus_or_oversize_members(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from pack_format import bundle_files, deterministic_tar_gz
        finally:
            sys.path.pop(0)

        manifest = b'{"files":{"events.ndjson":{"bytes":0,"sha256":"sha256:x"}}}\n'
        with self.subTest(reason="surplus"):
            bundle = deterministic_tar_gz(
                {
                    "manifest.json": manifest,
                    "events.ndjson": b"",
                    "surplus": b"x",
                },
                preserve_order=True,
            )
            with self.assertRaisesRegex(ValueError, "surplus"):
                bundle_files(bundle)

        with self.subTest(reason="oversize"):
            bundle = deterministic_tar_gz(
                {
                    "manifest.json": manifest,
                    "events.ndjson": b"x" * (8 * 1024 * 1024 + 1),
                },
                preserve_order=True,
            )
            with self.assertRaisesRegex(ValueError, "exceeds"):
                bundle_files(bundle)

    def test_stream_identity_rewrite_rejects_duplicate_sequences(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from pack_format import deterministic_tar_gz, rewrite_bundle_stream_identity
        finally:
            sys.path.pop(0)

        cases = (
            (
                (
                    b'{"assayrunid":"source","assayseq":1,"id":"source:first"}\n'
                    b'{"assayrunid":"source","assayseq":1,"id":"source:second"}\n'
                ),
                "collide on assayseq 1",
            ),
            (
                b'{"assayrunid":"source","assayseq":"1","id":"source:string"}\n',
                "requires integer assayseq values",
            ),
            (
                b'{"assayrunid":"source","assayseq":[1],"id":"source:list"}\n',
                "requires integer assayseq values",
            ),
        )
        for events, error in cases:
            with self.subTest(error=error):
                manifest = {
                    "run_id": "source",
                    "files": {
                        "events.ndjson": {
                            "bytes": len(events),
                            "sha256": sha256(events),
                        }
                    },
                }
                bundle = deterministic_tar_gz(
                    {
                        "manifest.json": (
                            json.dumps(
                                manifest,
                                separators=(",", ":"),
                                sort_keys=True,
                            ).encode()
                            + b"\n"
                        ),
                        "events.ndjson": events,
                    },
                    preserve_order=True,
                )
                with self.assertRaisesRegex(ValueError, error):
                    rewrite_bundle_stream_identity(bundle, "pmav0-case-001")


class CandidateScorerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tmp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.tmp.name)
        cls.pack = cls.root / "pack.tar.gz"
        run(
            str(BUILD_SCRIPT),
            "--repo-root",
            str(REPO_ROOT),
            "--source-commit",
            SOURCE_COMMIT,
            "--output",
            str(cls.pack),
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.tmp.cleanup()

    def candidate(
        self,
        mode: str,
        *,
        oracle_to_rewrite: Path | None = None,
        pack_to_mutate: Path | None = None,
    ) -> Path:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        pack_files = read_archive(self.pack)
        cases = json.loads(pack_files["privileged-mcp-action-v0/cases.json"])["cases"]
        vectors = sorted(manifest["vectors"], key=lambda vector: vector["sha256"])
        expected_by_sha = {
            case["sha256"]: vector["expected"]
            for case, vector in zip(cases, vectors, strict=True)
        }
        script = self.root / f"candidate-{mode}.py"
        script.write_text(
            textwrap.dedent(
                f"""\
                import hashlib
                import json
                from pathlib import Path
                import subprocess
                import sys

                expected = {expected_by_sha!r}
                data = Path(sys.argv[1]).read_bytes()
                digest = "sha256:" + hashlib.sha256(data).hexdigest()
                result = dict(expected[digest])
                mode = {mode!r}
                oracle_to_rewrite = {str(oracle_to_rewrite) if oracle_to_rewrite else None!r}
                pack_to_mutate = {str(pack_to_mutate) if pack_to_mutate else None!r}
                if mode == "rewrite-oracle":
                    manifest_path = Path(oracle_to_rewrite)
                    manifest = json.loads(manifest_path.read_text())
                    for vector in manifest["vectors"]:
                        vector["expected"] = {{"bundle_integrity": "fail"}}
                    manifest_path.write_text(json.dumps(manifest))
                    result = {{"bundle_integrity": "fail"}}
                if mode == "mutate-pack":
                    pack_path = Path(pack_to_mutate)
                    pack_path.write_bytes(pack_path.read_bytes() + b"x")
                if mode == "mismatch" and result.get("verdict") == "valid":
                    result["verdict"] = "invalid"
                    result.pop("claims", None)
                if mode == "malformed":
                    print("not json")
                    raise SystemExit(2)
                if mode == "oversize-integer":
                    sys.stdout.write('{{"attacker_number":' + "9" * 5000 + '}}')
                    raise SystemExit(0)
                if mode == "flood":
                    subprocess.Popen([
                        sys.executable,
                        "-c",
                        "import pathlib,time; time.sleep(0.5); "
                        "pathlib.Path({str(self.root / 'escaped-child')!r}).write_text('escaped')",
                    ])
                    sys.stdout.write("x" * (2 * 1024 * 1024))
                    raise SystemExit(2)
                report = {{
                    "schema": "assay.privileged_mcp_action.verify.report.v0",
                    "profile": "privileged-mcp-action/v0",
                    "non_claims": [
                        "allow does not prove upstream delivery",
                        "deny does not establish maliciousness",
                        "caller-visible denial does not prove external side-effect absence",
                        "bundle integrity does not upgrade source class",
                    ],
                    **result,
                }}
                if result.get("bundle_integrity") == "pass" and result.get("verdict") == "invalid":
                    if mode != "reasonless":
                        report["findings"] = [{{"detail": "candidate explanation"}}]
                else:
                    report["findings"] = []
                if mode == "utf16":
                    sys.stdout.buffer.write(json.dumps(report).encode("utf-16"))
                    raise SystemExit(0)
                print(json.dumps(report))
                if mode == "trailing":
                    print("second document")
                """
            )
        )
        return script

    def score(
        self,
        candidate: Path,
        output: Path,
        *,
        pack: Path | None = None,
        manifest: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return run(
            str(SCORE_SCRIPT),
            "--pack",
            str(pack or self.pack),
            "--manifest",
            str(manifest or CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            shlex.join([sys.executable, str(candidate)]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )

    def test_matching_candidate_scores_all_cases(self) -> None:
        output = self.root / "report.json"
        result = self.score(self.candidate("match"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"], {
            "total": 14,
            "match": 14,
            "mismatch": 0,
            "execution_error": 0,
            "harness_error": 0,
            "review_warnings": 0,
        })
        self.assertEqual(
            report["source_corpus_digest"],
            "sha256:cb58ce91863f52e0568742b977f0642158453ec11bbcd25821f9171dccd03342",
        )
        self.assertRegex(report["rendered_set_digest"], r"^sha256:[0-9a-f]{64}$")
        self.assertEqual(
            {case["case_id"] for case in report["cases"]},
            {f"case-{index:03d}" for index in range(1, 15)},
        )
        self.assertEqual(
            report["implementation"]["reproduction_mode"],
            "blind_from_spec",
        )
        self.assertEqual(
            report["pack_provenance_verification"],
            "not_performed_by_scorer",
        )
        self.assertEqual(run(str(VALIDATE_SCRIPT), str(output)).returncode, 0)
        self.assertNotIn("ok-", output.read_text())
        self.assertNotIn("bad-", output.read_text())

        report["summary"]["match"] -= 1
        tampered = self.root / "tampered-report.json"
        tampered.write_text(json.dumps(report))
        self.assertEqual(
            run(str(VALIDATE_SCRIPT), str(tampered), check=False).returncode,
            2,
        )

    def test_normative_mismatch_fails(self) -> None:
        output = self.root / "mismatch.json"
        result = self.score(self.candidate("mismatch"), output)
        self.assertEqual(result.returncode, 1)
        report = json.loads(output.read_text())
        self.assertGreater(report["summary"]["mismatch"], 0)

    def test_malformed_trailing_or_flooded_output_is_execution_error(self) -> None:
        for mode in ("malformed", "oversize-integer", "trailing", "utf16", "flood"):
            with self.subTest(mode=mode):
                output = self.root / f"{mode}.json"
                result = self.score(self.candidate(mode), output)
                self.assertEqual(result.returncode, 2)
                report = json.loads(output.read_text())
                self.assertGreater(report["summary"]["execution_error"], 0)
                if mode == "flood" and os.name == "posix":
                    time.sleep(2)
                    self.assertFalse((self.root / "escaped-child").exists())

    def test_manifest_pack_desynchronization_is_harness_error(self) -> None:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        manifest["vectors"] = manifest["vectors"][:-1]
        manifest_root = self.root / "canonical"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest))
        output = self.root / "desync.json"

        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(manifest_path),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )

        self.assertEqual(result.returncode, 2)
        report = json.loads(output.read_text())
        self.assertGreater(report["summary"]["harness_error"], 0)
        self.assertEqual(report["summary"]["execution_error"], 0)
        self.assertTrue(report["harness_errors"])
        self.assertEqual(
            report["summary"]["harness_error"],
            sum(case["status"] == "harness_error" for case in report["cases"]),
        )

    def test_global_harness_diagnostic_is_not_a_case_status(self) -> None:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        manifest["corpus_digest"] = "sha256:" + "0" * 64
        manifest_root = self.root / "canonical-global-diagnostic"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest))
        output = self.root / "global-diagnostic.json"

        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(manifest_path),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )

        self.assertEqual(result.returncode, 2)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"]["match"], 14)
        self.assertEqual(report["summary"]["harness_error"], 0)
        self.assertEqual(len(report["harness_errors"]), 1)
        self.assertEqual(run(str(VALIDATE_SCRIPT), str(output)).returncode, 0)

    def test_run_record_rejects_non_object_cases_and_impossible_observed_shapes(self) -> None:
        output = self.root / "validator-source.json"
        result = self.score(self.candidate("match"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        clean = json.loads(output.read_text())
        mutations = {
            "non-object-case": lambda report: report["cases"].__setitem__(0, "not-an-object"),
            "empty-observed": lambda report: report["cases"][0].__setitem__("observed", {}),
            "relative-source": lambda report: report["implementation"].__setitem__(
                "source", "./verifier"
            ),
            "wrong-harness-count": lambda report: report["summary"].__setitem__(
                "harness_error", 1
            ),
            "boolean-exit-code": lambda report: report["cases"][0].__setitem__(
                "exit_code", True
            ),
            "boolean-summary-count": lambda report: report["summary"].__setitem__(
                "mismatch", False
            ),
            "replaced-non-claims": lambda report: report.__setitem__(
                "non_claims",
                ["certifies security", "certifies compliance", "certifies provider outcomes"],
            ),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                report = json.loads(json.dumps(clean))
                mutate(report)
                path = self.root / f"{name}.json"
                path.write_text(json.dumps(report))
                invalid = run(str(VALIDATE_SCRIPT), str(path), check=False)
                self.assertEqual(invalid.returncode, 2, invalid.stderr)

    def test_standalone_run_record_validator_bounds_bytes_and_nesting(self) -> None:
        oversized = self.root / "oversized-run-record.json"
        oversized.write_bytes(b'{"padding":"' + b"x" * (4 * 1024 * 1024) + b'"}')
        too_deep = self.root / "deep-run-record.json"
        too_deep.write_text("[" * 65 + "0" + "]" * 65)

        for path, diagnostic in (
            (oversized, "exceeds 4194304 bytes"),
            (too_deep, "nesting exceeds 64"),
        ):
            with self.subTest(path=path.name):
                result = run(str(VALIDATE_SCRIPT), str(path), check=False)
                self.assertEqual(result.returncode, 2)
                self.assertIn(diagnostic, result.stderr)
                self.assertNotIn("Traceback", result.stderr)

    def test_candidate_cannot_rewrite_oracle_or_report_a_mutated_pack_hash(self) -> None:
        manifest_root = self.root / "oracle-snapshot"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_bytes((CORPUS_DIR / "MANIFEST.json").read_bytes())
        oracle_output = self.root / "oracle-rewrite.json"

        oracle_result = self.score(
            self.candidate("rewrite-oracle", oracle_to_rewrite=manifest_path),
            oracle_output,
            manifest=manifest_path,
        )

        self.assertEqual(oracle_result.returncode, 1)
        oracle_report = json.loads(oracle_output.read_text())
        self.assertGreater(oracle_report["summary"]["mismatch"], 0)
        self.assertEqual(oracle_report["summary"]["harness_error"], 0)

        mutable_pack = self.root / "mutable-pack.tar.gz"
        mutable_pack.write_bytes(self.pack.read_bytes())
        original_pack_hash = sha256(mutable_pack.read_bytes())
        pack_output = self.root / "pack-mutation.json"
        pack_result = self.score(
            self.candidate("mutate-pack", pack_to_mutate=mutable_pack),
            pack_output,
            pack=mutable_pack,
        )

        self.assertEqual(pack_result.returncode, 0, pack_result.stderr)
        self.assertNotEqual(sha256(mutable_pack.read_bytes()), original_pack_hash)
        self.assertEqual(
            json.loads(pack_output.read_text())["pack_sha256"],
            original_pack_hash,
        )

    def test_timeout_kills_candidate_process_group(self) -> None:
        marker = self.root / "escaped-timeout-child"
        candidate = self.root / "candidate-timeout.py"
        candidate.write_text(
            textwrap.dedent(
                f"""\
                import subprocess
                import sys
                import time

                subprocess.Popen([
                    sys.executable,
                    "-c",
                    "import pathlib,time; time.sleep(1.2); "
                    "pathlib.Path({str(marker)!r}).write_text('escaped')",
                ])
                time.sleep(5)
                """
            )
        )
        bundle = self.root / "ignored.bundle"
        bundle.write_bytes(b"ignored")
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from score_candidate import CandidateError, run_candidate
        finally:
            sys.path.pop(0)

        with self.assertRaisesRegex(CandidateError, "timed out"):
            run_candidate([sys.executable, str(candidate)], bundle, 1)
        if os.name == "posix":
            time.sleep(1.5)
            self.assertFalse(marker.exists())

    @unittest.skipUnless(os.name == "posix", "process-group containment requires POSIX")
    def test_leader_exit_still_kills_descendants_holding_capture_pipes(self) -> None:
        marker = self.root / "escaped-after-leader-exit"
        candidate = self.root / "candidate-leader-exits.py"
        candidate.write_text(
            textwrap.dedent(
                f"""\
                import subprocess
                import sys

                subprocess.Popen([
                    sys.executable,
                    "-c",
                    "import pathlib,time; time.sleep(1.2); "
                    "pathlib.Path({str(marker)!r}).write_text('escaped'); time.sleep(5)",
                ])
                """
            )
        )
        bundle = self.root / "ignored-after-leader-exit.bundle"
        bundle.write_bytes(b"ignored")
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from score_candidate import CandidateError, run_candidate
        finally:
            sys.path.pop(0)

        with self.assertRaises(CandidateError):
            run_candidate([sys.executable, str(candidate)], bundle, 5)
        time.sleep(1.5)
        self.assertFalse(marker.exists())

    def test_non_positive_timeout_is_rejected_before_execution(self) -> None:
        output = self.root / "invalid-timeout.json"
        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--timeout-seconds",
            "0",
            "--output",
            str(output),
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("must be a positive integer", result.stderr)
        self.assertFalse(output.exists())

    def test_capture_failure_is_a_harness_error_not_empty_output(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from bounded_process import ProcessCaptureError, run_bounded
        finally:
            sys.path.pop(0)

        with (
            mock.patch(
                "bounded_process._capture_stream",
                side_effect=OSError("synthetic capture failure"),
            ),
            self.assertRaisesRegex(
                ProcessCaptureError,
                r"process output capture failed: stderr, stdout",
            ),
        ):
            run_bounded(
                [sys.executable, "-c", "pass"],
                timeout_seconds=5,
                stdout_limit=1024,
                stderr_limit=1024,
            )

    def test_capture_failure_is_recorded_as_harness_error(self) -> None:
        output = self.root / "capture-harness-error.json"
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            import score_candidate
        finally:
            sys.path.pop(0)

        argv = [
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            "unused-candidate",
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
        ]
        with (
            mock.patch.object(sys, "argv", argv),
            mock.patch.object(
                score_candidate,
                "run_candidate",
                side_effect=score_candidate.HarnessError("synthetic capture failure"),
            ),
        ):
            self.assertEqual(score_candidate.main(), 2)

        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["summary"]["harness_error"], 14)
        self.assertEqual(report["summary"]["execution_error"], 0)
        self.assertTrue(
            all(case["status"] == "harness_error" for case in report["cases"])
        )

    def test_reject_reason_is_visible_but_not_scored(self) -> None:
        output = self.root / "reasonless.json"
        result = self.score(self.candidate("reasonless"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"]["match"], 14)
        self.assertGreater(report["summary"]["review_warnings"], 0)
        self.assertTrue(
            any(
                "reject_reason_missing" in case.get("review_warnings", [])
                for case in report["cases"]
            )
        )

    def test_duplicate_or_surplus_pack_members_fail_closed(self) -> None:
        files = read_archive(self.pack)
        for mode in ("duplicate", "surplus"):
            with self.subTest(mode=mode):
                tampered = self.root / f"{mode}.tar.gz"
                with tarfile.open(tampered, "w:gz") as archive:
                    for name, data in files.items():
                        info = tarfile.TarInfo(name)
                        info.size = len(data)
                        archive.addfile(info, io.BytesIO(data))
                    if mode == "duplicate":
                        name = "privileged-mcp-action-v0/cases.json"
                        data = files[name]
                    else:
                        name = "privileged-mcp-action-v0/answers.txt"
                        data = b"unexpected"
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))

                output = self.root / f"{mode}-report.json"
                result = self.score(self.candidate("match"), output, pack=tampered)
                self.assertEqual(result.returncode, 2)
                self.assertFalse(output.exists())

    def test_truncated_gzip_pack_is_invalid_input_not_a_mismatch(self) -> None:
        truncated = self.root / "truncated.tar.gz"
        data = self.pack.read_bytes()
        truncated.write_bytes(data[: len(data) // 2])
        output = self.root / "truncated-report.json"

        result = self.score(self.candidate("match"), output, pack=truncated)

        self.assertEqual(result.returncode, 2)
        self.assertNotIn("Traceback", result.stderr)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
