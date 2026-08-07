#!/usr/bin/env python3
"""Decide which Criterion benchmarks a changed-file set can actually affect.

The previous gate matched `^crates/`, which is all 21 workspace crates. That let
`store_write_heavy` — whose compilation unit is `assay-core` and its four workspace
dependencies — run and alert on pull requests touching crates `assay-core` does not
depend on. Rul1an/assay#2119 (`assay-runner-schema` only) alerted at +1,782%;
#2114 (`assay-mcp-server`) and #2074 (`assay-monitor`) alerted the same way. None of
those crates is in `assay-core`'s dependency closure, so none of them contributed a
single instruction to the measured code.

The relevant set is therefore derived from `cargo metadata`, not written down. A
hand-maintained list is a second statement of the dependency graph, and the two
drift; this reads the graph itself.

Usage:
    git diff --name-only BASE...HEAD | scripts/ci/perf_bench_relevance.py

Writes `<bench>_relevant=true|false` lines to stdout (and to $GITHUB_OUTPUT when set).

Fail-open by design: if the graph cannot be read, every benchmark is reported
relevant. A gate that silently stops benchmarking is a worse failure than one that
benchmarks too much.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

# Benchmark id -> the workspace crate whose bench target it is.
BENCHES = {
    "store": "assay-core",  # crates/assay-core/benches/store_write_heavy.rs
    "suite": "assay-cli",  # crates/assay-cli/benches/suite_run_worstcase.rs
}

# Changed paths outside any crate that still change what the benchmarks measure or how
# they are reported. Workspace manifests move dependency versions; the lockfile moves
# transitive code; the workflow moves the thresholds themselves.
GLOBAL_TRIGGERS = (
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".github/workflows/perf_pr.yml",
    ".github/workflows/perf_main.yml",
    "scripts/ci/perf_bench_relevance.py",
)


def workspace_graph() -> tuple[dict[str, set[str]], dict[str, str]]:
    """Return (crate -> workspace deps, crate manifest dir -> crate name)."""
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    root = Path(meta["workspace_root"])
    members = {p["name"]: p for p in meta["packages"]}

    deps: dict[str, set[str]] = {}
    dirs: dict[str, str] = {}
    for name, pkg in members.items():
        # `kind: null` is a normal dependency; build- and dev-dependencies are excluded
        # because they do not contribute code to the benchmarked binary.
        deps[name] = {
            d["name"]
            for d in pkg["dependencies"]
            if d["name"] in members and d.get("kind") is None
        }
        rel = Path(pkg["manifest_path"]).parent.relative_to(root).as_posix()
        dirs[rel] = name
    return deps, dirs


def closure(deps: dict[str, set[str]], root: str) -> set[str]:
    seen: set[str] = set()
    stack = [root]
    while stack:
        name = stack.pop()
        if name in seen or name not in deps:
            continue
        seen.add(name)
        stack.extend(deps[name])
    return seen


def crate_for_path(path: str, dirs: dict[str, str]) -> str | None:
    """Longest matching manifest directory wins, so nested crates resolve correctly."""
    best: tuple[int, str | None] = (-1, None)
    for crate_dir, name in dirs.items():
        if path == crate_dir or path.startswith(crate_dir + "/"):
            if len(crate_dir) > best[0]:
                best = (len(crate_dir), name)
    return best[1]


def main() -> int:
    changed = [line.strip() for line in sys.stdin if line.strip()]

    try:
        deps, dirs = workspace_graph()
    except Exception as exc:  # noqa: BLE001 - fail open, loudly
        print(f"::warning::cargo metadata failed ({exc}); treating all benches as relevant")
        emit({bench: True for bench in BENCHES}, changed, reason="cargo metadata unavailable")
        return 0

    global_hit = [p for p in changed if p in GLOBAL_TRIGGERS]
    touched = {crate_for_path(p, dirs) for p in changed} - {None}

    results: dict[str, bool] = {}
    detail: dict[str, str] = {}
    for bench, root in BENCHES.items():
        unit = closure(deps, root)
        hits = sorted(touched & unit)
        if global_hit:
            results[bench] = True
            detail[bench] = f"workspace-wide change: {', '.join(global_hit)}"
        elif hits:
            results[bench] = True
            detail[bench] = f"changed crates in compilation unit: {', '.join(hits)}"
        else:
            results[bench] = False
            detail[bench] = (
                f"no changed crate is in {root}'s dependency closure "
                f"({len(unit)} crates: {', '.join(sorted(unit))})"
            )

    emit(results, changed, detail=detail)
    return 0


def emit(
    results: dict[str, bool],
    changed: list[str],
    *,
    detail: dict[str, str] | None = None,
    reason: str | None = None,
) -> None:
    lines = [f"{bench}_relevant={str(v).lower()}" for bench, v in sorted(results.items())]
    for line in lines:
        print(line)

    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as fh:
            fh.write("\n".join(lines) + "\n")

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write("## Perf benchmark relevance\n\n")
            if reason:
                fh.write(f"> {reason}\n\n")
            for bench, value in sorted(results.items()):
                why = (detail or {}).get(bench, "")
                fh.write(f"- **{bench}**: `{value}` — {why}\n")
            fh.write("\n<details><summary>changed files</summary>\n\n")
            for path in changed:
                fh.write(f"- `{path}`\n")
            fh.write("\n</details>\n")


if __name__ == "__main__":
    raise SystemExit(main())
