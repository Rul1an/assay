#!/usr/bin/env bash
# shellcheck disable=SC2329 # Test doubles are invoked indirectly by sourced production functions.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="${SCRIPT:-${ROOT}/infra/bpf-runner/health_check.sh}"
EVENTS="$(mktemp)"
GUEST_TEST_ROOT="$(mktemp -d)"
trap 'rm -f "${EVENTS}"; rm -rf "${GUEST_TEST_ROOT}"' EXIT

# shellcheck source=/dev/null
source "${SCRIPT}"

ORIGINAL_RUNNER_DIR="${RUNNER_DIR}"
ORIGINAL_CLEANUP="$(declare -f cleanup_runner_config)"
ORIGINAL_SYNC="$(declare -f sync_vm_time)"
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
eval "${ORIGINAL_SYNC}"
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
        *"timedatectl set-ntp"*) phase="sync-ntp" ;;
        *"systemd-timesyncd"*) phase="sync-restart" ;;
        *"date +%s"*) phase="sync-read" ;;
        *"date -s"*) phase="sync-force" ;;
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
    if [[ "$phase" == sync-read ]]; then
        echo 0
    fi
}
check_runner_service() { return 0; }

sync_vm_time
# Restored from the sourced production definition above via eval.
# shellcheck disable=SC2218
cleanup_runner_config
configure_runner fresh-token
start_runner_service

if grep -Fq 'unbounded:' "${EVENTS}"; then
    echo "destructive runner recovery invoked multipass without a timeout" >&2
    cat "${EVENTS}" >&2
    exit 1
fi
for expected in \
    "bounded:sync-ntp:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:sync-restart:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:sync-read:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
    "bounded:sync-force:${MULTIPASS_RECOVERY_TIMEOUT_SECONDS}" \
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
if grep -Eq 'svc\.sh (stop|uninstall).*\|\| true|systemctl daemon-reload.*\|\| true' <<<"${ORIGINAL_CLEANUP}"; then
    echo "guest-side runner cleanup failures are still ignored" >&2
    exit 1
fi

mkdir -p "${GUEST_TEST_ROOT}/runner" "${GUEST_TEST_ROOT}/bin"
touch "${GUEST_TEST_ROOT}/runner/.service"
cat >"${GUEST_TEST_ROOT}/runner/svc.sh" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
    stop) exit 42 ;;
    uninstall) exit 0 ;;
esac
EOF
cat >"${GUEST_TEST_ROOT}/bin/sudo" <<'EOF'
#!/usr/bin/env bash
exec "$@"
EOF
chmod +x "${GUEST_TEST_ROOT}/runner/svc.sh" "${GUEST_TEST_ROOT}/bin/sudo"
RUNNER_DIR="${GUEST_TEST_ROOT}/runner"
timeout() {
    shift
    "$@"
}
multipass() {
    if [[ "$*" == *"svc.sh stop"* ]]; then
        PATH="${GUEST_TEST_ROOT}/bin:${PATH}" bash -c "${!#}"
    fi
}
set +e
cleanup_runner_config
guest_stop_status=$?
set -e
if [[ "${guest_stop_status}" -ne 42 ]]; then
    echo "runner cleanup masked guest service-stop exit 42 as ${guest_stop_status}" >&2
    exit 1
fi
RUNNER_DIR="${ORIGINAL_RUNNER_DIR}"
eval "${ORIGINAL_CLEANUP}"

for sync_phase in ntp restart read force; do
    timeout() {
        shift
        case "${sync_phase}:$*" in
            ntp:*"timedatectl set-ntp"* | \
            restart:*"systemd-timesyncd"* | \
            read:*"date +%s"* | \
            force:*"date -s"*) return 124 ;;
        esac
        "$@"
    }
    set +e
    sync_vm_time
    sync_status=$?
    set -e
    if [[ "${sync_status}" -ne 124 ]]; then
        echo "runner time sync ${sync_phase} did not propagate timeout exit 124 (got ${sync_status})" >&2
        exit 1
    fi
done

timeout() {
    shift
    if [[ "$*" == *"timedatectl set-ntp"* ]]; then
        return 42
    fi
    "$@"
}
set +e
sync_vm_time
sync_non_timeout_status=$?
set -e
if [[ "${sync_non_timeout_status}" -ne 42 ]]; then
    echo "runner time sync collapsed non-timeout exit 42 to ${sync_non_timeout_status}" >&2
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

: >"${EVENTS}"
log_error() { printf '%s\n' "$*" >>"${EVENTS}"; }
timeout() {
    shift
    echo fresh-token
    return 1
}
set +e
configure_runner fresh-token
configure_secret_status=$?
set -e
if [[ "${configure_secret_status}" -ne 1 ]]; then
    echo "runner configuration secret-output probe returned ${configure_secret_status}" >&2
    exit 1
fi
if grep -Fq fresh-token "${EVENTS}"; then
    echo "runner configuration copied guest output containing the token into logs" >&2
    exit 1
fi
log_error() { :; }

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

sync_vm_time() { :; }
cleanup_runner_config() { :; }
generate_runner_token() { printf '%s\n' fresh-token; }
configure_runner() { return 124; }
set +e
recover_runner
recovery_configure_status=$?
set -e
if [[ "${recovery_configure_status}" -ne 124 ]]; then
    echo "runner recovery collapsed configuration timeout 124 to ${recovery_configure_status}" >&2
    exit 1
