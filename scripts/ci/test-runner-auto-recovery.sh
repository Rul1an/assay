#!/usr/bin/env bash
# shellcheck disable=SC2218,SC2329 # Test doubles are restored/redefined and invoked indirectly.
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

# Admission uses only synthetic API responses; no guest or credentials are read.
gh() {
    [[ "$*" == "api --paginate repos/$REPO/actions/runners?per_page=100" ]] || return 92
    printf '%s' "${ADMISSION_BODY}"
    return "${ADMISSION_API_RC:-0}"
}
GH_CMD=gh
timeout() {
    [[ "$1" == "$MULTIPASS_RECOVERY_TIMEOUT_SECONDS" ]] || return 93
    shift
    [[ "${ADMISSION_TIMEOUT:-0}" == 0 ]] || return 124
    "$@"
}
ADMISSION_API_RC=0
while IFS='|' read -r case_name expected ADMISSION_BODY; do
    : >"${EVENTS}"
    rc=0
    recover_runner || rc=$?
    if [[ "$expected" == refuse ]]; then
        if [[ "$rc" -eq 0 || -s "${EVENTS}" ]]; then
            echo "admission must refuse ${case_name} before recovery side effects" >&2
            exit 1
        fi
    elif [[ "$rc" -ne 0 ]] || ! grep -Fxq cleanup "${EVENTS}"; then
        echo "admission must allow ${case_name}" >&2
        exit 1
    fi
done <<'ADMISSION_CASES'
busy|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"online","busy":true}]}
missing|refuse|{"runners":[]}
empty|refuse|
malformed|refuse|{
wrong-page|refuse|{"runners":null}
missing-busy|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline"}]}
string-busy|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":"false"}]}
unknown-status|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"unknown","busy":false}]}
missing-id|refuse|{"runners":[{"name":"assay-bpf-runner","status":"offline","busy":false}]}
zero-id|refuse|{"runners":[{"id":0,"name":"assay-bpf-runner","status":"offline","busy":false}]}
fractional-id|refuse|{"runners":[{"id":1.5,"name":"assay-bpf-runner","status":"offline","busy":false}]}
malformed-second-page|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":false}]} {}
duplicate-pages|refuse|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":false}]} {"runners":[{"id":2,"name":"assay-bpf-runner","status":"offline","busy":false}]}
online-idle|allow|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"online","busy":false}]}
offline-idle|allow|{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":false}]}
second-page|allow|{"runners":[]} {"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":false}]}
ADMISSION_CASES
ADMISSION_BODY='{"runners":[{"id":1,"name":"assay-bpf-runner","status":"offline","busy":false}]}'
ADMISSION_API_RC=1
: >"${EVENTS}"
rc=0
recover_runner || rc=$?
if [[ "$rc" -eq 0 || -s "${EVENTS}" ]]; then
    echo "failed API after valid partial output must refuse recovery" >&2
    exit 1
fi
ADMISSION_API_RC=0
ADMISSION_VALID_BODY="$ADMISSION_BODY"
for admission_failure in timeout oversize; do
    ADMISSION_BODY="$ADMISSION_VALID_BODY"
    ADMISSION_TIMEOUT=0
    if [[ "$admission_failure" == timeout ]]; then
        ADMISSION_TIMEOUT=1
    else
        # Valid JSON plus spaces, exactly one byte beyond the response ceiling.
        printf -v padding '%*s' "$((1048577 - ${#ADMISSION_BODY}))" ''
        ADMISSION_BODY+="$padding"
    fi
    : >"${EVENTS}"
    rc=0
    recover_runner || rc=$?
    if [[ "$rc" -eq 0 || -s "${EVENTS}" ]]; then
        echo "${admission_failure} observation must refuse before recovery" >&2
        exit 1
    fi
