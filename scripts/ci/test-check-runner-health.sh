#!/usr/bin/env bash
# Behavioral contract for scripts/ci/check-runner-health.sh + runner-health.yml.
#
# The monitor samples control-plane runner registration and label-specific demand.
# It does not prove host functionality or SLA. Silence / expected_offline is not health.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${SCRIPT:-${ROOT}/scripts/ci/check-runner-health.sh}"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/runner-health.yml}"
MUTATION_TEST="${ROOT}/scripts/ci/test-check-runner-health-mutations.sh"
FAILURES=0
TEST_TEMP_DIR=""

cleanup() { [[ -n "$TEST_TEMP_DIR" ]] && rm -rf "$TEST_TEMP_DIR"; }
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  FAILURES=$((FAILURES + 1))
}

ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "check-runner-health contract aborted (exit ${rc}); treat as failure" >&2
}
trap 'abort_is_failure "$?"' ERR

[[ -f "$SCRIPT" ]] || { echo "FAIL: missing $SCRIPT" >&2; exit 1; }
[[ -f "$WORKFLOW" ]] || { echo "FAIL: missing $WORKFLOW" >&2; exit 1; }
[[ -f "$MUTATION_TEST" ]] || { echo "FAIL: missing $MUTATION_TEST" >&2; exit 1; }

make_fake_gh() {
  cat >"$1" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${FAKE_GH_LOG}"

# Drop flags; keep the resource path as the last non-option argument.
path=""
for arg in "$@"; do
  case "$arg" in
    -* ) ;;
    * ) path="$arg" ;;
  esac
done

