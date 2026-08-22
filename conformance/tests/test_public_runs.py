#!/usr/bin/env python3
"""Public-run projection contract. First RED is a stored-record byte flip.

    python3 -W error::ResourceWarning conformance/tests/test_public_runs.py
"""

from __future__ import annotations

import ast
import contextlib
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))
sys.path.insert(0, str(REPO / "conformance/adequacy"))
sys.path.insert(0, str(REPO / "conformance/privileged-mcp-action-v0/scripts"))

HOSTED_SHA256 = (
    "9275ac65b1f2dde89299fcc811c733096b3b5683cb5ed15a8f32560d4580ae27"
)
HOSTED_DIGEST = "sha256:" + HOSTED_SHA256
COPIED = (
    "conformance/public-runs.json",
    "conformance/IMPLEMENTATIONS.md.in",
    "conformance/IMPLEMENTATIONS.md",
    "conformance/implementations.json",
    f"conformance/public-runs/{HOSTED_SHA256}",
)


def _sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


@contextlib.contextmanager
def sandbox():
    import project_public_runs as project

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        for rel in COPIED:
            dst = root / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, dst)
        saved = project.REPO
        project.REPO = root
        try:
            yield root, project
        finally:
            project.REPO = saved


def run_check(project, repo: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(REPO / "conformance/project_public_runs.py"), "--check", "--repo", str(repo)],
        capture_output=True,
        text=True,
        check=False,
    )


def _table_cells(row: str) -> list[str]:
    body = row.strip()
    if body.startswith("|"):
        body = body[1:]
    if body.endswith("|"):
        body = body[:-1]
    return [cell.strip() for cell in body.split(" | ")]


def _rebind_record(root: Path, mutate_payload, mutate_index=None, mutate_registry=None) -> str:
    record = root / "conformance/public-runs" / HOSTED_SHA256
    payload = json.loads(record.read_text())
    mutate_payload(payload)
    mutated = json.dumps(payload, indent=2, sort_keys=True).encode() + b"\n"
    digest = hashlib.sha256(mutated).hexdigest()
    record.unlink()
    (root / "conformance/public-runs" / digest).write_bytes(mutated)
    index_path = root / "conformance/public-runs.json"
    index = json.loads(index_path.read_text())
    index["runs"][0]["record_sha256"] = "sha256:" + digest
    if mutate_index is not None:
        mutate_index(index)
    index_path.write_text(json.dumps(index) + "\n")
    if mutate_registry is not None:
        registry_path = root / "conformance/implementations.json"
        registry = json.loads(registry_path.read_text())
        mutate_registry(registry)
        registry_path.write_text(json.dumps(registry) + "\n")
    return digest


class PublicRunProjection(unittest.TestCase):
    def test_record_byte_flip_fails_check_and_preserves_markdown(self) -> None:
        with sandbox() as (root, project):
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            before = markdown.read_bytes()
            record = root / "conformance/public-runs" / HOSTED_SHA256
            data = bytearray(record.read_bytes())
            data[0] ^= 0x01
            record.write_bytes(bytes(data))
            findings = project.projection_findings(root)
            self.assertTrue(findings, "byte-flipped record must fail --check")
            self.assertTrue(
                any("digest" in item or "sha256" in item for item in findings),
                findings,
            )
            self.assertEqual(markdown.read_bytes(), before)
            completed = run_check(project, root)
            self.assertNotEqual(completed.returncode, 0)
            self.assertEqual(markdown.read_bytes(), before)

    def test_pristine_tree_is_green(self) -> None:
        import project_public_runs as project

        self.assertEqual(project.projection_findings(REPO), [])
        completed = run_check(project, REPO)
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_hosted_record_is_the_exact_validated_bytes(self) -> None:
        import project_public_runs as project
        import validate_run_record

        path = REPO / "conformance/public-runs" / HOSTED_SHA256
        data = path.read_bytes()
        self.assertEqual(_sha256(data), HOSTED_DIGEST)
        self.assertEqual(len(data), 8102)
        report = validate_run_record.load_run_record(path)
        validate_run_record.validate_run_record(report)
        self.assertEqual(report["implementation"]["id"], "pma-v0-repro")

    def test_sorts_by_identity_and_digest_never_score(self) -> None:
        import project_public_runs as project

        low_score_first = {
            "implementation_id": "aaa",
            "record_sha256": "sha256:" + ("ab" * 32),
            "summary": {"match": 0, "mismatch": 14, "execution_error": 0, "harness_error": 0},
        }
        high_score_second = {
            "implementation_id": "bbb",
            "record_sha256": "sha256:" + ("01" * 32),
            "summary": {"match": 14, "mismatch": 0, "execution_error": 0, "harness_error": 0},
        }
        ordered = project.sort_publication_rows([high_score_second, low_score_first])
        self.assertEqual(
            [row["implementation_id"] for row in ordered],
            ["aaa", "bbb"],
        )
        source = ast.parse((REPO / "conformance/project_public_runs.py").read_text(encoding="utf-8"))
        names = {node.id for node in ast.walk(source) if isinstance(node, ast.Name)}
        self.assertIn("sort_publication_rows", names)
        self.assertNotIn("score_percent", ast.dump(source))

    def test_reuses_canonical_loader_validator_and_registry(self) -> None:
        source = (REPO / "conformance/project_public_runs.py").read_text(encoding="utf-8")
        tree = ast.parse(source)
        names = {node.id for node in ast.walk(tree) if isinstance(node, ast.Name)}
        attrs = {node.attr for node in ast.walk(tree) if isinstance(node, ast.Attribute)}
        self.assertIn("validate_run_record", names | attrs)
        self.assertIn("load_implementations", names | attrs)
        self.assertIn("read_regular_file", names | attrs)
        self.assertIn("parse_json_object", names | attrs)
        self.assertNotIn("json.loads", source.replace("parse_json_object", ""))