done
ADMISSION_TIMEOUT=0
ADMISSION_BODY="$ADMISSION_VALID_BODY"
: >"${EVENTS}"
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
TIMEOUT_SERVICE_STATUS=124
timeout() {
    shift
    if [[ "${TIMEOUT_SERVICE_PHASE}" == install && "$*" == *"svc.sh install"* ]]; then
        echo "installed"
        return "${TIMEOUT_SERVICE_STATUS}"
    fi
    if [[ "${TIMEOUT_SERVICE_PHASE}" == start && "$*" == *"svc.sh start"* ]]; then
        echo "started"
        return "${TIMEOUT_SERVICE_STATUS}"
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

TIMEOUT_SERVICE_PHASE="install"
TIMEOUT_SERVICE_STATUS=73
set +e
start_runner_service
install_non_timeout_status=$?
set -e
if [[ "${install_non_timeout_status}" -ne 73 ]]; then
    echo "runner service install collapsed non-timeout exit 73 to ${install_non_timeout_status}" >&2
    exit 1
fi

TIMEOUT_SERVICE_PHASE="start"
TIMEOUT_SERVICE_STATUS=42
set +e
start_runner_service
start_non_timeout_status=$?
set -e
if [[ "${start_non_timeout_status}" -ne 42 ]]; then
    echo "runner service start collapsed non-timeout exit 42 to ${start_non_timeout_status}" >&2
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

# Cancellation is asynchronous: command success is a request, not a terminal run.
# shellcheck disable=SC2030,SC2031 # Each test subshell supplies its own complete stub environment.
(
    export GH_CMD=cancel_test_gh
    log_info() { printf 'INFO %s\n' "$*"; }
    log_warn() { printf 'WARN %s\n' "$*"; }
    log_ok() { printf 'OK %s\n' "$*"; }
    cancel_test_gh() {
        case "$1 $2" in
            'run list')
                if [[ "$mode" == list_failure ]]; then return 1; fi
                if [[ "$mode" == malformed ]]; then printf '{\n'; return; fi
                if [[ "$mode" == blank ]]; then return 0; fi
                if [[ "$mode" == object ]]; then printf '{}\n'; return; fi
                if [[ "$mode" == scalar ]]; then printf 'null\n'; return; fi
                if [[ "$mode" == multiple ]]; then printf '[]\n[]\n'; return; fi
                if [[ "$mode" == empty ]]; then printf '[]\n'; return; fi
                printf '[{"databaseId":123,"createdAt":"2000-01-01T00:00:00Z"}]\n'
                ;;
            'run cancel')
                printf '%s\n' "$3" >>"${EVENTS}"
                [[ "$mode" != rejected ]]
                ;;
            *) echo 'unexpected GitHub operation' >&2; return 99 ;;
        esac
    }
    for mode in accepted rejected list_failure malformed blank object scalar multiple empty; do
        : >"${EVENTS}"
        rc=0
        output=$(cancel_stale_jobs) || rc=$?
        if [[ "$mode" == accepted ]]; then
            [[ "$rc" == 0 && "$output" == *'Cancellation requested for 1 stale queued runs'* ]] || {
                echo "accepted cancellation must be reported as requested: $output" >&2; exit 1;
            }
        elif [[ "$mode" == empty ]]; then
            [[ "$rc" == 0 && "$output" == *'No stale jobs found'* ]] || exit 1
        else
            [[ "$rc" != 0 ]] || { echo "$mode must not return success: $output" >&2; exit 1; }
        fi
        if [[ "$mode" == accepted || "$mode" == rejected ]]; then
            [[ "$(cat "${EVENTS}")" == 123 ]] || { echo 'cancellation was not invoked exactly once' >&2; exit 1; }
        elif [[ -s "${EVENTS}" ]]; then
            echo 'invalid or empty inventory invoked cancellation' >&2; exit 1
        fi
        if [[ "$output" == *'Cancelled '* ]]; then
            echo "unconfirmed cancellation reported as completed: $output" >&2
            exit 1
        fi
    done

    # Call in a conditional context, where Bash disables implicit errexit.
    check_gh_auth() { :; }
    check_vm_running() { :; }
    ensure_assay_cli_current() { :; }
    get_runner_status() { printf 'online\n'; }
    cancel_superseded_runs() { printf 'MAINTENANCE_AFTER_STALE\n'; }
    prioritize_pr_runs() { :; }
    heal_action_cache() { :; }
    clean_actions_cache() { :; }
    mode=rejected
    if output=$(health_check); then
        echo "failed cancellation must fail the health-check caller: $output" >&2
        exit 1
    fi
    [[ "$output" != *MAINTENANCE_AFTER_STALE* ]] || exit 1
    mode=accepted
    output=$(health_check)
    [[ "$output" == *MAINTENANCE_AFTER_STALE* ]] || exit 1
)

