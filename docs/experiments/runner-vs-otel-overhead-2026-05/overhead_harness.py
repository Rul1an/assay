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
PAIRED_SEQUENCE_SCHEMA = "assay.experiment.paired_sequence.v0"
EVENT_RATE_SWEEP_SCHEMA = "assay.experiment.event_rate_sweep.v0"
DEFAULT_ARM = "arm-b-otel"
ARM_A = "arm-a-runner-only"
ARM_B = "arm-b-otel"
ARM_C = "arm-c-dual-capture"
PAIRED_AC = "paired-a-c"
RSS_TIME_PATH = Path("/usr/bin/time")
RSS_SYSTEMS = {"darwin", "linux"}
PHASE_TIMING_KEYS = [
    "preflight_ms",
    "cgroup_prepare_ms",
    "monitor_attach_ms",
    "child_spawn_ms",
    "child_runtime_ms",
    "event_flush_ms",
    "archive_write_ms",
]
SWEEP_RATE_LEVELS = ("baseline", "low", "medium", "high")
SWEEP_KERNEL_EVENT_TARGETS = {
    "baseline": 0,
    "low": 1,
    "medium": 25,
    "high": 100,
}
SWEEP_SPAN_EVENT_TARGETS = {
    "baseline": 0,
    "low": 1,
    "medium": 25,
    "high": 100,
}
SWEEP_PAYLOAD_BYTES = {
    "small": 128,
    "medium": 4096,
    "large": 65536,
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[3]


def default_workload_dir() -> Path:
    return repo_root() / "docs/experiments/runner-vs-otel-2026-05/workload"


def default_runner_fixture_agent() -> Path:
    return repo_root() / "runner-fixtures/openai-agents/fixture-agent.js"


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


def load_phase_timings(path: Path) -> dict[str, float] | None:
    if not path.exists():
        return None
    payload = json.loads(path.read_text(encoding="utf-8"))
    phases = payload.get("phases_ms")
    if not isinstance(phases, dict):
        return None
    parsed: dict[str, float] = {}
    for key, value in phases.items():
        if key in PHASE_TIMING_KEYS and isinstance(value, (int, float)):
            parsed[key] = float(value)
    return parsed or None


def event_rate_sweep_config(args: argparse.Namespace) -> dict[str, Any] | None:
    kernel_level = args.sweep_kernel_event_rate
    span_level = args.sweep_span_event_rate
    concurrency = args.sweep_concurrency
    payload_size = args.sweep_payload_size
    if (
        kernel_level == "baseline"
        and span_level == "baseline"
        and concurrency == 1
        and payload_size == "small"
    ):
        return None
    return {
        "schema": EVENT_RATE_SWEEP_SCHEMA,
        "kernel_event_rate": kernel_level,
        "span_event_rate": span_level,
        "concurrency": concurrency,
        "payload_size": payload_size,
        "target_kernel_events": SWEEP_KERNEL_EVENT_TARGETS[kernel_level],
        "target_span_events": SWEEP_SPAN_EVENT_TARGETS[span_level],
        "payload_bytes": SWEEP_PAYLOAD_BYTES[payload_size],
    }


def sweep_workload_args(event_rate_sweep: dict[str, Any] | None) -> list[str]:
    if event_rate_sweep is None:
        return []
    return [
        "--sweep-kernel-events",
        str(event_rate_sweep["target_kernel_events"]),
        "--sweep-span-events",
        str(event_rate_sweep["target_span_events"]),
        "--sweep-concurrency",
        str(event_rate_sweep["concurrency"]),
        "--sweep-payload-bytes",
        str(event_rate_sweep["payload_bytes"]),
    ]


def event_rate_sweep_for_arm(
    arm: str,
    event_rate_sweep: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if event_rate_sweep is None:
        return None
    if arm != ARM_A:
        return event_rate_sweep
    arm_sweep = dict(event_rate_sweep)
    arm_sweep["span_event_rate"] = "baseline"
    arm_sweep["target_span_events"] = 0
    return arm_sweep


def sweep_env(event_rate_sweep: dict[str, Any] | None) -> dict[str, str]:
    if event_rate_sweep is None:
        return {}
    return {
        "ASSAY_SWEEP_KERNEL_EVENTS": str(event_rate_sweep["target_kernel_events"]),
        "ASSAY_SWEEP_CONCURRENCY": str(event_rate_sweep["concurrency"]),
        "ASSAY_SWEEP_PAYLOAD_BYTES": str(event_rate_sweep["payload_bytes"]),
    }


def phase_residual_ms(sample: dict[str, Any]) -> float | None:
    phases = sample.get("phase_timings_ms")
    if not isinstance(phases, dict):
        return None
    values = [
        float(phases[key])
        for key in PHASE_TIMING_KEYS
        if isinstance(phases.get(key), (int, float))
    ]
    if not values:
        return None
    return float(sample["wall_clock_ms"]) - sum(values)


def extract_archive(archive: Path, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as tar:
        tar.extractall(destination, filter="data")


def rss_time_tool_version() -> str | None:
    time_path = RSS_TIME_PATH
    if not time_path.exists():
        return None
    system = platform.system().lower()
    if system == "linux":
        return run_text([str(time_path), "--version"]) or "GNU time"
    if system == "darwin":
        return "/usr/bin/time -l"
    return str(time_path)


def rss_time_prefix() -> list[str]:
    time_path = RSS_TIME_PATH
    if not time_path.exists():
        raise RuntimeError(f"{time_path} is required for --measure-rss")
    system = platform.system().lower()
    if system == "linux":
        return [str(time_path), "-v"]
    if system == "darwin":
        return [str(time_path), "-l"]
    raise RuntimeError(f"unsupported RSS measurement platform: {system}")


def rss_time_preflight_error(
    *,
    system: str | None = None,
    time_path: Path = RSS_TIME_PATH,
) -> str | None:
    host = (system or platform.system()).lower()
    if host not in RSS_SYSTEMS:
        return f"--measure-rss supports Linux and macOS only, got {host!r}"
    if not time_path.exists():
        return f"--measure-rss requires {time_path}"
    return None


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
    runner_fixture_agent: Path | None = None,
    event_rate_sweep: dict[str, Any] | None = None,
) -> dict[str, Any]:
    sample_event_rate_sweep = event_rate_sweep_for_arm(arm, event_rate_sweep)
    run_id = f"overhead_{arm.replace('-', '_')}_{iteration:03d}"
    run_dir = arm_dir / f"run_{iteration:03d}"
    work_dir = run_dir / "work"
    trace_path = run_dir / "trace.json"
    archive_path = run_dir / "archive.tar.gz"
    archive_contents = run_dir / "archive-contents"
    sdk_log = run_dir / "sdk-events.ndjson"
    phase_timing_path = run_dir / "phase-timing.json"
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
        ] + sweep_workload_args(sample_event_rate_sweep)
    elif arm in {ARM_A, ARM_C}:
        if assay_bin is None or ebpf_obj is None:
            raise ValueError(f"{arm} requires --assay-bin and --ebpf-obj")
        if arm == ARM_A:
            agent_command = [
                "node",
                str(runner_fixture_agent or default_runner_fixture_agent()),
                str(work_dir),
            ]
        else:
            agent_command = [
                "node",
                str(workload_dir / "dist/workload.js"),
                "--run-id",
                run_id,
                "--work-dir",
                str(work_dir),
                "--trace-out",
                str(trace_path),
            ] + sweep_workload_args(sample_event_rate_sweep)
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
            "--phase-timing-log",
            str(phase_timing_path),
            "--",
        ] + agent_command
        if use_sudo:
            command = ["sudo", "-E", "env", f"PATH={os_environ_path()}"] + command
    else:
        raise ValueError(f"unsupported arm: {arm}")

    run_command = rss_time_prefix() + command if measure_rss else command
    env = None
    if measure_rss:
        env = os.environ.copy()
        env["LC_ALL"] = "C"
        env["LANG"] = "C"
    sweep_environment = sweep_env(sample_event_rate_sweep)
    if sweep_environment:
        env = os.environ.copy() if env is None else env
        env.update(sweep_environment)
    start = time.perf_counter()
    try:
        result = subprocess.run(
            run_command,
            cwd=workload_dir,
            env=env,
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
        "phase_timings_ms": load_phase_timings(phase_timing_path),
        "event_rate_sweep": sample_event_rate_sweep,
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
    phase_timings = summarize_phase_timings(valid)
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
        "phase_timings_ms": phase_timings,
        "event_rate_sweep": first.get("event_rate_sweep"),
    }


