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

# ------------------------------------------------------------------------------
# 1. Build Phase (Consistent across all envs via Docker)
# ------------------------------------------------------------------------------

# Build eBPF (Kernel Space) via Builder Image
echo "----------------------------------------------------------------"
echo "� [0/3] Preparing Docker Builder Image..."
echo "----------------------------------------------------------------"
cargo xtask build-image

echo "----------------------------------------------------------------"
echo "�🛠️  [1/3] Building eBPF bytecode (assay-ebpf)..."
echo "----------------------------------------------------------------"
cargo clean -p assay-ebpf
cargo xtask build-ebpf --docker
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

  # Detect Architecture
  ARCH=$(uname -m)
  if [ "$ARCH" == "arm64" ] || [ "$ARCH" == "aarch64" ]; then
    TARGET="aarch64-unknown-linux-musl"
    # Pin SHA for security (Verified 2026-01-14)
    BUILDER_IMAGE="messense/rust-musl-cross@sha256:8ce9001cba339adabb99bfc06184b4da8d7fcdf381883279a35a5ec396a3f476"
    echo "🍎 Detected ARM64 (Apple Silicon). Building for target: $TARGET"
  else
    TARGET="x86_64-unknown-linux-musl"
    # TODO: Pin SHA for x86_64 once verified
    BUILDER_IMAGE="messense/rust-musl-cross:x86_64-musl"
    echo "💻 Detected x86_64. Building for target: $TARGET"
  fi

  docker run --rm -v "${WORKDIR}:/code" -w /code \
    -e CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    "$BUILDER_IMAGE" \
    cargo build --package assay-cli --bin assay --release --target "$TARGET"

  # Move binary to root for parity with download mode
  cp "target/${TARGET}/release/assay" ./assay
fi

# Create structured log directory for CI
mkdir -p /tmp/assay-lsm-verify
LOG_DIR="/tmp/assay-lsm-verify"

# Generate Policy (Legacy format for reference, but we use deny_modern.yaml)
echo "----------------------------------------------------------------"
echo "📝 [3/3] Generating Test Policy (deny.yaml)..."
echo "----------------------------------------------------------------"
cat > deny.yaml <<EOF
files:
  deny: ["/tmp/assay-test/secret.txt"]
EOF

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
        path_globs: ["/tmp/assay-test/secret.txt"]
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
RUN_TEST_CMD='
set -e
# Cleanup any stale monitors
pkill -x assay || true
rm -f /tmp/assay-test/secret.txt || true

echo ">> [Diag] Kernel: $(uname -r)"
echo ">> [Diag] Active LSMs: $(cat /sys/kernel/security/lsm 2>/dev/null || echo "N/A")"
echo ">> [Diag] Tracefs: $(mount | grep tracefs || echo "Missing")"
echo ">> [Diag] BPFFS: $(mount | grep bpf || echo "Missing")"

if ! grep -q "bpf" /sys/kernel/security/lsm 2>/dev/null; then
  echo "⚠️  SKIP: 'bpf' not found in Active LSMs. Kernel cmdline needs 'lsm=...,bpf'."
  if [ "${CI_MODE:-0}" -eq 1 ] && [ "${STRICT_LSM_CHECK:-0}" -eq 1 ]; then
      echo "❌ FAILURE: CI Mode (Strict) requires BPF LSM support."
      exit 1
  fi
  echo "⚠️  Soft Skip in CI Mode (LSM missing on this runner)."
  exit 0
fi

echo ">> [Test] Setting up test files..."
mkdir -p /tmp/assay-test
echo "TOP SECRET DATA" > /tmp/assay-test/secret.txt
chmod 600 /tmp/assay-test/secret.txt

# Start Monitor
# Use specific log location for CI collection
rm -rf /tmp/assay-lsm-verify
mkdir -p /tmp/assay-lsm-verify

# Debug: Check binary
# Debug: Check binary DIRECTLY to stdout
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
# Capture the launch output specifically
# Capture the launch output specifically
(
  echo ">>> [Monitor Wrapper] Launching..."
  # Explicitly list the binary to prove it exists inside subshell
  ls -l ./assay
  ls -l ./assay-ebpf.o
  RUST_LOG=info ./assay monitor --ebpf ./assay-ebpf.o --policy ./deny_modern.yaml --monitor-all --print
  echo ">>> [Monitor Wrapper] Exited with code $?"
) > /tmp/assay-lsm-verify/monitor.log 2>&1 &
MONITOR_PID=$!
echo "Monitor PID: $MONITOR_PID" >> /tmp/assay-lsm-verify/debug_binary.txt
sleep 5 # Wait for attachment

