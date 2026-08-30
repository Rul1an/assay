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
EXAMPLE_RUN="${EXAMPLE_RUN:-${ROOT}/examples/privileged-action-gate/run.sh}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing published-release golden-path workflow"
[[ -f "$DRIVER" ]] || fail "missing published-release golden-path driver"
[[ -f "$MANIFEST" ]] || fail "missing published-release golden-path harness manifest"
[[ -f "$RELEASE_WORKFLOW" ]] || fail "missing release workflow"
[[ -f "$CHECKER" ]] || fail "missing published-release golden-path checker"
[[ -f "$EXAMPLE_RUN" ]] || fail "missing privileged-action-gate example"

python3 "$CHECKER" \
  --workflow "$WORKFLOW" \
  --release-workflow "$RELEASE_WORKFLOW" \
  --driver "$DRIVER" \
  --manifest "$MANIFEST" \
  --source-root "$ROOT"
bash "$ROOT/scripts/ci/test-release-attestation-enforce.sh"
PYTHONPATH="$ROOT/scripts/ci" python3 "$ROOT/scripts/ci/test_safe_extract_release_archive.py"
PYTHONPATH="$ROOT/scripts/ci" python3 "$ROOT/scripts/ci/test_bounded_download.py"
PYTHONPATH="$ROOT/scripts/ci" python3 "$ROOT/scripts/ci/test_published_release_proxy_phase.py"
PYTHONPATH="$ROOT/scripts/ci" python3 "$ROOT/scripts/ci/test_published_release_session_phase.py"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
verifier_call='bash "$harness_root/scripts/ci/release_attestation_enforce.sh" '"\\"
workflow_driver_call='          bash scripts/ci/published-release-golden-path.sh '"\\"
workflow_driver_decoy=$'          # bash scripts/ci/published-release-golden-path.sh\n          echo skipped-reviewed-driver '"\\"

expect_mutation_failure() {
  local name="$1" target="$2" old="$3" new="$4" expected="$5" refresh_path="${6:-}"
  local second_old="${7:-}" second_new="${8:-}"
  local case_root="$scratch/$name"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  cp "$RELEASE_WORKFLOW" "$case_root/release.yml"
  cp "$DRIVER" "$case_root/driver.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$MANIFEST" "$ROOT" "$case_root" <<'PY'
import json, pathlib, shutil, sys
manifest_path, source_root, destination_root = map(pathlib.Path, sys.argv[1:])
for row in json.loads(manifest_path.read_text(encoding="utf-8"))["files"]:
    relative = pathlib.PurePosixPath(row["path"])
    source = source_root.joinpath(*relative.parts)
    destination = destination_root.joinpath(*relative.parts)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
PY
  python3 - "$case_root/$target" "$old" "$new" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
old, new = sys.argv[2:]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
  if [[ -n "$second_old" ]]; then
    python3 - "$case_root/$target" "$second_old" "$second_new" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
old, new = sys.argv[2:]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"second mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
  fi
  if [[ -n "$refresh_path" ]]; then
    python3 - "$case_root/manifest.json" "$case_root/$target" "$refresh_path" <<'PY'
import hashlib, json, pathlib, sys
manifest_path, changed_path = map(pathlib.Path, sys.argv[1:3])
logical_path = sys.argv[3]
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
rows = [row for row in manifest["files"] if row["path"] == logical_path]
if len(rows) != 1:
    raise SystemExit(f"refresh path is not unique in manifest: {logical_path}")
rows[0]["sha256"] = hashlib.sha256(changed_path.read_bytes()).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  fi
  if python3 "$CHECKER" \
      --workflow "$case_root/workflow.yml" \
      --release-workflow "$case_root/release.yml" \
      --driver "$case_root/driver.sh" \
      --manifest "$case_root/manifest.json" \
      --source-root "$case_root" >"$case_root/output" 2>&1; then
    fail "mutation stayed green: $name"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "mutation $name missed expected guard: $expected"
}

