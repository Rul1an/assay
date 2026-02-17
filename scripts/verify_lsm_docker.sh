#!/bin/bash
set -e
RELEASE_TAG=""

CI_MODE=0

# Parse args
while [[ "$#" -gt 0 ]]; do
    case $1 in

        --release-tag) RELEASE_TAG="$2"; shift ;;
        --ci-mode) CI_MODE=1 ;;
        --enforce-lsm) ENFORCE_LSM=1 ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

ENFORCE_LSM=${ENFORCE_LSM:-0}

# ==============================================================================
# ==============================================================================
# Assay Verification Runner (Polyglot)
# Supports:
# 1. Native Linux (Direct Execution) - Best for CI/Production
# 2. macOS + Lima VM (Option B) - Best for Local Dev
# 3. macOS + Docker (Option C) - Fallback (Skipped if tracefs missing)
# ==============================================================================

echo "🚀 Starting Assay Verification..."
WORKDIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKDIR"

# Resolve cargo early because this script may run under sudo with a minimal PATH.
# Prefer current PATH, then known self-hosted runner locations.
# shellcheck disable=SC1090
if [ -n "${SUDO_USER:-}" ] && [ -f "/home/${SUDO_USER}/.cargo/env" ]; then
    source "/home/${SUDO_USER}/.cargo/env"
fi
# shellcheck disable=SC1090
if [ -n "${SUDO_USER:-}" ] && [ -f "/Users/${SUDO_USER}/.cargo/env" ]; then
    source "/Users/${SUDO_USER}/.cargo/env"
fi
# shellcheck disable=SC1091
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

CARGO_BIN=""
for candidate in \
    "$(command -v cargo 2>/dev/null || true)" \
    "/opt/rust/bin/cargo" \
    "/usr/local/bin/cargo" \
    "/home/${SUDO_USER:-}/.cargo/bin/cargo" \
    "/Users/${SUDO_USER:-}/.cargo/bin/cargo" \
    "$HOME/.cargo/bin/cargo"
do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        CARGO_BIN="$candidate"
        break
    fi
done

if [ -z "$CARGO_BIN" ]; then
    echo "❌ cargo not found (checked PATH, /opt/rust/bin, /usr/local/bin, and cargo homes)"
    echo "💡 Ensure Rust toolchain is available on this runner before running verify_lsm_docker.sh"
    exit 127
fi
echo "🦀 Using cargo at: $CARGO_BIN"

# Test path for LSM block: use ASSAY_TEST_DIR if set (e.g. workspace on restricted /tmp runners)
export ASSAY_TEST_DIR="${ASSAY_TEST_DIR:-/tmp/assay-test}"
ASSAY_TEST_PATH="${ASSAY_TEST_DIR}/secret.txt"

# Cleanup stale artifacts immediately to free disk space
echo "🧹 [Init] Cleaning up stale verification artifacts..."
sudo rm -rf /tmp/assay-lsm-verify || true

# ------------------------------------------------------------------------------
# 1. Build Phase (Consistent across all envs via Docker)
# ------------------------------------------------------------------------------

# Build eBPF (Kernel Space) via Builder Image
echo "----------------------------------------------------------------"
echo " [0/3] Preparing Docker Builder Image..."
echo "----------------------------------------------------------------"
"$CARGO_BIN" xtask build-image

echo "----------------------------------------------------------------"
echo "🛠️  [1/3] Building eBPF bytecode (assay-ebpf)..."
echo "----------------------------------------------------------------"
"$CARGO_BIN" clean -p assay-ebpf
"$CARGO_BIN" xtask build-ebpf --docker
if [ ! -f target/assay-ebpf.o ]; then
    echo "❌ Build failed: target/assay-ebpf.o not found"
    exit 1
fi
cp target/assay-ebpf.o ./assay-ebpf.o




if [ -n "$RELEASE_TAG" ]; then
  # 1b. Download Mode
  echo "----------------------------------------------------------------"
  echo "⬇️  [1/3] Downloading Release Artifacts (${RELEASE_TAG})..."
  echo "----------------------------------------------------------------"

  # Determine arch for download
  ARCH=$(uname -m)
  if [ "$ARCH" == "arm64" ] || [ "$ARCH" == "aarch64" ]; then
    RELEASE_ARCH="aarch64-unknown-linux-gnu"
  else
    RELEASE_ARCH="x86_64-unknown-linux-gnu"
  fi

  URL="https://github.com/Rul1an/assay/releases/download/${RELEASE_TAG}/assay-${RELEASE_TAG}-${RELEASE_ARCH}.tar.gz"
  echo "Downloading from: $URL"
  curl -L -o assay.tar.gz "$URL"
  tar -xzf assay.tar.gz
  # Find binary in extracted folder (assay-v2.1-aarch64-.../assay)
  EXTRACTED_DIR=$(find . -maxdepth 1 -type d -name "assay-${RELEASE_TAG}-*" | head -n 1)
  cp "${EXTRACTED_DIR}/assay" ./assay
  chmod +x assay
  echo "✅ Downloaded and extracted release binary."


