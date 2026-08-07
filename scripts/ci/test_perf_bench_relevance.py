#!/usr/bin/env python3
"""Tests for scripts/ci/perf_bench_relevance.py.

The cases below are the pull requests that actually produced Bencher alerts on
`sw/12xlarge` and `sw/50x400b` while changing no code in `assay-core`'s compilation
unit. They are fixtures, not illustrations: if a future edit to the gate lets any of
them back through, the alert that trained reviewers to ignore Bencher comes back.

Run: python3 scripts/ci/test_perf_bench_relevance.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "perf_bench_relevance.py"

# (name, changed paths, expected store_relevant, expected suite_relevant)
CASES: list[tuple[str, list[str], bool, bool]] = [
    (
        # Rul1an/assay#2119 — alerted +1,782% (sw/12xlarge) and +1,960% (sw/50x400b).
        # assay-core does not depend on assay-runner-schema.
        "pr2119-runner-schema-only",
        [
            "crates/assay-runner-schema/src/claim_parity.rs",
            "crates/assay-runner-schema/src/lib.rs",
            "crates/assay-runner-schema/tests/claim_support_parity.rs",
            "scripts/ci/assay_runner_gating_map.txt",
        ],
        False,
        True,
    ),
    (
        # Rul1an/assay#2114 — alerted +166% and +352%. assay-mcp-server depends on
        # assay-core, not the reverse; the store bench cannot see it.
        "pr2114-mcp-server-only",
        [
            "crates/assay-mcp-server/src/tools/check_sequence.rs",
            "crates/assay-mcp-server/tests/sequence_eval_parity.rs",
        ],
        False,
        True,
    ),
    (
        "pr2074-monitor-only",
        ["crates/assay-monitor/src/lib.rs", "crates/assay-monitor/src/loader.rs"],
        False,
        True,
    ),
    (
        # A lockfile change moves transitive code into both benchmarks.
        "pr2013-cli-and-lockfile",
        ["Cargo.lock", "crates/assay-cli/src/main.rs"],
        True,
        True,
    ),
    (
        "docs-only",
        ["docs/PERFORMANCE-ASSESSMENT.md", "README.md"],
        False,
        False,
    ),
    (
        # assay-ebpf is in neither closure.
        "ebpf-only",
        ["crates/assay-ebpf/src/lib.rs"],
        False,
        False,
    ),
    (
        "store-crate-itself",
        ["crates/assay-core/src/storage/store.rs"],
        True,
        True,
    ),
    (
        # assay-common is a leaf both closures contain.
        "shared-leaf-crate",
        ["crates/assay-common/src/lib.rs"],
        True,
        True,
    ),
    (
        "bench-source-itself",
        ["crates/assay-core/benches/store_write_heavy.rs"],
        True,
        True,
    ),
    (
        # Editing the thresholds must re-measure both, or the change ships unverified.
        "threshold-config-change",
        [".github/workflows/perf_main.yml"],
        True,
        True,
    ),
]


def run(paths: list[str]) -> dict[str, bool]:
    proc = subprocess.run(
        [sys.executable, str(SCRIPT)],
        input="\n".join(paths) + "\n",
        capture_output=True,
        text=True,
        check=True,
    )
    out: dict[str, bool] = {}
    for line in proc.stdout.splitlines():
        if "=" in line and line.split("=")[0].endswith("_relevant"):
            key, value = line.split("=", 1)
            out[key] = value == "true"
    return out


def main() -> int:
    failures: list[str] = []
    for name, paths, want_store, want_suite in CASES:
        got = run(paths)
        mismatches = [
            f"{name}: {key}={got.get(key)} want={want}"
            for key, want in (("store_relevant", want_store), ("suite_relevant", want_suite))
            if got.get(key) != want
        ]
        failures.extend(mismatches)
        print(f"  {'FAIL' if mismatches else 'ok  '} {name}")

    if failures:
        print("\nFAILURES:")
        for line in failures:
            print(f"  - {line}")
        return 1
    print(f"\n{len(CASES)} cases passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