expect_proxy_helper_behavior_failure() {
  local name="$1" old="$2" new="$3"
  local case_root="$scratch/$name"
  mkdir -p "$case_root/scripts/ci"
  cp "$ROOT/scripts/ci/published_release_proxy_phase.py" \
    "$case_root/scripts/ci/published_release_proxy_phase.py"
  cp "$ROOT/scripts/ci/test_published_release_proxy_phase.py" \
    "$case_root/scripts/ci/test_published_release_proxy_phase.py"
  python3 - "$case_root/scripts/ci/published_release_proxy_phase.py" "$old" "$new" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
old, new = sys.argv[2:]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"proxy helper mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
  if python3 "$case_root/scripts/ci/test_published_release_proxy_phase.py" \
      >"$case_root/output" 2>&1; then
    fail "proxy helper mutation stayed green: $name"
  fi
}

expect_mutation_failure \
  "preflight-call-commented" "driver.sh" \
  'run_published_release_session_product' '# run_published_release_session_product' \
  "driver must execute the tested pre-init session in the session cwd" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "preflight-uses-unverified-library" "driver.sh" \
  'source "$harness_root/scripts/ci/lib/published-release-capture.sh"' \
  'source "$ROOT/scripts/ci/lib/published-release-capture.sh"' \
  "driver must source the manifest-verified capture library exactly once" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "attestation-removed" "driver.sh" \
  'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"' \
  'echo skipped-attestation' \
  "driver must execute the reviewed attestation verifier exactly once"

expect_mutation_failure \
  "attestation-suppressed" "driver.sh" \
  'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"' \
  'bash "$harness_root/scripts/ci/release_attestation_enforce.sh" || true' \
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
  "github-token-reaches-release-binaries" "driver.sh" \
  'unset GH_TOKEN GITHUB_TOKEN PYTHONPATH' ': # ambient environment retained' \
  "release binaries must not inherit GitHub credentials" \
  "scripts/ci/published-release-golden-path.sh"

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
  "produced-verify-default-v0" "driver.sh" \
  'assay evidence verify-privileged-mcp-action "$bundle" --format json --profile-version v1' \
  'assay evidence verify-privileged-mcp-action "$bundle" --format json' \
  "driver must verify the produced denial-observation bundle with --profile-version v1 exactly once" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "tampered-verify-default-v0" "driver.sh" \
  'assay evidence verify-privileged-mcp-action "$tampered" --format json --profile-version v1' \
  'assay evidence verify-privileged-mcp-action "$tampered" --format json' \
  "driver must verify the tampered denial-observation bundle with --profile-version v1 exactly once" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "produced-bundle-verdict-loosened" "driver.sh" \
  '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "pass" and .verdict == "valid"' \
  '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "pass"' \
  "driver lost produced bundle valid verdict" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "verifier-commented" "driver.sh" \
  'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"' \
  '# bash "$harness_root/scripts/ci/release_attestation_enforce.sh"' \
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
  'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"' \
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

expect_mutation_failure \
  "tag-peel-type-check-removed" "driver.sh" \
  '[[ "$source_type" == "commit" && "$source_digest" =~ ^[0-9a-f]{40}$ ]]' \
  '[[ "$source_digest" =~ ^[0-9a-f]{40}$ ]]' \
  "release tag must peel to a commit before attestation verification"

expect_mutation_failure \
  "raw-attestations-not-retained" "driver.sh" \
  'OUT_RAW_DIR="$results/attestation-raw"' \
  'OUT_RAW_DIR="$run_root/attestation-raw"' \
  "raw attestation inputs must be retained"

expect_mutation_failure \
  "reviewed-driver-dead-code-decoy" "driver.sh" \
  "$verifier_call" \
  $'if false; then\n      bash "$harness_root/scripts/ci/release_attestation_enforce.sh" \\\n    fi\n    bash "$downloads/verifier.sh" \\' \
  "driver attestation execution block drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "mcp-version-regex-loosened" "driver.sh" \
  '"assay-mcp-server $version"' \
  '"$version"' \
  "exact MCP version execution block drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_proxy_helper_behavior_failure \
  "proxy-provenance-truncated" \
  'append_command_record(results / "commands.ndjson", status, argv)' \
  'append_command_record(results / "commands.ndjson", status, argv[:-2])'

expect_mutation_failure \
  "proxy-python-isolation-removed" "driver.sh" \
  $'"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" \\' \
  $'"$PYTHON_BIN" "$harness_root/scripts/ci/published_release_proxy_phase.py" \\' \
  "proxy execution and provenance block drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "workflow-driver-comment-decoy" "workflow.yml" \
  "$workflow_driver_call" \
  "$workflow_driver_decoy" \
  "workflow must execute only the exact reviewed driver invocation" \
  ".github/workflows/published-release-golden-path.yml"

