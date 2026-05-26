#!/usr/bin/env python3
"""Local Arm B overhead harness for runner-vs-OTel.

Slice 1 intentionally measures only the local OTel-only workload path.
It emits experiment-scoped samples and summaries; the optional BMF file
is derived from the summary and is the only Bencher-shaped artifact.
"""

from __future__ import annotations

import argparse
import json
import math
import platform
import shutil
import socket
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

EXPERIMENT = "runner-vs-otel-overhead-2026-05"
SAMPLE_SCHEMA = "assay.experiment.overhead_sample.v0"
SUMMARY_SCHEMA = "assay.experiment.overhead_summary.v0"
DEFAULT_ARM = "arm-b-otel"


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def default_workload_dir() -> Path:
    return repo_root() / "docs/experiments/runner-vs-otel-2026-05/workload"


def default_out_dir() -> Path:
    return (
        repo_root()
        / "docs/experiments/runner-vs-otel-2026-05/runs/overhead-2026-05"
    )


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace(
        "+00:00", "Z"
    )


def run_text(command: list[str], cwd: Path | None = None) -> str | None:
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip().splitlines()[0] if result.stdout.strip() else None


def assay_commit() -> str:
    commit = run_text(["git", "rev-parse", "--short=8", "HEAD"], repo_root())
    if not commit:
        raise RuntimeError("could not resolve git commit for overhead sample provenance")
    return commit


def host_class() -> str:
    parts = [
        platform.system().lower() or "unknown",
        platform.machine().lower() or "unknown",
        (platform.release() or "unknown").replace(" ", "_"),
    ]
    return "-".join(part.replace("/", "_") for part in parts)


def tool_versions(workload_dir: Path) -> dict[str, str | None]:
    def clean(value: str | None) -> str | None:
        if value in (None, "", "undefined"):
            return None
        return value

    return {
        "python": platform.python_version(),
        "node": clean(run_text(["node", "--version"])),
        "npm": clean(run_text(["npm", "--version"])),
        "hyperfine": clean(run_text(["hyperfine", "--version"])),
        "time": "python-time.perf_counter",
        "workload_package": clean(
            run_text(["node", "-p", "require('./package.json').version"], workload_dir)
        ),
    }


def ensure_workload_built(workload_dir: Path, *, skip_build: bool) -> None:
    if skip_build:
        return
    if not (workload_dir / "node_modules").exists():
        subprocess.run(
            ["npm", "install", "--no-audit", "--no-fund", "--ignore-scripts"],
            cwd=workload_dir,
            check=True,
        )
    subprocess.run(["npx", "tsc", "-p", "tsconfig.json"], cwd=workload_dir, check=True)


def file_size(path: Path) -> int | None:
    return path.stat().st_size if path.exists() else None


def percentile(values: list[float], pct: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil((pct / 100.0) * len(ordered)) - 1))
    return ordered[index]


def median(values: list[float]) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2


def normalized_exit_code(returncode: int) -> int:
    if returncode < 0:
        return 128 + abs(returncode)
    return returncode


def one_sample(
    *,
    arm_dir: Path,
    workload_dir: Path,
    iteration: int,
    commit: str,
    versions: dict[str, str | None],
    timeout_seconds: float,
) -> dict[str, Any]:
    run_id = f"overhead_arm_b_{iteration:03d}"
    run_dir = arm_dir / f"run_{iteration:03d}"
    work_dir = run_dir / "work"
    trace_path = run_dir / "trace.json"
    run_dir.mkdir(parents=True, exist_ok=True)
    started_at = utc_now()
    command = [
        "node",
        "dist/workload.js",
        "--run-id",
        run_id,
        "--work-dir",
        str(work_dir),
        "--trace-out",
        str(trace_path),
    ]

    start = time.perf_counter()
    try:
        result = subprocess.run(
            command,
            cwd=workload_dir,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_seconds,
        )
        stdout = result.stdout
        stderr = result.stderr
        exit_code = normalized_exit_code(result.returncode)
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout if isinstance(exc.stdout, str) else ""
        stderr = exc.stderr if isinstance(exc.stderr, str) else ""
        stderr += f"\noverhead harness timeout after {timeout_seconds} seconds\n"
        exit_code = 124
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    (run_dir / "stdout.log").write_text(stdout, encoding="utf-8")
    (run_dir / "stderr.log").write_text(stderr, encoding="utf-8")

    return {
        "schema": SAMPLE_SCHEMA,
        "experiment": EXPERIMENT,
        "arm": DEFAULT_ARM,
        "iteration": iteration,
        "host": socket.gethostname(),
        "host_class": host_class(),
        "assay_commit": commit,
        "started_at": started_at,
        "tool_versions": versions,
        "wall_clock_ms": elapsed_ms,
        "peak_rss_bytes": None,
        "exit_code": exit_code,
        "health": None,
        "artifact_bytes": {
            "trace_json": file_size(trace_path),
            "archive_targz": None,
            "archive_extracted": None,
        },
    }


