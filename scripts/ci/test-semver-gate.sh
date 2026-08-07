#!/usr/bin/env bash
# The semver gate must be able to fail.
#
# It could not (#2088). `WAVE0_SEMVER_BASELINE_SHA` was pinned to `9cc23b4c`, which is
# `chore: release v2.18.0` from 2026-02-11. `cargo semver-checks check-release` does not ask "is
# there a breaking change"; it asks whether the declared version increment covers the changes it
# finds. Against a 2.x baseline the manifest already declared 3.x -- a major -- so the tool skipped
# every check:
#
#     Checking assay-core v2.18.0 -> v3.38.0 (major change)
#     Checked [0.000s] 0 checks: 0 pass, 254 skip
#     Summary no semver update required
#
# Zero checks run, reported as success. #2068 added a public field to `MetricResult` and this job
# said nothing; the break was found by hand during a release prep, after it had merged.
#
# Measured against the last release tag instead, the same tree gives:
#
#     Checked [0.115s] 223 checks: 222 pass, 1 fail, 0 warn, 31 skip
#     --- failure constructible_struct_adds_field ---
#       field MetricResult.exercised in crates/assay-core/src/metrics_api.rs:38
#
# This script holds the properties that make that difference real. The cheap ones always run. The
# expensive one -- actually planting a break and requiring a non-zero exit -- runs when
# ASSAY_SEMVER_GATE_FULL=1, because it costs a rustdoc build per crate and the workflow it guards
# changes rarely.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="$ROOT/.github/workflows/split-wave0-gates.yml"
FAILURES=0

ok()   { echo "ok    $1"; }
bad()  { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

# --- the baseline is resolved, not pinned ---------------------------------------------------
#
# The whole defect was a constant. A SHA in this file cannot know that the project released twice
# since it was written, and a stale baseline looks exactly like a clean one.
if grep -q 'WAVE0_SEMVER_BASELINE_SHA' "$WORKFLOW"; then
  bad "the workflow still carries a pinned baseline SHA"
else
  ok "no pinned baseline SHA"
fi

if grep -q "git tag --list 'v\[0-9\]\*' --sort=-v:refname" "$WORKFLOW"; then
  ok "the baseline is resolved from the newest release tag"
else
  bad "the baseline is no longer resolved from a release tag"
fi

# --- a missing baseline is a failure, not a skip ---------------------------------------------
#
# "Could not check" and "nothing to report" must not be spelled the same way. This is the rule the
# Linux gate (#2076) and the release gate (#1993) were both fixed to follow.
if grep -q 'no v\* release tag found' "$WORKFLOW" && \
   awk '/no v\* release tag found/,/exit 1/' "$WORKFLOW" | grep -q 'exit 1'; then
  ok "a missing release tag fails the job"
else
  bad "a missing release tag no longer fails the job"
fi

# --- the tag the workflow would pick is the one we expect ------------------------------------
resolved="$(cd "$ROOT" && git tag --list 'v[0-9]*' --sort=-v:refname | head -n1)"
if [ -z "$resolved" ]; then
  bad "no v* tag in this clone, so the workflow would fail closed here (fetch tags to test)"
else
  ok "baseline resolves to ${resolved}"
fi

# --- the expensive one: a planted break must fail --------------------------------------------
#
# Everything above checks the shape of the gate. This checks that the gate reaches a verdict, which
# is the property that was actually missing: the job ran, on the right crates, with the right tool,
# and could not fail.
if [ "${ASSAY_SEMVER_GATE_FULL:-0}" = "1" ]; then
  if ! command -v cargo-semver-checks >/dev/null 2>&1; then
    bad "ASSAY_SEMVER_GATE_FULL=1 but cargo-semver-checks is not installed"
  else
    subject="$ROOT/crates/assay-core/src/metrics_api.rs"
    backup="$(mktemp)"
    cp "$subject" "$backup"

    # The manifest version is set to the baseline's first, and that is not incidental.
    #
    # The first version of this check planted a break and expected a failure, and got a pass: main
    # was at 4.0.0 with the newest tag at v3.38.0, so `check-release` saw a declared major and
    # skipped all 254 checks. That is the gate being CORRECT -- a break merged before 4.0.0 ships is
    # genuinely licensed by the major that has not shipped yet -- and the test being wrong.
    #
    # But a self-test whose result depends on whether a release happens to be pending is not a
    # self-test. Equalising the versions asks the question that is always meaningful: with no bump
    # to hide behind, does this gate reach a verdict?
    baseline_version="$(cd "$ROOT" && git show "${resolved}:Cargo.toml" | awk -F'"' '/^version = /{print $2; exit}')"
    current_version="$(awk -F'"' '/^version = /{print $2; exit}' "$ROOT/Cargo.toml")"
    manifest_backup="$(mktemp -d)"
    (cd "$ROOT" && cp Cargo.toml "$manifest_backup/root.toml" && \
      for m in crates/*/Cargo.toml; do mkdir -p "$manifest_backup/$(dirname "$m")"; cp "$m" "$manifest_backup/$m"; done)
    restore() {
      cp "$backup" "$subject"
      cp "$manifest_backup/root.toml" "$ROOT/Cargo.toml"
      (cd "$ROOT" && for m in crates/*/Cargo.toml; do cp "$manifest_backup/$m" "$m"; done)
      rm -rf "$backup" "$manifest_backup" "${scratch_target:-}"
    }
    trap restore EXIT

    # Every manifest, not only the root. The first version of this moved the workspace version alone
    # and `cargo metadata` refused: nine internal dependencies still declared `version = "4.0.0"`,
    # which the downgraded workspace no longer satisfied. The gate failed, but on a resolver error
    # rather than on the lint -- which is why the check below asserts WHY it failed and not merely
    # that it did.
    (cd "$ROOT" && \
      sed -i.bak "s/^version = \"${current_version}\"$/version = \"${baseline_version}\"/" Cargo.toml && \
      sed -i.bak "s/version = \"${current_version}\"/version = \"${baseline_version}\"/g" Cargo.toml crates/*/Cargo.toml && \
      rm -f Cargo.toml.bak crates/*/Cargo.toml.bak)

    # A new pub field on a pub struct with no `#[non_exhaustive]`: the same shape as the break that
    # got through, so this tests the lint that actually missed it rather than any breaking change.
    python3 - "$subject" <<'PY'
