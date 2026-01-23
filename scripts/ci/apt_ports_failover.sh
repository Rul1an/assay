#!/bin/bash
set -euo pipefail

# Shared logic for robust Ubuntu Ports mirror failover
# Used in CI workflows to prevent 404 errors on ARM/Self-hosted runners

MIRRORS=(
  "https://mirror.gofoss.xyz/ubuntu-ports"
  "http://ports.ubuntu.com/ubuntu-ports"
)

switch_mirror() {
  local m="$1"
  # deb822 (24.04+)
  if [ -f /etc/apt/sources.list.d/ubuntu.sources ]; then
    sudo sed -i \
      -e "s|http://ports.ubuntu.com/ubuntu-ports|${m}|g" \
      -e "s|https://ports.ubuntu.com/ubuntu-ports|${m}|g" \
      /etc/apt/sources.list.d/ubuntu.sources 2>/dev/null || true
  fi
  # legacy
  if [ -f /etc/apt/sources.list ]; then
    sudo sed -i \
      -e "s|http://ports.ubuntu.com/ubuntu-ports|${m}|g" \
      -e "s|https://ports.ubuntu.com/ubuntu-ports|${m}|g" \
      /etc/apt/sources.list 2>/dev/null || true
  fi
}

apt_update() {
  sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
    -o Acquire::Queue-Mode=access \
    -o Acquire::Retries=10 \
    -o Acquire::http::Timeout=60 \
    -o Acquire::https::Timeout=60 \
    -o Acquire::http::Pipeline-Depth=0 \
    -o Acquire::https::Pipeline-Depth=0 \
    -o Acquire::CompressionTypes::Order::=gz \
    -o Acquire::ForceIPv4=true \
    -o Acquire::Languages=none
}

ok=0
# Temporarily disable exit-on-error to allow loop to try next mirror
set +e
for m in "${MIRRORS[@]}"; do
  echo "Trying Ubuntu Ports mirror: $m"
  switch_mirror "$m"
  if apt_update; then
    ok=1
    break
  fi
done
set -e

if [ "$ok" -ne 1 ]; then
  echo "ERROR: apt-get update failed on all mirrors"
  exit 1
fi
