#!/usr/bin/env bash
# Literal workflow commands below must retain their dollar signs and backslashes.
# shellcheck disable=SC1003,SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/mcp-upstream-reference.yml}"
VALIDATOR="${ROOT}/scripts/ci/verify-mcp-upstream-reference.py"
VALIDATOR_TEST="${ROOT}/scripts/ci/test_verify_mcp_upstream_reference.py"
SDK_LOCK_FIXTURE="${ROOT}/scripts/ci/fixtures/mcp-upstream-reference/rust-sdk-3240b6e7828ed4146041d32dd0ce4ced7c04e411.Cargo.lock"
GIT_ATTRIBUTES="${ROOT}/.gitattributes"
MUTATION_TEST="${ROOT}/scripts/ci/test-mcp-upstream-reference-contract-mutations.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing upstream reference workflow"
[[ -f "$VALIDATOR" ]] || fail "missing upstream result validator"
[[ -f "$VALIDATOR_TEST" ]] || fail "missing validator regression tests"
[[ -f "$SDK_LOCK_FIXTURE" ]] || fail "missing reviewed Rust SDK dependency lock"
[[ -f "$GIT_ATTRIBUTES" ]] || fail "missing git attributes"
[[ -f "$MUTATION_TEST" ]] || fail "missing workflow contract mutation tests"

pin() {
  local needle="$1" message="$2"
  grep -F -- "$needle" "$WORKFLOW" >/dev/null || fail "$message"
}

event_path_count() {
  local event="$1" needle="$2"
  awk -v event="$event" -v needle="$needle" '
    function indent(line) {
      match(line, /^ */)
      return RLENGTH
    }
    function trim(line) {
      sub(/^[[:space:]]+/, "", line)
      sub(/[[:space:]]+$/, "", line)
      return line
    }
    trim($0) == event ":" && indent($0) == 2 {
      in_event = 1
      event_indent = indent($0)
      next
    }
    in_event && trim($0) != "" && $0 !~ /^[[:space:]]*#/ \
      && indent($0) <= event_indent { exit }
    in_event && trim($0) == "paths:" {
      in_paths = 1
      paths_indent = indent($0)
      next
    }
    in_paths && trim($0) != "" && $0 !~ /^[[:space:]]*#/ \
      && indent($0) <= paths_indent { in_paths = 0 }
    in_paths {
      item = trim($0)
      if (substr(item, 1, 2) != "- ") next
      item = substr(item, 3)
      if (item ~ /^".*"$/ || item ~ /^'"'"'.*'"'"'$/) {
        item = substr(item, 2, length(item) - 2)
      }
      if (item == needle) count++
    }
    END { print count + 0 }
  ' "$WORKFLOW"
}

step_line() {
  local step="$1" needle="$2" message="$3"
  awk -v step="$step" -v needle="$needle" '
    $0 == "      - name: " step { in_step = 1; next }
    in_step && $0 ~ /^      - name:/ { exit }
    in_step {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      sub(/[[:space:]]+$/, "", line)
      if (line == needle) found = 1
    }
    END { exit found ? 0 : 1 }
  ' "$WORKFLOW" || fail "$message"
}

pin "49103de6ed70804e940637bf3e9e29e4a3f54e64" \
  "workflow must pin the conformance source commit"
pin "e48c96369788414b05c6d3cf4a7233d01ff6e6a88e8f8943d586ae3dda1b87fe" \
  "workflow must verify the source archive digest"
pin "161aef794720d2393a6a3db64e9751f2d52730b49f662e84b23363df5c1196e1" \
  "workflow must verify the package-lock digest"
pin "3240b6e7828ed4146041d32dd0ce4ced7c04e411" \
  "workflow must pin the Rust SDK reference"
pin "4ce506ce729ad4ed2b28de6bad157eda83247a2d298924c7590c0fc938e4351c" \
  "workflow must verify the Rust SDK source archive"
pin "ff0bab171e7e812b41c8c653cd33ac07c948f594cc2beedf6e34ac5711ecc031" \
  "workflow must verify the Rust SDK dependency lock"
pin 'RUST_SDK_LOCKFILE: "scripts/ci/fixtures/mcp-upstream-reference/rust-sdk-3240b6e7828ed4146041d32dd0ce4ced7c04e411.Cargo.lock"' \
  "workflow must name the reviewed Rust SDK dependency lock"
pin 'NODE_VERSION: "24.18.0"' "workflow must pin the Node runtime"
pin "node-version: \"\${{ env.NODE_VERSION }}\"" \
  "workflow must install the pinned Node version"
pin "npm ci" "workflow must install the pinned lock"
pin "sep-2322-client-request-state" "workflow must run the client scenario"
pin "input-required-result-result-type" \
  "workflow must run the explicit resultType server scenario"
pin "input-required-result-request-state" \
  "workflow must run the requestState server control"
pin "verify-mcp-upstream-reference.py" \
  "workflow must validate named check records"
pin "Upstream reference only" \
  "workflow must state its conformance non-claim"
pin "fetch --depth=1 origin \"\$CONFORMANCE_COMMIT\"" \
  "workflow must fetch the pinned conformance commit"
pin "test \"\$actual_archive\" = \"\$CONFORMANCE_ARCHIVE_SHA256\"" \
  "workflow must verify the conformance archive pin"
pin "test \"\$actual_lock\" = \"\$CONFORMANCE_LOCK_SHA256\"" \
  "workflow must verify the conformance lock pin"
pin "fetch --depth=1 origin \"\$RUST_SDK_COMMIT\"" \
  "workflow must fetch the pinned Rust SDK commit"