def summarize(
    samples: list[dict[str, Any]],
    *,
    delegated_workflow_url: str | None,
) -> dict[str, Any]:
    valid = [sample for sample in samples if sample["exit_code"] == 0]
    wall = [float(sample["wall_clock_ms"]) for sample in valid]
    rss = [
        int(sample["peak_rss_bytes"])
        for sample in valid
        if sample.get("peak_rss_bytes") is not None
    ]
    trace_sizes = [
        int(sample["artifact_bytes"]["trace_json"])
        for sample in valid
        if sample["artifact_bytes"].get("trace_json") is not None
    ]
    archive_targz = [
        int(sample["artifact_bytes"]["archive_targz"])
        for sample in valid
        if sample["artifact_bytes"].get("archive_targz") is not None
    ]
    archive_extracted = [
        int(sample["artifact_bytes"]["archive_extracted"])
        for sample in valid
        if sample["artifact_bytes"].get("archive_extracted") is not None
    ]
    wall_median = median(wall)
    wall_p99 = percentile(wall, 99)
    first = samples[0] if samples else {}
    return {
        "schema": SUMMARY_SCHEMA,
        "experiment": EXPERIMENT,
        "arm": DEFAULT_ARM,
        "host": first.get("host", socket.gethostname()),
        "host_class": first.get("host_class", host_class()),
        "kernel": platform.release(),
        "assay_commit": first.get("assay_commit", assay_commit()),
        "delegated_workflow_url": delegated_workflow_url,
        "valid_samples": len(valid),
        "discarded_samples": len(samples) - len(valid),
        "wall_clock_ms": {
            "median": wall_median,
            "p95": percentile(wall, 95),
            "p99": wall_p99,
            "p99_over_median": (wall_p99 / wall_median)
            if wall_p99 is not None and wall_median
            else None,
        },
        "peak_rss_bytes": {
            "median": median([float(value) for value in rss]),
            "max": max(rss) if rss else None,
        },
        "artifact_bytes": {
            "trace_json_median": median([float(value) for value in trace_sizes]),
            "archive_targz_median": median([float(value) for value in archive_targz]),
            "archive_extracted_median": median(
                [float(value) for value in archive_extracted]
            ),
        },
    }


def bmf_export(summary: dict[str, Any]) -> dict[str, dict[str, float | int]]:
    prefix = "runner_vs_otel.arm_b"
    values: dict[str, float | int | None] = {
        f"{prefix}.wall_clock_ms.median": summary["wall_clock_ms"]["median"],
        f"{prefix}.wall_clock_ms.p95": summary["wall_clock_ms"]["p95"],
        f"{prefix}.wall_clock_ms.p99": summary["wall_clock_ms"]["p99"],
        f"{prefix}.wall_clock_ms.p99_over_median": summary["wall_clock_ms"][
            "p99_over_median"
        ],
        f"{prefix}.artifact_bytes.trace_json_median": summary["artifact_bytes"][
            "trace_json_median"
        ],
    }
    return {key: {"value": value} for key, value in values.items() if value is not None}


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--out-dir", type=Path, default=default_out_dir())
    parser.add_argument("--workload-dir", type=Path, default=default_workload_dir())
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--clean", action="store_true")
    parser.add_argument("--delegated-workflow-url")
    parser.add_argument("--timeout-seconds", type=float, default=300.0)
    args = parser.parse_args(argv)

    if args.iterations < 1:
        parser.error("--iterations must be >= 1")
    if args.clean and args.out_dir.exists():
        shutil.rmtree(args.out_dir)

    ensure_workload_built(args.workload_dir, skip_build=args.skip_build)
    arm_dir = args.out_dir / DEFAULT_ARM
    artifacts_dir = args.out_dir / "artifacts"
    arm_dir.mkdir(parents=True, exist_ok=True)
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    commit = assay_commit()
    versions = tool_versions(args.workload_dir)
    samples = [
        one_sample(
            arm_dir=arm_dir,
            workload_dir=args.workload_dir,
            iteration=iteration,
            commit=commit,
            versions=versions,
            timeout_seconds=args.timeout_seconds,
        )
        for iteration in range(1, args.iterations + 1)
    ]
    summary = summarize(samples, delegated_workflow_url=args.delegated_workflow_url)

    samples_path = arm_dir / "samples.jsonl"
    samples_path.write_text(
        "".join(json.dumps(sample, sort_keys=True) + "\n" for sample in samples),
        encoding="utf-8",
    )
    write_json(arm_dir / "summary.json", summary)
    write_json(
        artifacts_dir / "trace-sizes.json",
        {
            "arm": DEFAULT_ARM,
            "trace_json_bytes": [
                sample["artifact_bytes"]["trace_json"] for sample in samples
            ],
        },
    )
    write_json(
        artifacts_dir / "archive-sizes.json",
        {
            "arm": DEFAULT_ARM,
            "archive_targz_bytes": [],
            "archive_extracted_bytes": [],
        },
    )
    write_json(artifacts_dir / "bmf.json", bmf_export(summary))

    print(f"wrote {samples_path}")
    print(f"wrote {arm_dir / 'summary.json'}")
    print(f"wrote {artifacts_dir / 'bmf.json'}")
    if summary["valid_samples"] != args.iterations:
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
