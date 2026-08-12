#!/usr/bin/env bash
# Biting mutations for the runner-health state contract.
# Each mutation must be rejected by test-check-runner-health.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACT="${ROOT}/scripts/ci/test-check-runner-health.sh"
SCRIPT="${ROOT}/scripts/ci/check-runner-health.sh"
WORKFLOW="${ROOT}/.github/workflows/runner-health.yml"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

expect_rejected() {
  local name="$1"
  local target="$2"
  local mutated="${TMP_DIR}/${name}.mut"

  python3 - "$target" "$mutated" "$name" <<'PY'
import pathlib
import sys

source, destination, name = sys.argv[1:]
text = pathlib.Path(source).read_text()
mutations = {
    # Offline/no-demand must not silently become healthy=true.
    "offline-to-healthy": (
        'elif [[ "$runner_status" == "offline" ]]; then\n'
        '  runner_state="expected_offline"\n'
        '  alert_required="false"\n'
        '  healthy="false"\n',
        'elif [[ "$runner_status" == "offline" ]]; then\n'
        '  runner_state="expected_offline"\n'
        '  alert_required="false"\n'
        '  healthy="true"\n',
    ),
    # API/classification failure must not become a clean available result.
    "api-error-to-clean": (
        'if [[ -n "$runner_status_error" ]]; then\n'
        '  runner_state="classification_unknown"\n'
        '  alert_required="true"\n'
        '  healthy="false"\n',
        'if [[ -n "$runner_status_error" ]]; then\n'
        '  runner_state="available"\n'
        '  alert_required="false"\n'
        '  healthy="true"\n',
    ),
    # Online must not mask a failed queue classification as available.
    "online-masks-queue-error": (
        'elif [[ -n "$queue_status_error" ]]; then\n'
        '  # Queue classification incomplete/failed is unknown even when the runner is online.\n'
        '  runner_state="classification_unknown"\n'
        '  alert_required="true"\n'
        '  healthy="false"\n'
        '  health_reason="runner_${runner_status}_queue_classification_unknown"\n'
        'elif [[ "$runner_status" == "online" ]]; then\n'
        '  runner_state="available"\n'
        '  alert_required="false"\n'
        '  healthy="true"\n'
        '  health_reason="runner_online"\n',
        'elif [[ "$runner_status" == "online" ]]; then\n'
        '  runner_state="available"\n'
        '  alert_required="false"\n'
        '  healthy="true"\n'
        '  health_reason="runner_online"\n'
        'elif [[ -n "$queue_status_error" ]]; then\n'
        '  # Queue classification incomplete/failed is unknown even when the runner is online.\n'
        '  runner_state="classification_unknown"\n'
        '  alert_required="true"\n'
        '  healthy="false"\n'
        '  health_reason="runner_${runner_status}_queue_classification_unknown"\n',
    ),
    # Parse failure must not become a silent clean/available result.
    "parse-failure-to-clean": (
        '  if jq "$@" >"$out_file" 2>/dev/null; then\n'
        '    cat "$out_file"\n'
        '    return 0\n'
        '  fi\n'
        '  printf \'%s\\n\' "$marker" >>"$err_file"\n'
        '  return 1\n',
        '  if jq "$@" >"$out_file" 2>/dev/null; then\n'
        '    cat "$out_file"\n'
        '    return 0\n'
        '  fi\n'
        '  # mutation: swallow parse failure without recording a marker\n'
        '  return 0\n',
    ),
    # Skipping shape validation + raw jq restores empty/malformed abort-or-misclassify paths.
    "parse-failure-uncaught": (
        '  if require_json_pages "$runner_json" "runners" "$runner_parse_err" "runner_json_incomplete"; then\n'
        '    # jq program: $name is a jq --arg, not a shell expansion.\n'
        '    # shellcheck disable=SC2016\n'
        '    if selected_runner="$(jq_try "$runner_parse_err" "runner_json_parse_failed" -sc --arg name "$runner_name" \'[.[].runners[]? | select(.name == $name)] | .[0] // empty\' "$runner_json")"; then\n',
        '  if true; then\n'
        '    # mutation: skip shape gate and use raw jq\n'
        '    # shellcheck disable=SC2016\n'
        '    if selected_runner="$(jq -sc --arg name "$runner_name" \'[.[].runners[]? | select(.name == $name)] | .[0] // empty\' "$runner_json")"; then\n',
    ),
    # Shape validation must not be weakened to a no-op (empty/wrong bodies ≠ clean).
    "shape-validation-removed": (
        'require_json_pages() {\n'
        '  local file="$1"\n'
        '  local array_key="$2"\n'
        '  local err_file="$3"\n'
        '  local marker="$4"\n'
        '\n'
        '  if [[ ! -s "$file" ]]; then\n'
        '    printf \'%s\\n\' "$marker" >>"$err_file"\n'
        '    return 1\n'
        '  fi\n',
        'require_json_pages() {\n'
        '  local file="$1"\n'
        '  local array_key="$2"\n'
        '  local err_file="$3"\n'
        '  local marker="$4"\n'
        '\n'
        '  # mutation: accept any body, including empty/wrong-shape streams\n'
        '  return 0\n'
        '  if [[ ! -s "$file" ]]; then\n'
        '    printf \'%s\\n\' "$marker" >>"$err_file"\n'
        '    return 1\n'
        '  fi\n',
    ),
    # Workflow must not fall back to step-outcome routing.
    "alert-from-step-outcome": (
        "        if: always() && steps.check.outputs.alert_required == 'true'\n",
        "        if: steps.check.outcome == 'failure'\n",
    ),
}
old, new = mutations[name]
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor matched {text.count(old)} times, expected once: {old!r}")
pathlib.Path(destination).write_text(text.replace(old, new, 1))
PY

  if [[ "$target" == "$SCRIPT" ]]; then
    if SCRIPT="$mutated" ASSAY_CONTRACT_MUTATION=1 bash "$CONTRACT" >/dev/null 2>&1; then
      echo "FAIL: contract accepted mutation: $name" >&2
      exit 1
    fi
  else
    if WORKFLOW="$mutated" ASSAY_CONTRACT_MUTATION=1 bash "$CONTRACT" >/dev/null 2>&1; then
      echo "FAIL: contract accepted mutation: $name" >&2
      exit 1
    fi
  fi
}

expect_rejected offline-to-healthy "$SCRIPT"
expect_rejected api-error-to-clean "$SCRIPT"
expect_rejected online-masks-queue-error "$SCRIPT"
expect_rejected parse-failure-to-clean "$SCRIPT"
expect_rejected parse-failure-uncaught "$SCRIPT"
expect_rejected shape-validation-removed "$SCRIPT"
expect_rejected alert-from-step-outcome "$WORKFLOW"

echo "ok: check-runner-health contract rejects inert substitutes"
