#!/usr/bin/env bash
# The generated-docs drift check must be able to fail, and must not destroy what it audits.
#
# Both properties have gone wrong in this repository before. `check-aee-seal-fixture-drift.sh`
# carries a header about an earlier version that ran its emitter in place and destroyed uncommitted
# edits in the fixtures it was checking. `check-linux.sh` returned 0 on every failure and could not
# report anything at all (#2076). This pins both for the docs check.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE="$ROOT/scripts/ci/check-docs-generated-drift.sh"
SUBJECT="$ROOT/docs/generated/crate-deps.mermaid"
FAILURES=0
BACKUP="$(mktemp)"

cleanup() { [ -f "$BACKUP" ] && cp "$BACKUP" "$SUBJECT"; rm -f "$BACKUP"; }
trap cleanup EXIT
cp "$SUBJECT" "$BACKUP"

check() {
  local name="$1" want="$2"
  (cd "$ROOT" && bash "$GATE" >/dev/null 2>&1)
  local status=$?
  if [ "$status" = "$want" ]; then echo "ok    $name"; else
    echo "FAIL  $name: exit $status, wanted $want"; FAILURES=$((FAILURES + 1)); fi
}

check "a tree in sync passes" 0

# --- drift is caught ---------------------------------------------------------------------------
printf '\n%%%% drift planted by the drift-check self-test\n' >> "$SUBJECT"
check "a hand-edited generated file fails" 1
cp "$BACKUP" "$SUBJECT"

# --- the check does not edit what it audits ----------------------------------------------------
#
# The generators write in place, so a check that ran them in the worktree would silently rewrite the
# very files under review — and would then always pass, having made itself right.
before="$(shasum -a 256 "$SUBJECT" | awk '{print $1}')"
(cd "$ROOT" && bash "$GATE" >/dev/null 2>&1)
after="$(shasum -a 256 "$SUBJECT" | awk '{print $1}')"
if [ "$before" = "$after" ]; then
  echo "ok    the check leaves the worktree untouched"
else
  echo "FAIL  the check rewrote the file it was auditing"
  FAILURES=$((FAILURES + 1))
fi

# --- an uncommitted change is what gets checked ------------------------------------------------
#
# The first version generated from `git archive HEAD`, so at pre-commit time it compared against the
# previous commit and would have passed on the very change being committed. This pins the fix.
if grep -q "git ls-files" "$GATE" && ! grep -q "git archive --format=tar HEAD" "$GATE"; then
  echo "ok    the check reads the working tree, not HEAD"
else
  echo "FAIL  the check generates from HEAD; an uncommitted change would go unchecked"
  FAILURES=$((FAILURES + 1))
fi

# --- a generator that cannot run is not a pass -------------------------------------------------
if grep -q "could not check" "$GATE"; then
  echo "ok    a generator failure refuses rather than passing"
else
  echo "FAIL  a generator failure has no explicit refusal"
  FAILURES=$((FAILURES + 1))
fi

if [ "$FAILURES" -ne 0 ]; then echo; echo "$FAILURES docs-drift case(s) failed"; exit 1; fi
echo
echo "generated-docs drift check: all cases pass"
