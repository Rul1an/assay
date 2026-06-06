#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-cli/src/cli/commands/watch.rs"
IMPL_DIR="crates/assay-cli/src/cli/commands/watch_next"

allowed_regex='^(crates/assay-cli/src/cli/commands/watch\.rs|crates/assay-cli/src/cli/commands/watch_next/(mod|paths|snapshot|tests)\.rs|docs/contributing/SPLIT-(PLAN|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave59-cli-watch-split\.md|scripts/ci/review-wave59-cli-watch-split\.sh)$'

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] scope allowlist"
changed_files="$(
  {
    git diff --name-only "$BASE_REF" --
    git ls-files --others --exclude-standard -- \
      "$FACADE" \
      "$IMPL_DIR" \
      docs/contributing/SPLIT-PLAN-wave59-cli-watch-split.md \
      docs/contributing/SPLIT-CHECKLIST-wave59-cli-watch-split.md \
      docs/contributing/SPLIT-MOVE-MAP-wave59-cli-watch-split.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave59-cli-watch-split.md \
      scripts/ci/review-wave59-cli-watch-split.sh
  } | sort -u
)"
while IFS= read -r file; do
  [ -n "$file" ] || continue
  if [[ ! "$file" =~ $allowed_regex ]]; then
    echo "FAIL: out-of-scope path changed: $file"
    exit 1
  fi
done <<EOF
$changed_files
EOF

echo "[review] forbidden drift"
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave59 watch split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-cli/src/cli/commands/profile.rs crates/assay-cli/src/cli/commands/run.rs crates/assay-cli/src/cli/commands/dispatch.rs crates/assay-cli/src/cli/args; then
  echo "FAIL: Wave59 watch split must not touch profile/run/dispatch/args surfaces"
  exit 1
fi

echo "[review] facade shape"
facade_loc="$(wc -l < "$FACADE" | tr -d ' ')"
if [ "$facade_loc" -gt 20 ]; then
  echo "FAIL: watch facade grew too large: $facade_loc LOC"
  exit 1
fi
assert_rg '#\[path = "watch_next/mod.rs"\]' "$FACADE" "watch facade must route to watch_next"
assert_rg 'pub use watch_next::run;' "$FACADE" "watch facade must re-export run"
if rg -n 'async fn run|collect_watch_paths|snapshot_paths|diff_paths|RunArgs' "$FACADE" >/dev/null; then
  echo "FAIL: watch facade still owns moved implementation"
  exit 1
fi

echo "[review] module ownership"
assert_rg '^mod paths;' "$IMPL_DIR/mod.rs" "paths module declaration missing"
assert_rg '^mod snapshot;' "$IMPL_DIR/mod.rs" "snapshot module declaration missing"
assert_rg 'pub async fn run' "$IMPL_DIR/mod.rs" "watch run must live in watch_next/mod.rs"
assert_rg 'crate::cli::commands::run::run' "$IMPL_DIR/mod.rs" "run_once must call the command run implementation"
assert_rg 'fn run_args_from_watch' "$IMPL_DIR/mod.rs" "RunArgs mapping must live in mod.rs"
assert_rg 'fn normalize_debounce_ms' "$IMPL_DIR/mod.rs" "debounce helper must live in mod.rs"
assert_rg 'fn collect_watch_paths' "$IMPL_DIR/paths.rs" "watch path collection must live in paths.rs"
assert_rg 'fn refresh_watch_targets' "$IMPL_DIR/paths.rs" "watch target refresh must live in paths.rs"
assert_rg 'struct FileSnapshot' "$IMPL_DIR/snapshot.rs" "snapshot state must live in snapshot.rs"
assert_rg 'fn snapshot_paths' "$IMPL_DIR/snapshot.rs" "snapshot_paths must live in snapshot.rs"
assert_rg 'fn diff_paths' "$IMPL_DIR/snapshot.rs" "diff_paths must live in snapshot.rs"
assert_rg 'fn coalesce_changed_paths' "$IMPL_DIR/snapshot.rs" "coalescing must live in snapshot.rs"
if rg -n '^pub ' "$IMPL_DIR"/{paths,snapshot}.rs >/dev/null; then
  echo "FAIL: paths/snapshot modules must not expose new public API"
  exit 1
fi

echo "[review] behavior constants and tests"
assert_rg 'MIN_DEBOUNCE_MS: u64 = 50' "$IMPL_DIR/mod.rs" "minimum debounce changed"
assert_rg 'MAX_DEBOUNCE_MS: u64 = 60_000' "$IMPL_DIR/mod.rs" "maximum debounce changed"
assert_rg 'MAX_SNAPSHOT_HASH_BYTES: u64 = 256 \* 1024' "$IMPL_DIR/mod.rs" "snapshot hash limit changed"
assert_rg 'collect_watch_paths_includes_policy' "$IMPL_DIR/tests.rs" "path collection test missing"
assert_rg 'run_args_from_watch_uses_run_defaults' "$IMPL_DIR/tests.rs" "RunArgs mapping test missing"
assert_rg 'diff_paths_detects_same_length_change_via_content_hash' "$IMPL_DIR/tests.rs" "content hash diff test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo test -p assay-cli watch
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
