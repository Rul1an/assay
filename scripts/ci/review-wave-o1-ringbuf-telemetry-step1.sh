#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/codebase-analysis-followups}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-common/src/lib.rs"
  "crates/assay-ebpf/src/main.rs"
  "crates/assay-ebpf/src/lsm.rs"
  "crates/assay-ebpf/src/socket_lsm.rs"
  "crates/assay-monitor/Cargo.toml"
  "crates/assay-monitor/src/lib.rs"
  "crates/assay-monitor/src/loader.rs"
  "crates/assay-cli/src/cli/commands/monitor_next/mod.rs"
  "docs/contributing/SPLIT-INVENTORY-wave-o1-ringbuf-telemetry-step1.md"
  "docs/contributing/SPLIT-CHECKLIST-wave-o1-ringbuf-telemetry-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave-o1-ringbuf-telemetry-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave-o1-ringbuf-telemetry-step1.md"
  "scripts/ci/review-wave-o1-ringbuf-telemetry-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave O1 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave O1 Step1: $f"
    exit 1
  fi
done < <({
  git diff --name-only "$BASE_REF"...HEAD
  git diff --name-only
  git diff --name-only --cached
  git ls-files --others --exclude-standard
} | awk 'NF' | sort -u)

echo "[review] marker checks"
rg -n 'MonitorStatsSnapshot' crates/assay-monitor/src/lib.rs >/dev/null || {
  echo "FAIL: missing MonitorStatsSnapshot"
  exit 1
}
rg -n 'snapshot_stats' crates/assay-monitor/src/loader.rs crates/assay-cli/src/cli/commands/monitor_next/mod.rs >/dev/null || {
  echo "FAIL: missing snapshot_stats path"
  exit 1
}
rg -n 'Ring buffer pressure detected' crates/assay-cli/src/cli/commands/monitor_next/mod.rs >/dev/null || {
  echo "FAIL: missing ring buffer pressure warning"
  exit 1
}
rg -n 'MONITOR_STAT_TRACEPOINT_RINGBUF_DROPPED|MONITOR_STAT_LSM_RINGBUF_DROPPED|SOCKET_STAT_RINGBUF_DROPPED' \
  crates/assay-common/src/lib.rs \
  crates/assay-ebpf/src/main.rs \
  crates/assay-ebpf/src/lsm.rs \
  crates/assay-ebpf/src/socket_lsm.rs >/dev/null || {
  echo "FAIL: missing ringbuf drop counters"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -q -p assay-monitor -p assay-cli --all-targets -- -D warnings
cargo check -q -p assay-monitor
cargo test -q -p assay-monitor
cargo check -q -p assay-cli

if rustup target list --installed | rg -qx 'bpfel-unknown-none'; then
  cargo check -q -p assay-ebpf --features ebpf --target bpfel-unknown-none
else
  echo "[review] SKIP: bpfel-unknown-none target not installed"
fi

echo "[review] PASS"