import sys
p = sys.argv[1]
t = open(p).read()
anchor = "pub struct MetricResult {"
assert anchor in t, "MetricResult moved; update the self-test"
t = t.replace(anchor, anchor + "\n    pub deliberately_planted_for_the_gate_test: bool,", 1)
open(p, "w").write(t)
PY
    # Its own target directory. This check downgrades the workspace version and plants a breaking
    # change; artifacts built from that tree are keyed to a version and a source that do not exist,
    # and writing them into the shared target dir would leave them there for the next build.
    scratch_target="$(mktemp -d)"
    out="$(cd "$ROOT" && CARGO_TARGET_DIR="$scratch_target" \
      cargo semver-checks check-release -p assay-core --baseline-rev "$resolved" 2>&1)"
    status=$?
    restore
    trap - EXIT

    if [ "$status" -eq 0 ]; then
      bad "a planted breaking change did not fail the gate"
      printf '%s\n' "$out" | tail -5 | sed 's/^/      /'
    else
      ok "a planted breaking change fails the gate"
    fi

    # And it failed for the right reason. A gate that fails because the tool crashed is not a gate.
    if printf '%s\n' "$out" | grep -q 'constructible_struct_adds_field'; then
      ok "  and names the lint that caught it"
    else
      bad "  but not via constructible_struct_adds_field; it may have failed for an unrelated reason"
    fi
  fi
else
  echo "skip  planted-break check (set ASSAY_SEMVER_GATE_FULL=1 to run it)"
  PLANTED_BREAK_RAN=0
fi

if [ "$FAILURES" -ne 0 ]; then
  echo
  echo "$FAILURES semver-gate case(s) failed"
  exit 1
fi
echo
if [ "${PLANTED_BREAK_RAN:-1}" = "1" ]; then
  echo "semver gate: all cases pass"
else
  # Not "all cases pass". The cheap cases check the gate's SHAPE; the planted break is the only one
  # that shows it reaches a verdict, and it did not run. Saying "all cases pass" here would be a
  # green line standing in for a check nobody performed, which is the defect this file exists for.
  echo "semver gate: shape cases pass; the planted-break case was NOT run"
fi
