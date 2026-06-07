#!/bin/bash
# ==============================================================================
# Assay BPF-LSM Runner Setup (Ubuntu 24.04 LTS "Noble Numbat")
# State-of-the-Art 2026 Configuration
#
# USAGE:
#   export GITHUB_TOKEN="your-pat-token"
#   export RUNNER_NAME="bpf-runner-01"
#   export REPO_URL="https://github.com/Rul1an/assay"
#   sudo -E ./setup.sh
#
# FEATURES:
# - Kernel Hardening: Enables BPF LSM via boot parameters.
# - Dependency Management: Rust (via rustup), native bpf-linker, Docker, LLVM/Clang.
# - Runner Security: Runs as non-root 'github-runner' user, standard Docker group.
# ==============================================================================

set -euo pipefail

# 1. System Updates & Dependencies
echo "🚀 [1/5] Updating System & Installing Dependencies..."
apt-get update
apt-get install -y \
    curl git build-essential \
    linux-tools-common linux-tools-generic linux-headers-generic \
    llvm llvm-dev clang libclang-dev \
    pkg-config libssl-dev libsqlite3-dev \
    jq

# 2. Configure Kernel for BPF LSM
echo "🛡️  [2/5] Configuring Kernel Boot Parameters for BPF LSM..."
# Ubuntu 24.04 defaults: lsm=landlock,lockdown,yama,integrity,apparmor
# We MUST append 'bpf' to the end.
if ! grep -q "lsm=.*bpf" /etc/default/grub; then
    echo "   -> Appending 'bpf' to GRUB_CMDLINE_LINUX..."
    # Backup
    cp /etc/default/grub "/etc/default/grub.bak.$(date +%s)"

    # Append bpf to existing line or create new
    # SOTA approach: Ensure we don't break existing args
    sed -i 's/GRUB_CMDLINE_LINUX="\([^"]*\)"/GRUB_CMDLINE_LINUX="\1 lsm=landlock,lockdown,yama,integrity,apparmor,bpf"/' /etc/default/grub

    update-grub
    echo "⚠️  REBOOT REQUIRED to enable BPF LSM!"
else
    echo "   -> BPF LSM already configured in GRUB."
fi

# 3. Install Docker (Official Repo)
echo "🐳 [3/5] Installing Docker Engine..."
if ! command -v docker &> /dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable --now docker
else
    echo "   -> Docker already installed."
fi

# 4. Install Rust Toolchain (System-wide for Runner)
echo "🦀 [4/5] Installing Rust Toolchain and native eBPF linker..."
export RUSTUP_HOME=/opt/rust
export CARGO_HOME=/opt/rust
if [ ! -d "/opt/rust" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    chmod -R 777 /opt/rust
else
    echo "   -> Rust already installed in /opt/rust."
fi
# Add to global path and keep rustup proxies pointed at the system toolchain.
cat > /etc/profile.d/rust.sh <<'EOF'
export RUSTUP_HOME=/opt/rust
export CARGO_HOME=/opt/rust
export PATH=/opt/rust/bin:$PATH
EOF
# shellcheck disable=SC1091
source /etc/profile.d/rust.sh
rustup toolchain install nightly-2026-01-01 --profile minimal
rustup component add rust-src --toolchain nightly-2026-01-01
if ! command -v bpf-linker &> /dev/null || ! bpf-linker --version | grep -Fq "bpf-linker 0.10.3"; then
    rustup run nightly-2026-01-01 cargo install bpf-linker --version 0.10.3 --locked --force
else
    echo "   -> bpf-linker 0.10.3 already installed."
fi
ln -sf /opt/rust/bin/cargo /usr/local/bin/cargo
ln -sf /opt/rust/bin/rustc /usr/local/bin/rustc
ln -sf /opt/rust/bin/rustup /usr/local/bin/rustup
ln -sf /opt/rust/bin/bpf-linker /usr/local/bin/bpf-linker

# 5. Connect GitHub Runner
echo "🏃 [5/5] Configuring GitHub Action Runner..."
RUNNER_USER="github-runner"
RUNNER_DIR="/opt/actions-runner"

if ! id "$RUNNER_USER" &>/dev/null; then
    useradd -m -s /bin/bash "$RUNNER_USER"
    usermod -aG docker "$RUNNER_USER"
    # Allow passwordless sudo for verify scripts (Strict hardening would restrict this,
    # but verify_lsm_docker.sh needs host privileges for BPF)
    echo "$RUNNER_USER ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/github-runner
fi
for rust_home in .cargo .rustup; do
    rust_home_path="/home/$RUNNER_USER/$rust_home"
    if [ ! -e "$rust_home_path" ]; then
        ln -s /opt/rust "$rust_home_path"
        chown -h "$RUNNER_USER":"$RUNNER_USER" "$rust_home_path"
    fi
done

mkdir -p "$RUNNER_DIR"
chown "$RUNNER_USER":"$RUNNER_USER" "$RUNNER_DIR"

# 5a. Install free_disk cron (run every hour to prevent "No space left on device")
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$RUNNER_DIR/scripts"
cp "$SCRIPT_DIR/free_disk.sh" "$RUNNER_DIR/scripts/free_disk.sh"
chmod +x "$RUNNER_DIR/scripts/free_disk.sh"
if ! (crontab -l 2>/dev/null | grep -q "free_disk.sh"); then
    (crontab -l 2>/dev/null; echo "0 * * * * $RUNNER_DIR/scripts/free_disk.sh >> /var/log/assay-free_disk.log 2>&1") | crontab -
    echo "   -> Installed hourly cron: $RUNNER_DIR/scripts/free_disk.sh"
fi

# 5b. Install Assay latest-release updater (keeps global PATH binary current)
install -m 0755 "$SCRIPT_DIR/update_assay_latest.sh" /usr/local/sbin/update-assay-latest
install -m 0644 "$SCRIPT_DIR/assay-update.service" /etc/systemd/system/assay-update.service
install -m 0644 "$SCRIPT_DIR/assay-update.timer" /etc/systemd/system/assay-update.timer
systemctl daemon-reload
systemctl enable --now assay-update.timer
/usr/local/sbin/update-assay-latest

if [ ! -f "$RUNNER_DIR/.runner" ]; then
    echo "   -> Downloading Runner Agent..."
    # Dynamic latest version fetch would be SOTA, but hardcoding known stable for reliability
    # Assuming x64 Linux for Ubuntu 24.04
    cd "$RUNNER_DIR"
    # Fetch latest version from GH API
    LATEST_VER=$(curl -s https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | sed 's/v//')
    curl -o "actions-runner-linux-x64-${LATEST_VER}.tar.gz" -L "https://github.com/actions/runner/releases/download/v${LATEST_VER}/actions-runner-linux-x64-${LATEST_VER}.tar.gz"

    tar xzf "./actions-runner-linux-x64-${LATEST_VER}.tar.gz"

    echo "   -> Registering Runner (Requires GITHUB_TOKEN interactions)..."
    echo "   ⚠️  Manual Step: Run 'config.sh' as $RUNNER_USER using the token."
    echo "   su - $RUNNER_USER -c 'cd $RUNNER_DIR && ./config.sh --url $REPO_URL --token $GITHUB_TOKEN --labels bpf-lsm,assay-bpf-runner --unattended'"
    echo "   su - $RUNNER_USER -c 'cd $RUNNER_DIR && sudo ./svc.sh install && sudo ./svc.sh start'"
fi

echo "✅ Setup Complete. Reboot machine to enable BPF LSM, then register the runner."
