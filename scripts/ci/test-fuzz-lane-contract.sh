#!/usr/bin/env bash
set -euo pipefail

# Contract assertions for the fuzz lane's lock-staleness guard.
#
# The guard wraps `cargo metadata --locked`, and a wrapper that names one cause for every failure
# is a diagnosis the command did not make. `--locked` fails on a stale lock, but equally on an
# unparsable manifest, an unavailable dependency, a registry or network fault, or a broken
# toolchain — and the first version of this wrapper reported all of them as "fuzz/Cargo.lock is
# stale" and told the reader to run `cargo update --workspace`. On a registry outage that advice
# is wrong and, followed, would rewrite a lock that was never the problem.
#
# So the wrapper must add exit-code discipline and nothing else: Cargo's own stderr stays visible
# and stays the diagnosis. These assertions pin that, because prose in a comment is what went
# stale last time.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/fuzz-smoke.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || fail "missing ${WORKFLOW#"$ROOT"/}"

# Every assertion takes the workflow path, so the same logic can be run against a deliberately
# broken copy below. A control that lives only in someone's shell history is not a control.
check_workflow() {
  local wf="$1"

  # The guard is the `||` arm attached to the `cargo metadata --locked` invocation. Anchor on the
  # invocation *line number*, not on any line mentioning a lock: the seed-corpus check also emits
  # `::error::`, and the workflow's own comments name the command in prose — a plain grep picked a
  # comment block and every assertion below then reported on the wrong two lines.
  local guard_ln guard_block guard_line guard_msg pin
  guard_ln="$(grep -nE '^[^#]*cargo[^#]*metadata --locked' "$wf" | head -1 | cut -d: -f1)"
  [[ -n "$guard_ln" ]] || fail "no \`cargo metadata --locked\` invocation found"
  guard_block="$(sed -n "${guard_ln},$((guard_ln + 1))p" "$wf")"

  guard_line="$(grep '::error::' <<<"$guard_block" | head -1 || true)"
  [[ -n "$guard_line" ]] || fail "no error message attached to the \`cargo metadata --locked\` guard"
  guard_msg="${guard_line#*::error::}"

  # 1. The message must not name a cause the command did not establish.
  for claim in stale "cargo update" "out of date" outdated regenerate; do
    if grep -qi -- "$claim" <<<"$guard_msg"; then
      fail "the lock-guard message claims '$claim', but \`cargo metadata --locked\` fails for \
several unrelated reasons (bad manifest, unavailable dependency, registry or network fault, \
toolchain configuration). Report the failure and let Cargo's stderr say why:\n  $guard_msg"
    fi
  done

  # 2. It must point the reader at the real diagnosis rather than replacing it.
  grep -qiE 'cargo error|error above|output above' <<<"$guard_msg" \
    || fail "the lock-guard message should send the reader to Cargo's own error, got:\n  $guard_msg"

  # 3. Cargo's stderr must stay visible. Only stdout is noise here — the metadata JSON.
  #    Any `2>` form, not an enumeration of them: `2>/dev/null` and `2>&1` were listed, so
  #    `2>cargo-error.log` passed both this test and actionlint while putting the only real
  #    diagnosis in a file nobody reads. Checked across the whole anchored block, since the
  #    redirect can sit on the invocation or on its `||` arm.
  if grep -qE '2>' <<<"$guard_block"; then
    fail "the guard redirects stderr, which discards the only real diagnosis it has:\n$guard_block"
  fi

  # 4. The failure must still be a failure — asserted on the guard's own `||` arm, not on the file.
  #    Searching the whole workflow passed on the seed-corpus check's unrelated `exit 1`, so
  #    removing only this guard's exit left the contract green. An assertion scoped wider than its
  #    subject reports on something else.
  grep -q 'exit 1' <<<"$guard_line" \
    || fail "the lock guard reports but does not exit nonzero:\n  $guard_line"

  # 5. The toolchain pin stays dated. A channel alias would hide both a break and its fix, which is
  #    exactly what happened when nightly-2026-07-24 began ICEing on tokio under sanitizer coverage.
  pin="$(grep -E '^\s*FUZZ_TOOLCHAIN:' "$wf" | head -1 | sed 's/.*: *//')"
  [[ "$pin" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] \
    || fail "FUZZ_TOOLCHAIN must be a dated nightly, got: ${pin:-<empty>}"
  PIN="$pin"

  # 6. Every local crate the fuzz graph resolves must trigger this lane.
  #
  #    Derived from `fuzz/Cargo.lock` rather than listed: a package with no `source` is a path
  #    dependency, so the set comes from the same file the lane pins. Listing them by hand is what
  #    left `assay-adapter-api`, `assay-canonical` and `assay-common` uncovered while
  #    `assay-core` and `assay-evidence` were named -- a change in any of the three could alter
  #    code, manifest or lock without this lane ever running, which is the staleness this whole
  #    branch exists to stop. Offline and cheap: no cargo invocation.
  #    Read from the `on.pull_request.paths` sequence itself, not from the file. A crate name
  #    appears in this workflow's own prose too, so a whole-file grep answered "covered" for
  #    `# - "crates/assay-common/**"` -- a commented-out entry that triggers nothing, and that
  #    actionlint passes as valid YAML. The extractor takes only `- ` items inside that one block.
  local paths_active locals missing=""
  paths_active="$(awk '
    /^  pull_request:[[:space:]]*$/ { in_pr=1; next }
    /^  [^[:space:]]/              { in_pr=0; in_paths=0 }
    in_pr && /^    paths:[[:space:]]*$/ { in_paths=1; next }
    in_pr && /^    [^[:space:]]/  { in_paths=0 }
    in_paths && /^      -[[:space:]]/ { print }
  ' "$wf")"
  [[ -n "$paths_active" ]] \
    || fail "could not read the \`on.pull_request.paths\` sequence from ${wf##*/}"

  local locals
  locals="$(awk '/^\[\[package\]\]/{name="";src=0} /^name = /{gsub(/[",]/,"",$3); name=$3} \
                 /^source = /{src=1} /^$/{if(name!="" && !src) print name} \
                 END{if(name!="" && !src) print name}' \
            "${ROOT}/fuzz/Cargo.lock" | grep -v '^assay-fuzz$' | sort -u)"
  [[ -n "$locals" ]] || fail "could not derive local path dependencies from fuzz/Cargo.lock"
  while read -r crate; do
    [[ -n "$crate" ]] || continue
    grep -qF "\"crates/${crate}/**\"" <<<"$paths_active" || missing="${missing} ${crate}"
  done <<<"$locals"
  [[ -z "$missing" ]] || fail "the fuzz lane resolves these local crates but does not trigger on \
them, so a change there can go untested:${missing}"
}

