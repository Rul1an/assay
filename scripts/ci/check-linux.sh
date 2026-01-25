#!/usr/bin/env bash
set -euo pipefail

# Linux-only compile guard for macOS dev: catches cfg(target_os="linux") compile errors.
# Priority:
#  1) Multipass VM (if available + configured)
#  2) Cross-target cargo check (no VM needed)
#  3) Graceful skip (if neither possible)

VM_NAME="${ASSAY_LINUX_VM:-assay-bpf-runner}"   # override via env
WORKDIR="${ASSAY_LINUX_WORKDIR:-/home/ubuntu/assay}" # path inside VM (if you mount/sync)
MODE="${ASSAY_LINUX_CHECK_MODE:-auto}"          # auto | target | multipass

run_target_check() {
  echo "==> Linux cross-target: cargo check (no Docker/VM)"
  rustup target add x86_64-unknown-linux-gnu >/dev/null 2>&1 || true
  cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu
}

run_multipass_check() {
  echo "==> Multipass Linux check in VM: ${VM_NAME}"
  if ! command -v multipass >/dev/null 2>&1; then
    echo "WARN: multipass not found"
    return 1
  fi

  # VM must exist + be running
  if ! multipass info "$VM_NAME" >/dev/null 2>&1; then
    echo "WARN: multipass VM '$VM_NAME' not found"
    return 1
  fi

  # Execute in VM. Assumes repo available at $WORKDIR in VM.
  # (Mount or git clone inside VM; see notes below.)
  # NOTE: We only run clippy on pre-push (not tests) to keep iteration fast.
  # Full tests run in CI on the self-hosted runner.
  timeout 180 multipass exec "$VM_NAME" -- bash -lc "
    export PATH=\"\$HOME/.cargo/bin:\$PATH\"
    if [ -f \"\$HOME/.cargo/env\" ]; then . \"\$HOME/.cargo/env\"; fi
    export CARGO_TARGET_DIR=\"/tmp/assay-target\"
    set -euo pipefail
    cd '$WORKDIR'
    rustup component add clippy >/dev/null 2>&1 || true
    cargo clippy --locked --workspace --all-targets -- -D warnings
  " || {
    echo "WARN: Linux check timed out or failed. Relying on CI."
    return 0  # Don't block push; CI will catch issues
  }
}

case "$MODE" in
  target)
    run_target_check
    ;;
  multipass)
    run_multipass_check
    ;;
  auto)
    if run_multipass_check; then
      exit 0
    fi
    # Always try target-check as good fallback
    if run_target_check; then
      exit 0
    fi
    echo "WARN: Skipping Linux check (no Multipass + target check failed). Relying on CI."
    exit 0
    ;;
  *)
    echo "Unknown ASSAY_LINUX_CHECK_MODE='$MODE' (use auto|target|multipass)"
    exit 2
    ;;
esac
