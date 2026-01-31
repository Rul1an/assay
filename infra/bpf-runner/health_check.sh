#!/bin/bash
# ==============================================================================
# Self-Hosted Runner Health Check & Auto-Recovery
#
# Checks if the runner is online and auto-recovers if:
# - Runner is offline
# - Token expired (clock drift)
# - Service not running
#
# USAGE:
#   # Manual run
#   ./health_check.sh
#
#   # Install as cron (every 5 minutes)
#   ./health_check.sh --install-cron
#
# REQUIREMENTS:
#   - gh CLI authenticated with repo admin access
#   - multipass installed and VM running
#   - GITHUB_TOKEN or gh auth login
#
# ==============================================================================
set -euo pipefail

# Configuration
VM_NAME="${VM_NAME:-assay-bpf-runner}"
REPO="${REPO:-Rul1an/assay}"
RUNNER_NAME="${RUNNER_NAME:-assay-bpf-runner}"
RUNNER_DIR="${RUNNER_DIR:-/opt/actions-runner}"
RUNNER_USER="${RUNNER_USER:-github-runner}"
RUNNER_LABELS="${RUNNER_LABELS:-self-hosted,Linux,X64,bpf-lsm}"
LOG_FILE="${LOG_FILE:-/tmp/runner-health-check.log}"
MAX_LOG_SIZE=1048576  # 1MB

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    local level="$1"
    shift
    local msg="$*"
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "[$timestamp] [$level] $msg" | tee -a "$LOG_FILE"
}

log_info()  { log "INFO" "$*"; }
log_warn()  { log "${YELLOW}WARN${NC}" "$*"; }
log_error() { log "${RED}ERROR${NC}" "$*"; }
log_ok()    { log "${GREEN}OK${NC}" "$*"; }

# Rotate log if too large
rotate_log() {
    if [[ -f "$LOG_FILE" ]] && [[ $(stat -f%z "$LOG_FILE" 2>/dev/null || stat -c%s "$LOG_FILE" 2>/dev/null) -gt $MAX_LOG_SIZE ]]; then
        mv "$LOG_FILE" "${LOG_FILE}.old"
        log_info "Log rotated"
    fi
}

# Check if gh CLI is available and authenticated
check_gh_auth() {
    # Try common gh locations for cron compatibility
    local gh_cmd=""
    for path in gh /opt/homebrew/bin/gh /usr/local/bin/gh /usr/bin/gh; do
        if command -v "$path" &>/dev/null; then
            gh_cmd="$path"
            break
        fi
    done

    if [[ -z "$gh_cmd" ]]; then
        log_error "gh CLI not found. Install with: brew install gh"
        return 1
    fi

    # Export GH path for use in other functions
    export GH_CMD="$gh_cmd"

    if ! $gh_cmd auth status &>/dev/null; then
        log_error "gh CLI not authenticated. Run: gh auth login"
        return 1
    fi
    return 0
}

# Check if multipass VM is running
check_vm_running() {
    if ! command -v multipass &>/dev/null; then
        log_error "multipass not found"
        return 1
    fi

    local state
    state=$(multipass info "$VM_NAME" 2>/dev/null | grep -E "^State:" | awk '{print $2}' || echo "NotFound")

    if [[ "$state" != "Running" ]]; then
        log_warn "VM '$VM_NAME' is not running (state: $state)"
        if [[ "$state" == "Stopped" ]] || [[ "$state" == "Suspended" ]]; then
            log_info "Starting VM..."
            multipass start "$VM_NAME"
            sleep 10
            return 0
        fi
        return 1
    fi
    return 0
}

# Check if there are queued jobs waiting for our runner labels
check_queued_jobs() {
    local queued_count
    local gh="${GH_CMD:-gh}"
    queued_count=$($gh api "repos/$REPO/actions/runs?status=queued" --jq '.workflow_runs | length' 2>/dev/null || echo "0")

    if [[ "$queued_count" -gt 0 ]]; then
        log_info "Found $queued_count queued workflow runs"
        return 0
    fi
    return 1
}