case "$path" in
  repos/*/actions/runners*)
    if [[ "${FAKE_RUNNERS_FAIL:-}" == "1" ]]; then
      echo "runner list failed" >&2
      exit 1
    fi
    cat "${FAKE_RUNNERS_JSON}"
    ;;
  repos/*/actions/runs/*/jobs*)
    if [[ "${FAKE_JOBS_FAIL:-}" == "1" ]]; then
      echo "jobs list failed" >&2
      exit 1
    fi
    run_id="$(printf '%s' "$path" | sed -n 's|.*/actions/runs/\([0-9][0-9]*\)/jobs.*|\1|p')"
    jobs_file="${FAKE_JOBS_DIR}/${run_id}.json"
    if [[ -f "$jobs_file" ]]; then
      cat "$jobs_file"
    else
      printf '%s\n' '{"jobs":[]}'
    fi
    ;;
  repos/*/actions/runs*)
    if [[ "${FAKE_RUNS_FAIL:-}" == "1" ]]; then
      echo "runs list failed" >&2
      exit 1
    fi
    # status=queued|in_progress is query-string; serve empty unless a fixture overrides.
    if [[ "$path" == *status=queued* ]]; then
      cat "${FAKE_QUEUED_RUNS_JSON}"
    else
      cat "${FAKE_IN_PROGRESS_RUNS_JSON}"
    fi
    ;;
  *)
    echo "unexpected gh api path: ${path}" >&2
    exit 1
    ;;
esac
EOF
  chmod +x "$1"
}

write_defaults() {
  printf '%s\n' '{"total_count":0,"runners":[]}' >"${FAKE_RUNNERS_JSON}"
  # Include one non-matching queued run so the current script materializes
  # matching-jobs.tsv (it only creates that file while inspecting runs). Empty
  # queues would abort before classification and hide the healthy=true defect.
  printf '%s\n' '{"total_count":1,"workflow_runs":[{"id":99,"name":"CI","status":"queued","html_url":"https://example.test/run/99"}]}' \
    >"${FAKE_QUEUED_RUNS_JSON}"
  printf '%s\n' '{"total_count":0,"workflow_runs":[]}' >"${FAKE_IN_PROGRESS_RUNS_JSON}"
  mkdir -p "${FAKE_JOBS_DIR}"
  printf '%s\n' '{"jobs":[{"name":"lint","status":"queued","labels":["ubuntu-latest"],"html_url":"https://example.test/job/99"}]}' \
    >"${FAKE_JOBS_DIR}/99.json"
}

run_check() {
  local out_file="$1"
  local summary_file="$2"
  shift 2
  : >"${FAKE_GH_LOG}"
  : >"${out_file}"
  : >"${summary_file}"
  set +e
  PATH="${TEST_TEMP_DIR}/bin:${PATH}" \
    GITHUB_REPOSITORY="Rul1an/assay" \
    RUNNER_STATUS_TOKEN="runner-token" \
    QUEUE_TOKEN="queue-token" \
    RUNNER_NAME="assay-bpf-runner" \
    REQUIRED_RUNNER_LABEL="assay-bpf-runner" \
    GITHUB_OUTPUT="${out_file}" \
    GITHUB_STEP_SUMMARY="${summary_file}" \
    FAKE_GH_LOG="${FAKE_GH_LOG}" \
    FAKE_RUNNERS_JSON="${FAKE_RUNNERS_JSON}" \
    FAKE_QUEUED_RUNS_JSON="${FAKE_QUEUED_RUNS_JSON}" \
    FAKE_IN_PROGRESS_RUNS_JSON="${FAKE_IN_PROGRESS_RUNS_JSON}" \
    FAKE_JOBS_DIR="${FAKE_JOBS_DIR}" \
    FAKE_RUNNERS_FAIL="${FAKE_RUNNERS_FAIL:-}" \
    FAKE_RUNS_FAIL="${FAKE_RUNS_FAIL:-}" \
    FAKE_JOBS_FAIL="${FAKE_JOBS_FAIL:-}" \
    bash "${SCRIPT}" "$@" >"${TEST_TEMP_DIR}/stdout.txt" 2>"${TEST_TEMP_DIR}/stderr.txt"
  CHECK_STATUS=$?
  set -e
  CHECK_STDERR="$(cat "${TEST_TEMP_DIR}/stderr.txt")"
  CHECK_OUTPUT="$(cat "${out_file}")"
}

output_value() {
  local key="$1"
  printf '%s\n' "${CHECK_OUTPUT}" | awk -F= -v k="$key" '$1==k {print substr($0, index($0,"=")+1); exit}'
}

expect_case() {
  local name="$1"
  local want_state="$2"
  local want_alert="$3"
  local want_exit="$4"
  local want_healthy="${5-}"

  local got_state got_alert got_healthy
  got_state="$(output_value runner_state)"
  got_alert="$(output_value alert_required)"
  got_healthy="$(output_value healthy)"

  if [[ "${CHECK_STATUS}" -ne "${want_exit}" ]]; then
    fail "${name}: exit ${CHECK_STATUS}, wanted ${want_exit} (stderr: ${CHECK_STDERR})"
    return
  fi
  if [[ "${got_state}" != "${want_state}" ]]; then
    fail "${name}: runner_state=${got_state:-<missing>}, wanted ${want_state}"
    return
  fi
  if [[ "${got_alert}" != "${want_alert}" ]]; then
    fail "${name}: alert_required=${got_alert:-<missing>}, wanted ${want_alert}"
    return
  fi
  if [[ -n "${want_healthy}" && "${got_healthy}" != "${want_healthy}" ]]; then
    fail "${name}: healthy=${got_healthy:-<missing>}, wanted ${want_healthy}"
    return
  fi
  # Silence / offline-without-demand must never publish healthy=true.
  if [[ "${want_state}" == "expected_offline" && "${got_healthy}" == "true" ]]; then
    fail "${name}: expected_offline published healthy=true (silence is not health)"
    return
  fi
  if [[ "${want_alert}" == "false" && "${CHECK_STATUS}" -ne 0 ]]; then
    fail "${name}: alert_required=false must exit 0"
    return
  fi
  if [[ "${want_alert}" == "true" && "${CHECK_STATUS}" -eq 0 ]]; then
    fail "${name}: alert_required=true must not exit 0"
    return
  fi
  ok "${name}"
}

TEST_TEMP_DIR="$(mktemp -d)"
mkdir -p "${TEST_TEMP_DIR}/bin"
FAKE_GH_LOG="${TEST_TEMP_DIR}/gh.log"
FAKE_RUNNERS_JSON="${TEST_TEMP_DIR}/runners.json"
FAKE_QUEUED_RUNS_JSON="${TEST_TEMP_DIR}/queued-runs.json"
FAKE_IN_PROGRESS_RUNS_JSON="${TEST_TEMP_DIR}/in-progress-runs.json"
FAKE_JOBS_DIR="${TEST_TEMP_DIR}/jobs"
make_fake_gh "${TEST_TEMP_DIR}/bin/gh"
write_defaults

# --- online → available ------------------------------------------------------------------
printf '%s\n' '{"total_count":1,"runners":[{"name":"assay-bpf-runner","status":"online","busy":false,"labels":[{"name":"assay-bpf-runner"}]}]}' \
  >"${FAKE_RUNNERS_JSON}"
run_check "${TEST_TEMP_DIR}/out-online.txt" "${TEST_TEMP_DIR}/sum-online.txt"
expect_case "online → available" "available" "false" 0 "true"

# --- offline / no demand → expected_offline (must NOT publish healthy=true) --------------
printf '%s\n' '{"total_count":1,"runners":[{"name":"assay-bpf-runner","status":"offline","busy":false,"labels":[{"name":"assay-bpf-runner"}]}]}' \
  >"${FAKE_RUNNERS_JSON}"
write_defaults
printf '%s\n' '{"total_count":1,"runners":[{"name":"assay-bpf-runner","status":"offline","busy":false,"labels":[{"name":"assay-bpf-runner"}]}]}' \
  >"${FAKE_RUNNERS_JSON}"
run_check "${TEST_TEMP_DIR}/out-offline.txt" "${TEST_TEMP_DIR}/sum-offline.txt"
expect_case "offline/no-demand → expected_offline" "expected_offline" "false" 0 "false"

# --- offline / matching demand → demand_backed_outage ------------------------------------
printf '%s\n' '{"total_count":1,"workflow_runs":[{"id":101,"name":"Kernel Matrix","status":"queued","html_url":"https://example.test/run/101"}]}' \
  >"${FAKE_QUEUED_RUNS_JSON}"
printf '%s\n' '{"jobs":[{"name":"matrix-test","status":"queued","labels":["assay-bpf-runner"],"html_url":"https://example.test/job/1"}]}' \
  >"${FAKE_JOBS_DIR}/101.json"
run_check "${TEST_TEMP_DIR}/out-demand.txt" "${TEST_TEMP_DIR}/sum-demand.txt"
expect_case "offline/demand → demand_backed_outage" "demand_backed_outage" "true" 1 "false"

# --- not_found → unavailable -------------------------------------------------------------
write_defaults
printf '%s\n' '{"total_count":0,"runners":[]}' >"${FAKE_RUNNERS_JSON}"
run_check "${TEST_TEMP_DIR}/out-missing.txt" "${TEST_TEMP_DIR}/sum-missing.txt"
expect_case "not_found → unavailable" "unavailable" "true" 1 "false"

# --- runner API failure → classification_unknown -----------------------------------------
write_defaults
FAKE_RUNNERS_FAIL=1
run_check "${TEST_TEMP_DIR}/out-runner-api.txt" "${TEST_TEMP_DIR}/sum-runner-api.txt"
expect_case "runner API failure → classification_unknown" "classification_unknown" "true" 1 "false"
FAKE_RUNNERS_FAIL=

# --- queue API failure while offline → classification_unknown ----------------------------
write_defaults
printf '%s\n' '{"total_count":1,"runners":[{"name":"assay-bpf-runner","status":"offline","busy":false,"labels":[{"name":"assay-bpf-runner"}]}]}' \
  >"${FAKE_RUNNERS_JSON}"
FAKE_RUNS_FAIL=1
run_check "${TEST_TEMP_DIR}/out-queue-api.txt" "${TEST_TEMP_DIR}/sum-queue-api.txt"
expect_case "queue API failure → classification_unknown" "classification_unknown" "true" 1 "false"
FAKE_RUNS_FAIL=

# --- workflow: schedule + alert_required routing + stale header --------------------------
if grep -qE 'Runs every 15 minutes' "${WORKFLOW}"; then
  fail "workflow header still claims 15 minutes"
else
  ok "workflow header does not claim 15 minutes"
fi

grep -qE "cron: '0 \\*/6 \\* \\* \\*'" "${WORKFLOW}" \
  || fail "workflow must keep the six-hour schedule"
