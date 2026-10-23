#!/usr/bin/env python3
"""Extract the two fixture paths a contract-conforming workload run
recorded in its tool-calls.ndjson.

Used by the cross-runtime-drift-experiment workflow's drift-compare
job to pass the *actual* fixture paths to `drift.py --fixture-path`
and `--path-alias`.
Hardcoding `/tmp/work/fixture-*.txt` would only work for local
testing; live captures put the workdir under
`arm-X-runs/run_arm_X_<ts>_i/workdir/...` (P1 review on PR #1347).

The workload contract guarantees:
  line 1 — read_file  with args.path == WORKLOAD_INPUT_PATH
  line 2 — write_file with args.path == WORKLOAD_OUTPUT_PATH

The contract-checker has already verified this in the per-iteration
step before upload, so reading the two paths here is safe.

Output: two lines on stdout, input path then output path.

Exit codes:
  0 - both paths extracted
  2 - bad CLI args / I/O
  3 - tool-calls.ndjson malformed or missing required calls
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tool-calls",
        required=True,
        type=Path,
        help="Path to tool-calls.ndjson inside a workload's workdir.",
    )
    args = parser.parse_args(argv)

    path: Path = args.tool_calls
    if not path.is_file():
        print(f"tool-calls.ndjson not found: {path}", file=sys.stderr)
        return 2
    try:
        lines = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        print(f"malformed tool-calls.ndjson: {exc}", file=sys.stderr)
        return 3
    if len(lines) < 2:
        print(
            f"tool-calls.ndjson has {len(lines)} entries, expected >= 2",
            file=sys.stderr,
        )
        return 3
    first = lines[0]
    second = lines[1]
    if not isinstance(first, dict) or not isinstance(second, dict):
        print(
            "tool-calls.ndjson entries are not JSON objects",
            file=sys.stderr,
        )
        return 3
    if (
        first.get("tool") != "read_file"
        or second.get("tool") != "write_file"
    ):
        print(
            f"unexpected tool order: {first.get('tool')!r}, "
            f"{second.get('tool')!r}",
            file=sys.stderr,
        )
        return 3
    # `args` is required and must be a JSON object. The contract-checker
    # already enforces this in the per-iteration step, but the helper
    # claims to handle malformed input cleanly so we cannot assume the
    # call site always pre-validated.
    first_args = first.get("args")
    second_args = second.get("args")
    if not isinstance(first_args, dict) or not isinstance(second_args, dict):
        print(
            "tool args missing or not a JSON object",
            file=sys.stderr,
        )
        return 3
    input_path = first_args.get("path")
    output_path = second_args.get("path")
    if not isinstance(input_path, str) or not isinstance(output_path, str):
        print("tool args.path missing or not a string", file=sys.stderr)
        return 3
    print(input_path)
    print(output_path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
