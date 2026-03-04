#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/session_state_window.rs"
  "crates/assay-cli/tests/mcp_wrap_state_window_out.rs"
  "scripts/ci/review-session-state-b1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n 'state_window_out' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: missing --state-window-out"
  exit 1
}
rg -n 'write_state_window_out' crates/assay-cli/src/cli/commands/mcp.rs >/dev/null || {
  echo "FAIL: mcp wrap missing export hook"
  exit 1
}

cargo test -p assay-cli mcp_wrap_state_window_out
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings

echo "[review] done"
