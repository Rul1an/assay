#!/usr/bin/env bash
# The Linux compile gate must be able to fail.
#
# It could not (#2076). Both branches of the old `run_target_check` ended in `return 0`, so a
# timeout, a missing toolchain, a third-party build script and a real compile error were one
# outcome. #2074 pushed four `E0425`s through it; the delegated host found them.
#
# Every case here is that defect or a way of re-introducing it. The first is the one that matters:
# a deliberate `cfg(target_os = "linux")` error must make the script exit non-zero.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE="$ROOT/scripts/ci/check-linux.sh"
FAILURES=0

# A crate that cross-checks cleanly on a stock macOS box, so the error under test is the only cause.
# `assay-cli` and `assay-core` cannot be used: `ring`'s build script aborts before cargo reaches
# them, which is the "unchecked" case rather than the "broken" one.
SUBJECT="$ROOT/crates/assay-monitor/src/lib.rs"
BACKUP="$(mktemp)"

cleanup() {
  [ -f "$BACKUP" ] && cp "$BACKUP" "$SUBJECT"
  rm -f "$BACKUP"
}
trap cleanup EXIT
cp "$SUBJECT" "$BACKUP"

check() {
  local name="$1" want="$2" out status
  out="$(cd "$ROOT" && bash "$GATE" 2>&1)"
  status=$?
  if [ "$status" != "$want" ]; then
    echo "FAIL  $name: exit $status, wanted $want"
    printf '%s\n' "$out" | sed 's/^/      /'
    FAILURES=$((FAILURES + 1))
    return
  fi
  echo "ok    $name"
  LAST_OUT="$out"
}

contains() {
  local name="$1" needle="$2"
  if grep -qF -- "$needle" <<<"$LAST_OUT"; then
    echo "ok    $name"
  else
    echo "FAIL  $name: output does not contain '$needle'"
    printf '%s\n' "$LAST_OUT" | sed 's/^/      /'
    FAILURES=$((FAILURES + 1))
  fi
}

# --- a clean tree passes, and says what it could not check -----------------------------------
check "a clean tree passes" 0
contains "  and names the crates it could not check" "unchecked"

# --- a Linux-gated compile error fails ---------------------------------------------------------
#
# The defect, exactly: this code is invisible to `cargo build` on macOS, which is why the gate
# exists and why its inability to fail mattered.
cat >> "$SUBJECT" <<'RS'

#[cfg(target_os = "linux")]
fn deliberately_broken_for_the_gate_test() {
    let _ = A_VALUE_THAT_DOES_NOT_EXIST;
}
RS
check "a cfg(linux) compile error fails the gate" 1
contains "  and names the crate" "assay-monitor"
cp "$BACKUP" "$SUBJECT"

# --- the same error outside the cfg would also fail, and must not be how we prove it ----------
#
# A test that planted an unconditional error would pass against a gate that only ever ran a host
# build. The `#[cfg(target_os = "linux")]` above is what makes the case load-bearing, so it is
# asserted rather than assumed.
if cargo check -p assay-monitor --quiet 2>/dev/null; then
  echo "ok    the host build is clean, so the planted error was Linux-only"
else
  echo "FAIL  the host build is dirty; the planted error was not Linux-only"
  FAILURES=$((FAILURES + 1))
fi

# --- incomplete coverage can be promoted to a failure ------------------------------------------
#
# On this host two crates cannot be cross-checked, so the strict mode has something to refuse.
if (cd "$ROOT" && ASSAY_LINUX_CHECK_REQUIRE_FULL=1 bash "$GATE" >/dev/null 2>&1); then
  echo "FAIL  REQUIRE_FULL=1 passed despite incomplete coverage"
  FAILURES=$((FAILURES + 1))
else
  echo "ok    REQUIRE_FULL=1 refuses incomplete coverage"
fi

# --- the crate list is derived, not hand-kept --------------------------------------------------
#
# A list beside the code drifts silently in the dangerous direction: a new crate with Linux-gated
# code would simply not be checked, and nothing would say so.
if grep -qE '^\s*linux_crates\(\)' "$GATE" && grep -q 'grep -rl' "$GATE"; then
  echo "ok    the crate set is derived from the source"
else
  echo "FAIL  the crate set is no longer derived from the source"
  FAILURES=$((FAILURES + 1))
fi

if [ "$FAILURES" -ne 0 ]; then
  echo
  echo "$FAILURES linux-gate case(s) failed"
  exit 1
fi
echo
echo "linux compile gate: all cases pass"
