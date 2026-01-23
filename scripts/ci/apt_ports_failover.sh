#!/bin/bash
set -euo pipefail

# Robust Ubuntu Ports mirror failover (ARM / ports sources)
# Rewrites ANY https?://*/ubuntu-ports to selected mirror (fixes leftover bad mirrors).

MIRRORS=(
  "https://mirror.gofoss.xyz/ubuntu-ports"
  "http://ports.ubuntu.com/ubuntu-ports"
)

SOURCES=()
[ -f /etc/apt/sources.list ] && SOURCES+=(/etc/apt/sources.list)
[ -f /etc/apt/sources.list.d/ubuntu.sources ] && SOURCES+=(/etc/apt/sources.list.d/ubuntu.sources)

# If we can't find any sources files, just run update (best effort)
if [ "${#SOURCES[@]}" -eq 0 ]; then
  echo "WARN: No apt sources files found; running apt-get update best-effort."
  timeout 900s sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
    -o Acquire::Queue-Mode=access \
    -o Acquire::Retries=10 \
    -o Acquire::http::Timeout=60 \
    -o Acquire::https::Timeout=60 \
    -o Acquire::http::Pipeline-Depth=0 \
    -o Acquire::https::Pipeline-Depth=0 \
    -o Acquire::CompressionTypes::Order::=gz \
    -o Acquire::ForceIPv4=true \
    -o Acquire::Languages=none
  exit 0
fi

# Detect any ubuntu-ports usage (covers ports.ubuntu.com, mirrors.edge.kernel.org, etc.)
if ! grep -qs "/ubuntu-ports" "${SOURCES[@]}" 2>/dev/null; then
  echo "No /ubuntu-ports sources detected; skipping ports mirror failover."
  exit 0
fi

echo "Ubuntu Ports detected. Engaging robust mirror failover..."
echo "Current ubuntu-ports entries:"
grep -nH "/ubuntu-ports" "${SOURCES[@]}" 2>/dev/null || true

switch_mirror() {
  local m="$1"
  # Replace ANY scheme://host/.../ubuntu-ports with chosen mirror (Deb822 + legacy safe)
  for f in "${SOURCES[@]}"; do
    sudo sed -i -E \
      -e "s|https?://[^[:space:]]*/ubuntu-ports|${m}|g" \
      "$f" 2>/dev/null || true
  done
}

apt_update() {
  timeout 900s sudo DEBIAN_FRONTEND=noninteractive apt-get update -y \
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
  echo "Final ubuntu-ports entries:"
  grep -nH "/ubuntu-ports" "${SOURCES[@]}" 2>/dev/null || true
  exit 1
fi

echo "Ports mirror OK."
