#!/usr/bin/env bash
set -euo pipefail

SOAK_PATH=""
READINESS_PATH=""
MANIFEST_PATH=""
OUT_JSON=""
OUT_MD=""
READINESS_POLICY_PATH="${READINESS_POLICY_PATH:-schemas/soak_readiness_policy_v1.json}"
SCORE_THRESHOLD="${SCORE_THRESHOLD:-0.80}"
POLICY_VERSION="${POLICY_VERSION:-closure_policy_v1}"
ASSAY_VERSION="${ASSAY_VERSION:-unknown}"

usage() {
  cat <<'USAGE'
Usage:
  adr025-i2-closure-evaluate.sh \
    --soak <path> \
    --readiness <path> \
    [--manifest <path>] \
    [--readiness-policy <path>] \
    [--score-threshold <0..1>] \
    [--policy-version <string>] \
    --out-json <path> \
    --out-md <path>

Exit codes:
  0 = pass
  1 = policy/closure fail
  2 = measurement/contract fail
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --soak)
      SOAK_PATH="$2"
      shift 2
      ;;
    --readiness)
      READINESS_PATH="$2"
      shift 2
      ;;
    --manifest)
      MANIFEST_PATH="$2"
      shift 2
      ;;
    --readiness-policy)
      READINESS_POLICY_PATH="$2"
      shift 2
      ;;
    --score-threshold)
      SCORE_THRESHOLD="$2"
      shift 2
      ;;
    --policy-version)
      POLICY_VERSION="$2"
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
      exit 0
      ;;
    *)
      echo "Measurement error: unknown arg: $1"
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$SOAK_PATH" || -z "$READINESS_PATH" || -z "$OUT_JSON" || -z "$OUT_MD" ]]; then
  echo "Measurement error: missing required args"
  usage
  exit 2
fi

if [[ ! -f "$SOAK_PATH" ]]; then
  echo "Measurement error: soak file not found: $SOAK_PATH"
  exit 2
fi
if [[ ! -f "$READINESS_PATH" ]]; then
  echo "Measurement error: readiness file not found: $READINESS_PATH"
  exit 2
fi
if [[ ! -f "$READINESS_POLICY_PATH" ]]; then
  echo "Measurement error: readiness policy file not found: $READINESS_POLICY_PATH"
  exit 2
fi
if [[ -n "$MANIFEST_PATH" && ! -f "$MANIFEST_PATH" ]]; then
  echo "Measurement error: manifest file not found: $MANIFEST_PATH"
  exit 2
fi

mkdir -p "$(dirname "$OUT_JSON")" "$(dirname "$OUT_MD")"

python3 - <<'PY' "$SOAK_PATH" "$READINESS_PATH" "$MANIFEST_PATH" "$READINESS_POLICY_PATH" "$OUT_JSON" "$OUT_MD" "$SCORE_THRESHOLD" "$POLICY_VERSION" "$ASSAY_VERSION"
import hashlib
import json
import sys
from pathlib import Path

(
    soak_path,
    readiness_path,
    manifest_path,
    readiness_policy_path,
    out_json,
    out_md,
    score_threshold_raw,
    policy_version,
    assay_version,
) = sys.argv[1:]

EXIT_PASS = 0
EXIT_POLICY_FAIL = 1
EXIT_MEASUREMENT_FAIL = 2


def die(msg):
    print(f"Measurement error: {msg}")
    raise SystemExit(EXIT_MEASUREMENT_FAIL)


def load_json(path):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except Exception as exc:
        die(f"failed to parse JSON {path}: {exc}")


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(65536)
            if not chunk:
                break
            h.update(chunk)
    return f"sha256:{h.hexdigest()}"


try:
    score_threshold = float(score_threshold_raw)
except ValueError:
    die(f"invalid --score-threshold: {score_threshold_raw}")
if score_threshold < 0.0 or score_threshold > 1.0:
    die(f"score threshold must be in [0,1], got {score_threshold}")

soak = load_json(soak_path)
readiness = load_json(readiness_path)
readiness_policy = load_json(readiness_policy_path)
manifest = None
if manifest_path:
    manifest = load_json(manifest_path)

if soak.get("schema_version") != "soak_report_v1":
    die(f"soak.schema_version mismatch: {soak.get('schema_version')}")
if readiness.get("schema_version") != "adr025-nightly-readiness-v1":
    die(f"readiness.schema_version mismatch: {readiness.get('schema_version')}")
if "classifier_version" not in readiness:
    die("readiness.classifier_version missing")
if "rates" not in readiness:
    die("readiness.rates missing")
if "window" not in readiness:
    die("readiness.window missing")

policy_classifier = readiness_policy.get("classifier_version")
if policy_classifier is None:
    die("readiness policy missing classifier_version")

# Required/captured model
required_signals = [
    "input.soak_report",
    "input.readiness",
    "manifest.packs_applied",
    "manifest.mappings_applied",
    "manifest.provenance",
]
captured_signals = ["input.soak_report", "input.readiness"]

