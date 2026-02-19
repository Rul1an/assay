#!/usr/bin/env bash
set -euo pipefail

POLICY_PATH="${POLICY_PATH:-schemas/soak_readiness_policy_v1.json}"
READINESS_PATH=""

usage() {
  echo "Usage: $0 --readiness <path/to/nightly_readiness.json> [--policy <path>]"
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --readiness) READINESS_PATH="$2"; shift 2 ;;
    --policy) POLICY_PATH="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "Unknown arg: $1"; usage ;;
  esac
done

if [[ -z "${READINESS_PATH}" ]]; then
  echo "Measurement error: missing --readiness"
  exit 2
fi

if [[ ! -f "${POLICY_PATH}" ]]; then
  echo "Measurement error: policy not found: ${POLICY_PATH}"
  exit 2
fi

if [[ ! -f "${READINESS_PATH}" ]]; then
  echo "Measurement error: readiness not found: ${READINESS_PATH}"
  exit 2
fi

python3 - <<'PY' "${POLICY_PATH}" "${READINESS_PATH}"
import json, sys

policy_path, readiness_path = sys.argv[1], sys.argv[2]

def die(code, msg):
    print(msg)
    raise SystemExit(code)

with open(policy_path, "r", encoding="utf-8") as f:
    policy = json.load(f)

with open(readiness_path, "r", encoding="utf-8") as f:
    r = json.load(f)

exit_pass = policy.get("exit_contract", {}).get("pass", 0)
exit_policy_fail = policy.get("exit_contract", {}).get("policy_fail", 1)
exit_measure_fail = policy.get("exit_contract", {}).get("measurement_fail", 2)

for key in ("schema_version", "classifier_version", "window", "rates"):
    if key not in r:
        die(exit_measure_fail, f"Measurement error: readiness missing required key: {key}")

expected_schema = "adr025-nightly-readiness-v1"
if r.get("schema_version") != expected_schema:
    die(exit_measure_fail, f"Measurement error: unexpected readiness schema_version: {r.get('schema_version')} (expected {expected_schema})")

expected_classifier = policy.get("classifier_version")
if expected_classifier is None:
    die(exit_measure_fail, "Measurement error: policy missing classifier_version")
if str(r.get("classifier_version")) != str(expected_classifier):
    die(exit_measure_fail, f"Measurement error: readiness classifier_version mismatch: {r.get('classifier_version')} (policy expects {expected_classifier})")

window = r.get("window") or {}
runs_observed = window.get("runs_observed")
if not isinstance(runs_observed, int):
    die(exit_measure_fail, "Measurement error: window.runs_observed must be int")

min_runs = int(policy.get("window", {}).get("runs_observed_minimum", 14))
target_runs = int(policy.get("window", {}).get("runs_observed_target", 20))

if runs_observed < min_runs:
    die(exit_measure_fail, f"Measurement error: insufficient runs_observed={runs_observed} (min {min_runs}, target {target_runs})")

rates = r.get("rates") or {}
def get_rate(name):
    v = rates.get(name)
    if not isinstance(v, (int, float)):
        die(exit_measure_fail, f"Measurement error: rates.{name} must be number")
    if v < 0 or v > 1:
        die(exit_measure_fail, f"Measurement error: rates.{name} must be within [0,1], got {v}")
    return float(v)

success_rate = get_rate("success_rate")
contract_fail_rate = get_rate("contract_fail_rate")
infra_fail_rate = get_rate("infra_fail_rate")
unknown_rate = get_rate("unknown_rate")

thresholds = policy.get("thresholds") or {}
success_min = float(thresholds.get("success_rate_min", 0.90))
contract_max = float(thresholds.get("contract_fail_rate_max", 0.05))
infra_max = float(thresholds.get("infra_fail_rate_max", 0.01))
unknown_max = float(thresholds.get("unknown_rate_max", 0.05))

violations = []
if success_rate < success_min:
    violations.append(f"success_rate {success_rate:.3f} < min {success_min:.3f}")
if contract_fail_rate > contract_max:
    violations.append(f"contract_fail_rate {contract_fail_rate:.3f} > max {contract_max:.3f}")
if infra_fail_rate > infra_max:
    violations.append(f"infra_fail_rate {infra_fail_rate:.3f} > max {infra_max:.3f}")
if unknown_rate > unknown_max:
    violations.append(f"unknown_rate {unknown_rate:.3f} > max {unknown_max:.3f}")

if violations:
    print("Policy fail: readiness below thresholds:")
    for v in violations:
        print(f"- {v}")
    raise SystemExit(exit_policy_fail)

print("Pass: readiness meets policy thresholds")
raise SystemExit(exit_pass)
PY
