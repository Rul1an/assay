#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-attach}"
POLICY="${POLICY:-schemas/closure_release_policy_v1.json}"
OUT_DIR="${OUT_DIR:-artifacts/adr025-closure}"
CLOSURE_JSON=""

usage() {
  echo "Usage: $0 [--mode off|attach|warn|enforce] [--policy <path>] [--out-dir <dir>] [--closure-json <path>]"
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="$2"
      shift 2
      ;;
    --policy)
      POLICY="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --closure-json)
      CLOSURE_JSON="$2"
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

case "$MODE" in
  off|attach|warn|enforce) ;;
  *)
    echo "Config error: invalid mode '$MODE'"
    exit 2
    ;;
esac

mkdir -p "$OUT_DIR"

warn_or_fail_measurement() {
  local msg="$1"
  if [[ "$MODE" == "warn" ]]; then
    echo "WARN: ${msg} (mode=warn, continuing)"
    return 0
  fi
  echo "Measurement error: ${msg}"
  return 2
}

if [[ "$MODE" == "off" ]]; then
  echo "ADR-025 closure: mode=off (skipping)"
  exit 0
fi

if [[ ! -f "$POLICY" ]]; then
  warn_or_fail_measurement "policy not found: $POLICY"
  exit $?
fi

if [[ -z "$CLOSURE_JSON" ]]; then
  : "${GH_TOKEN:?Missing GH_TOKEN}"

  rid="$(gh run list --workflow "adr025-nightly-closure.yml" --branch main --status success --limit 1 --json databaseId --jq '.[0].databaseId')"
  if [[ -z "$rid" || "$rid" == "null" ]]; then
    warn_or_fail_measurement "could not find successful adr025-nightly-closure run"
    exit $?
  fi

  echo "ADR-025 closure: using run id: $rid"
  gh run download "$rid" -n "adr025-closure-report" -D "$OUT_DIR" || true

  found_json="$(find "$OUT_DIR" -name 'closure_report_v1.json' -print -quit)"
  if [[ -z "$found_json" ]]; then
    warn_or_fail_measurement "missing closure_report_v1.json in downloaded artifact"
    exit $?
  fi
  CLOSURE_JSON="$found_json"
fi

if [[ ! -f "$CLOSURE_JSON" ]]; then
  warn_or_fail_measurement "closure report JSON not found: $CLOSURE_JSON"
  exit $?
fi

python3 - <<'PY' "$MODE" "$POLICY" "$CLOSURE_JSON"
import json
import sys

mode, policy_path, report_path = sys.argv[1:]


def die(code, msg):
    print(msg)
    raise SystemExit(code)


try:
    policy = json.load(open(policy_path, "r", encoding="utf-8"))
except Exception as exc:
    die(2, f"Measurement error: invalid policy json: {exc}")

try:
    report = json.load(open(report_path, "r", encoding="utf-8"))
except Exception as exc:
    die(2, f"Measurement error: invalid closure report json: {exc}")

if report.get("schema_version") != "closure_report_v1":
    die(2, f"Measurement error: unexpected closure schema_version: {report.get('schema_version')}")

score = report.get("score")
if not isinstance(score, (int, float)):
    die(2, "Measurement error: closure report missing numeric score")
score = float(score)

threshold = policy.get("score_threshold")
if threshold is None:
    die(2, "Measurement error: policy missing score_threshold")
try:
    threshold = float(threshold)
except ValueError:
    die(2, f"Measurement error: invalid score_threshold in policy: {threshold}")

classifier_expected = policy.get("classifier_version")
readiness = ((report.get("inputs") or {}).get("readiness") or {})
classifier_found = readiness.get("classifier_version")

if classifier_expected is not None:
    if classifier_found is None:
        die(2, "Measurement error: closure report missing inputs.readiness.classifier_version")
    if str(classifier_found) != str(classifier_expected):
        die(1, f"Policy fail: classifier_version mismatch (report={classifier_found}, policy={classifier_expected})")

violations = report.get("violations") or []
hard_error = any(isinstance(v, dict) and v.get("severity") == "error" for v in violations)

if mode == "enforce":
    if hard_error:
        die(1, "Policy fail: closure report contains error-severity violations")
    if score < threshold:
        die(1, f"Policy fail: closure score {score:.3f} < threshold {threshold:.3f}")
    print(f"Pass: closure score {score:.3f} >= threshold {threshold:.3f}")
    raise SystemExit(0)

if mode == "attach":
    if hard_error or score < threshold:
        print(f"ADR-025 closure: mode=attach score={score:.3f} threshold={threshold:.3f} (non-blocking attach)")
    else:
        print(f"ADR-025 closure: mode=attach score={score:.3f} threshold={threshold:.3f}")
    raise SystemExit(0)

# warn mode
if hard_error or score < threshold:
    print(f"WARN: ADR-025 closure non-pass (mode=warn) score={score:.3f}, threshold={threshold:.3f}")
else:
    print(f"ADR-025 closure: mode=warn pass score={score:.3f}, threshold={threshold:.3f}")
raise SystemExit(0)
PY

found_md="$(find "$OUT_DIR" -name 'closure_report_v1.md' -print -quit || true)"
if [[ -z "$found_md" ]]; then
  echo "NOTE: closure_report_v1.md missing (non-fatal)"
fi

echo "ADR-025 closure: ready in $OUT_DIR"
exit 0
