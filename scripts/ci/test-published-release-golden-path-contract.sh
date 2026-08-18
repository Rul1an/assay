#!/usr/bin/env bash
# Mutation anchors intentionally preserve literal shell and Actions expressions.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/published-release-golden-path.yml}"
DRIVER="${DRIVER:-${ROOT}/scripts/ci/published-release-golden-path.sh}"
MANIFEST="${MANIFEST:-${ROOT}/scripts/ci/fixtures/published-release-golden-path/v1/harness-manifest.json}"
RELEASE_WORKFLOW="${RELEASE_WORKFLOW:-${ROOT}/.github/workflows/release.yml}"
CHECKER="${ROOT}/scripts/ci/check-published-release-golden-path-contract.py"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing published-release golden-path workflow"
[[ -f "$DRIVER" ]] || fail "missing published-release golden-path driver"
[[ -f "$MANIFEST" ]] || fail "missing published-release golden-path harness manifest"
[[ -f "$RELEASE_WORKFLOW" ]] || fail "missing release workflow"
[[ -f "$CHECKER" ]] || fail "missing published-release golden-path checker"

python3 "$CHECKER" \
  --workflow "$WORKFLOW" \
  --release-workflow "$RELEASE_WORKFLOW" \
  --driver "$DRIVER" \
  --manifest "$MANIFEST" \
  --source-root "$ROOT"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

expect_mutation_failure() {
  local name="$1" target="$2" old="$3" new="$4" expected="$5"
  local case_root="$scratch/$name"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  cp "$RELEASE_WORKFLOW" "$case_root/release.yml"
  cp "$DRIVER" "$case_root/driver.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
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
      --release-workflow "$case_root/release.yml" \
      --driver "$case_root/driver.sh" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/output" 2>&1; then
    fail "mutation stayed green: $name"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "mutation $name missed expected guard: $expected"
}

expect_mutation_failure \
  "attestation-removed" "driver.sh" \
  '"$kit_root/verify-offline.sh" --assets-dir "$downloads" >"$results/attestation-verify.log" 2>&1' \
  'echo skipped-attestation >"$results/attestation-verify.log"' \
  "release attestations must verify before the first Assay invocation"

expect_mutation_failure \
  "attestation-suppressed" "driver.sh" \
  '"$kit_root/verify-offline.sh" --assets-dir "$downloads" >"$results/attestation-verify.log" 2>&1' \
  '"$kit_root/verify-offline.sh" --assets-dir "$downloads" >"$results/attestation-verify.log" 2>&1 || true' \
  "driver suppresses a failure"

expect_mutation_failure \
  "ambient-home" "driver.sh" \
  'export HOME="$run_root/home"' 'export HOME="${HOME}"' \
  "driver lost disposable HOME"

expect_mutation_failure \
  "ambient-path" "driver.sh" \
  'export PATH="$install_root/bin:/usr/bin:/bin"' 'export PATH="/usr/bin:/bin"' \
  "driver lost restricted PATH"

expect_mutation_failure \
  "preexisting-inspection" "driver.sh" \
  'assay evidence show --format json -- "$bundle"' \
  'assay evidence show --format json -- conformance/privileged-mcp-action-v0/vectors/ok-001.bundle.tar.gz' \
  "driver lost inspect produced bundle"

expect_mutation_failure \
  "provenance-collapsed" "driver.sh" \
  '"harness": {' '"release_harness": {' \
  "driver lost separate harness provenance"

expect_mutation_failure \
  "fixture-digest-drift" "manifest.json" \
  '4b489043fed724b332f9f216942778992270d85a89de4589c73a23fbb86aa48d' \
  '0b489043fed724b332f9f216942778992270d85a89de4589c73a23fbb86aa48d' \
  "harness digest drifted"

expect_mutation_failure \
  "artifact-omitted" "driver.sh" \
  '"tamper-verify.json", "enforcement.sarif",' \
  '"enforcement.sarif",' \
  "driver no longer requires retained artifact: tamper-verify.json"

expect_mutation_failure \
  "release-input-drift" "release.yml" \
  'release_tag: ${{ needs.release-contract.outputs.version }}' \
  'release_tag: v0.0.0' \
  "release transaction must pass its validated version"

echo "ok: published-release golden-path contract"
