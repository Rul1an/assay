#!/usr/bin/env python3
import argparse
import json
import math
from pathlib import Path


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
            for line in handle:
                line = line.strip()
                if line:
                    records.append(json.loads(line))
    return records


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("inputs", nargs="+")
    args = parser.parse_args()

    records = load_records(args.inputs)
    attack = [r for r in records if r["scenario"] == "attack"]
    legit = [r for r in records if r["scenario"] == "legit"]
    protected_attack = [r for r in attack if r["mode"] == "protected"]
    protected_legit = [r for r in legit if r["mode"] == "protected"]
    all_latencies = [lat for r in records for lat in r.get("latencies_ms", [])]

    baseline_attack_successes = sum(1 for r in attack if r["mode"] == "baseline" and r["attack_success"])
    protected_blocks = sum(1 for r in protected_attack if r["blocked_by_sequence"] or r["blocked_by_wrap"])
    protected_misses = sum(1 for r in protected_attack if not (r["blocked_by_sequence"] or r["blocked_by_wrap"]))
    protected_fp = sum(1 for r in protected_legit if r["false_positive"])

    summary = {
        "runs_total": len(records),
        "attack_runs": len(attack),
        "legit_runs": len(legit),
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
        "records": records,
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
