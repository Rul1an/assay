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
WORKFLOW="$ROOT/.github/workflows/split-wave0-gates.yml"
KERNEL_MATRIX="$ROOT/.github/workflows/kernel-matrix.yml"
MONITOR="$ROOT/crates/assay-cli/src/cli/commands/monitor.rs"

FAILURES=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

LEDGER_PREFIX='- The public execution ledger'

assert_extracted_block() {
  local kind="$1" expected="$2" text="$3"
  PROGRAMME_TRUTH_KIND="$kind" \
  PROGRAMME_TRUTH_EXPECTED="$expected" \
  PROGRAMME_TRUTH_DOC="$text" \
  PROGRAMME_TRUTH_LEDGER_PREFIX="$LEDGER_PREFIX" python3 - <<'PY'
import os

def normalize(text: str) -> str:
    lines = [line.rstrip() for line in text.splitlines()]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    return "\n".join(lines)

def extract_heading_section(text: str, heading: str) -> str:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if line.strip() == heading), None)
    if start is None:
        raise SystemExit(f"missing {heading} section")
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")),
        len(lines),
    )
    return normalize("\n".join(lines[start:end]))

def extract_top_level_bullet(text: str, prefix: str) -> str:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if line.startswith(prefix)), None)
    if start is None:
        raise SystemExit("missing ledger bullet")
    end = next(
        (
            i
            for i in range(start + 1, len(lines))
            if lines[i].startswith("- ") or lines[i].startswith("## ")
        ),
        len(lines),
    )
    return normalize("\n".join(lines[start:end]))

kind = os.environ["PROGRAMME_TRUTH_KIND"]
text = os.environ["PROGRAMME_TRUTH_DOC"]
if kind == "ledger":
    actual = extract_top_level_bullet(text, os.environ["PROGRAMME_TRUTH_LEDGER_PREFIX"])
    label = "ledger bullet mismatch"
else:
    actual = extract_heading_section(text, "## Required checks")
    label = "## Required checks section mismatch"
if actual != normalize(os.environ["PROGRAMME_TRUTH_EXPECTED"]):
    raise SystemExit(label)
PY
}

