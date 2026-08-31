#!/usr/bin/env bash
# Behavioural tests for generate-configuration-vocabulary-crosswalk.py.
#
# The drift gate is a round-trip against output the generator itself produced, so it proves
# reproducibility, not correctness: every guard in the generator could be removed and the gate would
# still be green. These pin the three that decide what the page is allowed to say.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
GEN="${ROOT}/scripts/docs/generate-configuration-vocabulary-crosswalk.py"
scratch="$(mktemp -d)"
# The probe lives in a unique directory under the repository root, and only that directory is
# removed. A fixed filename would overwrite a pre-existing untracked file of the same name and then
# delete it -- destroying someone's work to test that untracked work is ignored.
probe_dir="$(mktemp -d "${ROOT}/zz-crosswalk-probe.XXXXXX")"
probe="${probe_dir}/record.json"
trap 'rm -rf "${scratch}" "${probe_dir}"' EXIT

run_gen() { python3 "${GEN}" --repo-root "${ROOT}" --out "$1" >/dev/null; }

run_gen "${scratch}/baseline.md"

# 1. An untracked file is not repository content. The drift gate seeds its scratch from
#    `git ls-files`, so a record the generator reads but the gate cannot see produces a drift
#    failure that re-running never clears.
printf '%s\n' '{"schema":"zz.probe.decision.v0","decision":"allow","policy_digest":"sha256:probe"}' \
  >"${probe}"
run_gen "${scratch}/untracked.md"
if ! diff -q "${scratch}/baseline.md" "${scratch}/untracked.md" >/dev/null; then
  echo "FAIL: an untracked record reached the page" >&2
  diff "${scratch}/baseline.md" "${scratch}/untracked.md" >&2 || true
  exit 1
fi
grep -q 'zz.probe.decision.v0' "${scratch}/untracked.md" &&
  { echo "FAIL: the probe schema is on the page" >&2; exit 1; }
echo "ok    untracked-record-does-not-reach-the-page"
rm -rf "${probe_dir}"

# 2. The tracked set must come from THIS tree. An inherited GIT_DIR or GIT_INDEX_FILE -- which
#    pre-commit and git hooks routinely set -- otherwise makes ls-files answer about another
#    repository, and the wrong tracked set is reached in silence.
poison="${scratch}/poison"
mkdir -p "${poison}"
env -u GIT_DIR -u GIT_INDEX_FILE git -c init.defaultBranch=main -C "${poison}" init -q .
printf 'x\n' >"${poison}/only.txt"
env -u GIT_DIR -u GIT_INDEX_FILE git -C "${poison}" add only.txt
GIT_DIR="${poison}/.git" GIT_INDEX_FILE="${poison}/.git/index" GIT_WORK_TREE="${poison}" \
  python3 "${GEN}" --repo-root "${ROOT}" --out "${scratch}/poisoned.md" >/dev/null
if ! diff -q "${scratch}/baseline.md" "${scratch}/poisoned.md" >/dev/null; then
  echo "FAIL: a poisoned git environment changed the page" >&2
  diff "${scratch}/baseline.md" "${scratch}/poisoned.md" | head -20 >&2 || true
  exit 1
fi
echo "ok    poisoned-git-environment-is-ignored"

# 3. A citation that no longer resolves must stop the write. This is the only mechanical check on
#    prose that asserts what the code does, and three reviews found three such assertions wrong.
copy="${scratch}/broken-citation.py"
sed 's|crates/assay-core/src/mcp/policy/mod\.rs|crates/assay-core/src/mcp/policy/DOES-NOT-EXIST.rs|' \
  "${GEN}" >"${copy}"
if python3 "${copy}" --repo-root "${ROOT}" --out "${scratch}/broken.md" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: a citation to a nonexistent path was written anyway" >&2
  exit 1
fi
grep -q 'cited path does not exist' "${scratch}/err" ||
  { echo "FAIL: generation failed for the wrong reason:" >&2; cat "${scratch}/err" >&2; exit 1; }