ok "six-hour schedule preserved"

# Intentional literal workflow expressions (not shell expansions).
# shellcheck disable=SC2016
grep -q 'RUNNER_STATUS_TOKEN: ${{ secrets.RUNNER_HEALTH_TOKEN || github.token }}' "${WORKFLOW}" \
  || fail "workflow must keep RUNNER_STATUS_TOKEN separation"
# shellcheck disable=SC2016
grep -q 'QUEUE_TOKEN: ${{ github.token }}' "${WORKFLOW}" \
  || fail "workflow must keep QUEUE_TOKEN separation"
ok "token separation preserved"

if grep -q "steps.check.outcome == 'failure'" "${WORKFLOW}"; then
  fail "workflow must not create alerts from steps.check.outcome"
else
  ok "create step is not keyed on step outcome"
fi

if grep -qE 'if: success\(\)' "${WORKFLOW}"; then
  fail "workflow must not close alerts from success()"
else
  ok "close step is not keyed on success()"
fi

grep -q "steps.check.outputs.alert_required == 'true'" "${WORKFLOW}" \
  || fail "create step must key on alert_required == true"
grep -q "steps.check.outputs.alert_required == 'false'" "${WORKFLOW}" \
  || fail "close step must key on alert_required == false"
ok "alert issue create/close keyed on alert_required"

# Public-string non-claim: allow explicit disclaimers; reject affirmative health/SLA claims.
if grep -qiE 'runner is healthy|meets SLA|proves host functionality' "${WORKFLOW}"; then
  fail "workflow must not make broad health/SLA claims"
else
  ok "workflow avoids broad health/SLA claims"
fi
if ! grep -q 'Does not claim host functionality or SLA' "${WORKFLOW}"; then
  fail "workflow must state the control-plane non-claim"
else
  ok "workflow states control-plane non-claim"
fi

if [[ "${ASSAY_CONTRACT_MUTATION:-}" != "1" ]]; then
  if ! bash "${MUTATION_TEST}"; then
    fail "mutation suite failed"
  else
    ok "mutation suite rejected inert substitutes"
  fi
fi

if [[ "${FAILURES}" -ne 0 ]]; then
  echo "FAIL: ${FAILURES} check-runner-health contract assertion(s) failed" >&2
  exit 1
fi

echo "ok: check-runner-health state contract"