else
  # 1a. Build Mode (Existing logic)
  # Build CLI (User Space) via Musl Cross (Static Binary)
  echo "----------------------------------------------------------------"
  echo "🛠️  [2/3] Building assay-cli (userspace)..."
  echo "----------------------------------------------------------------"

if [ "$(uname -s)" == "Linux" ] && [ -x "$CARGO_BIN" ]; then
      echo "🐧 Linux detected with Cargo at $CARGO_BIN. Using Native Build (Skip Docker)..."
      "$CARGO_BIN" build --package assay-cli --bin assay --release
      cp target/release/assay ./assay
  else
      # Detect Architecture
      ARCH=$(uname -m)
      if [ "$ARCH" == "arm64" ] || [ "$ARCH" == "aarch64" ]; then
        TARGET="aarch64-unknown-linux-musl"
        # Pin SHA for security (Verified 2026-01-14)
        BUILDER_IMAGE="messense/rust-musl-cross@sha256:8ce9001cba339adabb99bfc06184b4da8d7fcdf381883279a35a5ec396a3f476"
        echo "🍎 Detected ARM64 (Apple Silicon). Building for target: $TARGET"
      else
        TARGET="x86_64-unknown-linux-musl"
        # Default = immutable digest (secure by default). Override for local dev if needed.
        DEFAULT_X86_64_BUILDER_IMAGE="messense/rust-musl-cross@sha256:c2b6442fad2e05c28db5006c745d10469b4d44ada7c1810b305fbc887a107b59"
        BUILDER_IMAGE="${ASSAY_MUSL_BUILDER_IMAGE_X86_64:-$DEFAULT_X86_64_BUILDER_IMAGE}"

        echo "💻 Detected x86_64. Building for target: $TARGET"
      fi

      echo "🐳 Falling back to Docker build (Cargo not found or non-Linux)..."
      docker run --rm -v "${WORKDIR}:/code" -w /code \
        -e CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
        "$BUILDER_IMAGE" \
        cargo build --package assay-cli --bin assay --release --target "$TARGET"

      # Move binary to root for parity with download mode
      cp "target/${TARGET}/release/assay" ./assay
  fi
fi

# Create structured log directory for CI
mkdir -p /tmp/assay-lsm-verify
# shellcheck disable=SC2034
LOG_DIR="/tmp/assay-lsm-verify"

# Generate Policy (Legacy format for reference, but we use deny_modern.yaml)

# Modern Policy for Shield/LSM enforcement
cat > deny_modern.yaml <<EOF
version: "2.0"
name: "Assay Shield Test"
runtime_monitor:
  enabled: true
  rules:
    - id: "block-secret"
      type: "file_open"
      match:
        path_globs: ["$ASSAY_TEST_PATH"]
      severity: "critical"
      action: "deny"
kill_switch:
  enabled: true
  triggers:
    - on_rule: "block-secret"
EOF

# ------------------------------------------------------------------------------
# 2. Runtime Verification Phase (Smart Runner)
# ------------------------------------------------------------------------------
# shellcheck disable=SC2016
RUN_TEST_CMD='
set -e
# Cleanup any stale monitors
pkill -x assay || true
rm -f "${ASSAY_TEST_DIR:-/tmp/assay-test}/secret.txt" || true

echo ">> [Diag] Kernel: $(uname -r)"
echo ">> [Diag] Active LSMs: $(cat /sys/kernel/security/lsm 2>/dev/null || echo "N/A")"
echo ">> [Diag] Tracefs: $(mount | grep tracefs || echo "Missing")"
echo ">> [Diag] BPFFS: $(mount | grep bpf || echo "Missing")"

if ! grep -q "bpf" /sys/kernel/security/lsm 2>/dev/null; then
  echo "⚠️  SKIP: '\''bpf'\'' not found in Active LSMs. Kernel cmdline needs '\''lsm=...,bpf'\''."
  if [ "${CI_MODE:-0}" -eq 1 ] && [ "${STRICT_LSM_CHECK:-0}" -eq 1 ]; then
      echo "❌ FAILURE: CI Mode (Strict) requires BPF LSM support."
      exit 1
  fi
  echo "⚠️  Soft Skip in CI Mode (LSM missing on this runner)."
  exit 0