# ------------------------------------------------------------------------------
# cancel_superseded_runs / prioritize_pr_runs result-truth (issue #2985)
# cancel_stale_jobs is exercised in the preceding block.
# ------------------------------------------------------------------------------
GH_STUB_DIR="$(mktemp -d)"
trap 'rm -f "${EVENTS}"; rm -rf "${GUEST_TEST_ROOT}" "${GH_STUB_DIR}"' EXIT

cat >"${GH_STUB_DIR}/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
mode_file="${GH_STUB_MODE_FILE:?}"
log_file="${GH_STUB_LOG:?}"
mode="$(cat "${mode_file}")"
printf 'ARGS:%s\n' "$*" >>"${log_file}"

if [[ "$*" == *"run list"* ]]; then
  case "${mode}" in
    supersede-ok|supersede-cancel-fail)
      cat <<'JSON'
[{"databaseId":111,"workflowName":"CI","headBranch":"feat","event":"push","createdAt":"2026-09-13T10:00:00Z"},{"databaseId":222,"workflowName":"CI","headBranch":"feat","event":"push","createdAt":"2026-09-13T11:00:00Z"}]
JSON
      exit 0
      ;;
    supersede-empty)
      echo '[]'
      exit 0
      ;;
    supersede-malformed)
      # gh succeeds but body is not JSON — jq must refuse
      printf '%s\n' '{'
      exit 0
      ;;
    supersede-blank)
      # gh succeeds with empty stdout — not a JSON array
      exit 0
      ;;
    supersede-object)
      printf '%s\n' '{}'
      exit 0
      ;;
    supersede-null)
      printf '%s\n' 'null'
      exit 0
      ;;
    supersede-list-fail|prio-list-fail)
      echo "stub list failure" >&2
      exit 1
      ;;
    prio-ok|prio-cancel-fail)
      if [[ "$*" == *"--event pull_request"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": i} for i in range(1, 7)]))'
        exit 0
      fi
      if [[ "$*" == *"--event push"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": 9001}, {"databaseId": 9002}]))'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    prio-balanced)
      if [[ "$*" == *"--event pull_request"* ]]; then
        echo '[{"databaseId":1}]'
        exit 0
      fi
      if [[ "$*" == *"--event push"* ]]; then
        echo '[{"databaseId":2}]'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    prio-malformed-pr)
      if [[ "$*" == *"--event pull_request"* ]]; then
        printf '%s\n' '{'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    prio-malformed-push)
      if [[ "$*" == *"--event pull_request"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": i} for i in range(1, 7)]))'
        exit 0
      fi
      if [[ "$*" == *"--event push"* ]]; then
        printf '%s\n' '{'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    prio-malformed-push2)
      if [[ "$*" == *"--event pull_request"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": i} for i in range(1, 7)]))'
        exit 0
      fi
      if [[ "$*" == *"--event push"* && "$*" == *"--limit 10"* ]]; then
        printf '%s\n' '{'
        exit 0
      fi
      if [[ "$*" == *"--event push"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": 9001}, {"databaseId": 9002}]))'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    prio-blank-pr|prio-object-pr|prio-null-pr)
      if [[ "$*" == *"--event pull_request"* ]]; then
        case "${mode}" in
          prio-blank-pr) exit 0 ;;
          prio-object-pr) printf '%s\n' '{}'; exit 0 ;;
          prio-null-pr) printf '%s\n' 'null'; exit 0 ;;
        esac
      fi
      echo '[]'
      exit 0
      ;;
    prio-blank-push|prio-object-push|prio-null-push)
      if [[ "$*" == *"--event pull_request"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": i} for i in range(1, 7)]))'
        exit 0
      fi
      if [[ "$*" == *"--event push"* ]]; then
        case "${mode}" in
          prio-blank-push) exit 0 ;;
          prio-object-push) printf '%s\n' '{}'; exit 0 ;;
          prio-null-push) printf '%s\n' 'null'; exit 0 ;;
        esac
      fi
      echo '[]'
      exit 0
      ;;
    prio-blank-push2|prio-object-push2|prio-null-push2)
      if [[ "$*" == *"--event pull_request"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": i} for i in range(1, 7)]))'
        exit 0
      fi
      if [[ "$*" == *"--event push"* && "$*" == *"--limit 10"* ]]; then
        case "${mode}" in
          prio-blank-push2) exit 0 ;;
          prio-object-push2) printf '%s\n' '{}'; exit 0 ;;
          prio-null-push2) printf '%s\n' 'null'; exit 0 ;;
        esac
      fi
      if [[ "$*" == *"--event push"* ]]; then
        python3 -c 'import json; print(json.dumps([{"databaseId": 9001}, {"databaseId": 9002}]))'
        exit 0
      fi
      echo '[]'
      exit 0
      ;;
    *)
      echo "unknown stub mode ${mode}" >&2
      exit 99
      ;;
  esac
