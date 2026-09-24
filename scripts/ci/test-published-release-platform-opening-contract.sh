#!/usr/bin/env bash
# Mutation anchors intentionally preserve literal shell and Actions expressions.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/published-release-golden-path.yml}"
DRIVER="${DRIVER:-${ROOT}/scripts/ci/published-release-platform-opening.sh}"
CHECKER="${ROOT}/scripts/ci/check-published-release-platform-opening-contract.py"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing published-release golden-path workflow"
[[ -f "$CHECKER" ]] || fail "missing published-release platform-opening checker"

python3 "$CHECKER" --workflow "$WORKFLOW" --driver "$DRIVER"
python3 "$CHECKER" --probe-opening-doctor --driver "$DRIVER"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

expect_mutation_failure() {
  local name="$1" target="$2" old="$3" new="$4" expected="$5"
  local case_root="$scratch/$name"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  if [[ -f "$DRIVER" ]]; then
    cp "$DRIVER" "$case_root/driver.sh"
  else
    : >"$case_root/driver.sh"
  fi
  python3 - "$case_root/$target" "$old" "$new" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
old, new = sys.argv[2:]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
  if python3 "$CHECKER" \
      --workflow "$case_root/workflow.yml" \
      --driver "$case_root/driver.sh" \
      >"$case_root/output" 2>&1; then
    fail "mutation stayed green: $name"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "mutation $name missed expected guard: $expected"
}

expect_mutation_failure \
  "windows-runner-removed" "workflow.yml" \
  "windows-latest" "ubuntu-latest" \
  "opening job must run on windows-latest"

expect_mutation_failure \
  "macos-runner-removed" "workflow.yml" \
  "macos-latest" "ubuntu-latest" \
  "opening job must run on macos-latest"

expect_mutation_failure \
  "windows-target-drifted" "workflow.yml" \
  "x86_64-pc-windows-msvc" "x86_64-unknown-linux-gnu" \
  "opening job must name the published Windows archive target"

expect_mutation_failure \
  "linux-job-changed" "workflow.yml" \
  "Linux x86_64 post-publication journey" \
  "Linux x86_64 post-publication journey (widened)" \
  "Linux x86_64 post-publication job steps must stay unchanged"

expect_mutation_failure \
  "same-run-artifact" "workflow.yml" \
  "      - name: Checkout the opening harness" \
  $'      - name: Fetch same-run build artifact\n        uses: actions/download-artifact@v4\n      - name: Checkout the opening harness' \
  "published-release workflow must not consume a same-run build artifact"

expect_mutation_failure \
  "release-url-rewritten" "driver.sh" \
  'asset_url="https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}"' \
  'asset_url="file://${GITHUB_WORKSPACE}/target/release/${asset_name}"' \
  "opening driver must bind the public release-by-tag download URL exactly once"

expect_mutation_failure \
  "version-from-cargo" "driver.sh" \
  'expected_version="${release_tag#v}"' \
  'expected_version="$(python3 -c "from pathlib import Path; import sys; sys.path.insert(0,\"scripts/ci/lib\"); from workspace_version import read_workspace_version; print(read_workspace_version(Path(\"Cargo.toml\")))")"' \
  "opening driver must compare version to the release tag, not Cargo.toml"

expect_mutation_failure \
  "tree-built-binary" "driver.sh" \
  'fail "assay version mismatch: expected ${expected_version}, got ${version_out}"' \
  $'cargo build --locked -p assay-cli --bin assay\n  fail "assay version mismatch: expected ${expected_version}, got ${version_out}"' \
  "opening driver must not run a tree-built binary"

echo "ok: published-release platform-opening contract"
