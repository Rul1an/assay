#!/usr/bin/env bash
# Parity between the two implementations of "should this walk read the tracked set, or everything?"
#
# scripts/ci/check-assay-action-pin.sh and scripts/docs/generate-configuration-vocabulary-crosswalk.py
# each encode that rule. One-rule-one-function says the second should call the first; it cannot here,
# because the checker's copy lives inside a shell heredoc and the generator must also run inside the
# drift gate's scratch tree, which has no repository at all. A parity test is the sanctioned fallback,
# and it is not decorative: the two have already diverged once, when only one of them announced its
# fallbacks.
#
# The comparison is behavioural. The checker reports its decision by printing a `note:` line when it
# falls back; the generator reports it by returning None. Four tree shapes, and the two must agree on
# every one.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
CHECKER="${ROOT}/scripts/ci/check-assay-action-pin.sh"
GENERATOR="${ROOT}/scripts/docs/generate-configuration-vocabulary-crosswalk.py"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

build_tree() {
  local dest="$1"
  mkdir -p "${dest}/.github/workflows" "${dest}/scripts/ci/fixtures/assay-action-pin"
  cp "${ROOT}/.github/assay-action-pin" "${dest}/.github/assay-action-pin"
  cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml" \
     "${dest}/scripts/ci/fixtures/assay-action-pin/action.yml"
  cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
     "${dest}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
  while IFS= read -r rel; do
    [[ -z "${rel}" ]] && continue
    mkdir -p "${dest}/$(dirname "${rel}")"
    cp "${ROOT}/${rel}" "${dest}/${rel}"
  done < <("${CHECKER}" --list-paths)
}

checker_fell_back() {
  local tree="$1"
  ASSAY_ACTION_TREE="${tree}" \
    ASSAY_ACTION_PIN_FILE="${tree}/.github/assay-action-pin" \
    ASSAY_ACTION_FIXTURE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/action.yml" \
    ASSAY_ACTION_PROVENANCE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
    "${CHECKER}" 2>"${scratch}/note" >/dev/null || true
  # Read the note from a file rather than a pipeline. Under `set -o pipefail` the pipeline's status
  # is the checker's, not grep's, so a non-zero checker exit reported "did not fall back" whatever
  # the diagnostic actually said. Every tree here exits 0 today, which is what kept it latent.
  if grep -q '^note:' "${scratch}/note"; then echo yes; else echo no; fi
}

generator_fell_back() {
  GEN="${GENERATOR}" TREE="$1" python3 - <<'PY'
import importlib.util, os
spec = importlib.util.spec_from_file_location("gen", os.environ["GEN"])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
from pathlib import Path
print("yes" if mod.tracked_paths(Path(os.environ["TREE"])) is None else "no")
PY
}

expect_parity() {
  local name="$1" tree="$2" want="$3"
  local a b
  a="$(checker_fell_back "${tree}")"
  b="$(generator_fell_back "${tree}")"
  if [[ "${a}" != "${b}" ]]; then
    echo "FAIL: ${name}: checker fell back=${a}, generator fell back=${b}" >&2
    exit 1
  fi
  if [[ "${a}" != "${want}" ]]; then
    echo "FAIL: ${name}: both said fell-back=${a}, expected ${want}" >&2
    exit 1
  fi
  echo "ok    ${name} (both fell back: ${a})"
}

# 1. A worktree root with tracked files: read the tracked set, no fallback.
build_tree "${scratch}/root"
git -c init.defaultBranch=main -C "${scratch}/root" init -q .
git -C "${scratch}/root" add .github/assay-action-pin
expect_parity "worktree-root-with-tracked-files" "${scratch}/root" no

# 2. A worktree root with nothing tracked: an empty listing is not a licence to skip everything.
build_tree "${scratch}/empty"
git -c init.defaultBranch=main -C "${scratch}/empty" init -q .
expect_parity "worktree-root-empty-index" "${scratch}/empty" yes

# 3. Inside a repository but below its root: a partial listing must not be trusted.
mkdir -p "${scratch}/outer"
git -c init.defaultBranch=main -C "${scratch}/outer" init -q .
build_tree "${scratch}/outer/subtree"
git -C "${scratch}/outer" add subtree/.github/assay-action-pin
expect_parity "subtree-of-a-repository" "${scratch}/outer/subtree" yes

# 4. No repository at all -- what the drift gate's scratch copy looks like.
build_tree "${scratch}/bare"
expect_parity "not-a-repository" "${scratch}/bare" yes

echo "tracked-paths parity: PASS"
