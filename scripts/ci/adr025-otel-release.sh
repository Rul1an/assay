#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-attach}"
POLICY="${POLICY:-schemas/otel_release_policy_v1.json}"
OUT_DIR="${OUT_DIR:-artifacts/adr025-otel}"
OTEL_JSON=""
RUN_ID=""

TEST_MODE="${ASSAY_OTEL_RELEASE_TEST_MODE:-0}"
TEST_LOCAL_JSON="${ASSAY_OTEL_RELEASE_LOCAL_JSON:-}"
TEST_SIMULATE_MISSING_ARTIFACT="${ASSAY_OTEL_RELEASE_SIMULATE_MISSING_ARTIFACT:-0}"

WORKFLOW_NAME="adr025-nightly-otel-bridge.yml"
ARTIFACT_NAME="adr025-otel-bridge-report"
ARTIFACT_JSON="otel_bridge_report_v1.json"
ARTIFACT_MD="otel_bridge_report_v1.md"
ENFORCE_SEMANTICS_MODE="contract_only"
POLICY_RULES_ENABLED="false"
EXIT_POLICY_FAIL=1
EXIT_MEASUREMENT_FAIL=2

usage() {
  echo "Usage: $0 [--mode off|attach|warn|enforce] [--policy <path>] [--out-dir <dir>] [--otel-json <path>]"
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
    --otel-json)
      OTEL_JSON="$2"
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

echo "ADR-025 otel: mode=$MODE policy=$POLICY out_dir=$OUT_DIR workflow=$WORKFLOW_NAME test_mode=$TEST_MODE"

emit_decision_json() {
  local mode="$1"
  local decision="$2"
  local exit_code="$3"
  local policy_path="$4"
  local report_path="$5"
  local run_id="$6"

  python3 - <<'PY' "$mode" "$decision" "$exit_code" "$policy_path" "$report_path" "$run_id"
import json
import sys

mode, decision, exit_code, policy_path, report_path, run_id = sys.argv[1:]

obj = {
    "event": "adr025.otel_release_decision",
    "mode": mode,
    "score": None,
    "threshold": None,
    "decision": decision,
    "exit_code": int(exit_code),
    "policy_path": policy_path or None,
    "report_path": report_path or None,
    "run_id": run_id or None,
}
print(json.dumps(obj, separators=(",", ":")))
PY
}

decision_exit() {
  local decision="$1"
  local exit_code="$2"
  emit_decision_json "$MODE" "$decision" "$exit_code" "$POLICY" "$OTEL_JSON" "$RUN_ID"
  exit "$exit_code"
}

measurement_issue() {
  local msg="$1"
  case "$MODE" in
    enforce)
      echo "Measurement error: ${msg}"
      return "$EXIT_MEASUREMENT_FAIL"
      ;;
    warn)
      echo "WARN: Measurement issue: ${msg} (mode=warn, continuing)"
      return 0
      ;;
    attach)
      echo "ADR-025 otel: mode=attach measurement issue: ${msg} (non-blocking attach)"
      return 0
      ;;
    *)
      echo "Measurement error: ${msg}"
      return "$EXIT_MEASUREMENT_FAIL"
      ;;
  esac
}

measurement_exit() {
  local msg="$1"
  local decision="measurement_fail"
  local exit_code=0

  if measurement_issue "$msg"; then
    exit_code=0
  else
    exit_code=$?
  fi

  if [[ "$MODE" == "attach" ]]; then
    decision="attach"
  elif [[ "$MODE" == "warn" ]]; then
    decision="warn"
  fi

  decision_exit "$decision" "$exit_code"
}