def summarize_phase_timings(
    samples: list[dict[str, Any]],
) -> dict[str, dict[str, float | None]] | None:
    summary: dict[str, dict[str, float | None]] = {}
    for key in PHASE_TIMING_KEYS:
        values = [
            float(sample["phase_timings_ms"][key])
            for sample in samples
            if isinstance(sample.get("phase_timings_ms"), dict)
            and sample["phase_timings_ms"].get(key) is not None
        ]
        if values:
            summary[key] = {
                "median": median(values),
                "p95": percentile(values, 95),
                "p99": percentile(values, 99),
            }
    return summary or None


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
    phase_timings = summary.get("phase_timings_ms")
    if isinstance(phase_timings, dict):
        for phase, stats in phase_timings.items():
            if not isinstance(stats, dict):
                continue
            for stat in ("median", "p95", "p99"):
                values[f"{prefix}.phase_timings_ms.{phase}.{stat}"] = stats.get(stat)
    return {key: {"value": value} for key, value in values.items() if value is not None}


def write_run_outputs(
    *,
    out_dir: Path,
    arm: str,
    samples: list[dict[str, Any]],
    summary: dict[str, Any],
    artifact_suffix: str = "",
) -> None:
    arm_dir = out_dir / arm
    artifacts_dir = out_dir / "artifacts"
    samples_path = arm_dir / "samples.jsonl"
    samples_path.write_text(
        "".join(json.dumps(sample, sort_keys=True) + "\n" for sample in samples),
        encoding="utf-8",
    )
    write_json(arm_dir / "summary.json", summary)
    (arm_dir / "summary.md").write_text(summary_markdown(summary), encoding="utf-8")
    write_json(
        artifacts_dir / f"trace-sizes{artifact_suffix}.json",
        {
            "arm": arm,
            "trace_json_bytes": [
                sample["artifact_bytes"]["trace_json"] for sample in samples
            ],
        },
    )
    write_json(
        artifacts_dir / f"archive-sizes{artifact_suffix}.json",
        {
            "arm": arm,
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
        artifacts_dir / f"rss-sizes{artifact_suffix}.json",
        {
            "arm": arm,
            "peak_rss_bytes": [
                sample["peak_rss_bytes"]
                for sample in samples
                if sample["peak_rss_bytes"] is not None
            ],
        },
    )
    write_json(
        artifacts_dir / f"phase-timings{artifact_suffix}.json",
        {
            "arm": arm,
            "phase_timings_ms": [
                sample["phase_timings_ms"]
                for sample in samples
                if sample.get("phase_timings_ms") is not None
            ],
        },
    )


def paired_ac_order(pair_index: int) -> list[str]:
    return [ARM_A, ARM_C] if pair_index % 2 else [ARM_C, ARM_A]


def run_paired_ac(args: argparse.Namespace) -> int:
    ensure_workload_built(args.workload_dir, skip_build=args.skip_build)
    artifacts_dir = args.out_dir / "artifacts"
    for arm in (ARM_A, ARM_C):
        (args.out_dir / arm).mkdir(parents=True, exist_ok=True)
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    commit = assay_commit()
    versions = tool_versions(args.workload_dir)
    event_rate_sweep = event_rate_sweep_config(args)
    samples_by_arm: dict[str, list[dict[str, Any]]] = {ARM_A: [], ARM_C: []}
    order: list[dict[str, Any]] = []
    for pair in range(1, args.iterations + 1):
        for arm in paired_ac_order(pair):
            sample = one_sample(
                arm_dir=args.out_dir / arm,
                arm=arm,
                workload_dir=args.workload_dir,
                iteration=pair,
                commit=commit,
                versions=versions,
                timeout_seconds=args.timeout_seconds,
                assay_bin=args.assay_bin,
                ebpf_obj=args.ebpf_obj,
                use_sudo=args.sudo,
                measure_rss=args.measure_rss,
                runner_fixture_agent=args.runner_fixture_agent,
                event_rate_sweep=event_rate_sweep,
            )
            samples_by_arm[arm].append(sample)
            order.append(
                {
                    "pair": pair,
                    "arm": arm,
                    "iteration": pair,
                    "started_at": sample["started_at"],
                    "wall_clock_ms": sample["wall_clock_ms"],
                    "phase_residual_ms": phase_residual_ms(sample),
                    "exit_code": sample["exit_code"],
                }
            )

    summaries = {
        arm: summarize(samples, delegated_workflow_url=args.delegated_workflow_url)
        for arm, samples in samples_by_arm.items()
    }
    bmf: dict[str, dict[str, float | int]] = {}
    for arm, samples in samples_by_arm.items():
        write_run_outputs(
            out_dir=args.out_dir,
            arm=arm,
            samples=samples,
            summary=summaries[arm],
            artifact_suffix=f"-{arm}",
        )
        bmf.update(bmf_export(summaries[arm]))
    write_json(artifacts_dir / "bmf.json", bmf)
    write_json(
        artifacts_dir / "paired-sequence.json",
        {
            "schema": PAIRED_SEQUENCE_SCHEMA,
            "kind": PAIRED_AC,
            "pairing": "counterbalanced-adjacent-pairs",
            "arms": [ARM_A, ARM_C],
            "pairs_per_arm": args.iterations,
            "order": order,
        },
    )

    print(f"wrote {args.out_dir / ARM_A / 'samples.jsonl'}")
    print(f"wrote {args.out_dir / ARM_C / 'samples.jsonl'}")
    print(f"wrote {artifacts_dir / 'paired-sequence.json'}")
    print(f"wrote {artifacts_dir / 'bmf.json'}")
    all_valid = all(
        summary["valid_samples"] == args.iterations for summary in summaries.values()
    )
    return 0 if all_valid else 2


def _md_value(value: Any, *, unit: str = "") -> str:
    if value is None:
        return "`null`"
    if isinstance(value, (float, int)):
        if unit == "bytes" and float(value).is_integer():
            rendered = f"{int(value):,}"
        else:
            rendered = f"{float(value):,.3f}".rstrip("0").rstrip(".")
    else:
        rendered = str(value)
    suffix = f" {unit}" if unit else ""
    return f"`{rendered}{suffix}`"


def tail_ratio_status(value: float | int | None) -> str:
    if value is None:
        return "unknown"
    if value < 1.5:
        return "healthy"
    if value <= 2.0:
        return "warning"
    return "fail"


def summary_markdown(summary: dict[str, Any], *, artifact_name: str | None = None) -> str:
    wall = summary["wall_clock_ms"]
    rss = summary["peak_rss_bytes"]
    artifacts = summary["artifact_bytes"]
    phase_timings = summary.get("phase_timings_ms")
    event_rate_sweep = summary.get("event_rate_sweep")
    tail_ratio = wall["p99_over_median"]
    lines = [
        "## Runner-vs-OTel Overhead Summary",
        "",
        "| Field | Value |",
        "|---|---:|",
        f"| Arm | `{summary['arm']}` |",
        f"| Host class | `{summary['host_class']}` |",
        f"| Kernel | `{summary['kernel']}` |",
        f"| Valid samples | `{summary['valid_samples']}` |",
        f"| Discarded samples | `{summary['discarded_samples']}` |",
        f"| Wall median | {_md_value(wall['median'], unit='ms')} |",
        f"| Wall p95 | {_md_value(wall['p95'], unit='ms')} |",
        f"| Wall p99 | {_md_value(wall['p99'], unit='ms')} |",
        f"| Wall p99/median | {_md_value(tail_ratio)} ({tail_ratio_status(tail_ratio)}) |",
        f"| Peak RSS median | {_md_value(rss['median'], unit='bytes')} |",
        f"| Peak RSS max | {_md_value(rss['max'], unit='bytes')} |",
        f"| Trace JSON median | {_md_value(artifacts['trace_json_median'], unit='bytes')} |",
        f"| Archive .tar.gz median | {_md_value(artifacts['archive_targz_median'], unit='bytes')} |",
        f"| Archive extracted median | {_md_value(artifacts['archive_extracted_median'], unit='bytes')} |",
    ]
    if summary.get("delegated_workflow_url"):
        lines.append(f"| Workflow | {summary['delegated_workflow_url']} |")
    if artifact_name:
        lines.append(f"| Artifact | `{artifact_name}` |")
    if isinstance(event_rate_sweep, dict):
        lines.append(
            "| Event-rate sweep | "
            f"`kernel={event_rate_sweep['kernel_event_rate']}; "
            f"span={event_rate_sweep['span_event_rate']}; "
            f"concurrency={event_rate_sweep['concurrency']}; "
            f"payload={event_rate_sweep['payload_size']}` |"
        )
    if isinstance(phase_timings, dict) and phase_timings:
        lines.extend(
            [
                "",
                "### Phase Timings",
                "",
                "| Phase | Median | p95 | p99 |",
                "|---|---:|---:|---:|",
            ]
        )
        for phase in PHASE_TIMING_KEYS:
            stats = phase_timings.get(phase)
            if not isinstance(stats, dict):
                continue
            lines.append(
                f"| `{phase}` | {_md_value(stats.get('median'), unit='ms')} | "
                f"{_md_value(stats.get('p95'), unit='ms')} | "
                f"{_md_value(stats.get('p99'), unit='ms')} |"
            )
    lines.extend(
        [
            "",
            "> Non-claim: this is a host-class baseline. Do not publish cross-host",
            "> overhead deltas unless compared arms were measured on the same host class.",
            "",
        ]
    )
    return "\n".join(lines)


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--arm",
        choices=[ARM_A, ARM_B, ARM_C, PAIRED_AC],
        default=ARM_B,
    )
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--out-dir", type=Path, default=default_out_dir())
    parser.add_argument("--workload-dir", type=Path, default=default_workload_dir())
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--clean", action="store_true")
    parser.add_argument("--delegated-workflow-url")
    parser.add_argument("--timeout-seconds", type=float, default=300.0)
    parser.add_argument("--assay-bin", type=Path)
    parser.add_argument("--ebpf-obj", type=Path)
    parser.add_argument(
        "--runner-fixture-agent",
        type=Path,
        default=default_runner_fixture_agent(),
    )
    parser.add_argument("--sudo", action="store_true")
    parser.add_argument("--measure-rss", action="store_true")
    parser.add_argument(
        "--sweep-kernel-event-rate",
        choices=SWEEP_RATE_LEVELS,
        default="baseline",
    )
    parser.add_argument(
        "--sweep-span-event-rate",
        choices=SWEEP_RATE_LEVELS,
        default="baseline",
    )
    parser.add_argument("--sweep-concurrency", type=int, default=1)
    parser.add_argument(
        "--sweep-payload-size",
        choices=tuple(SWEEP_PAYLOAD_BYTES),
        default="small",
    )
    args = parser.parse_args(argv)

    if args.iterations < 1:
        parser.error("--iterations must be >= 1")
    if args.sweep_concurrency < 1:
        parser.error("--sweep-concurrency must be >= 1")
    if args.measure_rss:
        rss_error = rss_time_preflight_error()
        if rss_error is not None:
            parser.error(rss_error)
    if args.clean and args.out_dir.exists():
        shutil.rmtree(args.out_dir)
    if args.arm == PAIRED_AC:
        if args.assay_bin is None or args.ebpf_obj is None:
            parser.error(f"--arm {PAIRED_AC} requires --assay-bin and --ebpf-obj")
        return run_paired_ac(args)

    ensure_workload_built(args.workload_dir, skip_build=args.skip_build)
    arm_dir = args.out_dir / args.arm
    artifacts_dir = args.out_dir / "artifacts"
    arm_dir.mkdir(parents=True, exist_ok=True)
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    commit = assay_commit()
    versions = tool_versions(args.workload_dir)
    event_rate_sweep = event_rate_sweep_config(args)
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
            runner_fixture_agent=args.runner_fixture_agent,
            event_rate_sweep=event_rate_sweep,
        )
        for iteration in range(1, args.iterations + 1)
    ]
    summary = summarize(samples, delegated_workflow_url=args.delegated_workflow_url)

    samples_path = arm_dir / "samples.jsonl"
    write_run_outputs(out_dir=args.out_dir, arm=args.arm, samples=samples, summary=summary)
    write_json(artifacts_dir / "bmf.json", bmf_export(summary))

    print(f"wrote {samples_path}")
    print(f"wrote {arm_dir / 'summary.json'}")
    print(f"wrote {arm_dir / 'summary.md'}")
    print(f"wrote {artifacts_dir / 'bmf.json'}")
    if summary["valid_samples"] != args.iterations:
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
