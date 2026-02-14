#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi

rg_bin="$(command -v rg)"

echo "== Wave4 Step3 quality checks =="
echo "using base_ref=${base_ref}"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Wave4 Step3 contract anchors =="
for test_name in \
  test_explain_simple_trace \
  test_explain_blocked_trace \
  test_explain_max_calls \
  test_terminal_output
 do
  echo "anchor: ${test_name}"
  cargo test -p assay-core "${test_name}" -- --nocapture
done

echo "== Wave4 Step3 facade gates =="
# explain facade must stay thin: only explain_next wiring + re-exports.
if "$rg_bin" -n 'SequenceRule|check_end_of_trace|to_terminal\(|to_markdown\(|to_html\(|summarize_args\(|evaluate_rule\(' crates/assay-core/src/explain.rs; then
  echo "explain.rs facade contains implementation logic"
  exit 1
fi

# render methods should only live in render.rs.
render_outside="$($rg_bin -n 'fn to_terminal\(|fn to_markdown\(|fn to_html\(' crates/assay-core/src/explain_next -g'*.rs' -g'!render.rs' || true)"
if [ -n "$render_outside" ]; then
  echo "render methods found outside explain_next/render.rs:"
  echo "$render_outside"
  exit 1
fi

# state-machine internals should only live in diff.rs.
diff_outside="$($rg_bin -n 'struct ExplainerState|fn evaluate_rule\(|fn check_end_of_trace\(' crates/assay-core/src/explain_next -g'*.rs' -g'!diff.rs' || true)"
if [ -n "$diff_outside" ]; then
  echo "diff/state internals found outside explain_next/diff.rs:"
  echo "$diff_outside"
  exit 1
fi

echo "== Wave4 Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-core/src/explain.rs$|^crates/assay-core/src/explain_next/|^docs/contributing/SPLIT-MOVE-MAP-wave4-step3.md$|^docs/contributing/SPLIT-CHECKLIST-wave4-step3.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave4-step3.md$|^scripts/ci/review-wave4-step3.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave4 Step3 reviewer script: PASS"