load_policy_contract() {
  python3 - <<'PY' "$POLICY"
import json
import sys

policy_path = sys.argv[1]


def die(msg):
    print(f"Measurement error: {msg}")
    raise SystemExit(2)

try:
    policy = json.load(open(policy_path, "r", encoding="utf-8"))
except Exception as exc:
    die(f"invalid policy json: {exc}")

if policy.get("policy_version") != "otel_release_policy_v1":
    die(f"unexpected policy_version: {policy.get('policy_version')}")

artifact = policy.get("artifact")
if not isinstance(artifact, dict):
    die("policy missing artifact object")

workflow = artifact.get("workflow")
name = artifact.get("name")
files = artifact.get("files")
if not isinstance(workflow, str) or not workflow:
    die("policy artifact.workflow must be non-empty string")
if not isinstance(name, str) or not name:
    die("policy artifact.name must be non-empty string")
if not isinstance(files, list):
    die("policy artifact.files must be array")

required_files = {"otel_bridge_report_v1.json", "otel_bridge_report_v1.md"}
file_set = set(str(x) for x in files)
if not required_files.issubset(file_set):
    die("policy artifact.files must include otel_bridge_report_v1.json and otel_bridge_report_v1.md")

sem = policy.get("enforce_semantics")
if not isinstance(sem, dict):
    die("policy missing enforce_semantics")
sem_mode = sem.get("mode")
if sem_mode != "contract_only":
    die(f"unsupported enforce_semantics.mode: {sem_mode}")
rules_enabled = sem.get("policy_rules_enabled")
if not isinstance(rules_enabled, bool):
    die("policy enforce_semantics.policy_rules_enabled must be boolean")

exit_contract = policy.get("exit_contract")
if not isinstance(exit_contract, dict):
    die("policy missing exit_contract")
for key in ("pass", "policy_fail", "measurement_fail"):
    val = exit_contract.get(key)
    if not isinstance(val, int):
        die(f"policy exit_contract.{key} must be int")

print(f"WORKFLOW_NAME={workflow}")
print(f"ARTIFACT_NAME={name}")
print("ARTIFACT_JSON=otel_bridge_report_v1.json")
print("ARTIFACT_MD=otel_bridge_report_v1.md")
print(f"ENFORCE_SEMANTICS_MODE={sem_mode}")
print(f"POLICY_RULES_ENABLED={'true' if rules_enabled else 'false'}")
print(f"EXIT_POLICY_FAIL={exit_contract['policy_fail']}")
print(f"EXIT_MEASUREMENT_FAIL={exit_contract['measurement_fail']}")
PY
}

if [[ "$MODE" == "off" ]]; then
  echo "ADR-025 otel: mode=off (skipping)"
  decision_exit "skip" 0
fi

if [[ ! -f "$POLICY" ]]; then
  measurement_exit "policy not found: $POLICY"
fi

set +e
policy_kv="$(load_policy_contract 2>&1)"
policy_rc=$?
set -e
if [[ "$policy_rc" -ne 0 ]]; then
  measurement_exit "$policy_kv"
fi

while IFS='=' read -r key val; do
  [[ -z "$key" ]] && continue
  case "$key" in
    WORKFLOW_NAME) WORKFLOW_NAME="$val" ;;
    ARTIFACT_NAME) ARTIFACT_NAME="$val" ;;
    ARTIFACT_JSON) ARTIFACT_JSON="$val" ;;
    ARTIFACT_MD) ARTIFACT_MD="$val" ;;
    ENFORCE_SEMANTICS_MODE) ENFORCE_SEMANTICS_MODE="$val" ;;
    POLICY_RULES_ENABLED) POLICY_RULES_ENABLED="$val" ;;
    EXIT_POLICY_FAIL) EXIT_POLICY_FAIL="$val" ;;
    EXIT_MEASUREMENT_FAIL) EXIT_MEASUREMENT_FAIL="$val" ;;
  esac
done <<< "$policy_kv"

