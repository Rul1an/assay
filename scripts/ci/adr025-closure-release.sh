#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-attach}"
POLICY="${POLICY:-schemas/closure_release_policy_v1.json}"
OUT_DIR="${OUT_DIR:-artifacts/adr025-closure}"
CLOSURE_JSON=""

WORKFLOW_NAME="adr025-nightly-closure.yml"
ARTIFACT_NAME="adr025-closure-report"

TEST_MODE="${ASSAY_CLOSURE_RELEASE_TEST_MODE:-0}"
TEST_LOCAL_JSON="${ASSAY_CLOSURE_RELEASE_LOCAL_JSON:-}"
TEST_SIMULATE_MISSING_ARTIFACT="${ASSAY_CLOSURE_RELEASE_SIMULATE_MISSING_ARTIFACT:-0}"

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

echo "ADR-025 closure: mode=$MODE policy=$POLICY out_dir=$OUT_DIR workflow=$WORKFLOW_NAME test_mode=$TEST_MODE"

measurement_issue() {
  local msg="$1"
  case "$MODE" in
    enforce)
      echo "Measurement error: ${msg}"
      return 2
      ;;
    warn)
      echo "WARN: Measurement issue: ${msg} (mode=warn, continuing)"
      return 0
      ;;
    attach)
      echo "ADR-025 closure: mode=attach measurement issue: ${msg} (non-blocking attach)"
      return 0
      ;;
    *)
      echo "Measurement error: ${msg}"
      return 2
      ;;
  esac
}

if [[ "$MODE" == "off" ]]; then
  echo "ADR-025 closure: mode=off (skipping)"
  exit 0
fi

if [[ ! -f "$POLICY" ]]; then
  measurement_issue "policy not found: $POLICY"
  exit $?
fi

if [[ -z "$CLOSURE_JSON" ]]; then
  if [[ "$TEST_MODE" == "1" ]]; then
    if [[ "$TEST_SIMULATE_MISSING_ARTIFACT" == "1" ]]; then
      measurement_issue "simulated missing closure artifact"
      exit $?
    fi

    if [[ -z "$TEST_LOCAL_JSON" ]]; then
      measurement_issue "test mode enabled but ASSAY_CLOSURE_RELEASE_LOCAL_JSON is unset"
      exit $?
    fi

    CLOSURE_JSON="$TEST_LOCAL_JSON"
    echo "ADR-025 closure: using local test json: $CLOSURE_JSON"
  else
    if [[ -z "${GH_TOKEN:-}" ]]; then
      measurement_issue "missing GH_TOKEN"
      exit $?
    fi

    run_list_err="$OUT_DIR/adr025-closure-run-list.err"
    rid=""
    if ! rid="$(gh run list --workflow "$WORKFLOW_NAME" --branch main --status success --limit 1 --json databaseId --jq '.[0].databaseId' 2>"$run_list_err")"; then
      err_out="$(tail -n 20 "$run_list_err" 2>/dev/null || true)"
      measurement_issue "failed to list nightly closure runs: ${err_out}"
      exit $?
    fi

    if [[ -z "$rid" || "$rid" == "null" ]]; then
      measurement_issue "could not find successful ${WORKFLOW_NAME} run"
      exit $?
    fi

    echo "ADR-025 closure: using run id: $rid"
    dl_log="$OUT_DIR/adr025-closure-download.log"
    download_failed=0
    if ! gh run download "$rid" -n "$ARTIFACT_NAME" -D "$OUT_DIR" >"$dl_log" 2>&1; then
      download_failed=1
    fi

    found_json="$(find "$OUT_DIR" -name 'closure_report_v1.json' -print -quit || true)"
    if [[ -z "$found_json" ]]; then
      dl_tail="$(tail -n 20 "$dl_log" 2>/dev/null || true)"
      if [[ "$download_failed" -eq 1 && -n "$dl_tail" ]]; then
        measurement_issue "missing closure_report_v1.json in downloaded artifact; gh run download output: ${dl_tail}"
      else
        measurement_issue "missing closure_report_v1.json in downloaded artifact"
      fi
      exit $?
    fi

    CLOSURE_JSON="$found_json"
    echo "ADR-025 closure: found closure report at $CLOSURE_JSON"
  fi
fi

if [[ ! -f "$CLOSURE_JSON" ]]; then
  measurement_issue "closure report JSON not found: $CLOSURE_JSON"
  exit $?
fi

set +e
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

violations = report.get("violations")
if violations is None:
    violations = []
elif not isinstance(violations, list):
    die(2, "Measurement error: closure report violations must be a list if present")

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
eval_code=$?
set -e

if [[ "$MODE" == "warn" && "$eval_code" -ne 0 ]]; then
  echo "WARN: ADR-025 closure evaluation returned code ${eval_code} (mode=warn, continuing)"
  eval_code=0
fi

if [[ "$MODE" == "attach" && "$eval_code" -ne 0 ]]; then
  echo "ADR-025 closure: mode=attach evaluation returned code ${eval_code} (non-blocking attach)"
  eval_code=0
fi

if [[ "$eval_code" -ne 0 ]]; then
  exit "$eval_code"
fi

found_md="$(find "$OUT_DIR" -name 'closure_report_v1.md' -print -quit || true)"
if [[ -z "$found_md" ]]; then
  echo "NOTE: closure_report_v1.md missing (non-fatal)"
fi

echo "ADR-025 closure: ready in $OUT_DIR"
exit 0