fi
configure_runner() { :; }
start_runner_service() { return 124; }
set +e
recover_runner
recovery_service_status=$?
set -e
if [[ "${recovery_service_status}" -ne 124 ]]; then
    echo "runner recovery collapsed service timeout 124 to ${recovery_service_status}" >&2
    exit 1
fi
start_runner_service() { return 73; }
set +e
recover_runner
recovery_service_non_timeout_status=$?
set -e
if [[ "${recovery_service_non_timeout_status}" -ne 73 ]]; then
    echo "runner recovery collapsed service exit 73 to ${recovery_service_non_timeout_status}" >&2
    exit 1
fi

sync_vm_time() { return 42; }
set +e
recover_runner
recovery_sync_non_timeout_status=$?
set -e
if [[ "${recovery_sync_non_timeout_status}" -ne 42 ]]; then
    echo "runner recovery collapsed time-sync exit 42 to ${recovery_sync_non_timeout_status}" >&2
    exit 1
fi

CRONTAB_CAPTURE="${EVENTS}.crontab"
CRONTAB_EXISTING=""
CRONTAB_LIST_STATUS=0
CRONTAB_LIST_ERROR=""
CRONTAB_WRITE_STATUS=0
crontab() {
    if [[ "${1:-}" == -l ]]; then
        if [[ "${CRONTAB_LIST_STATUS}" -ne 0 ]]; then
            printf '%s' "${CRONTAB_LIST_ERROR}" >&2
            return "${CRONTAB_LIST_STATUS}"
        fi
        printf '%s' "${CRONTAB_EXISTING}"
        return 0
    fi
    cat >"${CRONTAB_CAPTURE}"
    return "${CRONTAB_WRITE_STATUS}"
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
CRONTAB_LIST_ERROR="no crontab for test-user"
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if ! grep -Fq '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}"; then
    echo "runner cron was not installed for a user without an existing crontab" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
CRONTAB_LIST_STATUS=0
CRONTAB_LIST_ERROR=""

# BSD/macOS prefixes the same explicit empty-crontab diagnostic with `crontab:`.
CRONTAB_LIST_STATUS=1
CRONTAB_LIST_ERROR="crontab: no crontab for test-user"
: >"${CRONTAB_CAPTURE}"
set +e
install_cron >/dev/null
bsd_no_crontab_status=$?
set -e
if [[ "${bsd_no_crontab_status}" -ne 0 ]]; then
    echo "BSD/macOS no-crontab diagnostic was rejected (got ${bsd_no_crontab_status})" >&2
    exit 1
fi
if ! grep -Fq '/usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock' "${CRONTAB_CAPTURE}"; then
    echo "BSD/macOS no-crontab diagnostic did not allow cron installation" >&2
    exit 1
fi
CRONTAB_LIST_STATUS=0
CRONTAB_LIST_ERROR=""

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

CRONTAB_EXISTING="*/5 * * * * PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin GH_TOKEN_FILE=/tmp/token-live /usr/bin/lockf -t 0 /tmp/assay-bpf-runner-health.lock ${CRON_SCRIPT_PATH} >> /tmp/live.log 2>&1"
GH_TOKEN_FILE="/tmp/token-current"
: >"${CRONTAB_CAPTURE}"
install_cron >/dev/null
if [[ "$(grep -Fc "${CRON_SCRIPT_PATH}" "${CRONTAB_CAPTURE}")" -ne 1 ]] || \
    ! grep -Fq 'PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin' "${CRONTAB_CAPTURE}"; then
    echo "deployed PATH-prefixed runner cron did not converge to one executable entry" >&2
    cat "${CRONTAB_CAPTURE}" >&2
    exit 1
fi
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
CRONTAB_LIST_ERROR="crontab: permission denied"
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

CRONTAB_LIST_STATUS=1
CRONTAB_LIST_ERROR="crontab: permission denied"
: >"${CRONTAB_CAPTURE}"
set +e
install_cron >/dev/null
crontab_exit_one_error_status=$?
set -e
if [[ "${crontab_exit_one_error_status}" -ne 1 ]]; then
    echo "ambiguous crontab exit 1 was not propagated (got ${crontab_exit_one_error_status})" >&2
    exit 1
fi
if [[ -s "${CRONTAB_CAPTURE}" ]]; then
    echo "ambiguous crontab exit 1 replaced existing cron content" >&2
    exit 1
fi

CRONTAB_LIST_STATUS=0
CRONTAB_LIST_ERROR=""
CRONTAB_WRITE_STATUS=73
CRONTAB_EXISTING="17 * * * * /usr/local/bin/keep-me"
: >"${CRONTAB_CAPTURE}"
set +e
install_cron >/dev/null
crontab_write_status=$?
set -e
if [[ "${crontab_write_status}" -ne 73 ]]; then
    echo "crontab write failure was not propagated (got ${crontab_write_status})" >&2
    exit 1
fi
rm -f "${CRONTAB_CAPTURE}"

echo "ok: runner auto-recovery keeps registration tokens fresh and bounds destructive calls"
