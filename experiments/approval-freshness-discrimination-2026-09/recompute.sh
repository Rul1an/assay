#!/usr/bin/env bash
# Recompute both observations in this record from what the record names.
# Run from any checkout of Rul1an/assay that contains the baseline commit:
#   docs/experiments/approval-freshness-discrimination-2026-09/recompute.sh
# The script contacts nothing; cargo resolves crates from Cargo.lock (--locked).
# Exit 0 only if every check below holds; any mismatch exits non-zero.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git -C "$HERE" rev-parse --show-toplevel)"
BASELINE=e8f3a2c24172bdab0cc369bb61a03f2b78d7d4a1
PATCHED_TREE=b9783c281c8c1e2d6622461fb27a027b6f487c7a
FILTER=mcp::tool_call_handler::tests::approval

git -C "$REPO" cat-file -e "$BASELINE^{commit}" ||
  { echo "baseline commit $BASELINE is not in $REPO" >&2; exit 2; }
(cd "$HERE" && shasum -a 256 -c --quiet SHA256SUMS.txt) ||
  { echo "record files do not match SHA256SUMS.txt" >&2; exit 2; }

WT="$(mktemp -d "${TMPDIR:-/tmp}/afd-recompute.XXXXXX")"
cleanup() { git -C "$REPO" worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"; }
trap cleanup EXIT
git -C "$REPO" worktree add --detach --quiet "$WT" "$BASELINE"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$WT/target}"

# Result lines: the per-test verdicts and the summary, minus the wall-clock suffix.
result_lines() { grep -E "^test (result:|$FILTER)" "$1" | sed -E 's/; finished in .*$//'; }

run_and_compare() { # <label> <expected exit status>
  local label=$1 want=$2 log="$WT/$1.log" got=0
  (cd "$WT" && cargo test --locked -p assay-core --lib "$FILTER" -- --test-threads=1) \
    >"$log" 2>&1 || got=$?
  [ "$got" -eq "$want" ] ||
    { echo "$label: exit status $got, expected $want" >&2; tail -40 "$log" >&2; exit 1; }
  diff <(result_lines "$HERE/observations/$label.txt") <(result_lines "$log") ||
    { echo "$label: result lines differ from observations/$label.txt" >&2; exit 1; }
  echo "$label: exit status $got, result lines match observations/$label.txt"
}

rustc --version
cargo --version
echo "baseline worktree: $WT at $(git -C "$WT" rev-parse HEAD)"
run_and_compare baseline 0

git -C "$WT" apply --index "$HERE/mutant.patch"
tree="$(git -C "$WT" write-tree)"
[ "$tree" = "$PATCHED_TREE" ] ||
  { echo "patched tree $tree, expected $PATCHED_TREE" >&2; exit 1; }
echo "mutant.patch applied; tree $tree matches the record"
run_and_compare mutant 101

echo "recompute: both observations reproduced"