fi

if [[ "$*" == *"run cancel"* ]]; then
  case "${mode}" in
    *cancel-fail*)
      echo "stub cancel refused" >&2
      exit 1
      ;;
    *)
      echo "stub cancel accepted" >&2
      exit 0
      ;;
  esac
fi

echo "stub unhandled: $*" >&2
exit 98
STUB
chmod +x "${GH_STUB_DIR}/gh"

CAPTURE="$(mktemp)"
restore_queue_logs() {
    log_info() { :; }
    log_warn() { :; }
    log_error() { :; }
    log_ok() { :; }
}
capture_queue_logs() {
    : >"${CAPTURE}"
    log_info() { printf 'INFO:%s\n' "$*" >>"${CAPTURE}"; }
    log_warn() { printf 'WARN:%s\n' "$*" >>"${CAPTURE}"; }
    log_error() { printf 'ERROR:%s\n' "$*" >>"${CAPTURE}"; }
    log_ok() { printf 'OK:%s\n' "$*" >>"${CAPTURE}"; }
}

run_queue_case() {
    local mode="$1"
    local fn="$2"
    local expect_exit="$3"
    local expect_ok_substr="${4:-}"
    local forbid_cancelled="${5:-1}"
    local outdir="${GH_STUB_DIR}/${mode}-${fn}"
    mkdir -p "${outdir}"
    printf '%s\n' "${mode}" >"${outdir}/mode"
    : >"${outdir}/stub.log"
    capture_queue_logs
    set +e
    # shellcheck disable=SC2030,SC2031 # Stub configuration is deliberately isolated per invocation.
    (
        export GH_CMD="${GH_STUB_DIR}/gh"
        export GH_STUB_MODE_FILE="${outdir}/mode"
        export GH_STUB_LOG="${outdir}/stub.log"
        "${fn}"
    )
    local got=$?
    set -e
    restore_queue_logs
    if [[ "${got}" -ne "${expect_exit}" ]]; then
        echo "cancel_superseded_runs result-truth: ${fn} mode=${mode} expected exit ${expect_exit}, got ${got}" >&2
        cat "${CAPTURE}" >&2
        cat "${outdir}/stub.log" >&2
        exit 1
    fi
    if [[ -n "${expect_ok_substr}" ]]; then
        if ! grep -Fq "OK:${expect_ok_substr}" "${CAPTURE}"; then
            echo "cancel_superseded_runs result-truth: ${fn} mode=${mode} missing OK substring: ${expect_ok_substr}" >&2
            cat "${CAPTURE}" >&2
            exit 1
        fi
    else
        if grep -Fq 'OK:' "${CAPTURE}"; then
            echo "cancel_superseded_runs result-truth: ${fn} mode=${mode} unexpected OK line" >&2
            cat "${CAPTURE}" >&2
            exit 1
        fi
    fi
    if [[ "${forbid_cancelled}" -eq 1 ]] && grep -Ei 'OK:.*[Cc]ancelled' "${CAPTURE}"; then
        echo "cancel_superseded_runs result-truth: must not claim terminal cancelled in OK line" >&2
        cat "${CAPTURE}" >&2
        exit 1
    fi
}

