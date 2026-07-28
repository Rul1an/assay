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

# The guard is the `||` arm attached to the `cargo metadata --locked` invocation, so anchor on
# that rather than on any line mentioning a lock — the seed-corpus check also emits `::error::`.
guard_line="$(grep -n -A1 'metadata --locked' "$WORKFLOW" | grep '::error::' | head -1 || true)"
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
guard_cmd="$(grep -n 'metadata --locked' "$WORKFLOW" | head -1 | cut -d: -f2-)"
[[ -n "$guard_cmd" ]] || fail "no \`cargo metadata --locked\` invocation found"
if grep -qE '2>[&]?[/ ]?(dev/null|1)' <<<"$guard_cmd"; then
  fail "the guard redirects stderr, which discards the only real diagnosis it has:\n  $guard_cmd"
fi

# 4. The failure must still be a failure. A guard that reports and continues is not a guard.
grep -q 'exit 1' "$WORKFLOW" || fail "the lock guard does not exit nonzero"

# 5. The toolchain pin stays dated. A channel alias would hide both a break and its fix, which is
#    exactly what happened when nightly-2026-07-24 began ICEing on tokio under sanitizer coverage.
pin="$(grep -E '^\s*FUZZ_TOOLCHAIN:' "$WORKFLOW" | head -1 | sed 's/.*: *//')"
[[ "$pin" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] \
  || fail "FUZZ_TOOLCHAIN must be a dated nightly, got: ${pin:-<empty>}"

echo "ok: the lock guard reports without diagnosing, keeps Cargo's stderr, and fails closed"
echo "ok: FUZZ_TOOLCHAIN is dated (${pin})"
echo "PASS: fuzz lane contract"