expect_mutation_failure \
  "signer-binding-removed" "scripts/ci/release_attestation_enforce.sh" \
  '    --signer-workflow "$SIGNER_WORKFLOW"' \
  '    --predicate-type https://slsa.dev/provenance/v1' \
  "attestation verification argv binding drifted" \
  "scripts/ci/release_attestation_enforce.sh"

expect_mutation_failure \
  "raw-attestation-count-disabled" "driver.sh" \
  '("attestation-raw/*.json", 2)' \
  '("attestation-raw/*.json", 0)' \
  "retained trust-input count enforcement drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "source-digest-binding-removed" "scripts/ci/release_attestation_enforce.sh" \
  '    --source-digest "$SOURCE_DIGEST"' \
  '    --source-ref refs/heads/main' \
  "attestation verification argv binding drifted" \
  "scripts/ci/release_attestation_enforce.sh"

expect_mutation_failure \
  "local-subject-binding-loosened" "scripts/ci/release_attestation_enforce.sh" \
  'any(.[]; any((.verificationResult.statement.subject // [])[]?; .digest.sha256? == $digest))' \
  'any(.[]; (.verificationResult.statement.subject // [] | length) > 0)' \
  "attestation local-subject digest execution block drifted" \
  "scripts/ci/release_attestation_enforce.sh"

expect_proxy_helper_behavior_failure \
  "proxy-execution-diverges-from-record" \
  $'            process = subprocess.Popen(\n                argv,' \
  $'            process = subprocess.Popen(\n                argv[:-2],'

expect_proxy_helper_behavior_failure \
  "proxy-exit-status-diverges" \
  'append_command_record(results / "commands.ndjson", status, argv)' \
  'append_command_record(results / "commands.ndjson", 0, argv)'

expect_proxy_helper_behavior_failure \
  "proxy-executes-twice" \
  '            process = subprocess.Popen(' \
  $'            subprocess.Popen(argv, env=child_environment()).wait()\n            process = subprocess.Popen('

expect_proxy_helper_behavior_failure \
  "proxy-shell-interpretation-enabled" \
  $'            process = subprocess.Popen(\n                argv,' \
  $'            process = subprocess.Popen(\n                " ".join(argv),\n                shell=True,'

expect_proxy_helper_behavior_failure \
  "github-token-added-to-child-environment" \
  '("HOME", "PATH", "LANG", "LC_ALL", "TZ")' \
  '("HOME", "PATH", "LANG", "LC_ALL", "TZ", "GITHUB_TOKEN")'

expect_proxy_helper_behavior_failure \
  "proxy-timeout-kills-only-parent" \
  '                stop_process_group(process)' \
  '                process.kill(); process.wait()'

expect_mutation_failure \
  "proxy-block-unreachable" "driver.sh" \
  'proxy_status=0' $'if false; then\nproxy_status=0' \
  "proxy execution and provenance block drifted" \
  "scripts/ci/published-release-golden-path.sh" \
  'bundle="$results/produced.bundle.tar.gz"' \
  $'fi\nbundle="$results/produced.bundle.tar.gz"'

expect_mutation_failure \
  "proxy-block-unreachable-subshell" "driver.sh" \
  'proxy_status=0' $'false && (\nproxy_status=0' \
  "proxy execution and provenance block drifted" \
  "scripts/ci/published-release-golden-path.sh" \
  'bundle="$results/produced.bundle.tar.gz"' \
  $')\nbundle="$results/produced.bundle.tar.gz"'