if [[ -z "$OTEL_JSON" ]]; then
  if [[ "$TEST_MODE" == "1" ]]; then
    if [[ "$TEST_SIMULATE_MISSING_ARTIFACT" == "1" ]]; then
      measurement_exit "simulated missing otel bridge artifact"
    fi

    if [[ -z "$TEST_LOCAL_JSON" ]]; then
      measurement_exit "test mode enabled but ASSAY_OTEL_RELEASE_LOCAL_JSON is unset"
    fi

    OTEL_JSON="$TEST_LOCAL_JSON"
    echo "ADR-025 otel: using local test json: $OTEL_JSON"
  else
    if [[ -z "${GH_TOKEN:-}" ]]; then
      measurement_exit "missing GH_TOKEN"
    fi

    run_list_err="$OUT_DIR/adr025-otel-run-list.err"
    rid=""
    if ! rid="$(gh run list --workflow "$WORKFLOW_NAME" --branch main --status success --limit 1 --json databaseId --jq '.[0].databaseId' 2>"$run_list_err")"; then
      err_out="$(tail -n 20 "$run_list_err" 2>/dev/null || true)"
      measurement_exit "failed to list nightly otel runs: ${err_out}"
    fi

    if [[ -z "$rid" || "$rid" == "null" ]]; then
      measurement_exit "could not find successful ${WORKFLOW_NAME} run"
    fi

    RUN_ID="$rid"
    echo "ADR-025 otel: using run id: $rid"
    dl_log="$OUT_DIR/adr025-otel-download.log"
    download_failed=0
    if ! gh run download "$rid" -n "$ARTIFACT_NAME" -D "$OUT_DIR" >"$dl_log" 2>&1; then
      download_failed=1
    fi

    found_json="$(find "$OUT_DIR" -name "$ARTIFACT_JSON" -print -quit || true)"
    if [[ -z "$found_json" ]]; then
      dl_tail="$(tail -n 20 "$dl_log" 2>/dev/null || true)"
      if [[ "$download_failed" -eq 1 && -n "$dl_tail" ]]; then
        measurement_exit "missing $ARTIFACT_JSON in downloaded artifact; gh run download output: ${dl_tail}"
      else
        measurement_exit "missing $ARTIFACT_JSON in downloaded artifact"
      fi
    fi

    OTEL_JSON="$found_json"
    echo "ADR-025 otel: found bridge report at $OTEL_JSON"
  fi
fi

if [[ ! -f "$OTEL_JSON" ]]; then
  measurement_exit "otel bridge report JSON not found: $OTEL_JSON"
fi

set +e
python3 - <<'PY' "$MODE" "$POLICY" "$OTEL_JSON" "$ENFORCE_SEMANTICS_MODE" "$POLICY_RULES_ENABLED"
import json
import re
import sys

mode, policy_path, report_path, enforce_mode, policy_rules_enabled = sys.argv[1:]

HEX32 = re.compile(r"^[0-9a-f]{32}$")
HEX16 = re.compile(r"^[0-9a-f]{16}$")
DIGITS = re.compile(r"^[0-9]+$")


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
    die(2, f"Measurement error: invalid otel bridge report json: {exc}")

if policy.get("policy_version") != "otel_release_policy_v1":
    die(2, f"Measurement error: unexpected policy_version: {policy.get('policy_version')}")

if enforce_mode != "contract_only":
    die(2, f"Measurement error: unsupported enforce semantics: {enforce_mode}")

if report.get("schema_version") != "otel_bridge_report_v1":
    die(2, f"Measurement error: unexpected otel schema_version: {report.get('schema_version')}")

source = report.get("source")
if not isinstance(source, dict) or source.get("kind") != "otel":
    die(2, "Measurement error: source.kind must be 'otel'")

summary = report.get("summary")
if not isinstance(summary, dict):
    die(2, "Measurement error: missing summary object")
for k in ("trace_count", "span_count"):
    if not isinstance(summary.get(k), int) or summary.get(k) < 0:
        die(2, f"Measurement error: summary.{k} must be non-negative int")

traces = report.get("traces")
if not isinstance(traces, list):
    die(2, "Measurement error: traces must be array")

