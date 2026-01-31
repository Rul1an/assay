#!/bin/bash
# ==============================================================================
# Setup Local BPF-LSM Runner (Multipass)
# SOTA 2026 Best Practices:
# - Uses separate VM for isolation
# - Enables BPF-LSM in kernel via cloud-init
# - Configures dedicated 'github-runner' user for service isolation
# ==============================================================================
set -e

# 1. Dependency Check: Multipass
if ! command -v multipass &> /dev/null; then
    echo "📦 Multipass not found. Installing via Homebrew..."
    if ! command -v brew &> /dev/null; then
        echo "❌ Homebrew is required but not found. Please install Homebrew first."
        exit 1
    fi
    brew install --cask multipass
else
    echo "✅ Multipass is already installed."
fi

# 2. Config Validation
CLOUD_INIT="infra/bpf-runner/cloud-init.yaml"
if [ ! -f "$CLOUD_INIT" ]; then
    echo "❌ Error: Cloud-init config not found at $CLOUD_INIT"
    exit 1
fi

VM_NAME="assay-bpf-runner"
VM_CPUS="4"
VM_MEM="8G"
VM_DISK="30G"   # 20G often too small for Kernel Matrix + actions cache; use free_disk.sh if full

# 3. VM Lifecycle Management
echo "🚀 Managing VM '$VM_NAME'..."

if multipass list | grep -q "^$VM_NAME "; then
    STATUS=$(multipass info $VM_NAME | grep State | awk '{print $2}')
    if [ "$STATUS" == "Running" ]; then
        echo "   -> VM is already running."
    else
        echo "   -> VM exists but is not running. Starting..."
        multipass start $VM_NAME
    fi
else
    echo "   -> Launching new Ubuntu 24.04 'noble' instance..."
    echo "   -> Config: $VM_CPUS CPUs, $VM_MEM RAM, $VM_DISK Disk, BPF-LSM Enabled"

    multipass launch noble \
        --name "$VM_NAME" \
        --cpus "$VM_CPUS" \
        --memory "$VM_MEM" \
        --disk "$VM_DISK" \
        --cloud-init "$CLOUD_INIT" \
        --timeout 600
fi

# 3.5 Connectivity Wait (Fix for "No route to host" race condition)
echo "⏳ Waiting for VM connectivity..."
for i in {1..15}; do
    if multipass exec "$VM_NAME" -- true &>/dev/null; then
        break
    fi
    echo "   ... waiting for SSH ($i/15)"
    sleep 4
done

echo "⏳ Waiting for Cloud-Init completion (Kernel params & Updates)..."
# SOTA: Wait for cloud-init to signal completion
multipass exec "$VM_NAME" -- cloud-init status --wait

# 4. Install free_disk cron on VM (run every hour to prevent "No space left on device")
echo "📋 Installing free_disk cron on VM..."
multipass exec "$VM_NAME" -- sudo mkdir -p /opt/actions-runner/scripts
multipass exec "$VM_NAME" -- sudo tee /opt/actions-runner/scripts/free_disk.sh < infra/bpf-runner/free_disk.sh
multipass exec "$VM_NAME" -- sudo chmod +x /opt/actions-runner/scripts/free_disk.sh
multipass exec "$VM_NAME" -- bash -c '(crontab -l 2>/dev/null | grep -q free_disk.sh) || (crontab -l 2>/dev/null; echo "0 * * * * /opt/actions-runner/scripts/free_disk.sh >> /var/log/assay-free_disk.log 2>&1") | crontab -'
echo "   -> Hourly free_disk cron installed."

# 5. Final Instructions
IP=$(multipass info "$VM_NAME" | grep IPv4 | awk '{print $2}')
echo ""
echo "========================================================================"
echo "✅ BPF-LSM Runner VM is Ready!"
echo "   IP Address: $IP"
echo "========================================================================"
echo ""
echo "⚠️  ACTION REQUIRED: Register the runner with GitHub."
echo ""
echo "1. Go to: https://github.com/Rul1an/assay/settings/actions/runners/new"
echo "2. Copy the token (e.g., A1B2C3D4...)"
echo "3. Run this command to enter the runner shell:"
echo ""
echo "   multipass shell $VM_NAME"
echo ""
echo "4. Inside the shell, run the registration (use user 'github-runner'):"
echo ""
echo "   sudo su - github-runner"
echo "   # Paste the 'Download' steps from GitHub if needed, or just config:"
echo "   ./config.sh --url https://github.com/Rul1an/assay --token <YOUR_TOKEN> --labels self-hosted,linux,x64,bpf-lsm,assay-bpf-runner"
echo "   sudo ./svc.sh install"
echo "   sudo ./svc.sh start"
echo ""
echo "========================================================================"
