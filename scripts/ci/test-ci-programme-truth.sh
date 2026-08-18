#!/usr/bin/env bash
# #2515: programme ledger, Wave 0 docs, and Wave71 status match live repository truth.
#
# Refuted issue premises, pinned so they cannot drive the patch:
# - crates/assay-cli/src/cli/commands/monitor.rs still exists; the preview is not
#   removed because that file was deleted.
# - the review-split-wave Assay Sim example path still exists (pinned in
#   test-review-split-wave.sh); do not replace it.
#
# The generic warn-only unsafe preview still goes: its Wave 3
# single-boundary TODO is false. This test does not freeze a catalogue of
# intentional unsafe sites; no such allowlist exists, and inventing one
# would break a later legitimate boundary split.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AGENTS="$ROOT/AGENTS.md"
WAVE0="$ROOT/docs/contributing/WAVE0-GATES.md"
STATUS="$ROOT/docs/contributing/REFACTOR-WAVE-STATUS.md"
RULESET="$ROOT/.github/rulesets/main-required-ci-contexts.json"
WORKFLOW="$ROOT/.github/workflows/split-wave0-gates.yml"
MONITOR="$ROOT/crates/assay-cli/src/cli/commands/monitor.rs"

FAILURES=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

live_required_contexts() {
  python3 - "$RULESET" <<'PY'
import json, sys
from pathlib import Path
data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
seen = []
for rule in data.get("rules", []):
    if rule.get("type") != "required_status_checks":
        continue
    for check in rule.get("parameters", {}).get("required_status_checks", []):
        ctx = check.get("context")
        if ctx and ctx not in seen:
            seen.append(ctx)
if not seen:
    raise SystemExit("ruleset has no required_status_checks contexts")
print("\n".join(seen))
PY
}

assert_agents_ledger() {
  local text="$1"
  local collapsed
  collapsed="$(printf '%s' "$text" | tr '\n' ' ' | tr -s ' ')"
  if ! grep -q 'public execution ledger' <<<"$collapsed"; then
    echo "AGENTS.md has no public execution ledger line"
    return 1
  fi
  if grep -Eq 'named on this line:[[:space:]]*\[issue #' <<<"$collapsed"; then
    echo "ledger line still names a GitHub issue as the active programme"
    return 1
  fi
  if ! grep -Eq 'No programme is active' <<<"$collapsed"; then
    echo "ledger line does not say no programme is active"
    return 1
  fi
  return 0
}

assert_wave0_semver_doc() {
  local text="$1"
  if grep -q 'WAVE0_SEMVER_BASELINE_SHA' <<<"$text"; then
    echo "WAVE0-GATES.md still documents WAVE0_SEMVER_BASELINE_SHA"
    return 1
  fi
  if ! grep -Eqi 'newest|latest' <<<"$text" || ! grep -Eq 'release tag' <<<"$text"; then
    echo "WAVE0-GATES.md does not describe the dynamic latest-release baseline"
    return 1
  fi
  if ! grep -q 'test-semver-gate.sh' <<<"$text"; then
    echo "WAVE0-GATES.md does not point at scripts/ci/test-semver-gate.sh"
    return 1
  fi
  return 0
}

assert_wave0_required_contexts() {
  local text="$1"
  local ctx
  while IFS= read -r ctx; do
    if ! grep -Fq -- "$ctx" <<<"$text"; then
      echo "WAVE0-GATES.md does not name live required context ${ctx}"
      return 1
    fi
  done < <(live_required_contexts)
  if grep -q 'Configure branch protection to require:' <<<"$text"; then
    echo "WAVE0-GATES.md still presents Wave 0 jobs as the branch-protection recommendation"
    return 1
  fi
  return 0
}

assert_monitor_rs_still_exists() {
  if [[ ! -f "$MONITOR" ]]; then
    echo "monitor.rs missing; do not treat that as the reason to drop the preview"
    return 1
  fi
  return 0
}

