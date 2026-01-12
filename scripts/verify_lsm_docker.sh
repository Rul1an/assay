#!/bin/bash
set -e

# ==============================================================================
# SOTA Verification Runner (Polyglot)
# Supports:
# 1. Native Linux (Direct Execution) - Best for CI/Production
# 2. macOS + Lima VM (Option B) - Best for Local Dev
# 3. macOS + Docker (Option C) - Fallback (Skipped if tracefs missing)
# ==============================================================================

echo "🚀 Starting Assay SOTA Verification..."
WORKDIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKDIR"

# ------------------------------------------------------------------------------
# 1. Build Phase (Consistent across all envs via Docker)
# ------------------------------------------------------------------------------

# Build eBPF (Kernel Space) via Builder Image
echo "----------------------------------------------------------------"
echo "🛠️  [1/3] Building eBPF bytecode (assay-ebpf)..."
echo "----------------------------------------------------------------"
cargo xtask build-ebpf --docker

# Build CLI (User Space) via Musl Cross (Static Binary)
echo "----------------------------------------------------------------"
echo "🛠️  [2/3] Building assay-cli (userspace)..."
echo "----------------------------------------------------------------"

# Detect Architecture
ARCH=$(uname -m)
if [ "$ARCH" == "arm64" ] || [ "$ARCH" == "aarch64" ]; then
  TARGET="aarch64-unknown-linux-musl"
  BUILDER_IMAGE="messense/rust-musl-cross:aarch64-musl"
  echo "🍎 Detected ARM64 (Apple Silicon). Building for target: $TARGET"
else
  TARGET="x86_64-unknown-linux-musl"
  BUILDER_IMAGE="messense/rust-musl-cross:x86_64-musl"
  echo "💻 Detected x86_64. Building for target: $TARGET"
fi

docker run --rm -v "${WORKDIR}:/code" -w /code \
  -e CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
  "$BUILDER_IMAGE" \
  cargo build --package assay-cli --bin assay --release --target "$TARGET"

# Generate Policy (Legacy format for reference, but we use deny_modern.yaml)
echo "----------------------------------------------------------------"
echo "📝 [3/3] Generating Test Policy (deny.yaml)..."
echo "----------------------------------------------------------------"
cat > deny.yaml <<EOF
files:
  deny: ["/secret.txt"]
EOF

# Modern Policy for Shield/LSM enforcement
cat > deny_modern.yaml <<EOF
version: "2.0"
name: "SOTA Shield Test"
runtime_monitor:
  enabled: true
  rules:
    - id: "block-secret"
      type: "file_open"
      match:
        path_globs: ["/secret.txt"]
      severity: "critical"
      action: "trigger_kill"
kill_switch:
  enabled: true
  triggers:
    - on_rule: "block-secret"
EOF

# ------------------------------------------------------------------------------
# 2. Runtime Verification Phase (Smart Runner)
# ------------------------------------------------------------------------------
echo "----------------------------------------------------------------"
echo "🧪 Starting Runtime Verification..."
echo "----------------------------------------------------------------"

RUN_TEST_CMD='
set -e
echo ">> [Diag] Kernel: $(uname -r)"
echo ">> [Diag] Active LSMs: $(cat /sys/kernel/security/lsm 2>/dev/null || echo "N/A")"
echo ">> [Diag] Tracefs: $(mount | grep tracefs || echo "Missing")"
echo ">> [Diag] BPFFS: $(mount | grep bpf || echo "Missing")"

if ! grep -q "bpf" /sys/kernel/security/lsm 2>/dev/null; then
  echo "⚠️  SKIP: 'bpf' not found in Active LSMs. Kernel cmdline needs 'lsm=...,bpf'."
  exit 0
fi

echo ">> [Test] Setting up test files..."
echo "TOP SECRET DATA" > /secret.txt
chmod 600 /secret.txt

# Start Monitor
RUST_LOG=info ./assay monitor --ebpf ./assay-ebpf.o --policy ./deny_modern.yaml --monitor-all --print > monitor.log 2>&1 &
MONITOR_PID=$!
sleep 5 # Wait for attachment



# Run the Victim Process (cat) SYNCHRONOUSLY to ensure it shares the Cgroup of $$
echo ">> [Test] Attempting Access (cat /secret.txt)..."
cat /secret.txt
EXIT_CODE=$?

# Kill monitor
kill $MONITOR_PID
wait $MONITOR_PID 2>/dev/null

tail -n 20 monitor.log

if [ $EXIT_CODE -ne 0 ]; then
    echo "✅ SUCCESS: Access Blocked (Exit code $EXIT_CODE)"
else
    echo "❌ FAILURE: Access Succeeded"
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
    cp target/$TARGET/release/assay "$TMP_DIR/"
    cp target/assay-ebpf.o "$TMP_DIR/"
    cp deny.yaml "$TMP_DIR/"

    cd "$TMP_DIR"
    bash -c "$RUN_TEST_CMD"
    rm -rf "$TMP_DIR"
    exit 0
fi

# --- Strategy B: macOS + Lima (The "SOTA Dev" Way) ---
if command -v limactl >/dev/null 2>&1; then
    LIMA_INSTANCE="default"
    if limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
        echo "🍋 Lima VM '$LIMA_INSTANCE' detected."
        echo "   Running test inside Lima..."

        # Copy artifacts to Lima
        # We assume /tmp is writable.
        limactl shell "$LIMA_INSTANCE" -- rm -rf /tmp/assay-test
        limactl shell "$LIMA_INSTANCE" -- mkdir -p /tmp/assay-test

        limactl cp target/$TARGET/release/assay "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp target/assay-ebpf.o "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp deny.yaml "$LIMA_INSTANCE":/tmp/assay-test/
        limactl cp deny_modern.yaml "$LIMA_INSTANCE":/tmp/assay-test/

        # Run test inside Lima (sudo required)
        limactl shell "$LIMA_INSTANCE" -- sudo bash -c "cd /tmp/assay-test && $RUN_TEST_CMD"
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
if [ "$(uname -s)" != "Linux" ]; then
    # We are on Mac/Windows, checking if we should skip
    echo "⚠️  Non-Linux Host + No Lima."
    echo "   Docker Desktop VM often lacks tracefs mounts."
    echo "   Proceeding with best-effort, but expecting SKIP."
fi

# Docker Args
DOCKER_ARGS=(run --rm --privileged --pid=host --cgroupns=host)
DOCKER_ARGS+=(-v "${WORKDIR}/target/$TARGET/release/assay:/usr/local/bin/assay")
DOCKER_ARGS+=(-v "${WORKDIR}/target/assay-ebpf.o:/assay-ebpf.o")
DOCKER_ARGS+=(-v "${WORKDIR}/deny.yaml:/deny.yaml")

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
    exit 0
  fi

  # Check for BPF LSM support
  if [ -r /sys/kernel/security/lsm ]; then
    if ! grep -q "bpf" /sys/kernel/security/lsm; then
       echo "⚠️  SKIP: BPF LSM not active in kernel (Docker Desktop limitation)."
       exit 0
    fi
  else
    # If securityfs is missing, we assume no LSM support
    echo "⚠️  SKIP: /sys/kernel/security/lsm missing (Docker Desktop limitation)."
    exit 0
  fi

  # Run Test
  echo "creation of secret..."
  echo "TOP SECRET DATA" > /secret.txt
  chmod 600 /secret.txt

  echo "1. Starting Assay Monitor..."
  RUST_LOG=info assay monitor --ebpf /assay-ebpf.o --policy /deny.yaml --print &
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
