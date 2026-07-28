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

  # The guard is the `||` arm attached to the `cargo metadata --locked` invocation, so anchor on
  # that rather than on any line mentioning a lock — the seed-corpus check also emits `::error::`.
  local guard_line guard_msg guard_cmd pin
  guard_line="$(grep -n -A1 'metadata --locked' "$wf" | grep '::error::' | head -1 || true)"
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
  guard_cmd="$(grep -n 'metadata --locked' "$wf" | head -1 | cut -d: -f2-)"
  [[ -n "$guard_cmd" ]] || fail "no \`cargo metadata --locked\` invocation found"
  if grep -qE '2>[&]?[/ ]?(dev/null|1)' <<<"$guard_cmd"; then
    fail "the guard redirects stderr, which discards the only real diagnosis it has:\n  $guard_cmd"
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

echo "ok: the lock guard reports without diagnosing, keeps Cargo's stderr, and fails closed"
echo "ok: FUZZ_TOOLCHAIN is dated (${PIN})"
echo "PASS: fuzz lane contract"