pin "test \"\$actual_sdk_archive\" = \"\$RUST_SDK_ARCHIVE_SHA256\"" \
  "workflow must verify the Rust SDK archive pin"
pin "cp \"\$GITHUB_WORKSPACE/\$RUST_SDK_LOCKFILE\" \"\$sdk_dir/Cargo.lock\"" \
  "workflow must install the reviewed Rust SDK lock"
pin "test \"\$actual_sdk_lock\" = \"\$RUST_SDK_LOCK_SHA256\"" \
  "workflow must verify the Rust SDK lock pin"
pin "Verify the installed Rust SDK lock remained unchanged" \
  "workflow must verify the Rust SDK lock after the scenarios"
step_line "Fetch the official Rust SDK source and install its reviewed lock" \
  'actual_reviewed_sdk_lock="$(sha256sum "$reviewed_sdk_lock" | cut -d'"'"' '"'"' -f1)"' \
  "workflow must hash the reviewed Rust SDK lock before installation"
step_line "Fetch the official Rust SDK source and install its reviewed lock" \
  'test "$actual_reviewed_sdk_lock" = "$RUST_SDK_LOCK_SHA256"' \
  "workflow must compare the reviewed Rust SDK lock with its pinned digest"
step_line "Fetch the official Rust SDK source and install its reviewed lock" \
  'actual_sdk_lock="$(sha256sum "$sdk_dir/Cargo.lock" | cut -d'"'"' '"'"' -f1)"' \
  "workflow must hash the installed Rust SDK lock"
step_line "Fetch the official Rust SDK source and install its reviewed lock" \
  'test "$actual_sdk_lock" = "$RUST_SDK_LOCK_SHA256"' \
  "workflow must compare the installed Rust SDK lock with its pinned digest"
step_line "Verify the installed Rust SDK lock remained unchanged" \
  'sha256sum "$RUNNER_TEMP/mcp-rust-sdk/Cargo.lock" | cut -d'"'"' '"'"' -f1' \
  "workflow must hash the installed Rust SDK lock after the scenarios"
step_line "Verify the installed Rust SDK lock remained unchanged" \
  'test "$actual_sdk_lock" = "$RUST_SDK_LOCK_SHA256"' \
  "workflow must compare the post-scenario SDK lock with the reviewed digest"
pin "--path \"\$sdk_dir\"" \
  "workflow must execute the verified Rust SDK checkout"
step_line "Run the focused official reference scenarios" \
  '--build-cmd "cargo build --locked -p mcp-conformance" \' \
  "workflow must build the Rust SDK with its verified lock"

grep -Eq 'uses: actions/checkout@[0-9a-f]{40}' "$WORKFLOW" \
  || fail "checkout action must be SHA-pinned"
grep -Eq 'uses: actions/setup-node@[0-9a-f]{40}' "$WORKFLOW" \
  || fail "setup-node action must be SHA-pinned"
grep -Eq 'uses: actions/upload-artifact@[0-9a-f]{40}' "$WORKFLOW" \
  || fail "upload-artifact action must be SHA-pinned"

if grep -Eq 'continue-on-error:[[:space:]]*true' "$WORKFLOW"; then
  fail "the reference run must not turn a failed check into success"
fi

if grep -F 'cargo generate-lockfile' "$WORKFLOW" >/dev/null; then
  fail "the reference run must not resolve a fresh Rust SDK dependency graph"
fi

if grep -F 'cargo update' "$WORKFLOW" >/dev/null; then
  fail "the reference run must not update the reviewed Rust SDK dependency graph"
fi

fixture_path='scripts/ci/fixtures/mcp-upstream-reference/**'
[[ "$(event_path_count pull_request "$fixture_path")" == "1" ]] \
  || fail "pull_request must run exactly once for reviewed Rust SDK lock changes"
[[ "$(event_path_count push "$fixture_path")" == "1" ]] \
  || fail "push must run exactly once for reviewed Rust SDK lock changes"
mutation_test_path='scripts/ci/test-mcp-upstream-reference-contract-mutations.sh'
[[ "$(event_path_count pull_request "$mutation_test_path")" == "1" ]] \
  || fail "pull_request must run exactly once for contract mutation changes"
[[ "$(event_path_count push "$mutation_test_path")" == "1" ]] \
  || fail "push must run exactly once for contract mutation changes"
attributes_path='.gitattributes'
[[ "$(event_path_count pull_request "$attributes_path")" == "1" ]] \
  || fail "pull_request must run exactly once for line-ending policy changes"
[[ "$(event_path_count push "$attributes_path")" == "1" ]] \
  || fail "push must run exactly once for line-ending policy changes"

actual_sdk_lock="$(shasum -a 256 "$SDK_LOCK_FIXTURE" | awk '{print $1}')"
[[ "$actual_sdk_lock" == "ff0bab171e7e812b41c8c653cd33ac07c948f594cc2beedf6e34ac5711ecc031" ]] \
  || fail "reviewed Rust SDK dependency lock digest drifted"
grep -Fx 'scripts/ci/fixtures/mcp-upstream-reference/** text eol=lf' "$GIT_ATTRIBUTES" \
  >/dev/null || fail "reviewed Rust SDK dependency lock must be checked out with LF endings"

python3 -m unittest "$VALIDATOR_TEST"

if [[ "${ASSAY_CONTRACT_MUTATION:-0}" != "1" ]]; then
  bash "$MUTATION_TEST"
fi

echo "ok: MCP upstream reference workflow contract"