EXPECTED_LEDGER_BULLET='- The public execution ledger for the active programme is named on this line. **No programme is
  active.** The previous one, [issue #2388](https://github.com/Rul1an/assay/issues/2388), closed on
  2026-08-15; name the new ledger here when one opens, and say so plainly here when none is. Keep
  the number to this one line; everywhere else the contract names the role, so the next programme
  costs one edit here rather than one in every section. Nothing enforces that, so it is an
  instruction and not a guarantee — and the way it fails is quiet: the line kept pointing at a
  finished programme, which reads as an active ledger and sends handoffs to a closed issue.'

EXPECTED_REQUIRED_CHECKS='## Required checks

The live required contexts are named once in `CI-CONTRACT.md` at
`Currently required live branch-protection contexts`, and
`scripts/ci/check-required-contexts.py` pins that list to
`.github/rulesets/main-required-ci-contexts.json`. Do not copy the names here.

Wave 0 job names (`Wave 0 feature matrix`, `Wave 0 quality gates`,
`Wave 0 semver checks (public crates)`) are workflow jobs, not current
required contexts.

Wave 0 workflow always triggers on `pull_request`; heavy jobs are conditional to avoid docs-only blocking.'

assert_agents_ledger() {
  assert_extracted_block ledger "$EXPECTED_LEDGER_BULLET" "$1"
}

assert_wave0_required_contexts() {
  assert_extracted_block required "$EXPECTED_REQUIRED_CHECKS" "$1"
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

insert_before_next_heading() {
  local heading="$1" extra="$2" text="$3"
  PROGRAMME_TRUTH_HEADING="$heading" PROGRAMME_TRUTH_EXTRA="$extra" PROGRAMME_TRUTH_DOC="$text" python3 - <<'PY'
import os

heading = os.environ["PROGRAMME_TRUTH_HEADING"]
extra = os.environ["PROGRAMME_TRUTH_EXTRA"]
text = os.environ["PROGRAMME_TRUTH_DOC"]
lines = text.splitlines(keepends=True)
start = next((i for i, line in enumerate(lines) if line.strip() == heading), None)
if start is None:
    raise SystemExit(f"missing {heading} section")
end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
lines.insert(end, extra if extra.endswith("\n") else extra + "\n")
print("".join(lines), end="")
PY
}

# Insert inside the ledger bullet, immediately before the next top-level "- ".
insert_into_ledger_bullet() {
  local extra="$1" text="$2"
  PROGRAMME_TRUTH_EXTRA="$extra" PROGRAMME_TRUTH_DOC="$text" \
  PROGRAMME_TRUTH_LEDGER_PREFIX="$LEDGER_PREFIX" python3 - <<'PY'
import os

prefix = os.environ["PROGRAMME_TRUTH_LEDGER_PREFIX"]
extra = os.environ["PROGRAMME_TRUTH_EXTRA"]
text = os.environ["PROGRAMME_TRUTH_DOC"]
lines = text.splitlines(keepends=True)
start = next((i for i, line in enumerate(lines) if line.startswith(prefix)), None)
if start is None:
    raise SystemExit("missing ledger bullet")
end = next(
    (
        i
        for i in range(start + 1, len(lines))
        if lines[i].startswith("- ") or lines[i].startswith("## ")
    ),
    len(lines),
)
lines.insert(end, extra if extra.endswith("\n") else extra + "\n")
print("".join(lines), end="")
PY
}

# Docs/ruleset surfaces the hook reads that `scripts/**` does not cover.
TRUTH_TRIGGER_INPUTS="$(printf '%s\n' \
  AGENTS.md \
  docs/contributing/WAVE0-GATES.md \
  docs/contributing/REFACTOR-WAVE-STATUS.md \
  .github/rulesets/main-required-ci-contexts.json)"

assert_ci_trigger_owns_truth_inputs() {
  local workflow_text="$1"
  PROGRAMME_TRUTH_WORKFLOW="$workflow_text" \
  PROGRAMME_TRUTH_INPUTS="$TRUTH_TRIGGER_INPUTS" python3 - <<'PY'
import os, re

text = os.environ["PROGRAMME_TRUTH_WORKFLOW"]
inputs = [line for line in os.environ["PROGRAMME_TRUTH_INPUTS"].splitlines() if line]
block = re.search(r"(?ms)^  pull_request:\n((?:    .+\n|\n)*)", text)
if not block:
    raise SystemExit("kernel-matrix.yml has no pull_request trigger block")
listed = re.findall(r'^\s*-\s*"([^"]+)"', block.group(1), re.M)
missing = [path for path in inputs if path not in listed]
if missing:
    raise SystemExit(
        "kernel-matrix.yml pull_request.paths omits "
        + ", ".join(missing)
        + "; docs-only truth drift would skip CI"
    )
PY
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
expect_ok "WAVE0-GATES.md points at the canonical required-context contract" \
  assert_wave0_required_contexts "$(<"$WAVE0")"
expect_ok "Wave71 is dormant or incomplete" assert_wave71_not_active "$(<"$STATUS")"
expect_ok "kernel-matrix.yml pull_request.paths owns programme-truth inputs" \
  assert_ci_trigger_owns_truth_inputs "$(<"$KERNEL_MATRIX")"

stale_ledger="$(insert_into_ledger_bullet \
  '  named on this line: [issue #2388](https://example.invalid).' \
  "$(<"$AGENTS")")"
expect_red "active-issue ledger pointer" "ledger bullet mismatch" \
  assert_agents_ledger "$stale_ledger"

stale_semver=$'Source of truth: workflow env WAVE0_SEMVER_BASELINE_SHA.\n'
expect_red "pinned baseline SHA" "WAVE0_SEMVER_BASELINE_SHA" assert_wave0_semver_doc "$stale_semver"

contradictory_ledger="$(insert_into_ledger_bullet \
  'The active programme ledger is issue #9999.' \
  "$(<"$AGENTS")")"
expect_red "additive active-issue ledger" "ledger bullet mismatch" \
  assert_agents_ledger "$contradictory_ledger"

stale_required="$(insert_before_next_heading "## Required checks" \
  'Configure branch protection to require:' \
  "$(<"$WAVE0")")"
expect_red "Wave 0 jobs as required checks" "## Required checks section mismatch" \
  assert_wave0_required_contexts "$stale_required"

extra_context="$(insert_before_next_heading "## Required checks" \
  'stale-required-context' \
  "$(<"$WAVE0")")"
expect_red "additive extra required context" "## Required checks section mismatch" \
  assert_wave0_required_contexts "$extra_context"

narrowed_trigger="$(printf '%s\n' "$(<"$KERNEL_MATRIX")" | grep -v '"AGENTS.md"')"
expect_red "AGENTS.md dropped from CI trigger" "omits AGENTS.md" \
  assert_ci_trigger_owns_truth_inputs "$narrowed_trigger"

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
