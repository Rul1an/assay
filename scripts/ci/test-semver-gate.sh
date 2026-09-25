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
source "$ROOT/scripts/ci/lib/clear-git-repository-env.sh"
WORKFLOW="$ROOT/.github/workflows/semver-public.yml"
FAILURES=0

ok()   { echo "ok    $1"; }
bad()  { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

# The full published-library pass must retain a bounded, usable job budget.
if ruby -ryaml - "$WORKFLOW" <<'RUBY'
doc = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
job = doc.fetch("jobs").fetch("semver-public")
abort "semver-public must have a 30-minute job budget" unless job.fetch("timeout-minutes", nil) == 30
RUBY
then
  ok "full semver pass has a bounded 30-minute budget"
else
  bad "full semver pass budget drifted"
fi

# --- the baseline is resolved, not pinned ---------------------------------------------------
#
# The whole defect was a constant. A SHA in this file cannot know that the project released twice
# since it was written, and a stale baseline looks exactly like a clean one.
if grep -q 'WAVE0_SEMVER_BASELINE_SHA' "$WORKFLOW"; then
  bad "the workflow still carries a pinned baseline SHA"
else
  ok "no pinned baseline SHA"
fi

# Execute the actual workflow step against local Git remotes, not a second selector.
# This also proves that a failed fetch cannot silently use a stale local baseline.
baseline_cases="$(mktemp -d)"
trap 'rm -rf "$baseline_cases"' EXIT
ruby -ryaml - "$WORKFLOW" > "$baseline_cases/step.sh" <<'RUBY'
doc = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
steps = doc.fetch("jobs").fetch("semver-public").fetch("steps")
print steps.find { |step| step["id"] == "baseline" }.fetch("run")
RUBY

baseline_case() {
  local name="$1" expected="$2"
  shift 2
  local case_root="$baseline_cases/$name" tag status
  mkdir -p "$case_root"
  git init -q "$case_root/source" || return 1
  git -C "$case_root/source" -c user.name=fixture -c user.email=fixture@example.invalid \
    -c core.hooksPath=/dev/null -c commit.gpgSign=false commit --allow-empty -qm fixture || return 1
  for tag in "$@"; do git -C "$case_root/source" -c tag.gpgSign=false tag "$tag" || return 1; done
  if [ "$name" = noncommit ]; then
    local blob
    blob="$(printf 'not a commit' | git -C "$case_root/source" hash-object -w --stdin)"
    git -C "$case_root/source" -c tag.gpgSign=false tag v99.0.0 "$blob" || return 1
  fi
  git clone -q --bare "$case_root/source" "$case_root/origin" || return 1
  git clone -q "$case_root/origin" "$case_root/clone" || return 1
  # Positive path control before interpreting a refusal.
  git -C "$case_root/clone" cat-file -e 'HEAD^{commit}' || return 1
  mkdir -p "$case_root/clone/scripts/ci"
  if [ -f "$ROOT/scripts/ci/resolve-semver-baseline.sh" ]; then
    cp "$ROOT/scripts/ci/resolve-semver-baseline.sh" "$case_root/clone/scripts/ci/"
  fi
  if [ "$name" = fetch_failure ]; then mv "$case_root/origin" "$case_root/unavailable"; fi
  (cd "$case_root/clone" && GITHUB_OUTPUT="$case_root/output" bash "$baseline_cases/step.sh") \
    > "$case_root/log" 2>&1
  status=$?
  if [ "$expected" = refuse ]; then
    [ "$status" -ne 0 ] && return 0
  elif [ "$status" -eq 0 ] && [ "$(cat "$case_root/output")" = "tag=$expected" ]; then
    return 0
  fi
  cat "$case_root/log" >&2
  echo "baseline case $name: exit=$status, expected=$expected" >&2
  return 1
}

for scenario in stable prereleases no_stable no_tags fetch_failure noncommit; do
  case "$scenario" in
    stable) baseline_case "$scenario" v6.10.0 v6.9.0 v6.10.0 ;;
    prereleases) baseline_case "$scenario" v6.3.0 v6.3.0 v6.3.1-rc.1 v7.0.0-beta.1 ;;
    no_stable) baseline_case "$scenario" refuse v6.3.1-rc.1 ;;
    no_tags) baseline_case "$scenario" refuse ;;
    fetch_failure) baseline_case "$scenario" refuse v6.3.0 ;;
    noncommit) baseline_case "$scenario" refuse v6.3.0 ;;
  esac
  if [ "$?" -eq 0 ]; then ok "workflow baseline: $scenario"; else bad "workflow baseline: $scenario"; fi
done
rm -rf "$baseline_cases"
trap - EXIT

# --- the tag the workflow would pick is the one we expect ------------------------------------
resolved="$(bash "$ROOT/scripts/ci/resolve-semver-baseline.sh" "$ROOT")"
if [ -z "$resolved" ]; then
  bad "no stable vX.Y.Z tag in this clone (fetch tags to test)"
else
  ok "baseline resolves to ${resolved}"
fi

