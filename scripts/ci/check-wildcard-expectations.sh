#!/usr/bin/env bash
# A `#[expect(clippy::wildcard_enum_match_arm)]` that has stopped being needed must be reported.
#
# `wildcard_enum_match_arm` is a restriction lint and is off by default, and #2055 measured why it
# stays off: it fires on foreign enums too, with no way to ask for ownership. At the CLI boundary
# its precision was 1 in 9, and `crossterm::event::KeyCode` alone has 27 variants, so satisfying it
# in the TUI would mean three blocks of about 25 arms that do nothing.
#
# The consequence, verified rather than assumed: with the lint off, `#[expect]` is silent AND
# `unfulfilled_lint_expectations` never fires either. So the attributes document a decision and
# nothing checks they are still true -- which is not what #2055's acceptance claims.
#
# This is the missing half. It runs clippy with the lint enabled, which makes every `#[expect]`
# checkable, and then fails on ONE thing: an expectation that is no longer fulfilled. The wildcards
# themselves stay advisory, because that is the judgement #2055 already made.
#
#   advisory  wildcard_enum_match_arm         -- a wildcard is a decision, not a defect
#   blocking  unfulfilled_lint_expectations   -- a suppression outliving its reason IS a defect

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

# `#[expect]` is only evaluated where the crate is actually re-checked, and clippy caches per
# fingerprint. Editing a file invalidates its own crate, so this is not what catches an edited
# suppression -- it guards the case where nothing changed and the cache would otherwise answer for
# a workspace nobody looked at.
find crates -maxdepth 3 -name lib.rs -exec touch {} +
find crates -maxdepth 3 -name main.rs -exec touch {} +

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

# Not `-D`: the wildcards are advisory. The lint is enabled only so the expectations get evaluated.
#
# Two things here were defects, both found by mutating this script rather than reading it.
#
# The status is captured, not discarded. This line ended in `|| true`, and when the workspace failed
# to compile there were no expectation diagnostics to find, so the grep below matched nothing and
# the check reported success. A gate that passes because it could not look is the failure it exists
# to prevent.
#
# `> "$log" 2>&1`, in that order. Written the other way round, `2>&1` duplicates stderr to the
# terminal's stdout *before* stdout is redirected, so clippy's diagnostics -- which go to stderr --
# never reach the log at all. The check reported "all still needed" while reading an empty file.
set +e
cargo clippy --workspace --lib --bins --message-format=short \
  -- -W clippy::wildcard_enum_match_arm > "$log" 2>&1
clippy_status=$?
set -e

if [ "$clippy_status" -ne 0 ] || grep -qE "^error(\[|:)" "$log"; then
  echo "error: clippy did not complete, so no expectation could be evaluated." >&2
  grep -E "^error(\[|:)" "$log" | head -20 >&2
  exit 1
fi

if grep -q "this lint expectation is unfulfilled" "$log"; then
  echo "error: a wildcard suppression is no longer needed and should be removed:" >&2
  grep -B1 "this lint expectation is unfulfilled" "$log" >&2
  echo >&2
  echo "The wildcard it covered is gone. Delete the #[expect] rather than leaving a recorded" >&2
  echo "decision about code that no longer exists." >&2
  exit 1
fi

# Counted on the lint name alone, because rustfmt splits a long `#[expect(...)]` across lines and a
# pattern anchored on `expect(clippy::…` then matches nothing. That is not cosmetic: under
# `pipefail` a zero-match grep exits non-zero and took the whole check down with it, after the real
# work had already passed.
expectations="$(grep -ro "clippy::wildcard_enum_match_arm" --include='*.rs' crates 2>/dev/null | wc -l | tr -d ' ')"
echo "wildcard suppressions: ${expectations}, all still needed."
