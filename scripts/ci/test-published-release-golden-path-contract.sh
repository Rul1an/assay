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
bash "$ROOT/scripts/ci/test-release-attestation-enforce.sh"
PYTHONPATH="$ROOT/scripts/ci" python3 "$ROOT/scripts/ci/test_safe_extract_release_archive.py"

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
  'bash "$ROOT/scripts/ci/release_attestation_enforce.sh"' \
  'echo skipped-attestation' \
  "driver must execute the reviewed attestation verifier exactly once"

expect_mutation_failure \
  "attestation-suppressed" "driver.sh" \
  'bash "$ROOT/scripts/ci/release_attestation_enforce.sh"' \
  'bash "$ROOT/scripts/ci/release_attestation_enforce.sh" || true' \
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

expect_mutation_failure \
  "publication-wait-removed" "release.yml" \
  $'  published-release-golden-path:\n    name: Verify the published release journey\n    needs: [release-contract, release]' \
  $'  published-release-golden-path:\n    name: Verify the published release journey\n    needs: [release-contract]' \
  "published-release job must uniquely wait for release publication"

expect_mutation_failure \
  "caller-failure-ignored" "release.yml" \
  $'    permissions:\n      contents: read\n    uses: ./.github/workflows/published-release-golden-path.yml' \
  $'    permissions:\n      contents: read\n    continue-on-error: true\n    uses: ./.github/workflows/published-release-golden-path.yml' \
  "release caller must not ignore failed published-release verification"

expect_mutation_failure \
  "caller-condition-disabled" "release.yml" \
  $'  published-release-golden-path:\n    name: Verify the published release journey\n    needs: [release-contract, release]\n    if: >-' \
  $'  published-release-golden-path:\n    name: Verify the published release journey\n    needs: [release-contract, release]\n    if: false' \
  "published-release job must have exactly one stable-release condition"

expect_mutation_failure \
  "linux-asset-drift" "driver.sh" \
  'cli_asset="assay-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"' \
  'cli_asset="assay-${release_tag}-aarch64-apple-darwin.tar.gz"' \
  "Linux x86_64 product asset assignment drifted"

expect_mutation_failure \
  "same-bundle-command-commented" "driver.sh" \
  'assay evidence show --format json -- "$bundle"' \
  $'assay evidence show --format json -- conformance/privileged-mcp-action-v0/vectors/ok-001.bundle.tar.gz\n  # assay evidence show --format json -- "$bundle"' \
  "driver must inspect the same bundle it produced exactly once"

expect_mutation_failure \
  "verifier-commented" "driver.sh" \
  'bash "$ROOT/scripts/ci/release_attestation_enforce.sh"' \
  '# bash "$ROOT/scripts/ci/release_attestation_enforce.sh"' \
  "driver must execute the reviewed attestation verifier exactly once"

expect_mutation_failure \
  "claim-ceiling-overstated" "driver.sh" \
  'the harness is not a shipped release asset' \
  'the harness is a shipped and attested release asset' \
  "driver lost claim ceiling"

expect_mutation_failure \
  "draft-release-accepted" "driver.sh" \
  $'"$JQ_BIN" -e \'.draft == false and .prerelease == false\' "$release_api" >/dev/null \\\n  || fail "release tag is still draft or prerelease"' \
  'echo accepted-unpublished-release >/dev/null' \
  "driver lost stable published release"

expect_mutation_failure \
  "release-carried-verifier" "driver.sh" \
  'bash "$ROOT/scripts/ci/release_attestation_enforce.sh"' \
  'bash "$downloads/release-proof-kit/verify-offline.sh"' \
  "driver must not execute or trust code carried by the release proof kit"

expect_mutation_failure \
  "external-tag-binding-removed" "driver.sh" \
  '"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}" >"$tag_ref"' \
  '# "$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}" >"$tag_ref"' \
  "external release-tag source binding drifted"

expect_mutation_failure \
  "asset-digest-comparison-inverted" "driver.sh" \
  '[[ "$actual_digest" == "$api_digest" ]] || fail "downloaded asset digest differs: $asset_name"' \
  '[[ "$actual_digest" != "$api_digest" ]] || fail "downloaded asset digest differs: $asset_name"' \
  "downloaded asset digest comparison drifted"

expect_mutation_failure \
  "unsafe-archive-extraction" "driver.sh" \
  'safe_extract "$downloads/$cli_asset" "$cli_extract" 134217728' \
  'tar -xzf "$downloads/$cli_asset" -C "$cli_extract"' \
  "CLI safe-extraction ceiling drifted"

expect_mutation_failure \
  "release-inputs-not-retained" "driver.sh" \
  'downloads="$results/release-assets"' \
  'downloads="$run_root/release-assets"' \
  "release inputs must be retained with the run artifact"

expect_mutation_failure \
  "workflow-run-unbound" "driver.sh" \
  '--bundle-out "$bundle" --run-id "published-release-${workflow_run_id}-${workflow_run_attempt}"' \
  '--bundle-out "$bundle" --run-id published-release-golden-path' \
  "evidence run id is not bound to the workflow invocation"

echo "ok: published-release golden-path contract"
