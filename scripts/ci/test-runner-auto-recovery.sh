#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${SCRIPT:-${ROOT}/infra/bpf-runner/health_check.sh}"
EVENTS="$(mktemp)"
trap 'rm -f "${EVENTS}"' EXIT

# shellcheck source=/dev/null
source "${SCRIPT}"

ORIGINAL_CLEANUP="$(declare -f cleanup_runner_config)"
ORIGINAL_CONFIGURE="$(declare -f configure_runner)"
ORIGINAL_START_SERVICE="$(declare -f start_runner_service)"

log_info() { :; }
log_warn() { :; }
log_error() { :; }
log_ok() { :; }

sync_vm_time() { echo sync >>"${EVENTS}"; }
cleanup_runner_config() { echo cleanup >>"${EVENTS}"; }
generate_runner_token() {
    echo token >>"${EVENTS}"
    printf '%s\n' fresh-token
}
configure_runner() {
    [[ "$1" == fresh-token ]]
    echo configure >>"${EVENTS}"
}
start_runner_service() { echo service >>"${EVENTS}"; }
get_runner_status() {
    echo status >>"${EVENTS}"
    printf '%s\n' online
}
sleep() { :; }

recover_runner

cat >"${EVENTS}.expected" <<'EOF'
sync
cleanup
token
configure
service
status
EOF
if ! cmp -s "${EVENTS}.expected" "${EVENTS}"; then
    echo "runner recovery must finish cleanup before generating its short-lived token" >&2
    diff -u "${EVENTS}.expected" "${EVENTS}" >&2 || true
    rm -f "${EVENTS}.expected"
    exit 1
fi
rm -f "${EVENTS}.expected"

: >"${EVENTS}"
eval "${ORIGINAL_CLEANUP}"
eval "${ORIGINAL_CONFIGURE}"
eval "${ORIGINAL_START_SERVICE}"
timeout() {
    shift
    ASSAY_TEST_TIMEOUT_ACTIVE=1 "$@"
}
multipass() {
    if [[ "${ASSAY_TEST_TIMEOUT_ACTIVE:-0}" == 1 ]]; then
        echo bounded >>"${EVENTS}"
    else
        echo unbounded >>"${EVENTS}"
    fi
    if [[ "$*" == *config.sh* ]]; then
        echo "Settings Saved"
    fi
}
check_runner_service() { return 0; }

cleanup_runner_config
configure_runner fresh-token
start_runner_service

if grep -Fxq unbounded "${EVENTS}"; then
    echo "destructive runner recovery invoked multipass without a timeout" >&2
    cat "${EVENTS}" >&2
    exit 1
fi
if [[ "$(grep -Fxc bounded "${EVENTS}")" -lt 5 ]]; then
    echo "runner recovery timeout test did not exercise every destructive phase" >&2
    cat "${EVENTS}" >&2
    exit 1
fi

echo "ok: runner auto-recovery keeps registration tokens fresh and bounds destructive calls"