assert_no_generic_unsafe_preview() {
  local workflow="$1" docs="$2"
  if grep -Fq 'deleted' <<<"$workflow$docs" && grep -Fq 'monitor.rs' <<<"$workflow$docs"; then
    echo "preview removal must not claim monitor.rs was deleted"
    return 1
  fi
  if grep -Fq 'Unsafe boundary preview' <<<"$workflow"; then
    echo "workflow still has the generic warn-only unsafe preview"
    return 1
  fi
  if grep -Fq 'unsafe allowed only in the monitor syscall boundary' <<<"$workflow$docs"; then
    echo "single-boundary Wave 3 TODO is still present"
    return 1
  fi
  if grep -Fq 'unsafe outside monitor.rs' <<<"$workflow"; then
    echo "workflow still treats paths outside monitor.rs as the deviation"
    return 1
  fi
  return 0
}

assert_wave71_not_active() {
  local text="$1"
  local row
  row="$(printf '%s\n' "$text" | grep -E '^\| Wave71 \|' || true)"
  if [[ -z "$row" ]]; then
    echo "REFACTOR-WAVE-STATUS.md has no Wave71 row"
    return 1
  fi
  if grep -Fq '| Active |' <<<"$row"; then
    echo "Wave71 row still claims Active without a current execution ledger"
    return 1
  fi
  if ! grep -Eqi 'Dormant|Incomplete' <<<"$row"; then
    echo "Wave71 row is not marked Dormant or Incomplete"
    return 1
  fi
  return 0
}

expect_ok() {
  local label="$1"
  shift
  local err
  if err="$("$@" 2>&1)"; then
    ok "$label"
  else
    bad "$label: $err"
  fi
}

expect_red() {
  local label="$1" needle="$2"
  shift 2
  local err
  if err="$("$@" 2>&1)"; then
    bad "$label left the contract green"
  elif grep -Fq -- "$needle" <<<"$err"; then
    ok "$label turns red ($err)"
  else
    bad "$label red without ${needle}: ${err}"
  fi
}

expect_ok "monitor.rs still exists" assert_monitor_rs_still_exists
expect_ok "generic unsafe preview and single-boundary TODO are gone" \
  assert_no_generic_unsafe_preview "$(<"$WORKFLOW")" "$(<"$WAVE0")"
expect_ok "AGENTS.md ledger says no programme is active" assert_agents_ledger "$(<"$AGENTS")"
expect_ok "WAVE0-GATES.md describes dynamic latest-release baseline" assert_wave0_semver_doc "$(<"$WAVE0")"
expect_ok "WAVE0-GATES.md names live required contexts" assert_wave0_required_contexts "$(<"$WAVE0")"
expect_ok "Wave71 is dormant or incomplete" assert_wave71_not_active "$(<"$STATUS")"

stale_ledger=$'The public execution ledger for the active programme is named on this line: [issue #2388](https://github.com/Rul1an/assay/issues/2388).\n'
expect_red "active-issue ledger pointer" "names a GitHub issue" assert_agents_ledger "$stale_ledger"

stale_semver=$'Source of truth: workflow env WAVE0_SEMVER_BASELINE_SHA.\n'
expect_red "pinned baseline SHA" "WAVE0_SEMVER_BASELINE_SHA" assert_wave0_semver_doc "$stale_semver"

stale_required="$(live_required_contexts)
Configure branch protection to require:
- Wave 0 feature matrix
"
expect_red "Wave 0 jobs as required checks" "branch-protection recommendation" assert_wave0_required_contexts "$stale_required"

stale_wave71=$'| Wave71 | Hotspot LOC under 600 | in progress | Active | still reducing |\n'
expect_red "Wave71 Active" "claims Active" assert_wave71_not_active "$stale_wave71"

stale_preview=$'      - name: Unsafe boundary preview (warn-only)\n        run: echo unsafe outside monitor.rs\n# unsafe allowed only in the monitor syscall boundary module.\n'
expect_red "restored generic preview" "generic warn-only unsafe preview" \
  assert_no_generic_unsafe_preview "$stale_preview" ""
expect_red "restored single-boundary TODO" "single-boundary Wave 3 TODO" \
  assert_no_generic_unsafe_preview "" "unsafe allowed only in the monitor syscall boundary module."
expect_red "deleted-monitor.rs claim" "must not claim monitor.rs was deleted" \
  assert_no_generic_unsafe_preview "monitor.rs was deleted" ""

if [[ "$FAILURES" -ne 0 ]]; then
  echo "$FAILURES programme-truth case(s) failed"
  exit 1
fi
echo "PASS: ci programme truth"
