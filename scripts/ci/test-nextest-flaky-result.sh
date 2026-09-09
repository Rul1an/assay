#!/usr/bin/env bash
# Pin that a test which fails once and passes on retry is reported as a failure by nextest (#2871).
#
# `.config/nextest.toml` retries every test once. Nextest's default treats a fail-then-pass as a
# pass, so a flaky test was green in every lane that runs `cargo nextest run` (local runs, and the
# Split Wave 0 workflow). `flaky-result = "fail"` keeps the retry and its FLAKY diagnostic but
# reports the run as failed. This script proves that property with a fixture test that is flaky by
# construction, under three arms:
#
#   control   the old behaviour, forced with NEXTEST_FLAKY_RESULT=pass: fresh marker, exit 0, FLAKY
#   target    the repository configuration: fresh marker, non-zero exit, FLKY-FL still reported
#   sanity    marker pre-created so the fixture passes first time: exit 0, neither label
#
# Nextest spells the two outcomes differently: a flake treated as a pass is `FLAKY`, a flake
# treated as a failure is `FLKY-FL` with the line `test configured to fail if flaky`. Both carry
# the retry; the target arm asserts the second so a profile that merely drops the retry (one
# plain FAIL, no diagnostic) is rejected as well.
#
# Deleting `flaky-result` from the profile turns the target arm green with exit 0 and this script red.
# Exit codes are captured without a pipe so the runner's status is what is asserted.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
FIXTURE_FILTER='test(flaky_by_construction_fails_once_then_passes)'
FIXTURE_ENV="ASSAY_FLAKY_FIXTURE_MARKER"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || fail "cargo not on PATH"
cargo nextest --version >/dev/null 2>&1 || fail "cargo-nextest is not installed; this contract needs it"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/assay-nextest-flaky.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# One invocation of the fixture. $1 = marker path, $2 = output file, remaining args = extra env.
run_fixture() {
  local marker="$1" out="$2"; shift 2
  set +e
  env "$@" "${FIXTURE_ENV}=${marker}" \
    cargo nextest run -p assay-xtask --run-ignored ignored-only -E "${FIXTURE_FILTER}" \
    >"$out" 2>&1
  local rc=$?
  set -e
  return "$rc"
}

# Build once so the three arms measure the runner, not compilation.
cargo nextest run -p assay-xtask --run-ignored ignored-only -E "${FIXTURE_FILTER}" --no-run \
  >"$scratch/build.out" 2>&1 || { cat "$scratch/build.out" >&2; fail "fixture did not build"; }

# The fixture must exist and be selected, or every arm below passes vacuously.
listed="$(cargo nextest list -p assay-xtask --run-ignored ignored-only -E "${FIXTURE_FILTER}" 2>/dev/null | grep -c 'flaky_by_construction_fails_once_then_passes' || true)"
[ "$listed" -eq 1 ] || fail "expected exactly one fixture test selected by ${FIXTURE_FILTER}, got ${listed}"

# control: old behaviour, fresh marker -> flaky is a pass
rc=0; run_fixture "$scratch/marker-control" "$scratch/control.out" NEXTEST_FLAKY_RESULT=pass || rc=$?
[ "$rc" -eq 0 ] || { cat "$scratch/control.out" >&2; fail "control arm: expected exit 0 under NEXTEST_FLAKY_RESULT=pass, got ${rc}"; }
grep -q 'FLAKY' "$scratch/control.out" || { cat "$scratch/control.out" >&2; fail "control arm: fixture did not register as FLAKY; it is not flaky by construction"; }
echo "ok    control: a retried pass is reported FLAKY and the run exits 0 when flaky-result=pass is forced"

# target: repository configuration, fresh marker -> flaky is a failure
rc=0; run_fixture "$scratch/marker-target" "$scratch/target.out" || rc=$?
[ "$rc" -ne 0 ] || { cat "$scratch/target.out" >&2; fail "target arm: the repository profile let a retried pass exit 0; flaky-result is not \"fail\""; }
grep -q 'FLKY-FL' "$scratch/target.out" || { cat "$scratch/target.out" >&2; fail "target arm: run failed but without the FLKY-FL diagnostic; the retry itself is gone"; }
echo "ok    target: under the repository profile a retried pass exits ${rc} and is reported FLKY-FL"

# sanity: marker present, first attempt passes -> ordinary pass, no FLAKY
: >"$scratch/marker-sanity"
rc=0; run_fixture "$scratch/marker-sanity" "$scratch/sanity.out" || rc=$?
[ "$rc" -eq 0 ] || { cat "$scratch/sanity.out" >&2; fail "sanity arm: fixture failed with the marker present; the fixture is broken, not flaky"; }
if grep -qE 'FLAKY|FLKY-FL' "$scratch/sanity.out"; then cat "$scratch/sanity.out" >&2; fail "sanity arm: a first-attempt pass was reported as a flake"; fi
echo "ok    sanity: with the marker present the fixture passes first time and carries no flake label"

echo "PASS: nextest flaky-result contract"
