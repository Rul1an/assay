#!/bin/bash
# ==============================================================================
# Helper: Register Local BPF-LSM Runner (Robust v2)
# Usage: ./register_local.sh <GITHUB_RUNNER_TOKEN>
# ==============================================================================
set -e

TOKEN=$1
VM="assay-bpf-runner"
REPO_URL="https://github.com/Rul1an/assay"

if [ -z "$TOKEN" ]; then
    echo "❌ Gebruik: $0 <GITHUB_TOKEN>"
    echo "   Haal je token hier op: $REPO_URL/settings/actions/runners/new"
    exit 1
fi

echo "🚀 Registering runner with GitHub..."

# 0. Repair / Ensure State (Idempotent Fix)
echo "🛠️  Ensuring VM state (User, Docker, Dependencies)..."
multipass exec "$VM" -- sudo bash -c '
    set -e
    # Ensure User
    if ! id -u github-runner >/dev/null 2>&1; then
        echo "   -> Creating github-runner user..."
        useradd -m -s /bin/bash github-runner
        usermod -aG docker github-runner
        echo "github-runner ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/github-runner
    fi

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
    if [ "$ARCH" == "arm64" ]; then
        RUNNER_URL="https://github.com/actions/runner/releases/download/v2.311.0/actions-runner-linux-arm64-2.311.0.tar.gz"
    else
        RUNNER_URL="https://github.com/actions/runner/releases/download/v2.311.0/actions-runner-linux-x64-2.311.0.tar.gz"
    fi

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
        echo "   -> Downloading Runner Agent ($ARCH)..."
        cd /opt/actions-runner

        curl -o runner.tar.gz -L "$RUNNER_URL"
        tar xzf ./runner.tar.gz
        rm runner.tar.gz

        chown -R github-runner:github-runner /opt/actions-runner
    fi

    # ALWAYS Fix Permissions (Fixes UnauthorizedAccessException on re-runs)
    # SOTA: Fix Permissions strictly for github-runner
    echo "   -> Enforcing strict ownership (github-runner:github-runner)..."
    chown -R github-runner:github-runner /opt/actions-runner
'

# 1. Configure (Unattended)
# Note: --labels bpf-lsm is CRITICAL for our workflow!
multipass exec "$VM" -- sudo su - github-runner -c \
    "cd /opt/actions-runner && ./config.sh --url $REPO_URL --token $TOKEN --labels self-hosted,linux,x64,bpf-lsm --unattended --replace || echo '⚠️  Config skipped (already configured?)'"

# 2. Install & Start Service (SOTA: As Dedicated User)
echo "🔌 Installing & Starting Service (User: github-runner)..."
# Force stop/uninstall old service if it exists (e.g. running as root/ubuntu)
multipass exec "$VM" -- sudo bash -c "cd /opt/actions-runner && ./svc.sh stop || true"
multipass exec "$VM" -- sudo bash -c "cd /opt/actions-runner && ./svc.sh uninstall || true"

# Re-fix ownership just in case uninstall messed with it
multipass exec "$VM" -- sudo chown -R github-runner:github-runner /opt/actions-runner

# Install as dedicated user
multipass exec "$VM" -- sudo bash -c "cd /opt/actions-runner && ./svc.sh install github-runner" || echo "⚠️  Service install skipped"
multipass exec "$VM" -- sudo bash -c "cd /opt/actions-runner && ./svc.sh start" || echo "⚠️  Service start skipped"
multipass exec "$VM" -- sudo bash -c "cd /opt/actions-runner && ./svc.sh status" || true

echo ""
echo "✅ Runner Registered & Active!"
echo "   Go verify here: $REPO_URL/settings/actions/runners"
