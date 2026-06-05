#!/usr/bin/env bash
set -euo pipefail

base_mode="working-tree"
if [[ -n "${BASE_REF:-}" ]]; then
  base_ref="$BASE_REF"
fi

if [[ -n "${base_ref:-}" ]] && git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  base_mode="base-ref"
  changed="$(git diff --name-only "$base_ref"...HEAD)"
else
  changed="$(git status --short --untracked-files=all | sed -E 's/^...//')"
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step1"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step1.sh"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

targets=(
  "crates/assay-runner-core/src/kernel.rs"
  "crates/assay-cli/src/cli/commands/runner_spike.rs"
  "crates/assay-ebpf/src/main.rs"
  "crates/assay-registry/src/lockfile.rs"
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-cli/src/cli/commands/bundle.rs"
  "crates/assay-core/src/report/summary.rs"
  "crates/assay-cli/src/cli/commands/doctor.rs"
)

for path in "${targets[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing Wave53 target file: $path"
    exit 1
  }
done

if [[ "$base_mode" == "base-ref" ]]; then
  if printf '%s\n' "$changed" | rg '^crates/.+\.rs$|^crates/.+/tests/' >/dev/null; then
    echo "FAIL: Rust source/test edits are out of scope for Wave53 Step1"
    exit 1
  fi
else
  target_pattern="$(printf '%s\n' "${targets[@]}" | sed 's/[.[\*^$()+?{}|]/\\&/g' | paste -sd '|' -)"
  if printf '%s\n' "$changed" | rg "^(${target_pattern})$" >/dev/null; then
    echo "FAIL: Wave53 target Rust files changed in local Step1 scope"
    exit 1
  fi
fi

rg -n 'vmlinux\.rs.*out of scope|generated `crates/assay-ebpf/src/vmlinux\.rs`' \
  docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md >/dev/null

rg -n 'selected top 2 through 9|selected 2-9 snapshot|selected top 2-9 snapshot' \
  docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md \
  docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md >/dev/null

dirty_rust="$(git status --short --untracked-files=all | sed -E 's/^...//' | rg '^crates/.+\.rs$' || true)"
if [[ -n "$dirty_rust" ]]; then
  echo "WARN: skipping workspace cargo fmt --check; dirty Rust exists outside Wave53 Step1:"
  printf '%s\n' "$dirty_rust"
else
  cargo fmt --check
fi
if [[ "$base_mode" == "base-ref" ]]; then
  git diff --check "$base_ref"...HEAD
else
  git diff --check
fi

echo "PASS: Wave53 Step1 freeze gate"
