#!/usr/bin/env bash
# Pin that a test which fails once and passes on retry is reported as a failure by nextest (#2871).
#
# `.config/nextest.toml` retries every test once. Nextest's default treats a fail-then-pass as a
# pass, so a flaky test was green in every lane that runs `cargo nextest run` (local runs, and the
# Split Wave 0 lane). `flaky-result = "fail"` keeps the retry and its diagnostic but reports the
# run as failed. The fixtures live in crates/gateway-evidence-replay/tests/ and are driven by
# NEXTEST_ATTEMPT, which nextest sets per attempt process.
#
# Three arms, each a separate nextest run:
#
#   control   flaky fixture with NEXTEST_FLAKY_RESULT=pass forced: exit 0, labelled FLAKY
#   target    flaky fixture under the repository profile: non-zero, labelled FLKY-FL
#   clean     the non-flaky fixture: exit 0, no flake label of either spelling
#
# Nextest spells the two outcomes differently: a flake treated as a pass is `FLAKY`, a flake
# treated as a failure is `FLKY-FL` with the line `test configured to fail if flaky`. The target
# arm asserts the second, so a profile that merely dropped the retry (one plain FAIL, no
# diagnostic) is rejected as well. Exit codes are captured without a pipe.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIXTURE_CRATE="gateway-evidence-replay"
FLAKY_TEST="flaky_by_construction_fails_once_then_passes"
CLEAN_TEST="clean_by_construction_passes_first_attempt"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || fail "cargo not on PATH"
cargo nextest --version >/dev/null 2>&1 || fail "cargo-nextest is not installed; this contract needs it"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/assay-nextest-flaky.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# One nextest run over a single named fixture. $1 = test name, $2 = output file, rest = extra env.
run_fixture() {
  local test_name="$1" out="$2"; shift 2
  set +e
  env "$@" cargo nextest run -p "$FIXTURE_CRATE" --run-ignored ignored-only \
    -E "test(=${test_name})" >"$out" 2>&1
  local rc=$?
  set -e
  return "$rc"
}

# Each arm must select exactly one test. A filter that matched nothing would make every
# assertion below pass vacuously, and nextest exits 0 on an empty selection by default.
assert_selects_one() {
  local test_name="$1" listed rc=0
  listed="$(cargo nextest list -p "$FIXTURE_CRATE" --run-ignored ignored-only \
    -E "test(=${test_name})" 2>"$scratch/list.err")" || rc=$?
  [ "$rc" -eq 0 ] || { cat "$scratch/list.err" >&2; fail "nextest list failed for ${test_name}"; }
  local count
  count="$(grep -c -- "$test_name" <<<"$listed" || true)"
  [ "$count" -eq 1 ] || fail "expected exactly one test named ${test_name}, nextest listed ${count}"
}

# Build once so the arms measure the runner rather than compilation.
cargo nextest run -p "$FIXTURE_CRATE" --run-ignored ignored-only --no-run \
  >"$scratch/build.out" 2>&1 || { cat "$scratch/build.out" >&2; fail "fixtures did not build"; }

assert_selects_one "$FLAKY_TEST"
assert_selects_one "$CLEAN_TEST"

# control: old behaviour forced -> a retried pass is a pass
rc=0; run_fixture "$FLAKY_TEST" "$scratch/control.out" NEXTEST_FLAKY_RESULT=pass || rc=$?
[ "$rc" -eq 0 ] || { cat "$scratch/control.out" >&2; fail "control arm: expected exit 0 under NEXTEST_FLAKY_RESULT=pass, got ${rc}"; }
grep -q 'FLAKY' "$scratch/control.out" || { cat "$scratch/control.out" >&2; fail "control arm: fixture did not register as FLAKY; it is not flaky by construction"; }
echo "ok    control: a retried pass is FLAKY and exits 0 when flaky-result=pass is forced"

# target: repository profile -> a retried pass is a failure, and still diagnosed
rc=0; run_fixture "$FLAKY_TEST" "$scratch/target.out" || rc=$?
[ "$rc" -ne 0 ] || { cat "$scratch/target.out" >&2; fail "target arm: the repository profile let a retried pass exit 0; flaky-result is not \"fail\""; }
grep -q 'FLKY-FL' "$scratch/target.out" || { cat "$scratch/target.out" >&2; fail "target arm: run failed but without the FLKY-FL diagnostic; the retry itself is gone"; }
echo "ok    target: under the repository profile a retried pass exits ${rc} and is reported FLKY-FL"

# clean: a test that passes first time carries no flake label under the same profile
rc=0; run_fixture "$CLEAN_TEST" "$scratch/clean.out" || rc=$?
[ "$rc" -eq 0 ] || { cat "$scratch/clean.out" >&2; fail "clean arm: the non-flaky fixture failed under the repository profile"; }
if grep -qE 'FLAKY|FLKY-FL' "$scratch/clean.out"; then cat "$scratch/clean.out" >&2; fail "clean arm: a first-attempt pass was labelled a flake"; fi
echo "ok    clean: a first-attempt pass exits 0 and carries no flake label"

echo "PASS: nextest flaky-result contract"
