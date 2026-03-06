#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

echo "== Wave C1 Closure quality checks =="
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli

echo "== Wave C1 Closure final-layout checks =="
required_files=(
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/src/cli/commands/replay/mod.rs"
  "crates/assay-cli/src/env_filter/mod.rs"
)
for file in "${required_files[@]}"; do
  if [ ! -f "${file}" ]; then
    echo "missing required final-layout file: ${file}"
    exit 1
  fi
done

if [ -f "crates/assay-cli/src/cli/commands/replay.rs" ]; then
  echo "legacy replay.rs should not exist after split"
  exit 1
fi
if [ -f "crates/assay-cli/src/env_filter.rs" ]; then
  echo "legacy env_filter.rs should not exist after split"
  exit 1
fi

args_lines="$(wc -l < crates/assay-cli/src/cli/args/mod.rs | tr -d ' ')"
replay_lines="$(wc -l < crates/assay-cli/src/cli/commands/replay/mod.rs | tr -d ' ')"
env_lines="$(wc -l < crates/assay-cli/src/env_filter/mod.rs | tr -d ' ')"
echo "facade lines: args=${args_lines} replay=${replay_lines} env_filter=${env_lines}"

if [ "${args_lines}" -gt 180 ] || [ "${replay_lines}" -gt 40 ] || [ "${env_lines}" -gt 40 ]; then
  echo "one or more facades exceeded thinness threshold"
  exit 1
fi

echo "== Wave C1 Closure diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-wave-c1-c-closure.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-c1-final.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-c1-c-closure.md$|^scripts/ci/review-wave-c1-c-closure.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave C1 Closure"
  exit 1
fi

echo "Wave C1 Closure reviewer script: PASS"
