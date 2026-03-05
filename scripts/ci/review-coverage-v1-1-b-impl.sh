#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/src/cli/commands/coverage.rs"
  "crates/assay-cli/src/cli/commands/coverage/format_md.rs"
  "crates/assay-cli/tests/coverage_out_md.rs"
  "crates/assay-cli/tests/coverage_routes_top.rs"
  "scripts/ci/review-coverage-v1-1-b-impl.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: coverage v1.1 B-slice must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in coverage v1.1 B-slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'out_md|out-md' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: missing --out-md arg wiring"
  exit 1
}
rg -n 'routes_top|routes-top' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: missing --routes-top arg wiring"
  exit 1
}
rg -n 'render_coverage_markdown' crates/assay-cli/src/cli/commands/coverage/format_md.rs >/dev/null || {
  echo "FAIL: markdown renderer entrypoint missing"
  exit 1
}
rg -n 'routes_top' crates/assay-cli/src/cli/commands/coverage/format_md.rs >/dev/null || {
  echo "FAIL: markdown renderer missing routes_top support"
  exit 1
}
rg -n 'coverage_out_md_writes_json_and_markdown_artifacts' crates/assay-cli/tests/coverage_out_md.rs >/dev/null || {
  echo "FAIL: missing coverage_out_md test"
  exit 1
}
rg -n 'coverage_routes_top_limits_markdown_route_rows' crates/assay-cli/tests/coverage_routes_top.rs >/dev/null || {
  echo "FAIL: missing coverage_routes_top test"
  exit 1
}

echo "[review] run targeted tests + fmt + clippy"
cargo test -p assay-cli coverage_out_md
cargo test -p assay-cli coverage_routes_top
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings

echo "[review] done"
