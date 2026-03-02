#!/usr/bin/env python3
import argparse
import json
import math
import os
import re
from pathlib import Path
from typing import Optional


MODE_LINE_RE = re.compile(r"^ABLATION_MODE=(.+)\s*$", re.MULTILINE)
SIDECAR_LINE_RE = re.compile(r"^SIDECAR=(enabled|disabled)\s*$", re.MULTILINE)
SIDECAR_NUM_RE = re.compile(r"^SEQUENCE_SIDECAR=([01])\s*$", re.MULTILINE)


def wilson(successes, total, z=1.96):
    if total == 0:
        return None
    phat = successes / total
    denom = 1 + z * z / total
    centre = phat + z * z / (2 * total)
    radius = z * math.sqrt((phat * (1 - phat) + z * z / (4 * total)) / total)
    return {
        "low": round((centre - radius) / denom, 4),
        "high": round((centre + radius) / denom, 4),
    }


def percentile(values, pct):
    if not values:
        return None
    ordered = sorted(values)
    idx = max(0, min(len(ordered) - 1, round((pct / 100) * (len(ordered) - 1))))
    return round(ordered[idx], 3)


def load_records(paths):
    records = []
    for path in paths:
        with Path(path).open(encoding="utf-8") as handle:
            for lineno, line in enumerate(handle, start=1):
                line = line.strip()
                if line:
                    if line in {"{", "}"} or not line.startswith("{"):
                        continue
                    try:
                        records.append(json.loads(line))
                    except json.JSONDecodeError as exc:
                        raise RuntimeError(f"{path}:{lineno}: invalid JSONL line: {line!r}") from exc
    return records


def read_optional_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def extract_mode(log_text: str) -> str:
    if not log_text:
        return os.environ.get("ABLATION_MODE", "unknown")
    match = MODE_LINE_RE.search(log_text)
    if match:
        return match.group(1).strip()
    return os.environ.get("ABLATION_MODE", "unknown")


def extract_sidecar_enabled(log_text: str) -> Optional[bool]:
    if not log_text:
        value = os.environ.get("SEQUENCE_SIDECAR")
        if value == "1":
            return True
        if value == "0":
            return False
        return None

    match = SIDECAR_LINE_RE.search(log_text)
    if match:
        return match.group(1) == "enabled"

    match = SIDECAR_NUM_RE.search(log_text)
    if match:
        return match.group(1) == "1"

    value = os.environ.get("SEQUENCE_SIDECAR")
    if value == "1":
        return True
    if value == "0":
        return False
    return None


def select_condition_mode(records, log_text: str) -> str:
    values = sorted({r.get("ablation_mode") for r in records if r.get("ablation_mode")})
    if len(values) == 1:
        return values[0]
    if values:
        return ",".join(values)
    return extract_mode(log_text)


def select_condition_sidecar(records, log_text: str) -> Optional[bool]:
    values = {r.get("sequence_sidecar_enabled") for r in records if r.get("sequence_sidecar_enabled") is not None}
    if len(values) == 1:
        return values.pop()
    if values:
        return None
    return extract_sidecar_enabled(log_text)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("inputs", nargs="+")
    args = parser.parse_args()

    records = load_records(args.inputs)
    attack = [r for r in records if r["scenario"] == "attack"]
    legit = [r for r in records if r["scenario"] == "legit"]
    baseline = [r for r in records if r["mode"] == "baseline"]
    protected = [r for r in records if r["mode"] == "protected"]
    protected_attack = [r for r in attack if r["mode"] == "protected"]
    protected_legit = [r for r in legit if r["mode"] == "protected"]
    all_latencies = [lat for r in records for lat in r.get("latencies_ms", [])]
    artifact_dir = Path(args.inputs[0]).resolve().parent if args.inputs else Path(".")
    baseline_log = read_optional_text(artifact_dir / "baseline.log")
    protected_log = read_optional_text(artifact_dir / "protected.log")

    baseline_mode = select_condition_mode(baseline, baseline_log)
    protected_mode = select_condition_mode(protected, protected_log)
    baseline_sidecar_enabled = select_condition_sidecar(baseline, baseline_log)
    protected_sidecar_enabled = select_condition_sidecar(protected, protected_log)
    protected_wrap_policies = sorted({r.get("wrap_policy") for r in protected if r.get("wrap_policy")})
    protected_sequence_policy_files = sorted({r.get("sequence_policy_file") for r in protected if r.get("sequence_policy_file")})

    baseline_attack_successes = sum(1 for r in attack if r["mode"] == "baseline" and r["attack_success"])
    protected_blocks = sum(1 for r in protected_attack if r["blocked_by_sequence"] or r["blocked_by_wrap"])
    protected_misses = sum(1 for r in protected_attack if not (r["blocked_by_sequence"] or r["blocked_by_wrap"]))
    protected_fp = sum(1 for r in protected_legit if r["false_positive"])

    summary = {
        "runs_total": len(records),
        "attack_runs": len(attack),
        "legit_runs": len(legit),
        "ablation_mode": protected_mode if protected_mode != "unknown" else baseline_mode,
        "protected_sequence_sidecar_enabled": protected_sidecar_enabled,
        "baseline_asr": round(baseline_attack_successes / len([r for r in attack if r["mode"] == "baseline"]), 4) if any(r["mode"] == "baseline" for r in attack) else None,
        "protected_tpr": round(protected_blocks / len(protected_attack), 4) if protected_attack else None,
        "protected_fnr": round(protected_misses / len(protected_attack), 4) if protected_attack else None,
        "protected_false_positive_rate": round(protected_fp / len(protected_legit), 4) if protected_legit else None,
        "baseline_asr_ci": wilson(baseline_attack_successes, len([r for r in attack if r["mode"] == "baseline"])),
        "protected_tpr_ci": wilson(protected_blocks, len(protected_attack)),
        "protected_fnr_ci": wilson(protected_misses, len(protected_attack)),
        "protected_false_positive_rate_ci": wilson(protected_fp, len(protected_legit)),
        "tool_latency_p50_ms": percentile(all_latencies, 50),
        "tool_latency_p95_ms": percentile(all_latencies, 95),
        "conditions": {
            "baseline": {
                "mode": baseline_mode,
                "sidecar_enabled": baseline_sidecar_enabled,
                "runs_total": len(baseline),
                "attack_success": sum(1 for r in baseline if r["attack_success"]),
                "blocked_best_effort": sum(1 for r in baseline if r["blocked_by_sequence"] or r["blocked_by_wrap"]),
            },
            "protected": {
                "mode": protected_mode,
                "sidecar_enabled": protected_sidecar_enabled,
                "runs_total": len(protected),
                "attack_success": sum(1 for r in protected if r["attack_success"]),
                "blocked_best_effort": sum(1 for r in protected if r["blocked_by_sequence"] or r["blocked_by_wrap"]),
            },
        },
        "protected_wrap_policies": protected_wrap_policies,
        "protected_sequence_policy_files": protected_sequence_policy_files,
        "records": records,
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