# Positive + failure matrix for cancel_superseded_runs
run_queue_case supersede-ok cancel_superseded_runs 0 "Cancel request accepted for 1 superseded runs"
run_queue_case supersede-cancel-fail cancel_superseded_runs 1 ""
run_queue_case supersede-list-fail cancel_superseded_runs 1 ""
run_queue_case supersede-empty cancel_superseded_runs 0 ""
run_queue_case supersede-malformed cancel_superseded_runs 1 ""
run_queue_case supersede-blank cancel_superseded_runs 1 ""
run_queue_case supersede-object cancel_superseded_runs 1 ""
run_queue_case supersede-null cancel_superseded_runs 1 ""

# Positive + failure matrix for prioritize_pr_runs
run_queue_case prio-ok prioritize_pr_runs 0 "Cancel request accepted for 2 push runs (PR priority)"
run_queue_case prio-cancel-fail prioritize_pr_runs 1 ""
run_queue_case prio-list-fail prioritize_pr_runs 1 ""
run_queue_case prio-balanced prioritize_pr_runs 0 ""
run_queue_case prio-malformed-pr prioritize_pr_runs 1 ""
run_queue_case prio-malformed-push prioritize_pr_runs 1 ""
run_queue_case prio-malformed-push2 prioritize_pr_runs 1 ""
run_queue_case prio-blank-pr prioritize_pr_runs 1 ""
run_queue_case prio-object-pr prioritize_pr_runs 1 ""
run_queue_case prio-null-pr prioritize_pr_runs 1 ""
run_queue_case prio-blank-push prioritize_pr_runs 1 ""
run_queue_case prio-object-push prioritize_pr_runs 1 ""
run_queue_case prio-null-push prioritize_pr_runs 1 ""
run_queue_case prio-blank-push2 prioritize_pr_runs 1 ""
run_queue_case prio-object-push2 prioritize_pr_runs 1 ""
run_queue_case prio-null-push2 prioritize_pr_runs 1 ""

# if-caller: cancel-fail must take the false branch
mkdir -p "${GH_STUB_DIR}/if-prio"
printf 'prio-cancel-fail\n' >"${GH_STUB_DIR}/if-prio/mode"
: >"${GH_STUB_DIR}/if-prio/stub.log"
capture_queue_logs
if_branch=""
set +e
# shellcheck disable=SC2030,SC2031 # This case does not inherit another case's stub configuration.
(
    export GH_CMD="${GH_STUB_DIR}/gh"
    export GH_STUB_MODE_FILE="${GH_STUB_DIR}/if-prio/mode"
    export GH_STUB_LOG="${GH_STUB_DIR}/if-prio/stub.log"
    if prioritize_pr_runs; then
        if_branch=true
    else
        if_branch=false
    fi
    printf '%s\n' "${if_branch}" >"${GH_STUB_DIR}/if-prio/branch"
)
set -e
restore_queue_logs
if [[ "$(cat "${GH_STUB_DIR}/if-prio/branch")" != "false" ]]; then
    echo "prioritize_pr_runs if-caller stayed true after cancel refusal" >&2
    exit 1
fi