for trace in traces:
    if not isinstance(trace, dict):
        die(2, "Measurement error: trace must be object")
    trace_id = trace.get("trace_id")
    if not isinstance(trace_id, str) or HEX32.fullmatch(trace_id) is None:
        die(2, f"Measurement error: invalid trace_id: {trace_id}")
    spans = trace.get("spans")
    if not isinstance(spans, list):
        die(2, "Measurement error: trace.spans must be array")

    for span in spans:
        if not isinstance(span, dict):
            die(2, "Measurement error: span must be object")
        span_id = span.get("span_id")
        if not isinstance(span_id, str) or HEX16.fullmatch(span_id) is None:
            die(2, f"Measurement error: invalid span_id: {span_id}")

        parent_span_id = span.get("parent_span_id")
        if parent_span_id is not None:
            if not isinstance(parent_span_id, str) or HEX16.fullmatch(parent_span_id) is None:
                die(2, f"Measurement error: invalid parent_span_id: {parent_span_id}")

        for tfield in ("start_time_unix_nano", "end_time_unix_nano"):
            value = span.get(tfield)
            if not isinstance(value, str) or DIGITS.fullmatch(value) is None:
                die(2, f"Measurement error: {tfield} must be digit-string")

        attrs = span.get("attributes")
        if not isinstance(attrs, list):
            die(2, "Measurement error: span.attributes must be array")
        for kv in attrs:
            if not isinstance(kv, dict) or "key" not in kv or "value" not in kv:
                die(2, "Measurement error: invalid span.attributes item")

        events = span.get("events")
        if events is not None:
            if not isinstance(events, list):
                die(2, "Measurement error: span.events must be array when present")
            for event in events:
                if not isinstance(event, dict):
                    die(2, "Measurement error: event must be object")
                if not isinstance(event.get("name"), str) or not event.get("name"):
                    die(2, "Measurement error: event.name must be non-empty string")
                etime = event.get("time_unix_nano")
                if not isinstance(etime, str) or DIGITS.fullmatch(etime) is None:
                    die(2, "Measurement error: event.time_unix_nano must be digit-string")
                eattrs = event.get("attributes")
                if not isinstance(eattrs, list):
                    die(2, "Measurement error: event.attributes must be array")

        links = span.get("links")
        if links is not None:
            if not isinstance(links, list):
                die(2, "Measurement error: span.links must be array when present")
            for link in links:
                if not isinstance(link, dict):
                    die(2, "Measurement error: link must be object")
                lid = link.get("trace_id")
                sid = link.get("span_id")
                if not isinstance(lid, str) or HEX32.fullmatch(lid) is None:
                    die(2, f"Measurement error: invalid link.trace_id: {lid}")
                if not isinstance(sid, str) or HEX16.fullmatch(sid) is None:
                    die(2, f"Measurement error: invalid link.span_id: {sid}")
                lattrs = link.get("attributes")
                if lattrs is not None and not isinstance(lattrs, list):
                    die(2, "Measurement error: link.attributes must be array when present")

if mode == "enforce" and policy_rules_enabled == "true":
    die(1, "Policy fail: policy rules are enabled but no explicit otel release rules are configured")

print("Pass: otel bridge report satisfies release contract")
raise SystemExit(0)
PY
eval_code=$?
set -e

if [[ "$MODE" == "warn" && "$eval_code" -ne 0 ]]; then
  echo "WARN: ADR-025 otel evaluation returned code ${eval_code} (mode=warn, continuing)"
  eval_code=0
fi

if [[ "$MODE" == "attach" && "$eval_code" -ne 0 ]]; then
  echo "ADR-025 otel: mode=attach evaluation returned code ${eval_code} (non-blocking attach)"
  eval_code=0
fi

decision="pass"
if [[ "$MODE" == "attach" ]]; then
  decision="attach"
elif [[ "$MODE" == "warn" ]]; then
  decision="warn"
elif [[ "$MODE" == "enforce" ]]; then
  if [[ "$eval_code" -eq "$EXIT_POLICY_FAIL" ]]; then
    decision="policy_fail"
  elif [[ "$eval_code" -eq "$EXIT_MEASUREMENT_FAIL" ]]; then
    decision="measurement_fail"
  fi
fi

found_md="$(find "$OUT_DIR" -name "$ARTIFACT_MD" -print -quit || true)"
if [[ -z "$found_md" ]]; then
  echo "NOTE: $ARTIFACT_MD missing (non-fatal)"
fi

echo "ADR-025 otel: ready in $OUT_DIR"
decision_exit "$decision" "$eval_code"
