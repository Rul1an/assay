#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACT="${ROOT}/scripts/ci/test-mcp-upstream-reference-contract.sh"
WORKFLOW="${ROOT}/.github/workflows/mcp-upstream-reference.yml"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

expect_rejected() {
  local name="$1"
  local mutated="${TMP_DIR}/${name}.yml"

  python3 - "$WORKFLOW" "$mutated" "$name" <<'PY'
import pathlib
import sys

source, destination, name = sys.argv[1:]
text = pathlib.Path(source).read_text()
mutations = {
    "paths-ignore": (
        "  pull_request:\n    paths:\n",
        "  pull_request:\n    paths-ignore:\n",
    ),
    "unlocked-build-hidden-by-comment": (
        '          --build-cmd "cargo build --locked -p mcp-conformance" \\\n',
        '          --build-cmd "cargo build -p mcp-conformance" \\\n'
        '          # --build-cmd "cargo build --locked -p mcp-conformance" \\\n',
    ),
    "wrong-reviewed-lock-comparison": (
        '          test "$actual_reviewed_sdk_lock" = "$RUST_SDK_LOCK_SHA256"\n',
        '          test "$actual_reviewed_sdk_lock" = "$RUST_SDK_ARCHIVE_SHA256"\n',
    ),
    "wrong-installed-lock-comparison": (
        '          actual_sdk_lock="$(sha256sum "$sdk_dir/Cargo.lock" | cut -d\' \' -f1)"\n'
        '          test "$actual_sdk_lock" = "$RUST_SDK_LOCK_SHA256"\n',
        '          actual_sdk_lock="$(sha256sum "$sdk_dir/Cargo.lock" | cut -d\' \' -f1)"\n'
        '          test "$actual_sdk_lock" = "$RUST_SDK_ARCHIVE_SHA256"\n',
    ),
    "post-check-hidden-by-comment": (
        '            sha256sum "$RUNNER_TEMP/mcp-rust-sdk/Cargo.lock" | cut -d\' \' -f1\n',
        '            sha256sum "$GITHUB_WORKSPACE/$RUST_SDK_LOCKFILE" | cut -d\' \' -f1\n'
        '            # sha256sum "$RUNNER_TEMP/mcp-rust-sdk/Cargo.lock" | cut -d\' \' -f1\n',
    ),
    "working-tree-lock-hidden-by-comment": (
        '          git show "HEAD:$RUST_SDK_LOCKFILE" > "$reviewed_sdk_lock"\n',
        '          cp "$GITHUB_WORKSPACE/$RUST_SDK_LOCKFILE" "$reviewed_sdk_lock"\n'
        '          # git show "HEAD:$RUST_SDK_LOCKFILE" > "$reviewed_sdk_lock"\n',
    ),
}
old, new = mutations[name]
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor matched {text.count(old)} times, expected once: {old!r}")
pathlib.Path(destination).write_text(text.replace(old, new, 1))
PY

  if WORKFLOW="$mutated" ASSAY_CONTRACT_MUTATION=1 bash "$CONTRACT" >/dev/null 2>&1; then
    echo "FAIL: contract accepted mutation: $name" >&2
    exit 1
  fi
}

expect_rejected paths-ignore
expect_rejected unlocked-build-hidden-by-comment
expect_rejected wrong-reviewed-lock-comparison
expect_rejected wrong-installed-lock-comparison
expect_rejected post-check-hidden-by-comment
expect_rejected working-tree-lock-hidden-by-comment

echo "ok: MCP upstream reference workflow contract rejects inert substitutes"