class PublicRunMutations(unittest.TestCase):
    def test_duplicate_identity_fails_closed(self) -> None:
        with sandbox() as (root, project):
            index = json.loads((root / "conformance/public-runs.json").read_text())
            index["runs"].append(index["runs"][0])
            (root / "conformance/public-runs.json").write_text(json.dumps(index) + "\n")
            findings = project.projection_findings(root)
            self.assertTrue(any("duplicate" in item for item in findings), findings)
            self.assertTrue(any(HOSTED_SHA256 in item for item in findings), findings)

    def test_same_implementation_two_digests_are_kept_and_sorted(self) -> None:
        with sandbox() as (root, project):
            import validate_run_record

            record = root / "conformance/public-runs" / HOSTED_SHA256
            payload = json.loads(record.read_text())
            payload["cases"][0]["review_warnings"] = ["one typed warning"]
            payload["summary"]["review_warnings"] = 1
            mutated = json.dumps(payload, indent=2, sort_keys=True).encode() + b"\n"
            digest = hashlib.sha256(mutated).hexdigest()
            (root / "conformance/public-runs" / digest).write_bytes(mutated)
            report = validate_run_record.load_run_record(root / "conformance/public-runs" / digest)
            validate_run_record.validate_run_record(report)
            index_path = root / "conformance/public-runs.json"
            index = json.loads(index_path.read_text())
            second = dict(index["runs"][0])
            second["record_sha256"] = "sha256:" + digest
            index["runs"] = [second, index["runs"][0]]
            index_path.write_text(json.dumps(index) + "\n")
            try:
                rows = project.load_publication(root)
            except ValueError as exc:
                self.fail("same implementation with two digests must load, got %s" % exc)
            self.assertEqual(len(rows), 2)
            self.assertEqual({row["implementation_id"] for row in rows}, {"pma-v0-repro"})
            digests = [row["record_sha256"] for row in rows]
            self.assertEqual(set(digests), {HOSTED_DIGEST, "sha256:" + digest})
            self.assertEqual(digests, sorted(digests))

    def test_missing_and_surplus_record_fail_closed(self) -> None:
        with sandbox() as (root, project):
            extra = root / "conformance/public-runs" / ("cd" * 32)
            extra.write_bytes(b'{"schema":"x"}')
            findings = project.projection_findings(root)
            self.assertTrue(any("surplus" in item for item in findings), findings)
            extra.unlink()
            (root / "conformance/public-runs" / HOSTED_SHA256).unlink()
            findings = project.projection_findings(root)
            self.assertTrue(any("missing" in item for item in findings), findings)

    def test_v0_and_unknown_schema_fail_closed(self) -> None:
        with sandbox() as (root, project):
            record = root / "conformance/public-runs" / HOSTED_SHA256
            payload = json.loads(record.read_text())
            payload["schema"] = "assay.privileged_mcp_action.conformance_run.v0"
            mutated = json.dumps(payload, indent=2, sort_keys=True).encode() + b"\n"
            record.write_bytes(mutated)
            index = json.loads((root / "conformance/public-runs.json").read_text())
            index["runs"][0]["record_sha256"] = _sha256(mutated)
            (root / "conformance/public-runs.json").write_text(json.dumps(index) + "\n")
            record.rename(root / "conformance/public-runs" / hashlib.sha256(mutated).hexdigest())
            findings = project.projection_findings(root)
            self.assertTrue(any("unsupported schema" in item for item in findings), findings)

    def test_identity_image_source_commit_mismatch_fails_closed(self) -> None:
        with sandbox() as (root, project):
            index = json.loads((root / "conformance/public-runs.json").read_text())
            for field, value in (
                ("implementation_id", "not-pma-v0-repro"),
                ("image", "ghcr.io/example/x@sha256:" + ("ab" * 32)),
                ("source", "https://example.invalid/other"),
                ("commit", "0" * 40),
            ):
                mutated = json.loads(json.dumps(index))
                mutated["runs"][0][field] = value
                (root / "conformance/public-runs.json").write_text(json.dumps(mutated) + "\n")
                findings = project.projection_findings(root)
                self.assertTrue(any("mismatch" in item for item in findings), findings)

    def test_stale_generated_markdown_fails_without_write(self) -> None:
        with sandbox() as (root, project):
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            before = markdown.read_bytes()
            markdown.write_bytes(before + b"\n# stale\n")
            findings = project.projection_findings(root)
            self.assertTrue(any("IMPLEMENTATIONS.md" in item for item in findings), findings)
            completed = run_check(project, root)
            self.assertNotEqual(completed.returncode, 0)
            self.assertEqual(markdown.read_bytes(), before + b"\n# stale\n")

    def test_validator_bypass_is_visible(self) -> None:
        with sandbox() as (root, project):
            with mock.patch.object(project, "validate_run_record", side_effect=AssertionError("bypass")):
                with self.assertRaises(AssertionError):
                    project.load_publication(root)

    def test_index_path_hostile_inputs_fail_before_render(self) -> None:
        with sandbox() as (root, project):
            index = root / "conformance/public-runs.json"
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            before = markdown.read_bytes()
            index.write_bytes(b'{"schema":1e999}\n')
            with self.assertRaises(ValueError):
                project.load_publication(root)
            self.assertEqual(markdown.read_bytes(), before)
            index.write_bytes(b'{"schema":"x","schema":"y"}\n')
            with self.assertRaises(ValueError):
                project.load_publication(root)
            shutil.copy2(REPO / "conformance/public-runs.json", index)
            hidden = root / "hidden-index.json"
            shutil.copy2(index, hidden)
            index.unlink()
            index.symlink_to(hidden)
            with self.assertRaises(ValueError):
                project.load_publication(root)
            index.unlink()
            shutil.copy2(REPO / "conformance/public-runs.json", index)
            if hasattr(os, "mkfifo"):
                index.unlink()
                os.mkfifo(index)
                with self.assertRaises(ValueError):
                    project.load_publication(root)
                index.unlink()
                shutil.copy2(REPO / "conformance/public-runs.json", index)
            index.write_bytes(b"{" + (b"a" * 65535) + b"}")
            with self.assertRaises(ValueError) as oversize:
                project.load_publication(root)
            self.assertIn("exceeds", str(oversize.exception))

    def test_write_document_replaces_with_exact_render_bytes(self) -> None:
        with sandbox() as (root, project):
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            markdown.write_bytes(b"stale-prior-bytes\n")
            project.write_document(root)
            expected = project.render_document(root, project.load_publication(root)).encode("utf-8")
            self.assertEqual(markdown.read_bytes(), expected)

    def test_write_document_failures_keep_prior_bytes_and_clean_temp(self) -> None:
        with sandbox() as (root, project):
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            prior = b"keep-these-bytes\n"
            markdown.write_bytes(prior)

            real_named = tempfile.NamedTemporaryFile

            def leaking_write(*args, **kwargs):
                handle = real_named(*args, **kwargs)
                handle.write = lambda _data: (_ for _ in ()).throw(OSError("injected write failure"))
                return handle

            with mock.patch.object(project.tempfile, "NamedTemporaryFile", side_effect=leaking_write):
                with self.assertRaises(OSError):
                    project.write_document(root)
            self.assertEqual(markdown.read_bytes(), prior)
            leftovers = [
                path.name
                for path in (root / "conformance").iterdir()
                if path.name.startswith("IMPLEMENTATIONS.md.") and path.name != "IMPLEMENTATIONS.md.in"
            ]
            self.assertEqual(leftovers, [])

            markdown.write_bytes(prior)
            with mock.patch.object(project.os, "replace", side_effect=OSError("injected replace failure")):
                with self.assertRaises(OSError):
                    project.write_document(root)
            self.assertEqual(markdown.read_bytes(), prior)
            leftovers = [
                path.name
                for path in (root / "conformance").iterdir()
                if path.name.startswith("IMPLEMENTATIONS.md.") and path.name != "IMPLEMENTATIONS.md.in"
            ]
            self.assertEqual(leftovers, [])

    def test_valid_review_warnings_are_projected(self) -> None:
        with sandbox() as (root, project):
            import validate_run_record

            def add_warning(payload: dict) -> None:
                payload["cases"][0]["review_warnings"] = ["one typed warning"]
                payload["summary"]["review_warnings"] = 1

            digest = _rebind_record(root, add_warning)
            report = validate_run_record.load_run_record(root / "conformance/public-runs" / digest)
            validate_run_record.validate_run_record(report)
            self.assertEqual(report["summary"]["review_warnings"], 1)
            rows = project.load_publication(root)
            self.assertEqual(rows[0]["summary"]["review_warnings"], 1)
            document = project.render_document(root, rows)
            self.assertIn("| review_warnings |", document)
            row = next(line for line in document.splitlines() if "pma-v0-repro" in line)
            cells = _table_cells(row)
            self.assertEqual(len(cells), 11)
            self.assertEqual(cells[-1], "1")

    def test_bound_image_markup_stays_one_cell(self) -> None:
        with sandbox() as (root, project):
            image = (
                "ghcr.io/rul1an/pma|<v0>@sha256:"
                "88a5ef285a80dc0caeb2b11093eba79f8f08c870b549cafc395aa77ee5ffc493"
            )

            def set_image(payload: dict) -> None:
                payload["implementation"]["image"] = image

            def set_index_image(index: dict) -> None:
                index["runs"][0]["image"] = image

            def set_registry_image(registry: dict) -> None:
                registry["implementations"][0]["image"] = image

            _rebind_record(root, set_image, set_index_image, set_registry_image)
            rows = project.load_publication(root)
            table = project.render_table(rows)
            cells = _table_cells(table.splitlines()[0])
            self.assertEqual(len(cells), 11)
            self.assertIn("\\|", cells[3])
            self.assertIn("&lt;", cells[3])
            self.assertIn("&gt;", cells[3])
            self.assertEqual(project.escape_markdown_cell("a|b<c>d"), "a\\|b&lt;c&gt;d")

    def test_valid_reproduction_mode_swap_fails_closed(self) -> None:
        with sandbox() as (root, project):
            import validate_run_record

            def swap_mode(payload: dict) -> None:
                self.assertEqual(payload["implementation"]["reproduction_mode"], "other_disclosed")
                payload["implementation"]["reproduction_mode"] = "blind_from_spec"

            digest = _rebind_record(root, swap_mode)
            report = validate_run_record.load_run_record(root / "conformance/public-runs" / digest)
            validate_run_record.validate_run_record(report)
            self.assertEqual(report["implementation"]["reproduction_mode"], "blind_from_spec")
            registry = json.loads((root / "conformance/implementations.json").read_text())
            self.assertEqual(registry["implementations"][0]["reproduction_mode"], "other_disclosed")
            with self.assertRaises(ValueError) as ctx:
                project.load_publication(root)
            self.assertIn("reproduction_mode", str(ctx.exception))

    def test_reproduction_mode_is_projected(self) -> None:
        with sandbox() as (root, project):
            document = project.render_document(root, project.load_publication(root))
            self.assertIn("| reproduction_mode |", document)
            cells = _table_cells(next(line for line in document.splitlines() if "pma-v0-repro" in line))
            self.assertEqual(len(cells), 11)
            self.assertEqual(cells[5], "other_disclosed")

    def test_record_digest_links_to_stored_bytes_and_names_opt_in(self) -> None:
        with sandbox() as (root, project):
            rows = project.load_publication(root)
            document = project.render_document(root, rows)
            self.assertIn(
                "[sha256:%s](public-runs/%s)" % (HOSTED_SHA256, HOSTED_SHA256),
                document,
            )
            self.assertIn("python3 conformance/project_public_runs.py --check", document)

    def test_check_does_not_write(self) -> None:
        with sandbox() as (root, project):
            markdown = root / "conformance/IMPLEMENTATIONS.md"
            markdown.write_bytes(b"wrong\n")
            stamp = markdown.stat()
            completed = run_check(project, root)
            self.assertNotEqual(completed.returncode, 0)
            self.assertEqual(markdown.stat().st_mtime_ns, stamp.st_mtime_ns)
            self.assertEqual(markdown.read_bytes(), b"wrong\n")


if __name__ == "__main__":
    unittest.main()
