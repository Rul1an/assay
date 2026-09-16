#!/bin/bash
# ==============================================================================
# Helper: Register Local BPF-LSM Runner (Robust v2)
# Usage: ./register_local.sh <GITHUB_RUNNER_TOKEN>
# ==============================================================================
set -euo pipefail

TOKEN=${1:-}
VM="assay-bpf-runner"
REPO_URL="https://github.com/Rul1an/assay"
# One-place pin: both arch download URLs and the post-extract version check
# read this. Current upstream latest: tagName=v2.337.0 publishedAt=2026-08-26T14:33:29Z.
ACTIONS_RUNNER_VERSION="2.337.0"
ACTIONS_RUNNER_SHA256_LINUX_ARM64="9b1dc70626422526e3c94767cf024896beb15da5342a3f4819bf2feac13e0393"
ACTIONS_RUNNER_SHA256_LINUX_X64="70920811a4f8ad4328818682bca5c6469c1c942fab52448868071d0063816613"

run_vm_root_script() {
    multipass exec "$VM" -- sudo bash -s
}

run_vm_root_cmd() {
    multipass exec "$VM" -- sudo bash -lc "$1"
}

run_vm_runner_cmd() {
    multipass exec "$VM" -- sudo -u github-runner bash -lc "$1"
}

if [ -z "$TOKEN" ]; then
    echo "❌ Gebruik: $0 <GITHUB_RUNNER_TOKEN>"
    echo "   Haal je token hier op: $REPO_URL/settings/actions/runners/new"
    exit 1
fi

echo "🚀 Registering runner with GitHub..."
echo "Pinned actions-runner version: ${ACTIONS_RUNNER_VERSION}"

# 0. Repair / Ensure State (Idempotent Fix)
echo "🛠️  Ensuring VM state (User, Docker, Dependencies)..."
run_vm_root_script <<EOF
    set -e
    ACTIONS_RUNNER_VERSION='${ACTIONS_RUNNER_VERSION}'
    ACTIONS_RUNNER_SHA256_LINUX_ARM64='${ACTIONS_RUNNER_SHA256_LINUX_ARM64}'
    ACTIONS_RUNNER_SHA256_LINUX_X64='${ACTIONS_RUNNER_SHA256_LINUX_X64}'