# Get runner status from GitHub API
get_runner_status() {
    local status
    local gh="${GH_CMD:-gh}"
    status=$($gh api "repos/$REPO/actions/runners" --jq ".runners[] | select(.name == \"$RUNNER_NAME\") | .status" 2>/dev/null || echo "unknown")
    echo "$status"
}

# Check if runner service is running in VM
check_runner_service() {
    local service_status
    # Use timeout to prevent hanging
    service_status=$(timeout 10 multipass exec "$VM_NAME" -- \
        sudo systemctl is-active "actions.runner.${REPO/\//-}.$RUNNER_NAME.service" 2>/dev/null || echo "inactive")

    if [[ "$service_status" == "active" ]]; then
        return 0
    fi
    return 1
}

# Sync VM time via NTP (fixes token expiration due to clock drift)
sync_vm_time() {
    log_info "Synchronizing VM time via NTP..."
    multipass exec "$VM_NAME" -- sudo timedatectl set-ntp true 2>/dev/null || true
    multipass exec "$VM_NAME" -- sudo systemctl restart systemd-timesyncd 2>/dev/null || true
    sleep 3

    local vm_time host_time
    vm_time=$(multipass exec "$VM_NAME" -- date '+%s' 2>/dev/null || echo "0")
    host_time=$(date '+%s')
    local drift=$((host_time - vm_time))

    if [[ ${drift#-} -gt 60 ]]; then
        log_warn "Clock drift detected: ${drift}s - forcing time sync"
        multipass exec "$VM_NAME" -- sudo date -s "@$host_time" 2>/dev/null || true
    fi

    log_ok "VM time synchronized (drift: ${drift}s)"
}

# Generate new runner registration token
generate_runner_token() {
    local token
    local gh="${GH_CMD:-gh}"
    token=$($gh api -X POST "repos/$REPO/actions/runners/registration-token" --jq '.token' 2>/dev/null)

    if [[ -z "$token" ]]; then
        log_error "Failed to generate runner token"
        return 1
    fi
    echo "$token"
}

# Clean up old runner configuration
cleanup_runner_config() {
    log_info "Cleaning up old runner configuration..."

    # Stop and uninstall service (MUST run from runner directory)
    multipass exec "$VM_NAME" -- bash -c "
        cd $RUNNER_DIR || exit 0
        sudo ./svc.sh stop 2>/dev/null || true
        sudo ./svc.sh uninstall 2>/dev/null || true
    " 2>/dev/null || true

    # Remove old service files
    multipass exec "$VM_NAME" -- sudo bash -c "
        rm -f /etc/systemd/system/actions.runner.*.service 2>/dev/null || true
        systemctl daemon-reload 2>/dev/null || true
    " 2>/dev/null || true

    # Remove credentials to force fresh registration
    multipass exec "$VM_NAME" -- sudo rm -f \
        "$RUNNER_DIR/.runner" \
        "$RUNNER_DIR/.credentials" \
        "$RUNNER_DIR/.credentials_rsaparams" \
        "$RUNNER_DIR/.service" \
        2>/dev/null || true

    log_ok "Runner configuration cleaned"
}

# Configure runner with new token
configure_runner() {
    local token="$1"

    log_info "Configuring runner with new token..."

    local result
    result=$(multipass exec "$VM_NAME" -- sudo -u "$RUNNER_USER" \
        "$RUNNER_DIR/config.sh" \
        --url "https://github.com/$REPO" \
        --token "$token" \
        --labels "$RUNNER_LABELS" \
        --name "$RUNNER_NAME" \
        --unattended \
        --replace 2>&1)

    # Check for success indicators (handles both fresh install and replacement)
    if echo "$result" | grep -qE "(Successfully|Settings Saved)"; then
        log_ok "Runner configured successfully"
        return 0
    else
        log_error "Runner configuration failed: $result"
        return 1
    fi
}

# Install and start runner service
start_runner_service() {
    log_info "Installing and starting runner service..."

    # MUST run svc.sh from the runner directory
    local install_result
    install_result=$(multipass exec "$VM_NAME" -- bash -c "
        cd $RUNNER_DIR && sudo ./svc.sh install $RUNNER_USER 2>&1
    ")
    log_info "Install output: $install_result"

    local start_result
    start_result=$(multipass exec "$VM_NAME" -- bash -c "
        cd $RUNNER_DIR && sudo ./svc.sh start 2>&1
    ")
    log_info "Start output: $start_result"

    sleep 5

    if check_runner_service; then
        log_ok "Runner service started"
        return 0
    else
        log_error "Runner service failed to start"
        return 1
    fi
}

# Full recovery procedure
recover_runner() {
    log_warn "Starting runner recovery..."

    # Step 1: Sync time
    sync_vm_time

    # Step 2: Generate new token
    local token
    token=$(generate_runner_token) || return 1

    # Step 3: Clean up old config
    cleanup_runner_config

    # Step 4: Configure with new token
    configure_runner "$token" || return 1

    # Step 5: Start service
    start_runner_service || return 1

    # Step 6: Verify online status
    sleep 10
    local status
    status=$(get_runner_status)

    if [[ "$status" == "online" ]]; then
        log_ok "Runner recovery successful! Status: online"
        return 0
    else
        log_error "Runner recovery completed but status is: $status"
        return 1
    fi
}

# Main health check logic
health_check() {
    log_info "=== Runner Health Check Started ==="

    # Pre-flight checks
    check_gh_auth || return 1
    check_vm_running || return 1

    # Get current status
    local status
    status=$(get_runner_status)
    log_info "Runner '$RUNNER_NAME' status: $status"

    if [[ "$status" == "online" ]]; then
        log_ok "Runner is healthy"
        return 0
    fi

    # Runner is not online - check if there are queued jobs
    if check_queued_jobs; then
        log_warn "Runner offline with queued jobs - attempting recovery"
        recover_runner
        return $?
    fi

    # No queued jobs, but still check service health
    if ! check_runner_service; then
        log_warn "Runner service not running - attempting recovery"
        recover_runner
        return $?
    fi

    log_warn "Runner offline but no queued jobs - skipping recovery"
    return 0
}

# Install cron job
install_cron() {
    local script_path
    script_path=$(realpath "$0")
    local cron_entry="*/5 * * * * $script_path >> $LOG_FILE 2>&1"

    if crontab -l 2>/dev/null | grep -q "health_check.sh"; then
        echo "Cron job already installed"
        crontab -l | grep "health_check.sh"
    else
        (crontab -l 2>/dev/null; echo "$cron_entry") | crontab -
        echo "Cron job installed: $cron_entry"
    fi
}

# Show status
show_status() {
    # Initialize gh path for status display
    check_gh_auth 2>/dev/null || true
    local gh="${GH_CMD:-gh}"

    echo "=== Runner Status ==="
    echo "VM: $VM_NAME"

    if check_vm_running 2>/dev/null; then
        echo "VM State: Running"
    else
        echo "VM State: Not Running"
    fi

    local status
    status=$(get_runner_status 2>/dev/null || echo "unknown")
    echo "GitHub Status: $status"

    if check_runner_service 2>/dev/null; then
        echo "Service: active"
    else
        echo "Service: inactive"
    fi

    local queued
    queued=$($gh api "repos/$REPO/actions/runs?status=queued" --jq '.workflow_runs | length' 2>/dev/null || echo "?")
    echo "Queued Workflows: $queued"

    echo ""
    echo "Recent log entries:"
    tail -10 "$LOG_FILE" 2>/dev/null || echo "(no log file)"
}

# Main
case "${1:-}" in
    --install-cron)
        install_cron
        ;;
    --status)
        show_status
        ;;
    --recover)
        rotate_log
        check_gh_auth || exit 1
        check_vm_running || exit 1
        recover_runner
        ;;
    --help|-h)
        echo "Usage: $0 [--install-cron|--status|--recover|--help]"
        echo ""
        echo "Options:"
        echo "  (none)          Run health check"
        echo "  --install-cron  Install cron job (every 5 minutes)"
        echo "  --status        Show current status"
        echo "  --recover       Force recovery"
        echo "  --help          Show this help"
        ;;
    *)
        rotate_log
        health_check
        ;;
esac