expect_mutation_failure \
  "alternate-proxy-path" "driver.sh" \
  'bundle="$results/produced.bundle.tar.gz"' \
  $'assay-mcp-server proxy-enforce --upstream-command "$PYTHON_BIN"\nbundle="$results/produced.bundle.tar.gz"' \
  "driver MCP binary invocation surface drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "split-token-alternate-proxy-path" "driver.sh" \
  'bundle="$results/produced.bundle.tar.gz"' \
  $'verb="$(printf \'%s%s\' proxy -enforce)"\n"$install_root/bin/assay-mcp-server" "$verb"\nbundle="$results/produced.bundle.tar.gz"' \
  "driver MCP binary invocation surface drifted" \
  "scripts/ci/published-release-golden-path.sh"

expect_mutation_failure \
  "combined-dead-helper-and-truncated-alternate" "driver.sh" \
  'proxy_status=0' $'false && (\nproxy_status=0' \
  "proxy execution and provenance block drifted" \
  "scripts/ci/published-release-golden-path.sh" \
  'bundle="$results/produced.bundle.tar.gz"' \
  $')\nverb="$(printf \'%s%s\' proxy -enforce)"\n"$install_root/bin/assay-mcp-server" "$verb"\nrecord_command "proxy-enforce" 0 "$install_root/bin/assay-mcp-server"\nbundle="$results/produced.bundle.tar.gz"'

expect_mutation_failure \
  "inventory-executable-flag-removed" "manifest.json" \
  '"executable": true' \
  '"executable": false' \
  "harness executable surface drifted"

# Same reported bypass against the old (driver-only) guard and the new example pairing guard.
# The mutant is the shipping denied-bundle verify line with PROFILE_VERSION_V1 removed — not a
# copied command literal.
expect_example_bypass_old_and_new_guard() {
  local case_root="$scratch/example-denied-verify-default-v0"
  local old_exit new_exit expected
  mkdir -p "$case_root"
  expected="$(
    python3 - "$CHECKER" "$EXAMPLE_RUN" "$case_root/run.sh" <<'PY'
import importlib.util, pathlib, sys

spec = importlib.util.spec_from_file_location("checker", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
shipping = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
line = mod.shipping_denied_verify_source_line(shipping)
if mod.PROFILE_VERSION_V1 not in line:
    raise SystemExit("shipping denied-bundle verify is not the paired constant")
pathlib.Path(sys.argv[3]).write_text(mod.seed_denied_verify_bypass(shipping), encoding="utf-8")
print(mod.EXAMPLE_DENIED_VERIFY_PAIRED)
PY
  )" || fail "could not seed the example pairing bypass from the shipping constant"

  if python3 - "$CHECKER" "$WORKFLOW" "$RELEASE_WORKFLOW" "$DRIVER" "$MANIFEST" "$ROOT" <<'PY'
import importlib.util, pathlib, sys

spec = importlib.util.spec_from_file_location("checker", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
problems = mod.validate_contract(*map(pathlib.Path, sys.argv[2:7]))
raise SystemExit(1 if problems else 0)
PY
  then
    old_exit=0
  else
    old_exit=$?
    fail "old guard rejected the example bypass (exit $old_exit)"
  fi

  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --release-workflow "$RELEASE_WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$MANIFEST" \
      --source-root "$ROOT" \
      --example-run "$case_root/run.sh" >"$case_root/output" 2>&1; then
    new_exit=0
    fail "new guard stayed green on the example bypass"
  else
    new_exit=$?
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "new guard missed shipping pairing constant: $expected"
  echo "example bypass old_guard_exit=$old_exit new_guard_exit=$new_exit"
}

expect_example_matrix_forward_mutation() {
  local case_root="$scratch/example-matrix-drops-verify-args"
  local expected
  mkdir -p "$case_root"
  expected="$(
    python3 - "$CHECKER" "$EXAMPLE_RUN" "$case_root/run.sh" <<'PY'
import importlib.util, pathlib, sys

spec = importlib.util.spec_from_file_location("checker", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
shipping = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
anchors = [
    line
    for line in shipping.splitlines()
    if mod.VERIFY_PRIVILEGED in line and '"$bundle"' in line
]
if len(anchors) != 1 or '"$@"' not in anchors[0]:
    raise SystemExit("shipping matrix verify is not the forwarded constant")
pathlib.Path(sys.argv[3]).write_text(
    shipping.replace(anchors[0], anchors[0].replace('"$@"', "", 1), 1),
    encoding="utf-8",
)
print(mod.EXAMPLE_MATRIX_FORWARDS_ARGS)
PY
  )" || fail "could not seed the example matrix-forward mutant from the shipping constant"
  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --release-workflow "$RELEASE_WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$MANIFEST" \
      --source-root "$ROOT" \
      --example-run "$case_root/run.sh" >"$case_root/output" 2>&1; then
    fail "matrix-forward mutation stayed green"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "matrix-forward mutation missed shipping guard constant: $expected"
}

expect_example_bypass_old_and_new_guard
expect_example_matrix_forward_mutation

echo "ok: published-release golden-path contract"
