"""Unit tests for the extract-fixture-paths helper.

Run from repo root:
  python3 -m unittest discover \
    -s docs/experiments/cross-runtime-drift-2026-05/compare \
    -p 'test_*.py'
"""
from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

THIS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(THIS_DIR))

import extract_fixture_paths  # noqa: E402


def _write_tool_calls(path: Path, lines: list[dict]) -> None:
    path.write_text(
        "\n".join(json.dumps(line) for line in lines) + "\n",
        encoding="utf-8",
    )


def _run_main_capturing(argv: list[str]) -> tuple[int, str]:
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = extract_fixture_paths.main(argv)
    return rc, buf.getvalue()


class HappyPathTests(unittest.TestCase):
    def test_two_paths_printed_in_order(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            _write_tool_calls(
                tc,
                [
                    {
                        "seq": 1,
                        "tool": "read_file",
                        "args": {"path": "/workdir/fixture-input.txt"},
                    },
                    {
                        "seq": 2,
                        "tool": "write_file",
                        "args": {
                            "path": "/workdir/fixture-output.txt",
                            "contents": "X",
                        },
                    },
                ],
            )
            rc, out = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 0)
            self.assertEqual(
                out.splitlines(),
                [
                    "/workdir/fixture-input.txt",
                    "/workdir/fixture-output.txt",
                ],
            )


class FailureTests(unittest.TestCase):
    def test_missing_file_returns_2(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc, _ = _run_main_capturing(
                ["--tool-calls", str(Path(tmp) / "nope.ndjson")]
            )
            self.assertEqual(rc, 2)

    def test_too_few_lines_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            _write_tool_calls(
                tc,
                [
                    {
                        "seq": 1,
                        "tool": "read_file",
                        "args": {"path": "/workdir/fixture-input.txt"},
                    },
                ],
            )
            rc, _ = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 3)

    def test_wrong_tool_order_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            _write_tool_calls(
                tc,
                [
                    {
                        "seq": 1,
                        "tool": "write_file",
                        "args": {"path": "/x", "contents": "y"},
                    },
                    {
                        "seq": 2,
                        "tool": "read_file",
                        "args": {"path": "/x"},
                    },
                ],
            )
            rc, _ = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 3)

    def test_malformed_json_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            tc.write_text("{bad json}\n", encoding="utf-8")
            rc, _ = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 3)

    def test_null_args_returns_3(self) -> None:
        """`{"args": null}` must surface as a clean exit 3, not a
        traceback (P2 review on PR #1348)."""
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            _write_tool_calls(
                tc,
                [
                    {"seq": 1, "tool": "read_file", "args": None},
                    {"seq": 2, "tool": "write_file", "args": None},
                ],
            )
            rc, _ = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 3)

    def test_non_object_entry_returns_3(self) -> None:
        """A list entry that is not a JSON object must not crash."""
        with tempfile.TemporaryDirectory() as tmp:
            tc = Path(tmp) / "tool-calls.ndjson"
            tc.write_text("[1, 2, 3]\n42\n", encoding="utf-8")
            rc, _ = _run_main_capturing(["--tool-calls", str(tc)])
            self.assertEqual(rc, 3)


if __name__ == "__main__":
    unittest.main()
