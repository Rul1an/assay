#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

base_ref="${BASE_REF:-origin/main}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  echo "FAIL: cannot resolve base ref: $base_ref"
  echo "Set BASE_REF to the intended Wave54 base ref."
  exit 1
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

allowed_pattern='^(docs/contributing/SAFETY-MAP-wave54-ebpf-unsafe\.md|scripts/ci/review-wave54-ebpf-safety\.sh|crates/assay-ebpf/Cargo\.toml|crates/assay-ebpf/src/main\.rs|crates/assay-ebpf/src/open_events\.rs|crates/assay-ebpf/src/connect_events\.rs|crates/assay-ebpf/src/fork_events\.rs|crates/assay-ebpf/src/lsm\.rs|crates/assay-ebpf/src/socket_lsm\.rs|crates/assay-cli/src/cli/commands/runner_spike/cgroup\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave54 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs is out of scope for Wave54"
  exit 1
fi

required=(
  "docs/contributing/SAFETY-MAP-wave54-ebpf-unsafe.md"
  "scripts/ci/review-wave54-ebpf-safety.sh"
  "crates/assay-ebpf/Cargo.toml"
  "crates/assay-ebpf/src/main.rs"
  "crates/assay-ebpf/src/open_events.rs"
  "crates/assay-ebpf/src/connect_events.rs"
  "crates/assay-ebpf/src/fork_events.rs"
  "crates/assay-ebpf/src/lsm.rs"
  "crates/assay-ebpf/src/socket_lsm.rs"
  "crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs"
)

for file in "${required[@]}"; do
  test -f "$file" || {
    echo "FAIL: missing required file: $file"
    exit 1
  }
done

require_marker() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -q "$pattern" "$file"; then
    echo "FAIL: $message"
    exit 1
  fi
}

require_marker '^unsafe_op_in_unsafe_fn = "warn"$' \
  "crates/assay-ebpf/Cargo.toml" \
  "assay-ebpf must warn on unsafe ops inside unsafe fn"
require_marker '^undocumented_unsafe_blocks = "warn"$' \
  "crates/assay-ebpf/Cargo.toml" \
  "assay-ebpf must warn on undocumented unsafe blocks"
require_marker '#\[allow\(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn\)\]' \
  "crates/assay-ebpf/src/main.rs" \
  "generated vmlinux module must be exempted without editing vmlinux.rs"

while IFS=: read -r line _text; do
  start=$(( line > 4 ? line - 4 : 1 ))
  if ! sed -n "${start},$((line - 1))p" \
    "crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs" | rg -q 'SAFETY:'; then
    echo "FAIL: runner_spike/cgroup.rs unsafe block at line ${line} lacks preceding SAFETY comment"
    exit 1
  fi
done < <(rg -n 'unsafe[[:space:]]*\{' \
  "crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs" || true)

cargo fmt --check
cargo check -p assay-ebpf --features ebpf --bin assay-ebpf
cargo clippy -p assay-ebpf --features ebpf --bin assay-ebpf -- \
  -D clippy::undocumented_unsafe_blocks \
  -D unsafe_op_in_unsafe_fn
cargo check -p assay-cli --features runner
cargo test -q -p assay-cli --features runner -- runner_spike
git diff --check "$base_ref"...HEAD
git diff --check
git diff --cached --check

echo "PASS: Wave54 eBPF safety lint gate"
