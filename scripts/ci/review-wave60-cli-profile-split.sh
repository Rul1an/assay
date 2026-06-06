#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-cli/src/cli/commands/profile.rs"
OLD_TESTS="crates/assay-cli/src/cli/commands/profile/tests.rs"
IMPL_DIR="crates/assay-cli/src/cli/commands/profile_next"

allowed_regex='^(crates/assay-cli/src/cli/commands/profile\.rs|crates/assay-cli/src/cli/commands/profile/tests\.rs|crates/assay-cli/src/cli/commands/profile_next/(mod|input|aggregate|display|tests)\.rs|docs/contributing/SPLIT-(PLAN|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave60-cli-profile-split\.md|scripts/ci/review-wave60-cli-profile-split\.sh)$'

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
      "$OLD_TESTS" \
      "$IMPL_DIR" \
      docs/contributing/SPLIT-PLAN-wave60-cli-profile-split.md \
      docs/contributing/SPLIT-CHECKLIST-wave60-cli-profile-split.md \
      docs/contributing/SPLIT-MOVE-MAP-wave60-cli-profile-split.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave60-cli-profile-split.md \
      scripts/ci/review-wave60-cli-profile-split.sh
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
  echo "FAIL: Wave60 profile split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-cli/src/cli/commands/watch.rs crates/assay-cli/src/cli/commands/watch_next crates/assay-cli/src/cli/commands/run.rs crates/assay-cli/src/cli/commands/dispatch.rs crates/assay-cli/src/cli/args; then
  echo "FAIL: Wave60 profile split must not touch watch/run/dispatch/args surfaces"
  exit 1
fi

echo "[review] facade shape"
facade_loc="$(wc -l < "$FACADE" | tr -d ' ')"
if [ "$facade_loc" -gt 20 ]; then
  echo "FAIL: profile facade grew too large: $facade_loc LOC"
  exit 1
fi
assert_rg '#\[path = "profile_next/mod.rs"\]' "$FACADE" "profile facade must route to profile_next"
assert_rg 'pub use profile_next::\{run, Event, InitArgs, ProfileArgs, ProfileCmd, ShowArgs, UpdateArgs\};' "$FACADE" "profile facade must re-export the public command surface"
if rg -n 'fn cmd_|fn read_events|fn aggregate_run|fn merge_run|fn show_summary|ProfilePerfMetrics' "$FACADE" >/dev/null; then
  echo "FAIL: profile facade still owns moved implementation"
  exit 1
fi

echo "[review] module ownership"
assert_rg '^mod aggregate;' "$IMPL_DIR/mod.rs" "aggregate module declaration missing"
assert_rg '^mod display;' "$IMPL_DIR/mod.rs" "display module declaration missing"
assert_rg '^mod input;' "$IMPL_DIR/mod.rs" "input module declaration missing"
assert_rg 'pub fn run' "$IMPL_DIR/mod.rs" "profile run must live in profile_next/mod.rs"
assert_rg 'fn cmd_init' "$IMPL_DIR/mod.rs" "cmd_init must live in profile_next/mod.rs"
assert_rg 'fn cmd_update' "$IMPL_DIR/mod.rs" "cmd_update must live in profile_next/mod.rs"
assert_rg 'fn cmd_show' "$IMPL_DIR/mod.rs" "cmd_show must live in profile_next/mod.rs"
assert_rg 'fn enforce_scope' "$IMPL_DIR/mod.rs" "scope guard must live in profile_next/mod.rs"
assert_rg 'struct ProfilePerfMetrics' "$IMPL_DIR/mod.rs" "perf metrics must live in profile_next/mod.rs"
assert_rg 'pub enum Event' "$IMPL_DIR/input.rs" "event schema must live in input.rs"
assert_rg 'fn read_events' "$IMPL_DIR/input.rs" "event reader must live in input.rs"
assert_rg 'struct RunData' "$IMPL_DIR/aggregate.rs" "RunData must live in aggregate.rs"
assert_rg 'fn aggregate_run' "$IMPL_DIR/aggregate.rs" "aggregate_run must live in aggregate.rs"
assert_rg 'fn merge_run' "$IMPL_DIR/aggregate.rs" "merge_run must live in aggregate.rs"
assert_rg 'fn show_summary' "$IMPL_DIR/display.rs" "show_summary must live in display.rs"
assert_rg 'fn show_top_stable' "$IMPL_DIR/display.rs" "show_top_stable must live in display.rs"

echo "[review] moved tests"
if [ -e "$OLD_TESTS" ]; then
  echo "FAIL: old profile/tests.rs must be moved to profile_next/tests.rs"
  exit 1
fi
assert_rg 'fn aggregate_dedup' "$IMPL_DIR/tests.rs" "aggregate test missing"
assert_rg 'fn merge_existing_entries' "$IMPL_DIR/tests.rs" "merge existing test missing"
assert_rg 'fn scope_guard_mismatch' "$IMPL_DIR/tests.rs" "scope mismatch test missing"
assert_rg 'fn scope_guard_noop' "$IMPL_DIR/tests.rs" "scope noop test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo test -p assay-cli profile
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
