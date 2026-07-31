#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/mcp-upstream-reference.yml"
VALIDATOR="${ROOT}/scripts/ci/verify-mcp-upstream-reference.py"
VALIDATOR_TEST="${ROOT}/scripts/ci/test_verify_mcp_upstream_reference.py"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing upstream reference workflow"
[[ -f "$VALIDATOR" ]] || fail "missing upstream result validator"
[[ -f "$VALIDATOR_TEST" ]] || fail "missing validator regression tests"

pin() {
  local needle="$1" message="$2"
  grep -F -- "$needle" "$WORKFLOW" >/dev/null || fail "$message"
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
pin "f4b9fd48cb9598f9658f00a9cc6f176739e97e45045e90b34e8bb4563937bc48" \
  "workflow must verify the Rust SDK dependency lock"
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
pin 'cargo generate-lockfile' \
  "workflow must materialize the reviewed Rust SDK lock"
pin "test \"\$actual_sdk_lock\" = \"\$RUST_SDK_LOCK_SHA256\"" \
  "workflow must verify the Rust SDK lock pin"
pin "--path \"\$sdk_dir\"" \
  "workflow must execute the verified Rust SDK checkout"
pin '--build-cmd "cargo build --locked -p mcp-conformance"' \
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

python3 -m unittest "$VALIDATOR_TEST"

echo "ok: MCP upstream reference workflow contract"
