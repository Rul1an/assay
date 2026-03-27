#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave49-memory-poison.md"
  "docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step3.md"
  "scripts/ci/review-wave49-memory-poison-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-sim/src/attacks"
  "crates/assay-sim/tests"
)

changed_files() {
  git diff --name-only "$BASE_REF"...HEAD || true
  git diff --name-only || true
  git diff --name-only --cached || true
  git ls-files --others --exclude-standard || true
}

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave49 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave49 Step3: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave49 Step3 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for p in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
PLAN="docs/contributing/SPLIT-PLAN-wave49-memory-poison.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step3.md"

for marker in \
  'Wave49 Step2 shipped on `main` via `#973`.' \
  '`#975` shipped the follow-up fail-closed hashing fix for replay basis comparison.' \
  'keep `memory_poison.rs` as the stable facade entrypoint' \
  'Step3 constraints:' \
  'no new module cuts' \
  'no behavior cleanup beyond internal follow-up notes'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-sim/src/attacks/memory_poison.rs' \
  'crates/assay-sim/src/attacks/memory_poison_next/basis.rs' \
  'crates/assay-sim/src/attacks/memory_poison_next/vectors.rs' \
  'crates/assay-sim/src/attacks/memory_poison_next/controls.rs' \
  'crates/assay-sim/src/attacks/memory_poison_next/conditions.rs' \
  'crates/assay-sim/src/attacks/memory_poison_next/matrix.rs' \
  'future internal visibility tightening only if it requires a separate code wave' \
  'memory-poison result or attack-name contract changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-sim --all-targets -- -D warnings

echo "[review] pinned memory-poison invariants"
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector1_activates_under_condition_a' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector3_activates_under_condition_a' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::controls_produce_no_false_positives' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::full_matrix_runs_without_panic' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_b_blocks_v1_and_v2' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_c_blocks_v3' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::overarching_invariant_controls_never_misclassify' -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant overarching_invariant_no_silent_downgrades_in_controls -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant attack_vectors_activate_under_condition_a -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant condition_b_blocks_replay_vectors -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant condition_c_blocks_context_envelope -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant full_matrix_structure -- --exact

echo "[review] PASS"
