#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-sim/src/attacks/memory_poison.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/mod.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/basis.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/vectors.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/controls.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/conditions.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/matrix.rs"
  "docs/contributing/SPLIT-PLAN-wave49-memory-poison.md"
  "docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step2.md"
  "scripts/ci/review-wave49-memory-poison-step2.sh"
)

DIFF_FILES=()
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git diff --name-only "$BASE_REF"...HEAD)
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git ls-files --others --exclude-standard)

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
    [[ -z "$file" ]] && continue
    if [[ "$file" == .github/workflows/* ]]; then
      echo "workflow file changed out of scope: $file" >&2
      exit 1
    fi

    allowed=false
    for allowed_file in "${ALLOWED_FILES[@]}"; do
      if [[ "$file" == "$allowed_file" ]]; then
        allowed=true
        break
      fi
    done
    if [[ "$allowed" == false ]]; then
      echo "out-of-scope file changed: $file" >&2
      exit 1
    fi
  done
fi

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
    [[ -z "$file" ]] && continue
    if [[ "$file" == crates/assay-sim/tests/* ]]; then
      echo "assay-sim tests must remain untouched in Step2" >&2
      exit 1
    fi
    if [[ "$file" == crates/assay-core/* || "$file" == crates/assay-cli/* || "$file" == crates/assay-evidence/* ]]; then
      echo "cross-crate drift out of scope in Wave49 Step2: $file" >&2
      exit 1
    fi
  done
fi

if ! rg -n '^#\[path = "memory_poison_next/mod.rs"\]$' crates/assay-sim/src/attacks/memory_poison.rs >/dev/null; then
  echo "memory_poison.rs must declare the sibling memory_poison_next path override" >&2
  exit 1
fi

if ! rg -n '^mod memory_poison_next;$' crates/assay-sim/src/attacks/memory_poison.rs >/dev/null; then
  echo "memory_poison.rs must declare memory_poison_next module" >&2
  exit 1
fi

for forbidden in \
  '^fn make_clean_deny_basis\(' \
  '^fn make_clean_allow_basis\(' \
  '^fn compute_snapshot_id\(' \
  '^fn condition_b_replay_integrity\(' \
  '^fn compute_basis_hash\(' \
  '^fn vector1_condition_b\(' \
  '^fn vector2_condition_b\(' \
  '^fn vector4_condition_b\(' \
  '^fn vector3_condition_c\(' \
  '^fn make_result\('
do
  if rg -n "$forbidden" crates/assay-sim/src/attacks/memory_poison.rs >/dev/null; then
    echo "memory_poison.rs still contains extracted implementation symbol: $forbidden" >&2
    exit 1
  fi
done

RUST_SCOPE_FILES=(
  "crates/assay-sim/src/attacks/memory_poison.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/mod.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/basis.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/vectors.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/controls.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/conditions.rs"
  "crates/assay-sim/src/attacks/memory_poison_next/matrix.rs"
)

count_base_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if git cat-file -e "$BASE_REF:$file" 2>/dev/null; then
      count=$(git show "$BASE_REF:$file" | rg -o "$pattern" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

count_head_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if [[ -f "$file" ]]; then
      count=$(rg -o "$pattern" "$file" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

for pattern in 'unwrap\(' 'expect\(' '\bunsafe\b' 'println!\(' 'eprintln!\(' 'panic!\(' 'todo!\(' 'unimplemented!\('; do
  base_count="$(count_base_matches "$pattern")"
  head_count="$(count_head_matches "$pattern")"
  if (( head_count > base_count )); then
    echo "pattern '$pattern' increased in memory-poison split scope: $base_count -> $head_count" >&2
    exit 1
  fi
done

cargo fmt --all --check
cargo clippy -q -p assay-sim --all-targets -- -D warnings
cargo check -q -p assay-sim

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
