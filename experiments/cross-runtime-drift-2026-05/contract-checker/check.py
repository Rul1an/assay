#!/usr/bin/env python3
"""Validate that a cross-runtime-drift workload run satisfied the contract.

See WORKLOAD_CONTRACT.md for the rules enforced here. This script is
stdlib-only by policy. Independent of Runner capture — runs against the
work directory the workload itself produced.

Output:
  Per-rule PASS/FAIL lines are written to stdout. Bad-input messages
  (exit 3) are written to stderr.

Exit codes:
  0 - all rules pass
  2 - one or more rules failed (details on stdout)
  3 - bad inputs (work-dir missing, unreadable JSON, etc.) — message on stderr
"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

ALLOWED_RUNTIMES = {"openai-agents", "gemini-genai"}
DEFAULT_INPUT_CONTENTS = "cross-runtime drift fixture\n"


class BadInputError(Exception):
    """Raised when an artifact is present but unreadable (corrupt JSON,
    decode error, etc.). Surfaced as exit code 3 — the contract cannot be
    evaluated, distinct from a rule failure (exit 2)."""


@dataclass
class CheckResult:
    rule: str
    passed: bool
    detail: str


def _load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise BadInputError(f"{path}: invalid JSON: {exc}") from exc
    except UnicodeDecodeError as exc:
        raise BadInputError(f"{path}: invalid UTF-8: {exc}") from exc


def _load_ndjson(path: Path) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as exc:
        raise BadInputError(f"{path}: invalid UTF-8: {exc}") from exc
    for lineno, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise BadInputError(
                f"{path}:{lineno}: invalid JSON: {exc}"
            ) from exc
    return out


def _result(rule: str, passed: bool, detail: str = "") -> CheckResult:
    return CheckResult(rule=rule, passed=passed, detail=detail)


def _strip_single_final_newline(value: str) -> str:
    """Ignore one terminal newline for model-style variance only.

    The drift experiment cares that the model/tool path uppercased the fixture
    content; it does not use a final line terminator as a drift dimension.
    """

    if value.endswith("\n"):
        return value[:-1]
    return value


def _contents_match(actual: str, expected: str) -> bool:
    return _strip_single_final_newline(actual) == _strip_single_final_newline(
        expected
    )


def check_output_exists(output_path: Path) -> CheckResult:
    if not output_path.is_file():
        return _result(
            "1. fixture-output.txt exists and is non-empty",
            False,
            f"missing or not a file: {output_path}",
        )
    if output_path.stat().st_size == 0:
        return _result(
            "1. fixture-output.txt exists and is non-empty",
            False,
            f"file exists but is empty: {output_path}",
        )
    return _result("1. fixture-output.txt exists and is non-empty", True)


def check_output_uppercased(
    output_path: Path, expected_contents: str
) -> CheckResult:
    actual = output_path.read_text(encoding="utf-8")
    expected = expected_contents.upper()
    if not _contents_match(actual, expected):
        return _result(
            "2. fixture-output.txt equals input uppercased (final newline-insensitive)",
            False,
            f"expected {expected!r}, got {actual!r}",
        )
    return _result(
        "2. fixture-output.txt equals input uppercased (final newline-insensitive)",
        True,
    )


def check_tool_calls_two_lines(
    tool_calls_path: Path,
) -> tuple[CheckResult, list[dict[str, Any]] | None]:
    if not tool_calls_path.is_file():
        return (
            _result(
                "3. tool-calls.ndjson exists, has exactly two lines",
                False,
                f"missing: {tool_calls_path}",
            ),
            None,
        )
    calls = _load_ndjson(tool_calls_path)
    if len(calls) != 2:
        return (
            _result(
                "3. tool-calls.ndjson exists, has exactly two lines",
                False,
                f"expected 2 calls, got {len(calls)}",
            ),
            calls,
        )
    return (
        _result("3. tool-calls.ndjson exists, has exactly two lines", True),
        calls,
    )


def check_first_call_read(
    calls: list[dict[str, Any]], input_path: Path
) -> CheckResult:
    call = calls[0]
    if call.get("tool") != "read_file":
        return _result(
            "4. line 1: read_file with correct path",
            False,
            f"first tool is {call.get('tool')!r}, expected 'read_file'",
        )
    args = call.get("args", {})
    if str(args.get("path")) != str(input_path):
        return _result(
            "4. line 1: read_file with correct path",
            False,
            f"expected path {input_path}, got {args.get('path')}",
        )
    return _result("4. line 1: read_file with correct path", True)


def check_second_call_write(
    calls: list[dict[str, Any]],
    output_path: Path,
    expected_contents: str,
) -> CheckResult:
    call = calls[1]
    if call.get("tool") != "write_file":
        return _result(
            "5. line 2: write_file with correct path + contents",
            False,
            f"second tool is {call.get('tool')!r}, expected 'write_file'",
        )
    args = call.get("args", {})
    if str(args.get("path")) != str(output_path):
        return _result(
            "5. line 2: write_file with correct path + contents",
            False,
            f"expected path {output_path}, got {args.get('path')}",
        )
    actual = str(args.get("contents", ""))
    expected = expected_contents.upper()
    if not _contents_match(actual, expected):
        return _result(
            "5. line 2: write_file with correct path + contents (final newline-insensitive)",
            False,
            f"expected contents {expected!r}, got {actual!r}",
        )
    return _result(
        "5. line 2: write_file with correct path + contents (final newline-insensitive)",
        True,
    )


def check_run_meta(meta_path: Path) -> CheckResult:
    if not meta_path.is_file():
        return _result(
            "6. run-meta.json exists, exit_code=0, runtime allowed",
            False,
            f"missing: {meta_path}",
        )
    # Corrupt JSON bubbles up as BadInputError (exit 3), distinct from
    # rule-violation (exit 2). The "rule failed" case is reserved for
    # well-formed-but-wrong content like a non-allowed runtime string.
    meta = _load_json(meta_path)
    issues: list[str] = []
    if meta.get("exit_code") != 0:
        issues.append(f"exit_code={meta.get('exit_code')!r}")
    if meta.get("runtime") not in ALLOWED_RUNTIMES:
        issues.append(
            f"runtime={meta.get('runtime')!r} not in {sorted(ALLOWED_RUNTIMES)}"
        )
    if issues:
        return _result(
            "6. run-meta.json exists, exit_code=0, runtime allowed",
            False,
            "; ".join(issues),
        )
    return _result(
        "6. run-meta.json exists, exit_code=0, runtime allowed", True
    )


def run_checks(
    work_dir: Path,
    input_path: Path,
    output_path: Path,
    input_contents: str,
) -> list[CheckResult]:
    results: list[CheckResult] = []
    results.append(check_output_exists(output_path))
    if results[-1].passed:
        results.append(check_output_uppercased(output_path, input_contents))
    else:
        results.append(
            _result(
                "2. fixture-output.txt equals input uppercased",
                False,
                "skipped (rule 1 failed)",
            )
        )

    tool_calls_path = work_dir / "tool-calls.ndjson"
    r3, calls = check_tool_calls_two_lines(tool_calls_path)
    results.append(r3)
    if r3.passed and calls is not None:
        results.append(check_first_call_read(calls, input_path))
        results.append(
            check_second_call_write(calls, output_path, input_contents)
        )
    else:
        results.append(
            _result(
                "4. line 1: read_file with correct path",
                False,
                "skipped (rule 3 failed)",
            )
        )
        results.append(
            _result(
                "5. line 2: write_file with correct path + contents",
                False,
                "skipped (rule 3 failed)",
            )
        )

    results.append(check_run_meta(work_dir / "run-meta.json"))
    return results


def render_results(results: Iterable[CheckResult]) -> str:
    lines: list[str] = []
    for r in results:
        status = "PASS" if r.passed else "FAIL"
        detail = f" — {r.detail}" if r.detail else ""
        lines.append(f"[{status}] {r.rule}{detail}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--work-dir",
        required=True,
        type=Path,
        help="Path to the workload's WORK_DIR (where it wrote outputs).",
    )
    parser.add_argument(
        "--input-path",
        type=Path,
        help=(
            "Override expected input path. Defaults to "
            "<work-dir>/fixture-input.txt."
        ),
    )
    parser.add_argument(
        "--output-path",
        type=Path,
        help=(
            "Override expected output path. Defaults to "
            "<work-dir>/fixture-output.txt."
        ),
    )
    parser.add_argument(
        "--input-contents",
        default=DEFAULT_INPUT_CONTENTS,
        help=(
            "Override the expected pre-seeded input contents. Defaults to "
            f"{DEFAULT_INPUT_CONTENTS!r}."
        ),
    )
    args = parser.parse_args(argv)

    # Use .absolute() (not .resolve()) so we do not collapse macOS
    # /var/... -> /private/var/... symlinks. The Node workloads use
    # `path.resolve()`, which also stops at filesystem-symlink boundaries.
    # The two sides must agree, otherwise local mktemp -d runs falsely
    # fail the path-equality rules.
    work_dir: Path = args.work_dir.absolute()
    if not work_dir.is_dir():
        print(f"work-dir is not a directory: {work_dir}", file=sys.stderr)
        return 3

    input_path = (
        args.input_path.absolute()
        if args.input_path
        else work_dir / "fixture-input.txt"
    )
    output_path = (
        args.output_path.absolute()
        if args.output_path
        else work_dir / "fixture-output.txt"
    )

    try:
        results = run_checks(
            work_dir, input_path, output_path, args.input_contents
        )
    except BadInputError as exc:
        print(f"bad input: {exc}", file=sys.stderr)
        return 3
    print(render_results(results))

    if all(r.passed for r in results):
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
