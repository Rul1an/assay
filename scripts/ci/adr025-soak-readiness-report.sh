#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   adr025-soak-readiness-report.sh <input_dir> <output_dir>
#
# input_dir should contain:
#   - reports/**.json (soak_report.json payloads)
#   - meta/runs.json (optional) describing run conclusions
#
# output_dir will contain:
#   - nightly_readiness.json
#   - nightly_readiness.md

IN_DIR="${1:-}"
OUT_DIR="${2:-}"

if [[ -z "$IN_DIR" || -z "$OUT_DIR" ]]; then
  echo "Usage: $0 <input_dir> <output_dir>"
  exit 2
fi

mkdir -p "$OUT_DIR"

python3 - <<'PY' "$IN_DIR" "$OUT_DIR"
import json, os, glob, sys, math

in_dir, out_dir = sys.argv[1], sys.argv[2]

reports = sorted(glob.glob(os.path.join(in_dir, "reports", "**", "*.json"), recursive=True))
meta_path = os.path.join(in_dir, "meta", "runs.json")

runs_meta = []
if os.path.exists(meta_path):
    with open(meta_path, "r", encoding="utf-8") as f:
        runs_meta = json.load(f)

# Helper: percentile on a sorted list
def percentile(sorted_vals, p):
    if not sorted_vals:
        return None
    if p <= 0:
        return sorted_vals[0]
    if p >= 100:
        return sorted_vals[-1]
    k = (len(sorted_vals)-1) * (p/100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_vals[int(k)]
    d0 = sorted_vals[f] * (c-k)
    d1 = sorted_vals[c] * (k-f)
    return d0 + d1

durations = []
report_count = 0

# Extract trial duration_ms from soak reports
for path in reports:
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception:
        continue

    if not isinstance(data, dict) or "trials" not in data:
        continue

    report_count += 1
    trials = data.get("trials", [])
    for t in trials:
        ms = t.get("duration_ms") if isinstance(t, dict) else None
        if isinstance(ms, int):
            durations.append(ms)

durations_sorted = sorted(durations)
p50 = percentile(durations_sorted, 50)
p95 = percentile(durations_sorted, 95)

# Classify outcomes from runs_meta if present, else unknown
# Conservative mapping for informational readiness:
#   success -> exit0 bucket
#   failure/timed_out/cancelled -> exit2 bucket
exit0 = exit1 = exit2 = exit3 = unknown = 0
created = []

for r in runs_meta:
    conc = (r.get("conclusion") or "").lower()
    ts = r.get("createdAt")
    if ts:
        created.append(ts)

    if conc == "success":
        exit0 += 1
    elif conc in ("failure", "timed_out", "cancelled"):
        exit2 += 1
    elif conc == "":
        unknown += 1
    else:
        unknown += 1

n = exit0 + exit1 + exit2 + exit3 + unknown

def rate(x):
    return (x / n) if n else 0.0

window_start = min(created) if created else None
window_end = max(created) if created else None

out = {
    "schema_version": "adr025-nightly-readiness-v1",
    "classifier_version": "1",
    "window": {
        "runs_observed": n,
        "window_start": window_start,
        "window_end": window_end,
        "reports_ingested": report_count,
        "trial_samples": len(durations_sorted),
    },
    "rates": {
        "success_rate": rate(exit0),
        "policy_fail_rate": rate(exit1),
        "contract_fail_rate": rate(exit2),
        "infra_fail_rate": rate(exit3),
        "unknown_rate": rate(unknown),
    },
    "latency_ms": {
        "p50": p50,
        "p95": p95,
    },
    "notes": [
        "Run conclusions are mapped conservatively for informational readiness.",
        "Refine classification rules in Step3 C3 once data stabilizes.",
    ],
}

json_path = os.path.join(out_dir, "nightly_readiness.json")
with open(json_path, "w", encoding="utf-8") as f:
    json.dump(out, f, indent=2, sort_keys=True)

md_path = os.path.join(out_dir, "nightly_readiness.md")
lines = []
lines.append("# ADR-025 Nightly Readiness (informational)")
lines.append("")
lines.append(f"- Runs observed: **{n}**")
lines.append(f"- Reports ingested: **{report_count}**")
lines.append(f"- Trial samples: **{len(durations_sorted)}**")
lines.append("")
lines.append("## Rates")
lines.append(f"- success_rate: {out['rates']['success_rate']:.3f}")
lines.append(f"- policy_fail_rate: {out['rates']['policy_fail_rate']:.3f}")
lines.append(f"- contract_fail_rate: {out['rates']['contract_fail_rate']:.3f}")
lines.append(f"- infra_fail_rate: {out['rates']['infra_fail_rate']:.3f}")
lines.append(f"- unknown_rate: {out['rates']['unknown_rate']:.3f}")
lines.append("")
lines.append("## Latency (trial duration_ms)")
lines.append(f"- p50: {p50 if p50 is not None else 'n/a'}")
lines.append(f"- p95: {p95 if p95 is not None else 'n/a'}")
lines.append("")
lines.append("## Window")
lines.append(f"- start: {window_start or 'n/a'}")
lines.append(f"- end: {window_end or 'n/a'}")
lines.append("")
lines.append("## Notes")
for note in out["notes"]:
    lines.append(f"- {note}")

with open(md_path, "w", encoding="utf-8") as f:
    f.write("\n".join(lines) + "\n")

print(f"Wrote {json_path}")
print(f"Wrote {md_path}")
PY