fi

echo ">> [Test] Setting up test files..."
mkdir -p "${ASSAY_TEST_DIR:-/tmp/assay-test}"
# Create secret only if missing (CI may pre-create as runner user when /tmp is restricted)
if [ ! -f "${ASSAY_TEST_DIR:-/tmp/assay-test}/secret.txt" ]; then
  echo "TOP SECRET DATA" > "${ASSAY_TEST_DIR:-/tmp/assay-test}/secret.txt"
fi
echo ">> [Info] Initial file creation complete."

# Start Monitor
# Use specific log location for CI collection
rm -rf /tmp/assay-lsm-verify
mkdir -p /tmp/assay-lsm-verify

# Debug: Check binary Direct to stdout
echo ">> [Debug] Checking binary (STDOUT)..."
ls -l ./assay
file ./assay || echo "file command missing"
chmod +x ./assay
./assay --version || echo "❌ Failed to run ./assay --version"

# Backup debug info to file (ignoring failure)
{
    echo "--- LDD ---"
    ldd ./assay || true
} > /tmp/assay-lsm-verify/debug_binary.txt 2>&1 || true

echo "Starting monitor..."
(
  echo ">>> [Monitor Wrapper] Launching..."
  # Explicitly list the binary to prove it exists inside subshell
  ls -l ./assay
  RUST_LOG=warn ./assay monitor --ebpf ./assay-ebpf.o --policy ./deny_modern.yaml --monitor-all
  echo ">>> [Monitor Wrapper] Exited with code $?"
) > /tmp/assay-lsm-verify/monitor.log 2>&1 &
MONITOR_PID=$!
echo "Monitor PID: $MONITOR_PID" >> /tmp/assay-lsm-verify/debug_binary.txt

# 2026 HARDENING: Ensure we are actually attached and policy is armed!
echo "Waiting for monitor readiness (attach)..."
ATTACHED=0
for _ in {1..20}; do
  if grep -q "Assay Monitor running" /tmp/assay-lsm-verify/monitor.log; then
    ATTACHED=1
    break
  fi
  # if it died, fail early
  if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
    echo "❌ FAILURE: Monitor exited early."
    cat /tmp/assay-lsm-verify/monitor.log
    exit 1
  fi
  sleep 0.5
done

if [ "$ATTACHED" -ne 1 ]; then
    echo "❌ FAILURE: Monitor attached but NOT running in time"
    cat /tmp/assay-lsm-verify/monitor.log
    exit 1
fi

if ! grep -q "DEBUG: CONFIG\[100\]=1 confirmed" /tmp/assay-lsm-verify/monitor.log; then
    echo "❌ FAILURE: MONITOR_ALL config not confirmed in logs."
    cat /tmp/assay-lsm-verify/monitor.log
    exit 1
fi

echo "Waiting for policy readiness signpost..."
READY=0
for _ in {1..30}; do
  if grep -q "✅ Policy applied: tier1 inode rules loaded" /tmp/assay-lsm-verify/monitor.log; then
    READY=1
    break
  fi
  if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
    echo "❌ FAILURE: Monitor exited before policy readiness."
    cat /tmp/assay-lsm-verify/monitor.log
    exit 1
  fi
  sleep 0.5
done

if [ "$READY" -ne 1 ]; then
  echo "❌ FAILURE: Policy readiness signpost not seen."
  cat /tmp/assay-lsm-verify/monitor.log
  exit 1
fi
echo "✅ Monitor Attached and Policy Armed"

SECRET_PATH="${ASSAY_TEST_DIR:-/tmp/assay-test}/secret.txt"
echo ">> [Test] Attempting Access (cat $SECRET_PATH)..."
echo ">> [Debug] File Stat:"
stat "$SECRET_PATH" || echo "stat failed"
stat -c "Dev: %d (0x%x) Ino: %i" "$SECRET_PATH" || true
ls -ln "$SECRET_PATH"

chmod 644 "$SECRET_PATH"

set +e
if id nobody >/dev/null 2>&1 && command -v sudo >/dev/null 2>&1; then
  echo ">> [Test] Access Command: sudo -u nobody -- cat ..."
  OUTPUT="$(sudo -u nobody -- cat "$SECRET_PATH" 2>&1)"
  EXIT_CODE=$?
