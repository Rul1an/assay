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
# Invoked by the sourced production functions below.
# shellcheck disable=SC2329
timeout() {
    local seconds="$1"
    shift
    ASSAY_TEST_TIMEOUT_ACTIVE=1 ASSAY_TEST_TIMEOUT_SECONDS="${seconds}" "$@"
}
multipass() {
    local phase="unknown"
    case "$*" in
        *"svc.sh stop"*) phase="cleanup-service" ;;
        *"actions.runner.*.service"*) phase="cleanup-unit" ;;
        *".credentials_rsaparams"*) phase="cleanup-credentials" ;;
        *"chown -R"*"config.sh"*) phase="configure" ;;
        *"chown -R"*) phase="cleanup-ownership" ;;
        *"svc.sh install"*) phase="service-install" ;;
        *"svc.sh start"*) phase="service-start" ;;
    esac
    if [[ "${ASSAY_TEST_TIMEOUT_ACTIVE:-0}" == 1 ]]; then
        echo "bounded:${phase}:${ASSAY_TEST_TIMEOUT_SECONDS}" >>"${EVENTS}"
    else
        echo "unbounded:${phase}" >>"${EVENTS}"
    fi
    if [[ "$*" == *fresh-token* ]]; then
        echo token-in-host-argv >>"${EVENTS}"
    fi
    if [[ "$*" == *config.sh* ]]; then
        echo "Settings Saved"
    fi
}
check_runner_service() { return 0; }

cleanup_runner_config
configure_runner fresh-token
start_runner_service

if grep -Fq 'unbounded:' "${EVENTS}"; then
    echo "destructive runner recovery invoked multipass without a timeout" >&2
    cat "${EVENTS}" >&2
    exit 1
fi
for expected in \
    "bounded:cleanup-service:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:cleanup-unit:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:cleanup-credentials:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:cleanup-ownership:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:configure:${RUNNER_CONFIG_TIMEOUT_SECONDS}" \
    "bounded:service-install:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:service-start:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}"; do
    if ! grep -Fxq "${expected}" "${EVENTS}"; then
        echo "runner recovery did not exercise bounded phase: ${expected}" >&2
        cat "${EVENTS}" >&2
        exit 1
    fi
done
if grep -Fxq token-in-host-argv "${EVENTS}"; then
    echo "runner registration token leaked into the host-side multipass argv" >&2
    exit 1
fi

# Invoked by the sourced production functions below.
# shellcheck disable=SC2329
timeout() {
    shift
    if [[ "$*" == *config.sh* ]]; then
        echo "Settings Saved"
        return 124
    fi
    "$@"
}
set +e
configure_runner fresh-token
configure_status=$?
set -e
if [[ "${configure_status}" -ne 124 ]]; then
    echo "runner configuration did not propagate timeout exit 124 (got ${configure_status})" >&2
    exit 1
fi

TIMEOUT_SERVICE_PHASE="install"
timeout() {
    shift
    if [[ "${TIMEOUT_SERVICE_PHASE}" == install && "$*" == *"svc.sh install"* ]]; then
        echo "installed"
        return 124
    fi
    if [[ "${TIMEOUT_SERVICE_PHASE}" == start && "$*" == *"svc.sh start"* ]]; then
        echo "started"
        return 124
    fi
    "$@"
}
set +e
start_runner_service
install_status=$?
set -e
if [[ "${install_status}" -ne 124 ]]; then
    echo "runner service install did not propagate timeout exit 124 (got ${install_status})" >&2
    exit 1
fi

TIMEOUT_SERVICE_PHASE="start"
set +e
start_runner_service
start_status=$?
set -e
if [[ "${start_status}" -ne 124 ]]; then
    echo "runner service start did not propagate timeout exit 124 (got ${start_status})" >&2
    exit 1
fi

CRONTAB_CAPTURE="${EVENTS}.crontab"
CRONTAB_EXISTING=""
crontab() {
    if [[ "${1:-}" == -l ]]; then
        printf '%s' "${CRONTAB_EXISTING}"
        return 0
    fi
    cat >"${CRONTAB_CAPTURE}"
}
install_cron >/dev/null
if ! grep -Fq '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}"; then
    echo "installed runner cron does not prevent overlapping recovery runs" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi

CRONTAB_EXISTING=$'17 * * * * /usr/local/bin/keep-me\n*/5 * * * * /old/health_check.sh >/tmp/old.log 2>&1\n'
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if grep -Fq '/old/health_check.sh' "${CRONTAB_CAPTURE}"; then
    echo "runner cron migration retained the unlocked legacy entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
if ! grep -Fxq '17 * * * * /usr/local/bin/keep-me' "${CRONTAB_CAPTURE}"; then
    echo "runner cron migration removed an unrelated cron entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
if [[ "$(grep -Fc '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}")" -ne 1 ]]; then
    echo "runner cron migration did not install exactly one canonical locked entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
rm -f "${CRONTAB_CAPTURE}"

echo "ok: runner auto-recovery keeps registration tokens fresh and bounds destructive calls"