echo "ok    unresolvable-citation-blocks-the-write"

# 4. Same for a symbol. The row that stopped paraphrasing what the digest covers points at a
#    function instead, which is only better while the function still carries that name.
#    The replacement name is assembled at run time on purpose. Writing it as a literal put it in
#    this file, which the symbol search reads, so the check found the name it was meant to miss and
#    the case passed while proving nothing.
sym="${scratch}/broken-symbol.py"
absent="zz$(printf %s _absent_symbol_)probe"
sed "s|project_and_normalize_declared|${absent}|" "${GEN}" >"${sym}"
if python3 "${sym}" --repo-root "${ROOT}" --out "${scratch}/sym.md" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: a citation to a nonexistent symbol was written anyway" >&2
  exit 1
fi
grep -q 'cited symbol appears nowhere' "${scratch}/err" ||
  { echo "FAIL: generation failed for the wrong reason:" >&2; cat "${scratch}/err" >&2; exit 1; }
echo "ok    unresolvable-symbol-blocks-the-write"

# 5. The symbol check must not be satisfied by the page's own toolchain. Every symbol the curated
#    prose cites also appears in the generator's SUBJECTS, in the rendered page, and in this script.
#    Without excluding those three, renaming the real declaration leaves all of them behind and the
#    check stays green -- verified by renaming `project_and_normalize_declared` in the Rust source
#    and watching it pass. Here the whole tree IS those excluded files and nothing else, so every
#    citation must be reported missing.
self_root="${scratch}/self-only"
mkdir -p "${self_root}/docs/architecture" "${self_root}/scripts/docs" "${self_root}/crates"
cp "${ROOT}/docs/architecture/CONFIGURATION-VOCABULARY-CROSSWALK.md" \
   "${self_root}/docs/architecture/CONFIGURATION-VOCABULARY-CROSSWALK.md"
cp "${GEN}" "${self_root}/scripts/docs/"
# Every cited PATH must exist here as an empty file, or the path check fires first and the case
# would pass for a reason that has nothing to do with symbols.
grep -oE '`(crates|docs|scripts|tests|packaging|conformance|\.github)(/[A-Za-z0-9_.-]+)+/?`' \
  "${ROOT}/docs/architecture/CONFIGURATION-VOCABULARY-CROSSWALK.md" |
  tr -d '`' | sort -u |
  while IFS= read -r cited; do
    case "${cited}" in
      */) mkdir -p "${self_root}/${cited}" ;;
      *)  mkdir -p "${self_root}/$(dirname "${cited}")"; : >"${self_root}/${cited}" ;;
    esac
  done
if python3 "${GEN}" --repo-root "${self_root}" --out "${scratch}/self.md" \
     >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: symbols validated themselves against the page and the generator" >&2
  exit 1
fi
grep -q 'cited symbol appears nowhere' "${scratch}/err" ||
  { echo "FAIL: refused for the wrong reason:" >&2; cat "${scratch}/err" >&2; exit 1; }
echo "ok    symbols-are-not-validated-by-the-pages-own-toolchain"

# 6. The populated column must count one path, not every key that ends alike. Two keys in the
#    vectors record end in `declared_policy_digest`; matching the tail reported their sum as though
#    it belonged to the curated one.
copy_tail="${scratch}/tail-match.py"
sed 's|    values = entry\["keys"\].get(field, \[\])|    values = [v for k, vs in entry["keys"].items() if k.split(".")[-1] == field.split(".")[-1] for v in vs]|' \
  "${GEN}" >"${copy_tail}"
python3 "${copy_tail}" --repo-root "${ROOT}" --out "${scratch}/tail.md" >/dev/null
if diff -q "${scratch}/baseline.md" "${scratch}/tail.md" >/dev/null; then
  echo "FAIL: tail matching produced identical output, so the whole-path rule is unpinned" >&2
  exit 1
fi
echo "ok    populated-counts-one-path-not-every-key-ending-alike"

echo "crosswalk generator contract: PASS"