# Collect dmesg logs (including segfaults)
dmesg -T | grep -Ei "bpf|verifier|lsm|aya|segfault" | tail -n 300 > /tmp/assay-lsm-verify/dmesg_bpf.log 2>&1 || true

# Check if monitor died prematurely (Verifier error or crash)
if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
  echo "❌ FAILURE: Monitor exited before test began!"
  echo ">> Monitor Logs (Last 50 lines):"
  cat /tmp/assay-lsm-verify/monitor.log
  echo ">> Debug Binary Info:"
  cat /tmp/assay-lsm-verify/debug_binary.txt || echo "debug_binary.txt missing"
  exit 1
fi

# 2026 HARDENING: Ensure we are actually attached!
# Grep for the specific log line that confirms the BPF program was loaded/attached.
# "Assay Monitor running" is printed after successful attach() in monitor.rs.
if ! grep -q "Assay Monitor running" /tmp/assay-lsm-verify/monitor.log; then
    echo "Monitor: Assay Monitor running not found in logs yet. Giving it 5 more seconds..."
    sleep 5
    if ! grep -q "Assay Monitor running" /tmp/assay-lsm-verify/monitor.log; then
        echo "❌ FAILURE: Monitor running but NOT attached (Verifier rejection?)"
        echo ">> Monitor Logs (Last 50 lines):"
        cat /tmp/assay-lsm-verify/monitor.log
        echo ">> DMESG (Verifier Debug):"
        cat /tmp/assay-lsm-verify/dmesg_bpf.log
        # Kill it to be safe
        kill $MONITOR_PID 2>/dev/null || true
        exit 1
    fi
fi
echo "✅ Monitor Attached (Wait complete)"

echo ">> [Test] Attempting Access (cat /tmp/assay-test/secret.txt)..."
set +e
cat /tmp/assay-test/secret.txt
EXIT_CODE=$?
set -e

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
    # Always copy from ./assay (downloaded or built) + eBPF object + modern policy
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
        # We assume /tmp is writable.
        limactl shell "$LIMA_INSTANCE" -- rm -rf /tmp/assay-test
        limactl shell "$LIMA_INSTANCE" -- mkdir -p /tmp/assay-test

        limactl cp ./assay "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp ./assay-ebpf.o "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp deny.yaml "$LIMA_INSTANCE":/tmp/assay-test/
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
[ -d /sys/kernel/tracing ] && HOST_HAS_TRACEFS=1
[ -d /sys/kernel/debug ] && HOST_HAS_DEBUGFS=1

# Preflight Skip logic
    # We are on Mac/Windows, checking if we should skip
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
DOCKER_ARGS+=(-v "${WORKDIR}/deny_modern.yaml:/deny_modern.yaml") # Fix: Mount modern policy

# Mounts if present
[ -d /sys/fs/bpf ] && DOCKER_ARGS+=(-v /sys/fs/bpf:/sys/fs/bpf)
[ -d /sys/kernel/debug ] && DOCKER_ARGS+=(-v /sys/kernel/debug:/sys/kernel/debug)
[ -d /sys/kernel/tracing ] && DOCKER_ARGS+=(-v /sys/kernel/tracing:/sys/kernel/tracing)

DOCKER_ARGS+=(ubuntu:22.04 bash -lc '
  set -euo pipefail
  mkdir -p /sys/kernel/tracing /sys/kernel/debug /sys/fs/bpf || true

  # Try in-container mounts
  mountpoint -q /sys/kernel/tracing || mount -t tracefs tracefs /sys/kernel/tracing 2>/dev/null || true
  mountpoint -q /sys/kernel/debug || mount -t debugfs debugfs /sys/kernel/debug 2>/dev/null || true
  mountpoint -q /sys/fs/bpf || mount -t bpf bpf /sys/fs/bpf 2>/dev/null || true

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
    # If securityfs is missing, we assume no LSM support
    echo "⚠️  SKIP: /sys/kernel/security/lsm missing (Docker Desktop limitation)."
    if [ "${ENFORCE_LSM:-0}" -eq 1 ]; then echo "❌ Enforcement Active: /sys/kernel/security/lsm missing"; exit 1; fi
    exit 0
  fi

  # Run Test
  echo "creation of secret..."
  echo "TOP SECRET DATA" > /secret.txt
  chmod 600 /secret.txt

  echo "1. Starting Assay Monitor..."
  RUST_LOG=info assay monitor --ebpf /assay-ebpf.o --policy /deny_modern.yaml --print &
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
