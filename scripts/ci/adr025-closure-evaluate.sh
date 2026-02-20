#!/usr/bin/env bash
set -euo pipefail

SOAK=""
READINESS=""
MANIFEST=""
POLICY="schemas/closure_policy_v1.json"
OUT_JSON=""
OUT_MD=""

usage() {
  echo "Usage: $0 --soak <soak.json> --readiness <nightly_readiness.json> --manifest <manifest.json> --out-json <closure.json> --out-md <closure.md> [--policy <policy.json>]"
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --soak)
      SOAK="$2"
      shift 2
      ;;
    --readiness)
      READINESS="$2"
      shift 2
      ;;
    --manifest)
      MANIFEST="$2"
      shift 2
      ;;
    --policy)
      POLICY="$2"
      shift 2
      ;;
    --out-json)
      OUT_JSON="$2"
      shift 2
      ;;
    --out-md)
      OUT_MD="$2"
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "Unknown arg: $1"
      usage
      ;;
  esac
done

[[ -z "$SOAK" || -z "$READINESS" || -z "$MANIFEST" || -z "$OUT_JSON" || -z "$OUT_MD" ]] && usage

[[ -f "$SOAK" ]] || { echo "Measurement error: missing soak file: $SOAK"; exit 2; }
[[ -f "$READINESS" ]] || { echo "Measurement error: missing readiness file: $READINESS"; exit 2; }
[[ -f "$MANIFEST" ]] || { echo "Measurement error: missing manifest file: $MANIFEST"; exit 2; }
[[ -f "$POLICY" ]] || { echo "Measurement error: missing policy file: $POLICY"; exit 2; }

python3 - <<'PY' "$SOAK" "$READINESS" "$MANIFEST" "$POLICY" "$OUT_JSON" "$OUT_MD"
import json
import sys

soak_p, ready_p, mani_p, pol_p, out_json, out_md = sys.argv[1:]


def die(code, msg):
    print(msg)
    raise SystemExit(code)


try:
    soak = json.load(open(soak_p, "r", encoding="utf-8"))
    readiness = json.load(open(ready_p, "r", encoding="utf-8"))
    manifest = json.load(open(mani_p, "r", encoding="utf-8"))
    policy = json.load(open(pol_p, "r", encoding="utf-8"))
except Exception as e:
    die(2, f"Measurement error: invalid json: {e}")

if soak.get("schema_version") != "soak_report_v1":
    die(2, f"Measurement error: unexpected soak schema_version: {soak.get('schema_version')}")
if readiness.get("schema_version") != "adr025-nightly-readiness-v1":
    die(2, f"Measurement error: unexpected readiness schema_version: {readiness.get('schema_version')}")
if "classifier_version" not in readiness:
    die(2, "Measurement error: readiness missing classifier_version")

policy_version = policy.get("policy_version", "closure_policy_v1")
classifier_expected = str(policy.get("classifier_version", "1"))
score_threshold = float(policy.get("score_threshold", 0.85))

weights = policy.get("weights") or {}
w = {
    "completeness": float(weights.get("completeness", 0.40)),
    "provenance": float(weights.get("provenance", 0.20)),
    "consistency": float(weights.get("consistency", 0.20)),
    "readiness": float(weights.get("readiness", 0.20)),
}
wsum = sum(w.values())
if abs(wsum - 1.0) > 1e-9:
    die(2, f"Measurement error: weights must sum to 1.0 (got {wsum})")

required_signals = [str(x) for x in (policy.get("required_signals") or [])]
captured = []
violations = []
notes = []


def cap(sig, ok: bool, detail=None):
    if ok:
        captured.append(sig)
    return {"id": sig, "status": "present" if ok else "missing", "detail": detail}


signals = {"completeness": [], "provenance": [], "consistency": [], "readiness": []}

signals["completeness"].append(cap("soak.report_present", True))
signals["completeness"].append(cap("readiness.report_present", True))

x = manifest.get("x-assay") or {}
packs = x.get("packs_applied")
maps = x.get("mappings_applied")

packs_ok = isinstance(packs, list) and len(packs) > 0
maps_ok = isinstance(maps, list) and len(maps) > 0

