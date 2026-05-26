#!/usr/bin/env python3
"""Overhead harness for runner-vs-OTel.

Slice 1 measures the local OTel-only workload path. Slice 2 adds the
delegated Arm C Runner capture path. The harness emits
experiment-scoped samples and summaries; the optional BMF file is
derived from the summary and is the only Bencher-shaped artifact.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import re
import shutil
import socket
import subprocess
import tarfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

EXPERIMENT = "runner-vs-otel-overhead-2026-05"
SAMPLE_SCHEMA = "assay.experiment.overhead_sample.v0"
SUMMARY_SCHEMA = "assay.experiment.overhead_summary.v0"
DEFAULT_ARM = "arm-b-otel"
ARM_B = "arm-b-otel"
ARM_C = "arm-c-dual-capture"


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
        "rss_time": clean(rss_time_tool_version()),
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


def directory_size(path: Path) -> int | None:
    if not path.exists():
        return None
    total = 0
    for child in path.rglob("*"):
        if child.is_file():
            total += child.stat().st_size
    return total


def load_archive_health(archive_contents: Path) -> dict[str, Any]:
    health_path = archive_contents / "observation-health.json"
    if not health_path.exists():
        return {
            "kernel_layer": "absent",
            "ringbuf_drops": 0,
            "cgroup_correlation": "unknown",
        }
    payload = json.loads(health_path.read_text(encoding="utf-8"))
    return {
        "kernel_layer": payload.get("kernel_layer", "unknown"),
        "ringbuf_drops": int(payload.get("ringbuf_drops", -1)),
        "cgroup_correlation": payload.get("cgroup_correlation", "unknown"),
    }


def extract_archive(archive: Path, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as tar:
        tar.extractall(destination, filter="data")


def rss_time_tool_version() -> str | None:
    time_path = Path("/usr/bin/time")
    if not time_path.exists():
        return None
    system = platform.system().lower()
    if system == "linux":
        return run_text([str(time_path), "--version"]) or "GNU time"
    if system == "darwin":
        return "/usr/bin/time -l"
    return str(time_path)


def rss_time_prefix() -> list[str]:
    system = platform.system().lower()
    if system == "linux":
        return ["/usr/bin/time", "-v"]
    if system == "darwin":
        return ["/usr/bin/time", "-l"]
    raise RuntimeError(f"unsupported RSS measurement platform: {system}")


def parse_peak_rss_bytes(stderr: str, *, system: str | None = None) -> int | None:
    host = (system or platform.system()).lower()
    if host == "linux":
        match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
        return int(match.group(1)) * 1024 if match else None
    if host == "darwin":
        match = re.search(r"^\s*(\d+)\s+maximum resident set size\s*$", stderr, re.M)
        return int(match.group(1)) if match else None
    return None


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
    arm: str,
    workload_dir: Path,
    iteration: int,
    commit: str,
    versions: dict[str, str | None],
    timeout_seconds: float,
    assay_bin: Path | None = None,
    ebpf_obj: Path | None = None,
    use_sudo: bool = False,
    measure_rss: bool = False,
) -> dict[str, Any]:
    run_id = f"overhead_{arm.replace('-', '_')}_{iteration:03d}"
    run_dir = arm_dir / f"run_{iteration:03d}"
    work_dir = run_dir / "work"
    trace_path = run_dir / "trace.json"
    archive_path = run_dir / "archive.tar.gz"
    archive_contents = run_dir / "archive-contents"
    sdk_log = run_dir / "sdk-events.ndjson"
    run_dir.mkdir(parents=True, exist_ok=True)
    started_at = utc_now()
    if arm == ARM_B:
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
    elif arm == ARM_C:
        if assay_bin is None or ebpf_obj is None:
            raise ValueError("Arm C requires --assay-bin and --ebpf-obj")
        command = [
            str(assay_bin),
            "runner-spike",
            "run",
            "--agent-shim",
            "openai-agents",
            "--kernel-capture",
            "--ebpf",
            str(ebpf_obj),
            "--run-id",
            run_id,
            "--output",
            str(archive_path),
            "--sdk-event-log",
            str(sdk_log),
            "--",
            "node",
            str(workload_dir / "dist/workload.js"),
            "--run-id",
            run_id,
            "--work-dir",
            str(work_dir),
            "--trace-out",
            str(trace_path),
        ]
        if use_sudo:
            command = ["sudo", "-E", "env", f"PATH={os_environ_path()}"] + command
    else:
        raise ValueError(f"unsupported arm: {arm}")

    run_command = rss_time_prefix() + command if measure_rss else command
    start = time.perf_counter()
    try:
        result = subprocess.run(
            run_command,
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
    peak_rss_bytes = parse_peak_rss_bytes(stderr) if measure_rss else None
    if measure_rss and peak_rss_bytes is None and exit_code == 0:
        stderr += "\noverhead harness failed to parse peak RSS from /usr/bin/time output\n"
        exit_code = 125
    (run_dir / "stdout.log").write_text(stdout, encoding="utf-8")
    (run_dir / "stderr.log").write_text(stderr, encoding="utf-8")
    if use_sudo and run_dir.exists():
        subprocess.run(
            ["sudo", "chown", "-R", f"{os_getuid()}:{os_getgid()}", str(run_dir)],
            check=False,
        )

    health = None
    archive_targz = None
    archive_extracted = None
    if archive_path.exists():
        archive_targz = file_size(archive_path)
        extract_archive(archive_path, archive_contents)
        health = load_archive_health(archive_contents)
        archive_extracted = directory_size(archive_contents)

    return {
        "schema": SAMPLE_SCHEMA,
        "experiment": EXPERIMENT,
        "arm": arm,
        "iteration": iteration,
        "host": socket.gethostname(),
        "host_class": host_class(),
        "assay_commit": commit,
        "started_at": started_at,
        "tool_versions": versions,
        "wall_clock_ms": elapsed_ms,
        "peak_rss_bytes": peak_rss_bytes,
        "exit_code": exit_code,
        "health": health,
        "artifact_bytes": {
            "trace_json": file_size(trace_path),
            "archive_targz": archive_targz,
            "archive_extracted": archive_extracted,
        },
    }


def os_environ_path() -> str:
    return os.environ.get("PATH", "")


def os_getuid() -> int:
    return os.getuid()


def os_getgid() -> int:
    return os.getgid()


def is_capture_clean(sample: dict[str, Any]) -> bool:
    health = sample.get("health")
    if health is None:
        return True
    return (
        health.get("kernel_layer") == "complete"
        and health.get("ringbuf_drops") == 0
        and health.get("cgroup_correlation") == "clean"
    )


def summarize(
    samples: list[dict[str, Any]],
    *,
    delegated_workflow_url: str | None,
) -> dict[str, Any]:
    valid = [
        sample
        for sample in samples
        if sample["exit_code"] == 0 and is_capture_clean(sample)
    ]
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
        "arm": first.get("arm", ARM_B),
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
    arm_key = str(summary["arm"]).replace("arm-", "arm_").replace("-", "_")
    prefix = f"runner_vs_otel.{arm_key}"
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
        f"{prefix}.peak_rss_bytes.median": summary["peak_rss_bytes"]["median"],
        f"{prefix}.peak_rss_bytes.max": summary["peak_rss_bytes"]["max"],
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
    parser.add_argument("--arm", choices=[ARM_B, ARM_C], default=ARM_B)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--out-dir", type=Path, default=default_out_dir())
    parser.add_argument("--workload-dir", type=Path, default=default_workload_dir())
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--clean", action="store_true")
    parser.add_argument("--delegated-workflow-url")
    parser.add_argument("--timeout-seconds", type=float, default=300.0)
    parser.add_argument("--assay-bin", type=Path)
    parser.add_argument("--ebpf-obj", type=Path)
    parser.add_argument("--sudo", action="store_true")
    parser.add_argument("--measure-rss", action="store_true")
    args = parser.parse_args(argv)

    if args.iterations < 1:
        parser.error("--iterations must be >= 1")
    if args.clean and args.out_dir.exists():
        shutil.rmtree(args.out_dir)

    ensure_workload_built(args.workload_dir, skip_build=args.skip_build)
    arm_dir = args.out_dir / args.arm
    artifacts_dir = args.out_dir / "artifacts"
    arm_dir.mkdir(parents=True, exist_ok=True)
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    commit = assay_commit()
    versions = tool_versions(args.workload_dir)
    samples = [
        one_sample(
            arm_dir=arm_dir,
            arm=args.arm,
            workload_dir=args.workload_dir,
            iteration=iteration,
            commit=commit,
            versions=versions,
            timeout_seconds=args.timeout_seconds,
            assay_bin=args.assay_bin,
            ebpf_obj=args.ebpf_obj,
            use_sudo=args.sudo,
            measure_rss=args.measure_rss,
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
            "arm": args.arm,
            "trace_json_bytes": [
                sample["artifact_bytes"]["trace_json"] for sample in samples
            ],
        },
    )
    write_json(
        artifacts_dir / "archive-sizes.json",
        {
            "arm": args.arm,
            "archive_targz_bytes": [
                sample["artifact_bytes"]["archive_targz"]
                for sample in samples
                if sample["artifact_bytes"]["archive_targz"] is not None
            ],
            "archive_extracted_bytes": [
                sample["artifact_bytes"]["archive_extracted"]
                for sample in samples
                if sample["artifact_bytes"]["archive_extracted"] is not None
            ],
        },
    )
    write_json(
        artifacts_dir / "rss-sizes.json",
        {
            "arm": args.arm,
            "peak_rss_bytes": [
                sample["peak_rss_bytes"]
                for sample in samples
                if sample["peak_rss_bytes"] is not None
            ],
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