check_workflow "$WORKFLOW"

# Negative control: strip the guard's exit and nothing else. The seed-corpus check keeps its own
# `exit 1`, which is exactly the state that used to pass.
mutant="$(mktemp)"
trap 'rm -f "$mutant"' EXIT
sed 's|\(inspect the Cargo error above"\); exit 1; }|\1; }|' "$WORKFLOW" > "$mutant"
if ! grep -q 'exit 1' "$mutant"; then
  fail "the mutation removed every exit, so it does not isolate the guard"
fi
if ( check_workflow "$mutant" ) >/dev/null 2>&1; then
  fail "removing only the guard's exit left the contract green — the fail-closed assertion is not \
bound to the guard"
fi
echo "ok: removing only the guard's exit turns the contract red"

# Negative control: send stderr to a file. It is not `/dev/null` and not `2>&1`, so the enumerated
# form of this check passed it -- and actionlint passes it too, since the shell is valid. The
# diagnosis simply lands somewhere no reviewer looks.
redirect_mutant="$(mktemp)"
trap 'rm -f "$mutant" "$redirect_mutant"' EXIT
sed 's|--format-version 1 >/dev/null )|--format-version 1 >/dev/null 2>cargo-error.log )|' \
  "$WORKFLOW" > "$redirect_mutant"
grep -q '2>cargo-error.log' "$redirect_mutant" \
  || fail "the redirect mutation did not apply, so it proves nothing"
if ( check_workflow "$redirect_mutant" ) >/dev/null 2>&1; then
  fail "sending Cargo's stderr to a file left the contract green — the check enumerates redirect \
forms instead of rejecting them"
fi
echo "ok: redirecting Cargo's stderr to a file turns the contract red"

# Negative control: drop one crate from the path filter. This is the state the branch shipped in --
# three of the five local crates were simply absent -- so the assertion that catches it has to be
# shown catching it.
paths_mutant="$(mktemp)"
trap 'rm -f "$mutant" "$redirect_mutant" "$paths_mutant"' EXIT
grep -v '"crates/assay-common/\*\*"' "$WORKFLOW" > "$paths_mutant"
if ( check_workflow "$paths_mutant" ) >/dev/null 2>&1; then
  fail "dropping a local crate from the path filter left the contract green — the coverage \
assertion is not bound to the lockfile"
fi
echo "ok: dropping a local crate from the path filter turns the contract red"

# Negative control: comment the entry out instead of deleting it. The line is still in the file and
# still says the crate's name, so a whole-file grep called it covered — while `pull_request.paths`
# no longer carries it and actionlint sees nothing wrong.
comment_mutant="$(mktemp)"
trap 'rm -f "$mutant" "$redirect_mutant" "$paths_mutant" "$comment_mutant"' EXIT
sed 's|^      - "crates/assay-common/\*\*"|      # - "crates/assay-common/**"|' \
  "$WORKFLOW" > "$comment_mutant"
grep -q '^      # - "crates/assay-common/\*\*"' "$comment_mutant" \
  || fail "the comment mutation did not apply, so it proves nothing"
if ( check_workflow "$comment_mutant" ) >/dev/null 2>&1; then
  fail "commenting out a path entry left the contract green — the coverage assertion reads the \
file rather than the active \`pull_request.paths\` sequence"
fi
echo "ok: commenting out a path entry turns the contract red"

echo "ok: the lock guard reports without diagnosing, keeps Cargo's stderr, and fails closed"
echo "ok: FUZZ_TOOLCHAIN is dated (${PIN})"
echo "PASS: fuzz lane contract"