manifest_root = manifest if isinstance(manifest, dict) else {}
manifest_ext = manifest_root.get("x-assay", {}) if isinstance(manifest_root, dict) else {}

has_packs = isinstance(manifest_ext.get("packs_applied"), list) and len(manifest_ext.get("packs_applied")) > 0
has_mappings = isinstance(manifest_ext.get("mappings_applied"), list) and len(manifest_ext.get("mappings_applied")) > 0
has_provenance = isinstance(manifest_ext.get("provenance"), dict)

if has_packs:
    captured_signals.append("manifest.packs_applied")
if has_mappings:
    captured_signals.append("manifest.mappings_applied")
if has_provenance:
    captured_signals.append("manifest.provenance")

captured_set = set(captured_signals)
gaps = [s for s in required_signals if s not in captured_set]

# Dimension helpers
violations = []


def signal_obj(sig_id, status, detail):
    o = {"id": sig_id, "status": status}
    if detail:
        o["detail"] = detail
    return o


# completeness
completeness_signals = []
for sig in required_signals:
    if sig in captured_set:
        completeness_signals.append(signal_obj(sig, "present", "captured"))
    else:
        completeness_signals.append(signal_obj(sig, "missing", "not found"))
        violations.append({
            "code": "CLSR-MISSING-SIGNAL",
            "message": f"missing required signal: {sig}",
            "severity": "warn",
        })
completeness_score = len(captured_set.intersection(set(required_signals))) / len(required_signals)

# provenance
prov_checks = [
    ("manifest.packs_applied", has_packs, "packs_applied present"),
    ("manifest.mappings_applied", has_mappings, "mappings_applied present"),
    ("manifest.provenance", has_provenance, "provenance present"),
]
provenance_signals = []
prov_present = 0
for sig, ok, detail in prov_checks:
    if ok:
        provenance_signals.append(signal_obj(sig, "present", detail))
        prov_present += 1
    else:
        provenance_signals.append(signal_obj(sig, "missing", "missing manifest extension"))
provenance_score = prov_present / len(prov_checks)

# consistency
consistency_signals = []
consistency_ok = 0
consistency_total = 3

if soak.get("schema_version") == "soak_report_v1":
    consistency_signals.append(signal_obj("soak.schema_version", "present", "soak_report_v1"))
    consistency_ok += 1
else:
    consistency_signals.append(signal_obj("soak.schema_version", "mismatch", str(soak.get("schema_version"))))
    violations.append({
        "code": "CLSR-CONSISTENCY-MISMATCH",
        "message": "soak schema_version mismatch",
        "severity": "error",
    })

if readiness.get("schema_version") == "adr025-nightly-readiness-v1":
    consistency_signals.append(signal_obj("readiness.schema_version", "present", "adr025-nightly-readiness-v1"))
    consistency_ok += 1
else:
    consistency_signals.append(signal_obj("readiness.schema_version", "mismatch", str(readiness.get("schema_version"))))
    violations.append({
        "code": "CLSR-CONSISTENCY-MISMATCH",
        "message": "readiness schema_version mismatch",
        "severity": "error",
    })

if str(readiness.get("classifier_version")) == str(policy_classifier):
    consistency_signals.append(signal_obj("readiness.classifier_version", "present", str(policy_classifier)))
    consistency_ok += 1
else:
    consistency_signals.append(signal_obj(
        "readiness.classifier_version",
        "mismatch",
        f"expected {policy_classifier}, got {readiness.get('classifier_version')}",
    ))
    violations.append({
        "code": "CLSR-CONSISTENCY-MISMATCH",
        "message": "readiness classifier_version mismatch",
        "severity": "error",
    })

consistency_score = consistency_ok / consistency_total

# readiness (derive from readiness rates + threshold policy)
rates = readiness.get("rates", {})
if not isinstance(rates, dict):
    die("readiness.rates must be object")

for req in ["success_rate", "contract_fail_rate", "infra_fail_rate", "unknown_rate"]:
    if req not in rates:
        die(f"readiness.rates.{req} missing")

for req in ["success_rate", "contract_fail_rate", "infra_fail_rate", "unknown_rate"]:
    val = rates[req]
    if not isinstance(val, (int, float)):
        die(f"readiness.rates.{req} must be number")
    if val < 0.0 or val > 1.0:
        die(f"readiness.rates.{req} must be in [0,1], got {val}")

thresholds = readiness_policy.get("thresholds", {})
if not isinstance(thresholds, dict):
    die("readiness policy thresholds must be object")