elif id nobody >/dev/null 2>&1 && command -v su >/dev/null 2>&1; then
  echo ">> [Test] Access Command: su -s /bin/bash nobody -c ..."
  OUTPUT="$(su -s /bin/bash nobody -c \"cat $SECRET_PATH\" 2>&1)"
  EXIT_CODE=$?
else
  echo ">> [Test] Access Command: cat (fallback to current user)..."
  OUTPUT="$(cat "$SECRET_PATH" 2>&1)"
  EXIT_CODE=$?
fi
set -e
echo "$OUTPUT"

echo ">> [Result] cat exit: $EXIT_CODE"

# Kill monitor (ignore exit code 143/SIGTERM)
kill $MONITOR_PID 2>/dev/null || true
wait $MONITOR_PID 2>/dev/null || true

echo ">> [Logs] Monitor Log (DEBUG):"
grep "DEBUG" /tmp/assay-lsm-verify/monitor.log || echo "No DEBUG lines found."
echo ">> [Logs] Monitor Log (Warning):"
grep "Warning" /tmp/assay-lsm-verify/monitor.log || echo "No Warning lines found."
echo ">> [Logs] Last 50 lines of monitor.log:"
tail -n 50 /tmp/assay-lsm-verify/monitor.log

echo ">> [BPF] LSM Counters (HIT / DENY / BYPASS):"
if command -v bpftool >/dev/null 2>&1; then
  echo "HIT:"
  bpftool map dump name LSM_HIT 2>/dev/null || echo "not found"
  echo "DENY:"
  bpftool map dump name LSM_DENY 2>/dev/null || echo "not found"
  echo "BYPASS:"
  bpftool map dump name LSM_BYPASS 2>/dev/null || echo "not found"
  echo "CONFIG:"
  bpftool map dump name CONFIG 2>/dev/null || echo "not found"
else
  echo "bpftool missing"
fi

if [ $EXIT_CODE -ne 0 ]; then
    echo "✅ SUCCESS: Access Blocked (Exit code $EXIT_CODE)"
    exit 0
else
    echo "❌ FAILURE: Access Succeeded"
    exit 1
fi
'

# --- Strategy A: Native Linux ---
if [ "$(uname -s)" == "Linux" ]; then
    echo "🐧 Linux Host Detected."
    if [ "$(id -u)" -ne 0 ]; then
        echo "⚠️  Root required for BPF. Please run with sudo."
        exit 1
    fi

    # Copy artifacts to temp dir to avoid pollution
    TMP_DIR=$(mktemp -d)
    cp ./assay "$TMP_DIR/"
    cp ./assay-ebpf.o "$TMP_DIR/"
    cp deny_modern.yaml "$TMP_DIR/"

    cd "$TMP_DIR"
    # Propagate CI_MODE to inner shell
    CI_MODE=$CI_MODE bash -c "$RUN_TEST_CMD"
    rc=$?
    cd /
    rm -rf "$TMP_DIR"
    exit $rc
fi

# --- Strategy B: macOS + Lima (The "Assay Dev" Way) ---
if command -v limactl >/dev/null 2>&1; then
    LIMA_INSTANCE="default"
    if limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
        echo "🍋 Lima VM '$LIMA_INSTANCE' detected."
        echo "   Running test inside Lima..."

        # Copy artifacts to Lima
        limactl shell "$LIMA_INSTANCE" -- rm -rf /tmp/assay-test
        limactl shell "$LIMA_INSTANCE" -- mkdir -p /tmp/assay-test

        limactl cp ./assay "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp ./assay-ebpf.o "$LIMA_INSTANCE":/tmp/assay-test/

        limactl cp deny_modern.yaml "$LIMA_INSTANCE":/tmp/assay-test/

        # Run test inside Lima (sudo required)
        limactl shell "$LIMA_INSTANCE" -- sudo bash -c "export CI_MODE=$CI_MODE; cd /tmp/assay-test && $RUN_TEST_CMD"
        exit $?
    else
        echo "⚠️  Lima installed but '$LIMA_INSTANCE' not running. Skipping Strategy B."
    fi
fi

# --- Strategy C: Docker (Fallback / CI) ---
echo "🐳 Docker Fallback..."

HOST_HAS_TRACEFS=0
# shellcheck disable=SC2034
[ -d /sys/kernel/tracing ] && HOST_HAS_TRACEFS=1
# shellcheck disable=SC2034
[ -d /sys/kernel/debug ] && HOST_HAS_DEBUGFS=1

# Preflight Skip logic
    echo "⚠️  Non-Linux Host + No Lima."
    echo "   Docker Desktop VM often lacks tracefs mounts."
    if [ "$ENFORCE_LSM" -eq 1 ]; then
        echo "❌ FAILURE: Enforcement required but environment incompatible."
        exit 1
    fi
    echo "   Proceeding with best-effort, but expecting SKIP."

