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

for cleanup_phase in service unit credentials ownership; do
    timeout() {
        shift
        case "${cleanup_phase}:$*" in
            service:*"svc.sh stop"* | \
            unit:*"actions.runner.*.service"* | \
            credentials:*".credentials_rsaparams"* | \
            ownership:*"chown -R"*) return 124 ;;
        esac
        "$@"
    }
    set +e
    cleanup_runner_config
    cleanup_status=$?
    set -e
    if [[ "${cleanup_status}" -ne 124 ]]; then
        echo "runner cleanup ${cleanup_phase} did not propagate timeout exit 124 (got ${cleanup_status})" >&2
        exit 1
    fi
done

: >"${EVENTS}"
cleanup_runner_config() {
    echo cleanup >>"${EVENTS}"
    return 124
}
set +e
recover_runner
recovery_cleanup_status=$?
set -e
if [[ "${recovery_cleanup_status}" -ne 124 ]]; then
    echo "runner recovery did not propagate cleanup timeout exit 124 (got ${recovery_cleanup_status})" >&2
    exit 1
fi
if grep -Fxq token "${EVENTS}"; then
    echo "runner recovery generated a token after cleanup failed" >&2
    exit 1
fi

CRONTAB_CAPTURE="${EVENTS}.crontab"
CRONTAB_EXISTING=""
CRONTAB_LIST_STATUS=0
crontab() {
    if [[ "${1:-}" == -l ]]; then
        if [[ "${CRONTAB_LIST_STATUS}" -ne 0 ]]; then
            return "${CRONTAB_LIST_STATUS}"
        fi
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

# Exit 1 is crontab's explicit "no crontab for user" result and is the only
# nonzero read status from which installation may proceed.
CRONTAB_LIST_STATUS=1
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if ! grep -Fq '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}"; then
    echo "runner cron was not installed for a user without an existing crontab" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
CRONTAB_LIST_STATUS=0

CRON_SCRIPT_PATH=$(realpath "$0")
CANONICAL_CRON=$(cat "${CRONTAB_CAPTURE}")

# Pre-marker entries belong to the installer independently of the token-file
# configuration used when they were generated. Each transition must converge
# to one current, locked entry rather than leaving the old recovery job active.
for transition in \
    "none-to-token|*/5 * * * * ${CRON_SCRIPT_PATH} >/tmp/old.log 2>&1|/tmp/token-current" \
    "token-to-none|*/5 * * * * GH_TOKEN_FILE=/tmp/token-old ${CRON_SCRIPT_PATH} >/tmp/old.log 2>&1|" \
    "token-a-to-b|*/5 * * * * GH_TOKEN_FILE=/tmp/token-a ${CRON_SCRIPT_PATH} >/tmp/old.log 2>&1|/tmp/token-b"; do
    IFS='|' read -r transition_name legacy_entry current_token_file <<<"${transition}"
    CRONTAB_EXISTING="${legacy_entry}"
    GH_TOKEN_FILE="${current_token_file}"
    : >"${CRONTAB_CAPTURE}"
    install_cron >/dev/null
    if [[ "$(grep -Fc '# assay-bpf-runner-health-check' "${CRONTAB_CAPTURE}")" -ne 1 ]] || \
        [[ "$(grep -Fc '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}")" -ne 1 ]] || \
        grep -Fq '>/tmp/old.log' "${CRONTAB_CAPTURE}"; then
        echo "runner cron configuration transition did not converge: ${transition_name}" >&2
        cat "${CRONTAB_CAPTURE}" >&2
        exit 1
    fi
done
export GH_TOKEN_FILE=""

CRONTAB_EXISTING=$(printf '%s\n%s\n%s\n%s\n%s\n' \
    '17 * * * * /usr/local/bin/keep-me' \
    '# retained operator note about health_check.sh' \
    '23 * * * * /usr/local/bin/backup-health_check.sh' \
    "${CANONICAL_CRON}" \
    "*/5 * * * * ${CRON_SCRIPT_PATH} >/tmp/old.log 2>&1")
CRONTAB_EXISTING+=$(printf '\n%s\n%s\n%s' \
    "# audit source path ${CRON_SCRIPT_PATH}" \
    "11 * * * * sha256sum ${CRON_SCRIPT_PATH} >/tmp/health.sha" \
    "12 * * * * ${CRON_SCRIPT_PATH}.backup --check")
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if [[ ! -s "${CRONTAB_CAPTURE}" ]]; then
    echo "runner cron migration did not rewrite the mixed canonical and legacy entries" >&2
    exit 1
fi
if grep -Fq "${CRON_SCRIPT_PATH} >/tmp/old.log" "${CRONTAB_CAPTURE}"; then
    echo "runner cron migration retained the unlocked legacy entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
if ! grep -Fxq '17 * * * * /usr/local/bin/keep-me' "${CRONTAB_CAPTURE}"; then
    echo "runner cron migration removed an unrelated cron entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
for unrelated in \
    '# retained operator note about health_check.sh' \
    '23 * * * * /usr/local/bin/backup-health_check.sh' \
    "# audit source path ${CRON_SCRIPT_PATH}" \
    "11 * * * * sha256sum ${CRON_SCRIPT_PATH} >/tmp/health.sha" \
    "12 * * * * ${CRON_SCRIPT_PATH}.backup --check"; do
    if ! grep -Fxq "${unrelated}" "${CRONTAB_CAPTURE}"; then
        echo "runner cron migration removed unrelated content: ${unrelated}" >&2
        cat "${CRONTAB_CAPTURE}" >&2
        exit 1
    fi
done
if [[ "$(grep -Fc '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}")" -ne 1 ]]; then
    echo "runner cron migration did not install exactly one canonical locked entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
if [[ "$(grep -Fc '# assay-bpf-runner-health-check' "${CRONTAB_CAPTURE}")" -ne 1 ]]; then
    echo "runner cron migration did not retain exactly one ownership marker" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi

CRONTAB_EXISTING=$(cat "${CRONTAB_CAPTURE}")
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if [[ -s "${CRONTAB_CAPTURE}" ]]; then
    echo "canonical runner cron installation was not idempotent" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi

CRONTAB_LIST_STATUS=2
: >"${CRONTAB_CAPTURE}"
set +e
install_cron >/dev/null
crontab_failure_status=$?
set -e
if [[ "${crontab_failure_status}" -ne 2 ]]; then
    echo "unexpected crontab read failure was not propagated (got ${crontab_failure_status})" >&2
    exit 1
fi
if [[ -s "${CRONTAB_CAPTURE}" ]]; then
    echo "unexpected crontab read failure replaced existing cron content" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
rm -f "${CRONTAB_CAPTURE}"

echo "ok: runner auto-recovery keeps registration tokens fresh and bounds destructive calls"
