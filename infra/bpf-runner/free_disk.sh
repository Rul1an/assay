#!/usr/bin/env bash
# ==============================================================================
# Free disk space on the BPF runner VM (assay-bpf-runner).
# Run when you see "No space left on device" in Kernel Matrix CI.
#
# Usage (on host, VM named assay-bpf-runner):
#   multipass exec assay-bpf-runner -- sudo bash -s < infra/bpf-runner/free_disk.sh
# Or inside the VM:
#   sudo bash /path/to/free_disk.sh
# ==============================================================================
set -euo pipefail

echo "=== Disk before ==="
df -h / /home 2>/dev/null || df -h /

echo ""
echo "=== Cleaning Docker ==="
docker system prune -af --volumes 2>/dev/null || true

echo ""
echo "=== Cleaning APT ==="
apt-get clean 2>/dev/null || true
rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/* 2>/dev/null || true

echo ""
echo "=== Cleaning runner _diag (old Worker logs) ==="
RUNNER_DIR="/opt/actions-runner"
if [ -d "${RUNNER_DIR}/_diag" ]; then
  # Keep only the 2 most recent Worker_*.log files; delete the rest to free space and avoid "No space left" when writing current log
  (cd "${RUNNER_DIR}/_diag" && ls -t Worker_*.log 2>/dev/null | tail -n +3 | xargs -r rm -f) 2>/dev/null || true
  du -sh "${RUNNER_DIR}/_diag" 2>/dev/null || true
fi

echo ""
echo "=== Cleaning runner _work (temp/tool/actions/update/externals caches) ==="
if [ -d "${RUNNER_DIR}/_work" ]; then
  # Remove temp/tool/actions to free space; workspace dirs stay so in-flight jobs are not broken
  rm -rf "${RUNNER_DIR}/_work/_temp" "${RUNNER_DIR}/_work/_tool" "${RUNNER_DIR}/_work/_actions" 2>/dev/null || true
  # Remove update/externals/dotnet caches (recreated on next job; frees hundreds of MB)
  rm -rf "${RUNNER_DIR}/_work/_update" "${RUNNER_DIR}/_work/__externals__" "${RUNNER_DIR}/_work/__dotnet_runtime__" 2>/dev/null || true
  # If you need maximum space (e.g. "Prepare workflow directory" still fails), run from host:
  #   multipass exec assay-bpf-runner -- sudo rm -rf /opt/actions-runner/_work/*
  du -sh "${RUNNER_DIR}/_work"/* 2>/dev/null | sort -hr | head -10 || true
fi

echo ""
echo "=== Disk after ==="
df -h / /home 2>/dev/null || df -h /