$(cat <<'GUEST'
    # Ensure User
    if ! id -u github-runner >/dev/null 2>&1; then
        echo "   -> Creating github-runner user..."
        useradd -m -s /bin/bash github-runner
        usermod -aG docker github-runner
        echo "github-runner ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/github-runner
    fi
    chown root:root /etc/sudoers.d/github-runner
    chmod 0440 /etc/sudoers.d/github-runner

    # Ensure Docker Group
    if ! getent group docker >/dev/null; then
        groupadd docker || true
        usermod -aG docker github-runner
    fi

    # Ensure Runner Dir
    mkdir -p /opt/actions-runner
    chown -R github-runner:github-runner /opt/actions-runner

    # Install dependencies if missing
    if ! command -v curl >/dev/null; then
        apt-get update && apt-get install -y curl git jq build-essential linux-tools-common linux-tools-generic linux-headers-generic llvm clang libclang-dev
    fi

    # SOTA: Detect Architecture (ARM64 vs x64 for Apple Silicon support)
    ARCH=$(dpkg --print-architecture)
    if [ "$ARCH" = "arm64" ]; then
        RUNNER_TARBALL="actions-runner-linux-arm64-${ACTIONS_RUNNER_VERSION}.tar.gz"
        RUNNER_SHA256="${ACTIONS_RUNNER_SHA256_LINUX_ARM64}"
    else
        RUNNER_TARBALL="actions-runner-linux-x64-${ACTIONS_RUNNER_VERSION}.tar.gz"
        RUNNER_SHA256="${ACTIONS_RUNNER_SHA256_LINUX_X64}"
    fi
    RUNNER_URL="https://github.com/actions/runner/releases/download/v${ACTIONS_RUNNER_VERSION}/${RUNNER_TARBALL}"

    # Check for corruption / incorrect arch
    CORRUPT=0
    if [ -f "/opt/actions-runner/config.sh" ]; then
        # Try running the listener to see if it execs
        if ! /opt/actions-runner/bin/Runner.Listener --version >/dev/null 2>&1; then
            echo "⚠️  Existing runner binary is failing (wrong arch?), cleaning up..."
            rm -rf /opt/actions-runner/*
            CORRUPT=1
        fi
    fi

    # Download Agent if missing or corrupt
    if [ ! -f "/opt/actions-runner/config.sh" ] || [ "$CORRUPT" -eq 1 ]; then
        echo "   -> Downloading Runner Agent ($ARCH), pinned ${ACTIONS_RUNNER_VERSION}..."
        cd /opt/actions-runner

        curl -o runner.tar.gz -L "$RUNNER_URL"
        if ! printf '%s  runner.tar.gz\n' "$RUNNER_SHA256" | sha256sum -c -; then
            echo "ERROR: actions-runner-checksum-mismatch: expected ${RUNNER_SHA256}" >&2
            exit 1
        fi
        tar xzf ./runner.tar.gz
        rm runner.tar.gz

        installed="$(./bin/Runner.Listener --version 2>/dev/null | tr -d '[:space:]')"
        if [ "$installed" != "$ACTIONS_RUNNER_VERSION" ]; then
            echo "ERROR: actions-runner-version-mismatch: expected ${ACTIONS_RUNNER_VERSION}, got ${installed:-<empty>}" >&2
            exit 1
        fi
        echo "Pinned actions-runner version: ${ACTIONS_RUNNER_VERSION}"

        chown -R github-runner:github-runner /opt/actions-runner
    fi

    # ALWAYS Fix Permissions (Fixes UnauthorizedAccessException on re-runs)
    # SOTA: Fix Permissions strictly for github-runner
    echo "   -> Enforcing strict ownership (github-runner:github-runner)..."
    chown -R github-runner:github-runner /opt/actions-runner
GUEST
)
EOF

# 1. Configure (Unattended)
# Note: only custom labels are configured here; GitHub automatically adds
# self-hosted/Linux plus the appropriate architecture label (for example ARM64
# or X64).
run_vm_root_script <<'EOF'
    rm -f /opt/actions-runner/.runner \
          /opt/actions-runner/.credentials \
          /opt/actions-runner/.credentials_rsaparams \
          /opt/actions-runner/.service \
          /opt/actions-runner/.runner_migrated || true
    chown -R github-runner:github-runner /opt/actions-runner
EOF
run_vm_runner_cmd \
    "cd /opt/actions-runner && ./config.sh --url '$REPO_URL' --token '$TOKEN' --labels bpf-lsm,assay-bpf-runner --unattended --replace" \
    || echo "⚠️  Config skipped (already configured?)"

# 2. Install & Start Service (SOTA: As Dedicated User)
echo "🔌 Installing & Starting Service (User: github-runner)..."
# Force stop/uninstall old service if it exists (e.g. running as root/ubuntu)
run_vm_root_cmd "cd /opt/actions-runner && ./svc.sh stop || true"
run_vm_root_cmd "cd /opt/actions-runner && ./svc.sh uninstall || true"

# Re-fix ownership just in case uninstall messed with it
run_vm_root_cmd "chown -R github-runner:github-runner /opt/actions-runner"

# Install as dedicated user
run_vm_root_cmd "cd /opt/actions-runner && ./svc.sh install github-runner" || echo "⚠️  Service install skipped"
run_vm_root_cmd "cd /opt/actions-runner && ./svc.sh start" || echo "⚠️  Service start skipped"
run_vm_root_cmd "cd /opt/actions-runner && ./svc.sh status" || true

echo ""
echo "✅ Runner Registered & Active!"
echo "   Go verify here: $REPO_URL/settings/actions/runners"
