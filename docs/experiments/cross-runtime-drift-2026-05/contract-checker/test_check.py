"""Unit tests for the workload contract checker.

Stdlib unittest only. Simulates a workload's WORK_DIR layout and exercises
both the happy path and each individual rule failure mode.

Run: python3 -m unittest contract-checker/test_check.py
or:  python3 -m unittest discover -s contract-checker -p 'test_*.py'
"""
from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

THIS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(THIS_DIR))

import check  # noqa: E402  (after sys.path tweak)


def write_valid_workdir(
    tmpdir: Path,
    *,
    runtime: str = "openai-agents",
    input_contents: str = "cross-runtime drift fixture\n",
    output_contents: str | None = None,
    tool_calls: list[dict] | None = None,
    exit_code: int = 0,
) -> tuple[Path, Path, Path]:
    """Materialize a workdir that should pass all checks by default."""
    input_path = tmpdir / "fixture-input.txt"
    output_path = tmpdir / "fixture-output.txt"
    input_path.write_text(input_contents, encoding="utf-8")
    output_path.write_text(
        output_contents
        if output_contents is not None
        else input_contents.upper(),
        encoding="utf-8",
    )
    if tool_calls is None:
        tool_calls = [
            {"seq": 1, "tool": "read_file", "args": {"path": str(input_path)}},
            {
                "seq": 2,
                "tool": "write_file",
                "args": {
                    "path": str(output_path),
                    "contents": input_contents.upper(),
                },
            },
        ]
    (tmpdir / "tool-calls.ndjson").write_text(
        "\n".join(json.dumps(c) for c in tool_calls) + "\n",
        encoding="utf-8",
    )
    (tmpdir / "run-meta.json").write_text(
        json.dumps(
            {
                "runtime": runtime,
                "model": "test-model",
                "sdk_version": "0.0.0",
                "started_at": "2026-05-25T00:00:00Z",
                "ended_at": "2026-05-25T00:00:01Z",
                "exit_code": exit_code,
            }
        ),
        encoding="utf-8",
    )
    return input_path, output_path, tmpdir


class HappyPathTests(unittest.TestCase):
    def test_openai_runtime_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir, runtime="openai-agents")
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 0)

    def test_gemini_runtime_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir, runtime="gemini-genai")
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 0)


class RuleFailureTests(unittest.TestCase):
    def test_missing_output_file_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir)
            (tmpdir / "fixture-output.txt").unlink()
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_empty_output_file_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir, output_contents="")
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_wrong_case_output_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(
                tmpdir, output_contents="not uppercased\n"
            )
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_three_tool_calls_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            extra_call = {
                "seq": 3,
                "tool": "read_file",
                "args": {"path": str(tmpdir / "fixture-input.txt")},
            }
            write_valid_workdir(tmpdir)
            # Append an extra call to break rule 3.
            with (tmpdir / "tool-calls.ndjson").open(
                "a", encoding="utf-8"
            ) as f:
                f.write(json.dumps(extra_call) + "\n")
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_wrong_first_tool_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            input_path = tmpdir / "fixture-input.txt"
            output_path = tmpdir / "fixture-output.txt"
            bad_calls = [
                {
                    "seq": 1,
                    "tool": "write_file",
                    "args": {
                        "path": str(output_path),
                        "contents": "WHATEVER",
                    },
                },
                {
                    "seq": 2,
                    "tool": "read_file",
                    "args": {"path": str(input_path)},
                },
            ]
            write_valid_workdir(tmpdir, tool_calls=bad_calls)
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_wrong_runtime_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir, runtime="anthropic-claude")
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)

    def test_nonzero_exit_code_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp).resolve()
            write_valid_workdir(tmpdir, exit_code=2)
            rc = check.main(["--work-dir", str(tmpdir)])
            self.assertEqual(rc, 2)


class BadInputTests(unittest.TestCase):
    def test_missing_workdir_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc = check.main(["--work-dir", os.path.join(tmp, "nope")])
            self.assertEqual(rc, 3)


if __name__ == "__main__":
    unittest.main()