signals["provenance"].append(cap("manifest.packs_applied_present", packs_ok))
signals["provenance"].append(cap("manifest.mappings_applied_present", maps_ok))

ready_classifier = str(readiness.get("classifier_version"))
cons_ok = ready_classifier == classifier_expected
signals["consistency"].append(
    cap("classifier_version_match", cons_ok, f"readiness={ready_classifier}, policy={classifier_expected}")
)
if not cons_ok:
    violations.append(
        {
            "code": "classifier_version_mismatch",
            "message": "readiness classifier_version != policy classifier_version",
            "severity": "error",
        }
    )

rates = readiness.get("rates") or {}


def rate(name):
    v = rates.get(name)
    if not isinstance(v, (int, float)):
        die(2, f"Measurement error: readiness rates.{name} must be number")
    return float(v)


success_rate = rate("success_rate")
contract_fail_rate = rate("contract_fail_rate")
infra_fail_rate = rate("infra_fail_rate")
unknown_rate = rate("unknown_rate")

ready_ok = (
    success_rate >= 0.90
    and contract_fail_rate <= 0.05
    and infra_fail_rate <= 0.01
    and unknown_rate <= 0.05
)
signals["readiness"].append(cap("readiness.within_budgets", ready_ok))
if not ready_ok:
    notes.append("readiness outside v1 budgets")

req_total = len(required_signals)
cap_required = len([s for s in required_signals if s in captured])
gaps = sorted([s for s in required_signals if s not in captured])
completeness_ratio = (cap_required / req_total) if req_total else 1.0

dim_scores = {
    "completeness": completeness_ratio,
    "provenance": 1.0 if (packs_ok and maps_ok) else 0.0,
    "consistency": 1.0 if cons_ok else 0.0,
    "readiness": 1.0 if ready_ok else 0.0,
}

score = (
    dim_scores["completeness"] * w["completeness"]
    + dim_scores["provenance"] * w["provenance"]
    + dim_scores["consistency"] * w["consistency"]
    + dim_scores["readiness"] * w["readiness"]
)

report = {
    "schema_version": "closure_report_v1",
    "report_version": "1",
    "assay_version": "0.0.0-script",
    "inputs": {
        "soak_report": {"schema_version": soak.get("schema_version"), "path": soak_p},
        "readiness": {
            "schema_version": readiness.get("schema_version"),
            "classifier_version": ready_classifier,
            "path": ready_p,
        },
        "manifest": {"path": mani_p},
    },
    "policy": {"policy_version": policy_version, "score_threshold": score_threshold},
    "dimensions": {
        k: {"score": float(dim_scores[k]), "weight": float(w[k]), "signals": signals[k]}
        for k in ("completeness", "provenance", "consistency", "readiness")
    },
    "summary": {
        "required_signals": required_signals,
        "captured_signals": sorted(captured),
        "gaps": gaps,
        "notes": notes,
    },
    "violations": violations,
    "score": float(score),
}

with open(out_json, "w", encoding="utf-8") as f:
    json.dump(report, f, indent=2, sort_keys=True)

md = []
md.append("# ADR-025 Closure Report (v1)")
md.append("")
md.append(f"- score: **{score:.3f}** (threshold {score_threshold:.3f})")
md.append(
    f"- decision: **{'PASS' if score >= score_threshold and not any(v.get('severity')=='error' for v in violations) else 'FAIL'}**"
)
md.append("")
md.append("## Dimensions")
for k in ("completeness", "provenance", "consistency", "readiness"):
    md.append(f"- {k}: {dim_scores[k]:.3f} (weight {w[k]:.2f})")
md.append("")
md.append("## Gaps")
if gaps:
    for g in gaps:
        md.append(f"- {g}")
else:
    md.append("- none")
md.append("")
md.append("## Violations")
if violations:
    for v in violations:
        md.append(f"- [{v.get('severity','warn')}] {v['code']}: {v['message']}")
else:
    md.append("- none")

with open(out_md, "w", encoding="utf-8") as f:
    f.write("\n".join(md) + "\n")

hard_error = any(v.get("severity") == "error" for v in violations)
if hard_error:
    raise SystemExit(1)
if score < score_threshold:
    raise SystemExit(1)
raise SystemExit(0)
PY