# --- the expensive one: planted breaks in two newly covered crates must fail ------------------
#
# Everything above checks baseline selection and the gate's shape. This checks its API verdict, which
# is the property that was actually missing: the job ran, on the right crates, with the right tool,
# and could not fail.
if [ "${ASSAY_SEMVER_GATE_FULL:-0}" = "1" ]; then
  if ! command -v cargo-semver-checks >/dev/null 2>&1; then
    bad "ASSAY_SEMVER_GATE_FULL=1 but cargo-semver-checks is not installed"
  else
    subject_runner_core="$ROOT/crates/assay-runner-core/src/run.rs"
    subject_sim="$ROOT/crates/assay-sim/src/report.rs"
    backup_runner_core="$(mktemp)"
    backup_sim="$(mktemp)"
    cp "$subject_runner_core" "$backup_runner_core"
    cp "$subject_sim" "$backup_sim"

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
    restore_all() {
      cp "$backup_runner_core" "$subject_runner_core"
      cp "$backup_sim" "$subject_sim"
      cp "$manifest_backup/root.toml" "$ROOT/Cargo.toml"
      (cd "$ROOT" && for m in crates/*/Cargo.toml; do cp "$manifest_backup/$m" "$m"; done)
      rm -rf "$backup_runner_core" "$backup_sim" "$manifest_backup" "${scratch_target:-}"
    }
    trap restore_all EXIT

    # Every manifest, not only the root. The first version of this moved the workspace version alone
    # and `cargo metadata` refused: nine internal dependencies still declared `version = "4.0.0"`,
    # which the downgraded workspace no longer satisfied. The gate failed, but on a resolver error
    # rather than on the lint -- which is why the check below asserts WHY it failed and not merely
    # that it did.
    (cd "$ROOT" && \
      sed -i.bak "s/^version = \"${current_version}\"$/version = \"${baseline_version}\"/" Cargo.toml && \
      sed -i.bak "s/version = \"${current_version}\"/version = \"${baseline_version}\"/g" Cargo.toml crates/*/Cargo.toml && \
      rm -f Cargo.toml.bak crates/*/Cargo.toml.bak)

    plant_break() {
      local subject="$1" anchor="$2" field="$3"
      python3 - "$subject" "$anchor" "$field" <<'PY'
import sys
p, anchor, field = sys.argv[1], sys.argv[2], sys.argv[3]
t = open(p).read()
assert anchor in t, "MetricResult moved; update the self-test"
t = t.replace(anchor, anchor + "\n" + field, 1)
open(p, "w").write(t)
PY
    }

    check_planted_break() {
      local crate="$1" subject="$2" anchor="$3"
      local field='    pub deliberately_planted_for_the_gate_test: bool,'
      local out status
      plant_break "$subject" "$anchor" "$field"
      scratch_target="$(mktemp -d)"
      out="$(cd "$ROOT" && CARGO_TARGET_DIR="$scratch_target" \
        cargo semver-checks check-release -p "$crate" --baseline-rev "$resolved" 2>&1)"
      status=$?
      rm -rf "$scratch_target"
      scratch_target=""
      if [ "$status" -eq 0 ]; then
        bad "a planted breaking change did not fail the gate for ${crate}"
      else
        ok "a planted breaking change fails the gate for ${crate}"
      fi
      if printf '%s\n' "$out" | grep -q 'constructible_struct_adds_field'; then
        ok "  and names constructible_struct_adds_field for ${crate}"
      else
        bad "  but ${crate} did not fail via constructible_struct_adds_field"
      fi
      # Restore file after each planted mutation so the second crate starts clean.
      if [ "$crate" = "assay-runner-core" ]; then
        cp "$backup_runner_core" "$subject_runner_core"
      elif [ "$crate" = "assay-sim" ]; then
        cp "$backup_sim" "$subject_sim"
      fi
    }

    # Two newly covered crates from issue #2983: assay-runner-core and assay-sim.
    check_planted_break \
      "assay-runner-core" \
      "$subject_runner_core" \
      "pub struct RunSpec {"
    check_planted_break \
      "assay-sim" \
      "$subject_sim" \
      "pub struct SimSummary {"
    restore_all
    trap - EXIT
  fi
else
  echo "skip  planted-break check (set ASSAY_SEMVER_GATE_FULL=1 to run it)"
  PLANTED_BREAK_RAN=0
fi

# --- the downstream jsonschema witness is wired and can fail the job (#3176) -----------------
#
# A witness nothing invokes is dormant. Execute the workflow's own step script against a stub
# harness: its exit status must reach the job, and nothing may make the step optional.
witness_cases="$(mktemp -d)"
if ruby -ryaml - "$WORKFLOW" > "$witness_cases/step.sh" <<'RUBY'
doc = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
job = doc.fetch("jobs").fetch("semver-public")
abort "semver-public job must not be continue-on-error" if job.key?("continue-on-error")
steps = job.fetch("steps").select { |s| s["name"] == "Downstream public-dependency witness (jsonschema)" }
abort "expected exactly one downstream witness step, found #{steps.length}" unless steps.length == 1
step = steps.first
abort "downstream witness step must not carry if:" if step.key?("if")
abort "downstream witness step must not be continue-on-error" if step.key?("continue-on-error")
print step.fetch("run")
RUBY
then
  witness_step_case() {
    local stub_exit="$1" case_root="$witness_cases/exit-$1" status
    mkdir -p "$case_root/scripts/ci"
    printf '#!/usr/bin/env bash\nexit %s\n' "$stub_exit" > "$case_root/scripts/ci/check-downstream-jsonschema-witness.sh"
    (cd "$case_root" && bash -eo pipefail "$witness_cases/step.sh") >/dev/null 2>&1
    status=$?
    [ "$status" -eq "$stub_exit" ]
  }
  if witness_step_case 7 && witness_step_case 0; then
    ok "downstream jsonschema witness step runs the harness and propagates its exit"
  else
    bad "downstream jsonschema witness step does not propagate the harness exit"
  fi
else
  bad "downstream jsonschema witness step is missing or optional"
fi
rm -rf "$witness_cases"

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
