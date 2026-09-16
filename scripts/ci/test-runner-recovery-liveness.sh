#!/usr/bin/env bash
# shellcheck disable=SC2218,SC2329 # Test doubles are restored/redefined and invoked indirectly.
#
# #2985: three of four delegated-run teardowns were the host cron's recovery
# path stopping a runner that was executing a job. The API snapshot said
# `offline` while the guest was busy, and the queued-job trigger counted every
# queued run in the repository. Recovery must refuse while a Runner.Worker
# process exists in the guest, and the queued-job trigger must only fire for
# jobs that require this runner's label.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${SCRIPT:-${ROOT}/infra/bpf-runner/health_check.sh}"
EVENTS="$(mktemp)"
trap 'rm -f "${EVENTS}"' EXIT

export RUNNER_NAME=assay-bpf-runner
# shellcheck source=/dev/null
source "${SCRIPT}"

log_info() { :; }
log_warn() { :; }
log_error() { :; }
log_ok() { :; }
sleep() { :; }

# Recovery side effects are recorded, never executed.
sync_vm_time() { echo sync >>"${EVENTS}"; }
cleanup_runner_config() { echo cleanup >>"${EVENTS}"; }
generate_runner_token() { printf '%s\n' fresh-token; }
configure_runner() { echo configure >>"${EVENTS}"; }
start_runner_service() { echo service >>"${EVENTS}"; }
get_runner_status() { printf '%s\n' online; }
# The API admission already passed in every #2985 incident; hold it constant.
require_recovery_admission() { echo admission >>"${EVENTS}"; }

# ---------------------------------------------------------------------------
# 1. Guest worker probe gates recovery, fail-closed.
# ---------------------------------------------------------------------------
GUEST_PGREP_RC=1
GUEST_TIMEOUT=0
timeout() {
    [[ "$1" == "$MULTIPASS_RECOVERY_TIMEOUT_SECONDS" ]] || return 93
    shift
    [[ "${GUEST_TIMEOUT}" == 0 ]] || return 124
    "$@"
}
multipass() {
    [[ "$*" == "exec $VM_NAME -- pgrep -x Runner.Worker" ]] || return 93
    echo probe >>"${EVENTS}"
    return "${GUEST_PGREP_RC}"
}

# pgrep exit 0: a worker exists, a job is executing. Recovery must refuse
# before any side effect, including the API admission's own successor steps.
GUEST_PGREP_RC=0
: >"${EVENTS}"
rc=0
recover_runner || rc=$?
if [[ "$rc" -eq 0 ]] || grep -Fxq cleanup "${EVENTS}"; then
    echo "recovery must refuse while a Runner.Worker process exists in the guest (rc=$rc)" >&2
    cat "${EVENTS}" >&2
    exit 1
fi
if ! grep -Fxq probe "${EVENTS}"; then
    echo "recovery did not probe the guest for a running worker" >&2
    exit 1
fi

# pgrep exit 1: no worker. Recovery proceeds, and the probe precedes cleanup.
GUEST_PGREP_RC=1
: >"${EVENTS}"
recover_runner
if [[ "$(grep -Fxn -e probe -e cleanup "${EVENTS}" | tr '\n' ' ')" != *"probe"*"cleanup"* ]]; then
    echo "recovery must probe the guest before cleanup" >&2
    cat "${EVENTS}" >&2
    exit 1
fi

# pgrep exit >1 (error) and a timed-out probe are not evidence of an idle
# guest; refuse rather than stop an unknown runner.
for probe_case in "2 0" "1 1"; do
    read -r GUEST_PGREP_RC GUEST_TIMEOUT <<<"${probe_case}"
    : >"${EVENTS}"
    rc=0
    recover_runner || rc=$?
    if [[ "$rc" -eq 0 ]] || grep -Fxq cleanup "${EVENTS}"; then
        echo "recovery must refuse when the guest probe is unavailable (pgrep rc=${GUEST_PGREP_RC}, timeout=${GUEST_TIMEOUT})" >&2
        cat "${EVENTS}" >&2
        exit 1
    fi
done
GUEST_PGREP_RC=1
GUEST_TIMEOUT=0