# Docker Args
DOCKER_ARGS=(run --rm --privileged --pid=host --cgroupns=host)
DOCKER_ARGS+=(-e CI_MODE="$CI_MODE")
DOCKER_ARGS+=(-e ENFORCE_LSM="$ENFORCE_LSM")
DOCKER_ARGS+=(-v "${WORKDIR}/assay:/usr/local/bin/assay")
DOCKER_ARGS+=(-v "${WORKDIR}/assay-ebpf.o:/assay-ebpf.o")
DOCKER_ARGS+=(-v "${WORKDIR}/deny_modern.yaml:/deny_modern.yaml")

# Mounts if present
[ -d /sys/fs/bpf ] && DOCKER_ARGS+=(-v /sys/fs/bpf:/sys/fs/bpf)
[ -d /sys/kernel/debug ] && DOCKER_ARGS+=(-v /sys/kernel/debug:/sys/kernel/debug)
[ -d /sys/kernel/tracing ] && DOCKER_ARGS+=(-v /sys/kernel/tracing:/sys/kernel/tracing)

# shellcheck disable=SC2016
DOCKER_ARGS+=(ubuntu:22.04 bash -lc '
  set -euo pipefail
  mkdir -p /sys/kernel/tracing /sys/kernel/debug /sys/fs/bpf || true

  # Opt-in to Azure mirrors for 10x speedup and retry logic
  sed -i "s/archive.ubuntu.com/azure.archive.ubuntu.com/g" /etc/apt/sources.list
  sed -i "s/security.ubuntu.com/azure.archive.ubuntu.com/g" /etc/apt/sources.list


  # Try in-container mounts
  mountpoint -q /sys/kernel/tracing || mount -t tracefs tracefs /sys/kernel/tracing 2>/dev/null || true
  mountpoint -q /sys/kernel/debug || mount -t debugfs debugfs /sys/kernel/debug 2>/dev/null || true
  mountpoint -q /sys/fs/bpf || mount -t bpf bpf /sys/fs/bpf 2>/dev/null || true

  timeout 300s DEBIAN_FRONTEND=noninteractive apt-get update -y \
    -o Acquire::Retries=10 \
    -o Acquire::http::Timeout=60 \
    -o Acquire::https::Timeout=60 \
    -o Acquire::CompressionTypes::Order::=gz \
    -o Acquire::ForceIPv4=true \
    -o Acquire::http::No-Cache=True \
    -o Acquire::http::Pipeline-Depth=0


  # Check availability
  if [ ! -d /sys/kernel/tracing ] && [ ! -d /sys/kernel/debug/tracing ]; then
    echo "⚠️  SKIP: tracefs not available (Docker Desktop limitation)."
    if [ "${ENFORCE_LSM:-0}" -eq 1 ]; then echo "❌ Enforcement Active: TraceFS missing"; exit 1; fi
    exit 0
  fi

  # Check for BPF LSM support
  if [ -r /sys/kernel/security/lsm ]; then
    if ! grep -q "bpf" /sys/kernel/security/lsm; then
       echo "⚠️  SKIP: BPF LSM not active in kernel (Docker Desktop limitation)."
       if [ "${ENFORCE_LSM:-0}" -eq 1 ]; then echo "❌ Enforcement Active: BPF LSM missing in /sys/kernel/security/lsm"; exit 1; fi
       exit 0
    fi
  else
    echo "⚠️  SKIP: /sys/kernel/security/lsm missing (Docker Desktop limitation)."
    if [ "${ENFORCE_LSM:-0}" -eq 1 ]; then echo "❌ Enforcement Active: /sys/kernel/security/lsm missing"; exit 1; fi
    exit 0
  fi

  # Run Test
  echo "creation of secret..."
  echo "TOP SECRET DATA" > /secret.txt
  chmod 600 /secret.txt

  echo "1. Starting Assay Monitor..."
  RUST_LOG=info assay monitor --ebpf /assay-ebpf.o --policy /deny_modern.yaml --monitor-all --print &
  MONITOR_PID=$!
  sleep 3

  echo "2. Accessing..."
  set +e
  cat /secret.txt
  EXIT=$?
  set -e

  kill $MONITOR_PID || true

  if [ $EXIT -ne 0 ]; then
     echo "✅ SUCCESS"
  else
     echo "❌ FAILURE"
     exit 1
  fi
')

docker "${DOCKER_ARGS[@]}"