checks = [
    (
        "readiness.success_rate",
        float(rates["success_rate"]) >= float(thresholds.get("success_rate_min", 0.90)),
        f"{rates['success_rate']} >= {thresholds.get('success_rate_min', 0.90)}",
    ),
    (
        "readiness.contract_fail_rate",
        float(rates["contract_fail_rate"]) <= float(thresholds.get("contract_fail_rate_max", 0.05)),
        f"{rates['contract_fail_rate']} <= {thresholds.get('contract_fail_rate_max', 0.05)}",
    ),
    (
        "readiness.infra_fail_rate",
        float(rates["infra_fail_rate"]) <= float(thresholds.get("infra_fail_rate_max", 0.01)),
        f"{rates['infra_fail_rate']} <= {thresholds.get('infra_fail_rate_max', 0.01)}",
    ),
    (
        "readiness.unknown_rate",
        float(rates["unknown_rate"]) <= float(thresholds.get("unknown_rate_max", 0.05)),
        f"{rates['unknown_rate']} <= {thresholds.get('unknown_rate_max', 0.05)}",
    ),
]

readiness_signals = []
readiness_ok = 0
for sig, ok, detail in checks:
    if ok:
        readiness_signals.append(signal_obj(sig, "present", detail))
        readiness_ok += 1
    else:
        readiness_signals.append(signal_obj(sig, "mismatch", detail))
        violations.append({
            "code": "CLSR-READINESS-THRESHOLD",
            "message": f"threshold failed: {sig} ({detail})",
            "severity": "error",
        })
readiness_score = readiness_ok / len(checks)

weights = {
    "completeness": 0.40,
    "provenance": 0.20,
    "consistency": 0.20,
    "readiness": 0.20,
}
score = (
    completeness_score * weights["completeness"]
    + provenance_score * weights["provenance"]
    + consistency_score * weights["consistency"]
    + readiness_score * weights["readiness"]
)
score = round(score, 6)

report = {
    "schema_version": "closure_report_v1",
    "report_version": "1",
    "assay_version": assay_version,
    "inputs": {
        "soak_report": {
            "schema_version": soak.get("schema_version", ""),
            "digest": digest(soak_path),
            "path": soak_path,
        },
        "readiness": {
            "schema_version": readiness.get("schema_version", ""),
            "classifier_version": str(readiness.get("classifier_version", "")),
            "digest": digest(readiness_path),
            "path": readiness_path,
        },
        "manifest": {
            "digest": digest(manifest_path) if manifest_path else "",
            "path": manifest_path if manifest_path else "",
        },
    },
    "policy": {
        "policy_version": policy_version,
        "score_threshold": score_threshold,
    },
    "dimensions": {
        "completeness": {
            "score": round(completeness_score, 6),
            "weight": weights["completeness"],
            "signals": completeness_signals,
        },
        "provenance": {
            "score": round(provenance_score, 6),
            "weight": weights["provenance"],
            "signals": provenance_signals,
        },
        "consistency": {
            "score": round(consistency_score, 6),
            "weight": weights["consistency"],
            "signals": consistency_signals,
        },
        "readiness": {
            "score": round(readiness_score, 6),
            "weight": weights["readiness"],
            "signals": readiness_signals,
        },
    },
    "summary": {
        "required_signals": required_signals,
        "captured_signals": sorted(captured_signals),
        "gaps": gaps,
        "notes": [
            "I2 Step2 script-first evaluator",
            "OTel bridge deferred to I3",
        ],
    },
    "violations": violations,
    "score": score,
}

# Minimal contract check against schema-required top-level keys for this script version.
for req in ["schema_version", "report_version", "assay_version", "inputs", "policy", "dimensions", "summary", "score"]:
    if req not in report:
        die(f"internal contract error: missing report key {req}")

Path(out_json).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

md_lines = []
md_lines.append("# ADR-025 I2 Closure Report (v1)")
md_lines.append("")
md_lines.append(f"- Score: **{score:.3f}**")
md_lines.append(f"- Threshold: **{score_threshold:.3f}**")
md_lines.append(f"- Violations: **{len(violations)}**")
md_lines.append("")
md_lines.append("## Dimensions")
for key in ["completeness", "provenance", "consistency", "readiness"]:
    d = report["dimensions"][key]
    md_lines.append(f"- {key}: score={d['score']:.3f}, weight={d['weight']:.2f}")
md_lines.append("")
md_lines.append("## Gaps")
if gaps:
    for gap in gaps:
        md_lines.append(f"- {gap}")
else:
    md_lines.append("- none")

Path(out_md).write_text("\n".join(md_lines) + "\n", encoding="utf-8")

hard_readiness_fail = any(v.get("code") == "CLSR-READINESS-THRESHOLD" for v in violations)
hard_consistency_fail = any(v.get("code") == "CLSR-CONSISTENCY-MISMATCH" and v.get("severity") == "error" for v in violations)

if hard_readiness_fail or hard_consistency_fail or score < score_threshold:
    print(f"Policy fail: score={score:.3f}, threshold={score_threshold:.3f}, violations={len(violations)}")
    raise SystemExit(EXIT_POLICY_FAIL)

print(f"Pass: score={score:.3f}, threshold={score_threshold:.3f}, violations={len(violations)}")
raise SystemExit(EXIT_PASS)
PY