# ---------------------------------------------------------------------------
# 2. Queued-job trigger counts only jobs that require this runner's label.
# ---------------------------------------------------------------------------
QUEUED_RUNS_RC=0
QUEUED_RUN_IDS=""
declare -A JOBS_BY_RUN=()
declare -A JOBS_PAGE2_BY_RUN=()
gh() {
    local paginate=0 path="" arg
    for arg in "$@"; do
        case "$arg" in
            --paginate) paginate=1 ;;
            -*) ;;
            *) path="$arg" ;;
        esac
    done
    case "$path" in
        "repos/$REPO/actions/runs?status=queued&per_page="*)
            [[ "${QUEUED_RUNS_RC}" == 0 ]] || return "${QUEUED_RUNS_RC}"
            printf '{"workflow_runs":[%s]}' "${QUEUED_RUN_IDS}"
            ;;
        "repos/$REPO/actions/runs/"*"/jobs"*)
            local run_id="${path#repos/"$REPO"/actions/runs/}"
            run_id="${run_id%%/*}"
            printf '%s' "${JOBS_BY_RUN[${run_id}]:-}"
            # Without --paginate, gh returns only the first jobs page.
            if [[ "$paginate" == 1 && -n "${JOBS_PAGE2_BY_RUN[${run_id}]:-}" ]]; then
                printf '\n%s' "${JOBS_PAGE2_BY_RUN[${run_id}]}"
            fi
            ;;
        *)
            return 92
            ;;
    esac
}
# shellcheck disable=SC2034 # read as ${GH_CMD:-gh} inside the sourced script.
GH_CMD=gh

job() { printf '{"status":"%s","labels":[%s]}' "$1" "$2"; }
HOSTED='"ubuntu-latest"'
OURS='"self-hosted","linux","assay-bpf-runner"'

expect_queue() {
    local case_name="$1" expected="$2" rc=0
    check_queued_jobs || rc=$?
    if [[ "$rc" -ne "$expected" ]]; then
        echo "check_queued_jobs ${case_name}: expected rc ${expected}, got ${rc}" >&2
        exit 1
    fi
}

# A deep hosted queue is not demand for this runner.
QUEUED_RUN_IDS='{"id":11},{"id":12}'
JOBS_BY_RUN[11]="{\"jobs\":[$(job queued "$HOSTED"),$(job queued "$HOSTED")]}"
JOBS_BY_RUN[12]="{\"jobs\":[$(job in_progress "$HOSTED")]}"
expect_queue hosted-only 1

# One waiting job that requires the label is demand.
JOBS_BY_RUN[12]="{\"jobs\":[$(job in_progress "$HOSTED"),$(job queued "$OURS")]}"
expect_queue labelled-queued 0

# A labelled job that already completed is not demand.
JOBS_BY_RUN[12]="{\"jobs\":[$(job completed "$OURS")]}"
expect_queue labelled-completed 1

# No queued runs at all.
QUEUED_RUN_IDS=""
expect_queue empty 1

# An unavailable API is unknown demand, not demand.
QUEUED_RUN_IDS='{"id":11}'
QUEUED_RUNS_RC=22
expect_queue api-failed 1
QUEUED_RUNS_RC=0

# A runs page whose jobs request fails contributes nothing rather than aborting.
QUEUED_RUN_IDS='{"id":13},{"id":12}'
JOBS_BY_RUN[13]='not json'
JOBS_BY_RUN[12]="{\"jobs\":[$(job waiting "$OURS")]}"
expect_queue unparsable-then-labelled 0

# Labelled demand on a later jobs page is still demand. Without --paginate,
# the mock serves only page 1 (hosted jobs), so this case must fail.
QUEUED_RUN_IDS='{"id":21}'
JOBS_BY_RUN[21]="{\"jobs\":[$(job queued "$HOSTED")]}"
JOBS_PAGE2_BY_RUN[21]="{\"jobs\":[$(job queued "$OURS")]}"
expect_queue labelled-beyond-first-jobs-page 0
unset 'JOBS_PAGE2_BY_RUN[21]'

echo "ok: runner recovery refuses while a guest worker runs and queued demand is label-scoped"
