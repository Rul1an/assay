#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

changed_files="$(mktemp)"
unexpected_files="$(mktemp)"
trap 'rm -f "${changed_files}" "${unexpected_files}"' EXIT

{
  git diff --name-only "${BASE_REF}" --
  git ls-files --others --exclude-standard -- \
    .config/nextest.toml \
    docs/superpowers/plans/2026-06-06-wave56-posture.md \
    rust-toolchain.toml \
    scripts/ci/review-wave56-posture.sh
} | sort -u > "${changed_files}"

required_files=(
  ".config/nextest.toml"
  "docs/superpowers/plans/2026-06-06-wave56-posture.md"
  "rust-toolchain.toml"
  "scripts/ci/review-wave56-posture.sh"
)

for path in "${required_files[@]}"; do
  if ! grep -Fxq "${path}" "${changed_files}"; then
    echo "missing required Wave56 file: ${path}" >&2
    exit 1
  fi
done

forbidden_regex='^(Cargo\.toml|Cargo\.lock|crates/assay-ebpf/.*|crates/assay-monitor/.*|crates/assay-runner-core/.*|crates/assay-runner-linux/.*|crates/assay-runner-schema/.*|crates/assay-xtask/.*|crates/assay-cli/src/cli/commands/runner_spike\.rs|crates/assay-cli/src/cgroup\.rs|\.github/workflows/runner-spike-delegated\.yml|\.github/workflows/runner-spike-sdk\.yml)$'

if grep -Eq "${forbidden_regex}" "${changed_files}"; then
  echo "Wave56 touched runner/eBPF/release-lane forbidden paths:" >&2
  grep -E "${forbidden_regex}" "${changed_files}" >&2
  exit 1
fi

allowed_regex='^(\.config/nextest\.toml|docs/superpowers/plans/2026-06-06-wave56-posture\.md|rust-toolchain\.toml|scripts/ci/review-wave56-posture\.sh|crates/assay-adapter-a2a/src/adapter_impl/payload\.rs|crates/assay-adapter-acp/src/adapter_impl/convert\.rs|crates/assay-adapter-ucp/src/adapter_impl/payload\.rs|crates/assay-core/src/judge/judge_internal/run\.rs|crates/assay-core/src/judge/mod\.rs|crates/assay-core/src/mcp/proxy/client\.rs|crates/assay-core/src/mcp/proxy/decisions\.rs|crates/assay-evidence/src/lint/packs/loader_internal/resolve\.rs|crates/assay-evidence/src/types\.rs|crates/assay-evidence/tests/pack_engine_manual_test\.rs|crates/assay-mcp-server/tests/auth_integration\.rs|crates/assay-policy/src/tiers\.rs|crates/assay-registry/src/verify_internal/tests/digest\.rs)$'

if grep -Ev "${allowed_regex}" "${changed_files}" > "${unexpected_files}"; then
  echo "Wave56 touched paths outside the allowlist:" >&2
  cat "${unexpected_files}" >&2
  exit 1
fi

if ! grep -Fq 'channel = "1.96.0"' rust-toolchain.toml; then
  echo "rust-toolchain.toml must pin channel = \"1.96.0\"" >&2
  exit 1
fi

if ! grep -Fq 'components = ["clippy", "rustfmt"]' rust-toolchain.toml; then
  echo "rust-toolchain.toml must install clippy and rustfmt" >&2
  exit 1
fi

if ! grep -Fq 'retries = { backoff = "fixed", count = 1, delay = "1s" }' .config/nextest.toml; then
  echo ".config/nextest.toml must keep the scoped one-retry policy" >&2
  exit 1
fi

if grep -Eq '(^|/)Cargo\.(toml|lock)$' "${changed_files}"; then
  echo "Wave56 must not change Cargo.toml or Cargo.lock" >&2
  exit 1
fi

expect_count="$(rg -n '#\[expect\(' --glob '*.rs' --glob '!crates/assay-ebpf/src/vmlinux.rs' | wc -l | tr -d ' ')"
if [[ "${expect_count}" -lt 1 ]]; then
  echo "expected at least one #[expect(...)] migration" >&2
  exit 1
fi

rustc --version | grep -Fq 'rustc 1.96.0'
cargo --version | grep -Fq 'cargo 1.96.0'
rustfmt --version | grep -Fq 'rustfmt 1.9.0'
cargo clippy --version | grep -Fq 'clippy 0.1.96'

cargo fmt --check
cargo check -p assay-core
cargo check -p assay-evidence
cargo check -p assay-policy
cargo check -p assay-registry
cargo check -p assay-cli
cargo check -p assay-mcp-server
cargo check -p assay-adapter-a2a
cargo check -p assay-adapter-acp
cargo check -p assay-adapter-ucp
cargo nextest --version || cargo install --locked cargo-nextest
cargo nextest show-config version
cargo nextest show-config test-groups -p assay-evidence --lib
cargo nextest run -p assay-evidence --lib
cargo clippy -p assay-core --all-targets -- -D warnings
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo clippy -p assay-policy --all-targets -- -D warnings
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo clippy -p assay-mcp-server --all-targets -- -D warnings
cargo clippy -p assay-adapter-a2a --all-targets -- -D warnings
cargo clippy -p assay-adapter-acp --all-targets -- -D warnings
cargo clippy -p assay-adapter-ucp --all-targets -- -D warnings

git diff --check "${BASE_REF}" --
while IFS= read -r path; do
  if [[ -f "${path}" ]] && grep -n '[[:blank:]]$' "${path}"; then
    echo "trailing whitespace in ${path}" >&2
    exit 1
  fi
done < "${changed_files}"
