#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

base_ref="${BASE_REF:-origin/codex/wave53-hotspot-top2-9-step3}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  if [[ -z "${BASE_REF:-}" ]] && git rev-parse --verify codex/wave53-hotspot-top2-9-step3 >/dev/null 2>&1; then
    base_ref="codex/wave53-hotspot-top2-9-step3"
  else
    echo "FAIL: cannot resolve Step4 base ref: $base_ref"
    echo "Set BASE_REF to the Step3 branch/ref used for this stacked review."
    exit 1
  fi
fi

base_changed="$(git diff --name-only "$base_ref"...HEAD)"
worktree_changed="$(
  {
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | sort -u
)"
changed="$(printf '%s\n%s\n' "$base_changed" "$worktree_changed" | sed '/^$/d' | sort -u)"

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9\.md|docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step4\.md|docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step4\.md|docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step4\.md|scripts/ci/review-wave53-hotspot-top2-9-step4\.sh|crates/assay-runner-core/src/kernel\.rs|crates/assay-runner-core/src/kernel/decode\.rs|crates/assay-runner-core/src/kernel/stats\.rs|crates/assay-runner-core/src/kernel/health\.rs|crates/assay-runner-core/src/kernel/notes\.rs|crates/assay-runner-core/src/kernel/tests\.rs|crates/assay-ebpf/src/main\.rs|crates/assay-ebpf/src/open_events\.rs|crates/assay-ebpf/src/connect_events\.rs|crates/assay-ebpf/src/fork_events\.rs|crates/assay-ebpf/src/path_filter\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave53 Step4 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step4"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs is out of scope for Wave53 Step4"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step4.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step4.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step4.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step4.sh"
  "crates/assay-runner-core/src/kernel.rs"
  "crates/assay-runner-core/src/kernel/decode.rs"
  "crates/assay-runner-core/src/kernel/stats.rs"
  "crates/assay-runner-core/src/kernel/health.rs"
  "crates/assay-runner-core/src/kernel/notes.rs"
  "crates/assay-runner-core/src/kernel/tests.rs"
  "crates/assay-ebpf/src/main.rs"
  "crates/assay-ebpf/src/open_events.rs"
  "crates/assay-ebpf/src/connect_events.rs"
  "crates/assay-ebpf/src/fork_events.rs"
  "crates/assay-ebpf/src/path_filter.rs"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

require_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if ! rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

forbid_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

for module in decode health notes stats; do
  require_marker "^mod ${module};$" "crates/assay-runner-core/src/kernel.rs" "kernel facade must declare ${module} module"
done
require_marker '^mod tests;$' "crates/assay-runner-core/src/kernel.rs" "kernel facade must keep tests in kernel/tests.rs"
require_marker '^pub const KERNEL_EVENT_SCHEMA:' "crates/assay-runner-core/src/kernel.rs" "kernel facade must preserve event schema constant"
require_marker '^pub struct KernelLayerEvent\b' "crates/assay-runner-core/src/kernel.rs" "kernel facade must preserve KernelLayerEvent"
require_marker '^pub struct KernelLayerCapture\b' "crates/assay-runner-core/src/kernel.rs" "kernel facade must preserve KernelLayerCapture"
require_marker '^pub enum KernelLayerError\b' "crates/assay-runner-core/src/kernel.rs" "kernel facade must preserve KernelLayerError"
require_marker '^pub struct KernelLayerBuilder\b' "crates/assay-runner-core/src/kernel.rs" "kernel facade must preserve KernelLayerBuilder"
forbid_marker '^fn decode_monitor_event\b|^fn ringbuf_drop_delta\b|^fn kernel_capture_note\b|^fn network_protocol_coverage_label\b' "crates/assay-runner-core/src/kernel.rs" "kernel facade must not own moved helper bodies"

for module in connect_events fork_events open_events path_filter; do
  require_marker "^mod ${module};$" "crates/assay-ebpf/src/main.rs" "eBPF facade must declare ${module} module"
done
for entrypoint in assay_monitor_openat assay_monitor_openat2 assay_monitor_openat_exit assay_monitor_openat2_exit assay_monitor_connect assay_monitor_sendto assay_monitor_sendmsg assay_monitor_fork; do
  require_marker "^pub fn ${entrypoint}\\b" "crates/assay-ebpf/src/main.rs" "eBPF tracepoint entrypoint ${entrypoint} must remain in main.rs"
done
for map_marker in 'static EVENTS:' 'pub static TP_HIT:' 'pub static MONITORED_CGROUPS:' 'pub static CONFIG:' 'static PENDING_OPEN:' 'static OPEN_SCRATCH:'; do
  require_marker "$map_marker" "crates/assay-ebpf/src/main.rs" "eBPF map declaration ${map_marker} must remain in main.rs"
done
if rg -n '#\[tracepoint\]|^pub fn assay_monitor_' crates/assay-ebpf/src/open_events.rs crates/assay-ebpf/src/connect_events.rs crates/assay-ebpf/src/fork_events.rs crates/assay-ebpf/src/path_filter.rs; then
  echo "FAIL: eBPF helper modules must not own tracepoint entrypoints"
  exit 1
fi

check_loc_max() {
  local path="$1"
  local max="$2"
  local loc
  loc="$(wc -l < "$path" | tr -d ' ')"
  if (( loc > max )); then
    echo "FAIL: $path has $loc LOC, expected <= $max"
    exit 1
  fi
}

check_loc_max "crates/assay-runner-core/src/kernel.rs" 320
check_loc_max "crates/assay-ebpf/src/main.rs" 320

cargo fmt --check
cargo check -p assay-runner-core
cargo test -q -p assay-runner-core
cargo clippy -p assay-runner-core --all-targets -- -D warnings
cargo check -p assay-ebpf
git diff --check "$base_ref"...HEAD
git diff --check
git diff --cached --check

echo "PASS: Wave53 Step4 runner/eBPF split gate"