mkdir -p "${GH_STUB_DIR}/if-supersede-malformed"
printf 'supersede-malformed\n' >"${GH_STUB_DIR}/if-supersede-malformed/mode"
: >"${GH_STUB_DIR}/if-supersede-malformed/stub.log"
capture_queue_logs
set +e
# shellcheck disable=SC2030,SC2031 # This case supplies all three stub variables independently.
(
    export GH_CMD="${GH_STUB_DIR}/gh"
    export GH_STUB_MODE_FILE="${GH_STUB_DIR}/if-supersede-malformed/mode"
    export GH_STUB_LOG="${GH_STUB_DIR}/if-supersede-malformed/stub.log"
    if cancel_superseded_runs; then
        printf 'true\n' >"${GH_STUB_DIR}/if-supersede-malformed/branch"
    else
        printf 'false\n' >"${GH_STUB_DIR}/if-supersede-malformed/branch"
    fi
)
set -e
restore_queue_logs
if [[ "$(cat "${GH_STUB_DIR}/if-supersede-malformed/branch")" != "false" ]]; then
    echo "cancel_superseded_runs if-caller stayed true after malformed list payload" >&2
    cat "${CAPTURE}" >&2
    exit 1
fi

# health_check online path must not report healthy when superseded cancel refuses
ORIGINAL_CHECK_GH="$(declare -f check_gh_auth)"
ORIGINAL_CHECK_VM="$(declare -f check_vm_running)"
ORIGINAL_ENSURE="$(declare -f ensure_assay_cli_current)"
ORIGINAL_GET_STATUS="$(declare -f get_runner_status)"
ORIGINAL_STALE="$(declare -f cancel_stale_jobs)"
ORIGINAL_HEAL="$(declare -f heal_action_cache)"
ORIGINAL_CLEAN_CACHE="$(declare -f clean_actions_cache)"
ORIGINAL_SUPERSEDED="$(declare -f cancel_superseded_runs)"
ORIGINAL_PRIO="$(declare -f prioritize_pr_runs)"

check_gh_auth() { return 0; }
check_vm_running() { return 0; }
ensure_assay_cli_current() { return 0; }
get_runner_status() { printf '%s\n' online; }
cancel_stale_jobs() { return 0; }
heal_action_cache() { return 0; }
clean_actions_cache() { return 0; }

# Force real cancel_superseded_runs against cancel-fail stub; keep prioritize no-op success
prioritize_pr_runs() { return 0; }
mkdir -p "${GH_STUB_DIR}/health"
printf 'supersede-cancel-fail\n' >"${GH_STUB_DIR}/health/mode"
: >"${GH_STUB_DIR}/health/stub.log"
capture_queue_logs
set +e
# shellcheck disable=SC2030,SC2031 # Health-check probe environment must not escape to later tests.
(
    export GH_CMD="${GH_STUB_DIR}/gh"
    export GH_STUB_MODE_FILE="${GH_STUB_DIR}/health/mode"
    export GH_STUB_LOG="${GH_STUB_DIR}/health/stub.log"
    health_check
)
health_status=$?
set -e
restore_queue_logs
if [[ "${health_status}" -eq 0 ]]; then
    echo "health_check returned 0 after superseded cancel refusal" >&2
    cat "${CAPTURE}" >&2
    exit 1
fi
if grep -Fq 'OK:Runner is healthy' "${CAPTURE}"; then
    echo "health_check claimed healthy after superseded cancel refusal" >&2
    cat "${CAPTURE}" >&2
    exit 1
fi

# restore originals used later? suite ends after this.
eval "${ORIGINAL_CHECK_GH}"
eval "${ORIGINAL_CHECK_VM}"
eval "${ORIGINAL_ENSURE}"
eval "${ORIGINAL_GET_STATUS}"
eval "${ORIGINAL_STALE}"
eval "${ORIGINAL_HEAL}"
eval "${ORIGINAL_CLEAN_CACHE}"
eval "${ORIGINAL_SUPERSEDED}"
eval "${ORIGINAL_PRIO}"
rm -f "${CAPTURE}"

echo "ok: runner auto-recovery keeps registration tokens fresh and bounds destructive calls"
