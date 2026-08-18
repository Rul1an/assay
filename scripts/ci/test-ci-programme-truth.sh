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
CEILINGS="$ROOT/scripts/ci/lib/resource_ceilings.py"
export PYTHONPATH="$ROOT/scripts/ci/lib${PYTHONPATH:+:$PYTHONPATH}"
AGENTS="${PROGRAMME_TRUTH_AGENTS:-$ROOT/AGENTS.md}"
WAVE0="$ROOT/docs/contributing/WAVE0-GATES.md"
STATUS="$ROOT/docs/contributing/REFACTOR-WAVE-STATUS.md"
WORKFLOW="$ROOT/.github/workflows/split-wave0-gates.yml"
KERNEL_MATRIX="$ROOT/.github/workflows/kernel-matrix.yml"
MONITOR="$ROOT/crates/assay-cli/src/cli/commands/monitor.rs"
TRUTH_TMP="$(mktemp -d)"
trap 'rm -rf -- "${TRUTH_TMP:?}"' EXIT

FAILURES=0
ok()  { echo "ok    $1"; }
bad() { echo "FAIL  $1"; FAILURES=$((FAILURES + 1)); }

LEDGER_PREFIX='- The public execution ledger'

assert_extracted_block() {
  local kind="$1" expected="$2" path="$3"
  PROGRAMME_TRUTH_KIND="$kind" \
  PROGRAMME_TRUTH_EXPECTED="$expected" \
  PROGRAMME_TRUTH_LEDGER_PREFIX="$LEDGER_PREFIX" \
  PROGRAMME_TRUTH_DOC_PATH="$path" \
  python3 - <<'PY'
import os
import re

from resource_ceilings import read_bounded_file

INACTIVE_DECL = "**No programme is active.**"
ISSUE_LINK = re.compile(
    r"\[issue #(\d+)\]\((https://github\.com/[^)\s]+/issues/(\d+))\)"
)
VISIBLE_ISSUE = re.compile(r"(?i)issue #(\d+)")
# Outside the canonical ledger bullet we recognize only checkable
# declaration forms. This is not semantic NLP.
#
# At line or sentence start, strip: leading whitespace, repeated `>`,
# one unordered marker [-*+], then an optional strong prefix.
# Active subject (exact): "The active programme ledger is"
# Active predicates (only):
#   a) issue #N
#   b) GitHub issue #N
#   c) [issue #N](.../issues/N) with matching N
#   d) named on this line
# Inactive (exact): "No programme is active."
ACTIVE_SUBJECT = "The active programme ledger is"
INACTIVE_OUTSIDE = "No programme is active."
ISSUE_PRED = re.compile(r"issue #\d+")
GITHUB_ISSUE_PRED = re.compile(r"GitHub issue #\d+")
NAMED_PRED = "named on this line"
# List-item declarations only. Ordinary prose such as "ensure required checks
# pass" is not a required-context declaration.
REQUIRED_OUTSIDE = re.compile(
    r"^\s*-\s+\S*required-contexts?\b|"
    r"^\s*-\s+\S*required-checks?\b",
    re.IGNORECASE | re.MULTILINE,
)


def normalize(text: str) -> str:
    lines = [line.rstrip() for line in text.splitlines()]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    return "\n".join(lines)


def heading_bounds(text: str, heading: str) -> tuple[int, int]:
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if line.strip() == heading), None)
    if start is None:
        raise SystemExit(f"missing {heading} section")
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")),
        len(lines),
    )
    return start, end


def extract_heading_section(text: str, heading: str) -> str:
    start, end = heading_bounds(text, heading)
    return normalize("\n".join(text.splitlines()[start:end]))


def split_top_level_bullet(text: str, prefix: str) -> tuple[str, str]:
    lines = text.splitlines()
    starts = [i for i, line in enumerate(lines) if line.startswith(prefix)]
    if not starts:
        raise SystemExit("missing ledger bullet")
    if len(starts) != 1:
        raise SystemExit("duplicate ledger bullet")
    start = starts[0]
    end = next(
        (
            i
            for i in range(start + 1, len(lines))
            if lines[i].startswith("- ") or lines[i].startswith("## ")
        ),
        len(lines),
    )
    bullet = normalize("\n".join(lines[start:end]))
    remainder = "\n".join(lines[:start] + lines[end:])
    return bullet, remainder


def collapsed(text: str) -> str:
    return re.sub(r"\s+", " ", text)


def strip_decl_wrappers(text: str) -> str:
    s = text.lstrip()
    while s.startswith(">"):
        s = s[1:].lstrip()
    if len(s) >= 2 and s[0] in "-*+" and s[1].isspace():
        s = s[2:].lstrip()
    if s.startswith("**"):
        s = s[2:]
    return s


def declaration_candidates(remainder: str) -> list[str]:
    found: list[str] = []
    for line in remainder.splitlines():
        for piece in re.split(r"(?<=[.!?])\s+", line):
            stripped = strip_decl_wrappers(piece)
            if stripped:
                found.append(stripped)
    return found


def is_active_declaration(text: str) -> bool:
    if not text.startswith(ACTIVE_SUBJECT):
        return False
    pred = text[len(ACTIVE_SUBJECT) :].lstrip()
    if ISSUE_PRED.match(pred) or GITHUB_ISSUE_PRED.match(pred):
        return True
    link = ISSUE_LINK.match(pred)
    if link is not None and link.group(1) == link.group(3):
        return True
    return pred.startswith(NAMED_PRED)


def is_inactive_declaration(text: str) -> bool:
    return text.startswith(INACTIVE_OUTSIDE)


def outside_ledger_declarations(remainder: str) -> list[str]:
    found: list[str] = []
    for candidate in declaration_candidates(remainder):
        if is_active_declaration(candidate):
            found.append("The active programme ledger is")
        if is_inactive_declaration(candidate):
            found.append("No programme is active")
    return found


def validate_ledger(text: str, prefix: str) -> None:
    # Non-claim: this offline structural check verifies issue-link form and
    # visible-text/URL-ID parity only. It does not query live GitHub OPEN or
    # CLOSED state. The current inactive AGENTS.md bullet satisfies #2515.
    bullet, remainder = split_top_level_bullet(text, prefix)
    bullet_c = collapsed(bullet)
    outside = outside_ledger_declarations(remainder)
    if outside:
        raise SystemExit(
            "contradictory ledger declaration outside the ledger bullet: "
            + ", ".join(outside)
        )
    links = ISSUE_LINK.findall(bullet)
    for visible, _url, url_id in links:
        if visible != url_id:
            raise SystemExit("visible issue-ID != URL-ID")
    stripped = ISSUE_LINK.sub("", bullet)
    unlinked = VISIBLE_ISSUE.findall(stripped)
    if unlinked:
        raise SystemExit(
            "unlinked issue # reference in the ledger bullet: "
            + ", ".join(unlinked)
        )
    inactive_count = bullet_c.count(INACTIVE_DECL)
    if inactive_count > 1:
        raise SystemExit("ledger bullet has multiple inactive declarations")
    if inactive_count == 1:
        if "active programme ledger" in bullet_c:
            raise SystemExit("contradictory ledger claims in the ledger bullet")
        return
    if len(links) != 1:
        raise SystemExit(
            "ledger bullet is neither a valid inactive nor a valid active state"
        )
    canonical_active = (
        "- The public execution ledger for the active programme is named on this line:"
    )
    if not bullet_c.startswith(canonical_active):
        raise SystemExit("ledger bullet is not the canonical active form")
    after = bullet_c[len(canonical_active) :].strip().rstrip(".").strip()
    link = ISSUE_LINK.fullmatch(after)
    if link is None or link.group(1) != link.group(3):
        raise SystemExit("ledger bullet is not the canonical active form")


def validate_required(text: str, expected: str) -> None:
    heading = "## Required checks"
    actual = extract_heading_section(text, heading)
    if actual != normalize(expected):
        raise SystemExit("## Required checks section mismatch")
    start, end = heading_bounds(text, heading)
    remainder = "\n".join(text.splitlines()[:start] + text.splitlines()[end:])
    if REQUIRED_OUTSIDE.search(remainder):
        raise SystemExit("required-context declaration outside ## Required checks")


kind = os.environ["PROGRAMME_TRUTH_KIND"]
text = read_bounded_file(os.environ["PROGRAMME_TRUTH_DOC_PATH"]).decode("utf-8")
if kind == "ledger":
    validate_ledger(text, os.environ["PROGRAMME_TRUTH_LEDGER_PREFIX"])
else:
    validate_required(text, os.environ["PROGRAMME_TRUTH_EXPECTED"])
PY
}

# shellcheck disable=SC2016 # Markdown backticks in this exact-section fixture are literal.
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
  assert_extracted_block ledger "" "$1"
}

assert_wave0_required_contexts() {
  assert_extracted_block required "$EXPECTED_REQUIRED_CHECKS" "$1"
}

assert_wave0_semver_file() {
  local path="$1"
  python3 "$CEILINGS" check-file "$path" || return 1
  if grep -q 'WAVE0_SEMVER_BASELINE_SHA' "$path"; then
    echo "WAVE0-GATES.md still documents WAVE0_SEMVER_BASELINE_SHA"
    return 1
  fi
  if ! grep -Eqi 'newest|latest' "$path" || ! grep -Eq 'release tag' "$path"; then
    echo "WAVE0-GATES.md does not describe the dynamic latest-release baseline"
    return 1
  fi
  if ! grep -q 'test-semver-gate.sh' "$path"; then
    echo "WAVE0-GATES.md does not point at scripts/ci/test-semver-gate.sh"
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

insert_before_next_heading() {
  local heading="$1" extra="$2" src="$3" dest="$4"
  PROGRAMME_TRUTH_HEADING="$heading" PROGRAMME_TRUTH_EXTRA="$extra" \
  python3 - "$src" "$dest" <<'PY'
import os
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

heading = os.environ["PROGRAMME_TRUTH_HEADING"]
extra = os.environ["PROGRAMME_TRUTH_EXTRA"]
text = read_bounded_file(sys.argv[1]).decode("utf-8")
lines = text.splitlines(keepends=True)
start = next((i for i, line in enumerate(lines) if line.strip() == heading), None)
if start is None:
    raise SystemExit(f"missing {heading} section")
end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
lines.insert(end, extra if extra.endswith("\n") else extra + "\n")
data = "".join(lines).encode("utf-8")
require_bounded_bytes(data, "programme-truth mutation output")
Path(sys.argv[2]).write_bytes(data)
PY
}

# Insert inside the ledger bullet, immediately before the next top-level "- ".
insert_into_ledger_bullet() {
  local extra="$1" src="$2" dest="$3"
  PROGRAMME_TRUTH_MODE="${PROGRAMME_TRUTH_MODE:-insert}" \
  PROGRAMME_TRUTH_EXTRA="$extra" \
  PROGRAMME_TRUTH_LEDGER_PREFIX="$LEDGER_PREFIX" \
  python3 - "$src" "$dest" <<'PY'
import os
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

prefix = os.environ["PROGRAMME_TRUTH_LEDGER_PREFIX"]
extra = os.environ["PROGRAMME_TRUTH_EXTRA"]
text = read_bounded_file(sys.argv[1]).decode("utf-8")
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
payload = extra if extra.endswith("\n") else extra + "\n"
if os.environ["PROGRAMME_TRUTH_MODE"] == "replace":
    lines[start:end] = [payload]
else:
    lines.insert(end, payload)
data = "".join(lines).encode("utf-8")
require_bounded_bytes(data, "programme-truth mutation output")
Path(sys.argv[2]).write_bytes(data)
PY
}

replace_ledger_bullet() {
  PROGRAMME_TRUTH_MODE=replace insert_into_ledger_bullet "$1" "$2" "$3"
}

append_doc_line() {
  local extra="$1" src="$2" dest="$3"
  PROGRAMME_TRUTH_EXTRA="$extra" python3 - "$src" "$dest" <<'PY'
import os
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

data = read_bounded_file(sys.argv[1])
if data and not data.endswith(b"\n"):
    data += b"\n"
extra = os.environ["PROGRAMME_TRUTH_EXTRA"].encode("utf-8")
if not extra.endswith(b"\n"):
    extra += b"\n"
out = data + extra
require_bounded_bytes(out, "programme-truth mutation output")
Path(sys.argv[2]).write_bytes(out)
PY
}

# Docs/ruleset surfaces the hook reads that `scripts/**` does not cover.
TRUTH_TRIGGER_INPUTS="$(printf '%s\n' \
  AGENTS.md \
  docs/contributing/WAVE0-GATES.md \
  docs/contributing/REFACTOR-WAVE-STATUS.md \
  .github/rulesets/main-required-ci-contexts.json)"

assert_ci_trigger_owns_truth_inputs() {
  local path="$1"
  PROGRAMME_TRUTH_INPUTS="$TRUTH_TRIGGER_INPUTS" \
  PROGRAMME_TRUTH_DOC_PATH="$path" \
  python3 - <<'PY'
import os, re

from resource_ceilings import read_bounded_file

text = read_bounded_file(os.environ["PROGRAMME_TRUTH_DOC_PATH"]).decode("utf-8")
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

assert_no_generic_unsafe_preview_files() {
  local workflow_path="$1" docs_path="$2"
  PROGRAMME_TRUTH_WORKFLOW="$workflow_path" PROGRAMME_TRUTH_DOCS="$docs_path" python3 - <<'PY'
import os

from resource_ceilings import read_bounded_file

workflow = read_bounded_file(os.environ["PROGRAMME_TRUTH_WORKFLOW"]).decode("utf-8")
docs = read_bounded_file(os.environ["PROGRAMME_TRUTH_DOCS"]).decode("utf-8")
combined = workflow + docs
if "deleted" in combined and "monitor.rs" in combined:
    raise SystemExit("preview removal must not claim monitor.rs was deleted")
if "Unsafe boundary preview" in workflow:
    raise SystemExit("workflow still has the generic warn-only unsafe preview")
if "unsafe allowed only in the monitor syscall boundary" in combined:
    raise SystemExit("single-boundary Wave 3 TODO is still present")
if "unsafe outside monitor.rs" in workflow:
    raise SystemExit("workflow still treats paths outside monitor.rs as the deviation")
PY
}

assert_prepush_covers_ceilings() {
  local config="$1"
  PROGRAMME_TRUTH_DOC_PATH="$config" python3 - <<'PY'
import os
import re

from resource_ceilings import read_bounded_file

text = read_bounded_file(os.environ["PROGRAMME_TRUTH_DOC_PATH"]).decode("utf-8")
match = re.search(
    r"- id: ci-programme-truth\n(?:.*\n)*?        files: (.+)\n",
    text,
)
if match is None:
    raise SystemExit("ci-programme-truth files regex is missing")
raw = match.group(1).strip()
if raw[0] in "'\"" and raw[-1] == raw[0]:
    raw = raw[1:-1]
helper = "scripts/ci/lib/resource_ceilings.py"
token = r"scripts/ci/lib/resource_ceilings\.py"
if token not in raw:
    raise SystemExit("files regex does not name resource_ceilings.py")
if re.search(raw, helper) is None:
    raise SystemExit("files regex does not match resource_ceilings.py")
stripped = raw.replace(f"|{token}", "").replace(f"{token}|", "")
if re.search(stripped, helper) is not None:
    raise SystemExit("stripped files regex still matches resource_ceilings.py")
PY
}

assert_wave71_file() {
  local path="$1"
  python3 "$CEILINGS" check-file "$path" || return 1
  local row
  row="$(grep -E '^\| Wave71 \|' "$path" || true)"
  assert_wave71_not_active "$row"
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

PRECOMMIT="$ROOT/.pre-commit-config.yaml"

for truth_file in "$AGENTS" "$WAVE0" "$STATUS" "$WORKFLOW" "$KERNEL_MATRIX"; do
  if ! python3 "$CEILINGS" check-file "$truth_file"; then
    echo "FAIL: canonical programme-truth input exceeds ceiling: $truth_file" >&2
    exit 1
  fi
done

expect_ok "monitor.rs still exists" assert_monitor_rs_still_exists
expect_ok "generic unsafe preview and single-boundary TODO are gone" \
  assert_no_generic_unsafe_preview_files "$WORKFLOW" "$WAVE0"
expect_ok "AGENTS.md ledger bullet is structurally valid" assert_agents_ledger "$AGENTS"
expect_ok "WAVE0-GATES.md describes dynamic latest-release baseline" assert_wave0_semver_file "$WAVE0"
expect_ok "WAVE0-GATES.md points at the canonical required-context contract" \
  assert_wave0_required_contexts "$WAVE0"
expect_ok "Wave71 is dormant or incomplete" assert_wave71_file "$STATUS"
expect_ok "kernel-matrix.yml pull_request.paths owns programme-truth inputs" \
  assert_ci_trigger_owns_truth_inputs "$KERNEL_MATRIX"
expect_ok "pre-push files regex covers resource_ceilings.py" \
  assert_prepush_covers_ceilings "$PRECOMMIT"

stale_semver=$'Source of truth: workflow env WAVE0_SEMVER_BASELINE_SHA.\n'
expect_red "pinned baseline SHA" "WAVE0_SEMVER_BASELINE_SHA" assert_wave0_semver_doc "$stale_semver"

insert_into_ledger_bullet \
  'The active programme ledger is issue #9999.' \
  "$AGENTS" "$TRUTH_TMP/contradictory.md"
expect_red "additive unlinked issue # in the ledger bullet" "unlinked issue #" \
  assert_agents_ledger "$TRUTH_TMP/contradictory.md"

append_doc_line 'The active programme ledger is issue #9999.' \
  "$AGENTS" "$TRUTH_TMP/elsewhere.md"
expect_red "active ledger declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/elsewhere.md"

append_doc_line 'Reviewers should consult the public execution ledger before handoff.' \
  "$AGENTS" "$TRUTH_TMP/ordinary-ledger.md"
expect_ok "ordinary public-ledger prose outside the bullet" \
  assert_agents_ledger "$TRUTH_TMP/ordinary-ledger.md"

append_doc_line '- The active programme ledger is issue #7777.' \
  "$AGENTS" "$TRUTH_TMP/list-active.md"
expect_red "list-item active ledger declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/list-active.md"

append_doc_line '- **No programme is active.**' \
  "$AGENTS" "$TRUTH_TMP/list-inactive.md"
expect_red "list-item inactive declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/list-inactive.md"

append_doc_line '* The active programme ledger is issue #7777.' \
  "$AGENTS" "$TRUTH_TMP/star-active.md"
expect_red "star list-item active ledger declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/star-active.md"

append_doc_line '+ **No programme is active.**' \
  "$AGENTS" "$TRUTH_TMP/plus-inactive.md"
expect_red "plus list-item inactive declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/plus-inactive.md"

append_doc_line '- **The active programme ledger is documented in the canonical bullet above.**' \
  "$AGENTS" "$TRUTH_TMP/documented.md"
expect_ok "documented-in-bullet prose is not a ledger declaration" \
  assert_agents_ledger "$TRUTH_TMP/documented.md"

append_doc_line '- The active programme ledger is [issue #7777](https://github.com/Rul1an/assay/issues/7777).' \
  "$AGENTS" "$TRUTH_TMP/link-active.md"
expect_red "markdown-link active ledger declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/link-active.md"

append_doc_line 'The active programme ledger is GitHub issue #7777.' \
  "$AGENTS" "$TRUTH_TMP/github-active.md"
expect_red "GitHub issue active ledger declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/github-active.md"

append_doc_line '> **No programme is active.**' \
  "$AGENTS" "$TRUTH_TMP/blockquote.md"
expect_red "blockquote inactive declaration elsewhere" \
  "contradictory ledger declaration outside the ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/blockquote.md"

append_doc_line '- The active programme ledger is named after the team that maintains it.' \
  "$AGENTS" "$TRUTH_TMP/named-after.md"
expect_ok "named-after prose is not a ledger declaration" \
  assert_agents_ledger "$TRUTH_TMP/named-after.md"

ACTIVE_LEDGER_BULLET='- The public execution ledger for the active programme is named on this line: [issue #4242](https://github.com/Rul1an/assay/issues/4242).'
replace_ledger_bullet "$ACTIVE_LEDGER_BULLET" "$AGENTS" "$TRUTH_TMP/active.md"
expect_ok "structurally active ledger fixture" assert_agents_ledger "$TRUTH_TMP/active.md"

replace_ledger_bullet \
  '- The public execution ledger historically recorded [issue #4242](https://github.com/Rul1an/assay/issues/4242).' \
  "$AGENTS" "$TRUTH_TMP/retired.md"
expect_red "retired historical wording with a valid issue-link" "canonical active form" \
  assert_agents_ledger "$TRUTH_TMP/retired.md"

replace_ledger_bullet \
  '- The public execution ledger for the active programme is named on this line: [issue #4242](https://github.com/Rul1an/assay/issues/4243).' \
  "$AGENTS" "$TRUTH_TMP/mismatch.md"
expect_red "active fixture with text/link issue mismatch" "visible issue-ID != URL-ID" \
  assert_agents_ledger "$TRUTH_TMP/mismatch.md"

# Synthetic inactive: previous-link mismatch without freezing a live issue number.
replace_ledger_bullet \
  '- The public execution ledger for the active programme is named on this line. **No programme is active.** The previous one, [issue #4242](https://github.com/Rul1an/assay/issues/9999).' \
  "$AGENTS" "$TRUTH_TMP/inactive-mismatch.md"
expect_red "inactive previous-link text/URL mismatch" "visible issue-ID != URL-ID" \
  assert_agents_ledger "$TRUTH_TMP/inactive-mismatch.md"

replace_ledger_bullet \
  '- The public execution ledger for the active programme is named on this line: [issue #7777](https://github.com/Rul1an/assay/issues/7777). Also issue #7777.' \
  "$AGENTS" "$TRUTH_TMP/linked-plus-plain.md"
expect_red "valid link plus extra plain issue # of the same ID" "unlinked issue #" \
  assert_agents_ledger "$TRUTH_TMP/linked-plus-plain.md"

insert_into_ledger_bullet "$ACTIVE_LEDGER_BULLET" "$AGENTS" "$TRUTH_TMP/duplicate.md"
expect_red "duplicate ledger bullet" "duplicate ledger bullet" \
  assert_agents_ledger "$TRUTH_TMP/duplicate.md"

insert_before_next_heading "## Required checks" \
  'Configure branch protection to require:' \
  "$WAVE0" "$TRUTH_TMP/stale-required.md"
expect_red "Wave 0 jobs as required checks" "## Required checks section mismatch" \
  assert_wave0_required_contexts "$TRUTH_TMP/stale-required.md"

insert_before_next_heading "## Required checks" \
  'stale-required-context' \
  "$WAVE0" "$TRUTH_TMP/extra-context.md"
expect_red "additive extra required context" "## Required checks section mismatch" \
  assert_wave0_required_contexts "$TRUTH_TMP/extra-context.md"

append_doc_line '- stale-required-context' \
  "$WAVE0" "$TRUTH_TMP/elsewhere-required.md"
expect_red "pointer-only required section plus stale-required-context elsewhere" \
  "required-context declaration outside ## Required checks" \
  assert_wave0_required_contexts "$TRUTH_TMP/elsewhere-required.md"

append_doc_line 'Before merging, ensure required checks pass.' \
  "$WAVE0" "$TRUTH_TMP/ordinary-required.md"
expect_ok "ordinary required-checks prose outside the section" \
  assert_wave0_required_contexts "$TRUTH_TMP/ordinary-required.md"

python3 - "$KERNEL_MATRIX" "$TRUTH_TMP/narrowed.yml" <<'PY'
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file, require_bounded_bytes

text = read_bounded_file(sys.argv[1]).decode("utf-8")
out = "".join(line for line in text.splitlines(keepends=True) if '"AGENTS.md"' not in line)
require_bounded_bytes(out.encode("utf-8"), "narrowed kernel-matrix")
Path(sys.argv[2]).write_text(out, encoding="utf-8")
PY
expect_red "AGENTS.md dropped from CI trigger" "omits AGENTS.md" \
  assert_ci_trigger_owns_truth_inputs "$TRUTH_TMP/narrowed.yml"

stale_wave71=$'| Wave71 | Hotspot LOC under 600 | in progress | Active | still reducing |\n'
expect_red "Wave71 Active" "claims Active" assert_wave71_not_active "$stale_wave71"

stale_preview=$'      - name: Unsafe boundary preview (warn-only)\n        run: echo unsafe outside monitor.rs\n# unsafe allowed only in the monitor syscall boundary module.\n'
expect_red "restored generic preview" "generic warn-only unsafe preview" \
  assert_no_generic_unsafe_preview "$stale_preview" ""
expect_red "restored single-boundary TODO" "single-boundary Wave 3 TODO" \
  assert_no_generic_unsafe_preview "" "unsafe allowed only in the monitor syscall boundary module."
expect_red "deleted-monitor.rs claim" "must not claim monitor.rs was deleted" \
  assert_no_generic_unsafe_preview "monitor.rs was deleted" ""

python3 - "$AGENTS" "$TRUTH_TMP/nul-agents.md" <<'PY'
import sys
from pathlib import Path

from resource_ceilings import read_bounded_file

Path(sys.argv[2]).write_bytes(read_bounded_file(sys.argv[1]) + b"\x00")
PY
assert_nul_preserved() {
  python3 - "$1" <<'PY'
import sys

from resource_ceilings import read_bounded_file

data = read_bounded_file(sys.argv[1])
if b"\x00" not in data:
    raise SystemExit("bounded reader dropped appended NUL")
if data.decode("utf-8").count("\x00") != 1:
    raise SystemExit("bounded reader did not keep exactly one appended NUL")
PY
}

expect_ok "appended NUL is preserved by the bounded reader" \
  assert_nul_preserved "$TRUTH_TMP/nul-agents.md"
expect_ok "appended NUL still validates the ledger" assert_agents_ledger "$TRUTH_TMP/nul-agents.md"

expect_red "non-regular programme-truth input" "not a regular file" \
  python3 "$CEILINGS" check-file /dev/null

python3 -c "import sys; sys.stdout.buffer.write(b'x' * 65537)" >"$TRUTH_TMP/oversized.md"
expect_red "oversized real document" "exceeds 65536-byte ceiling" \
  python3 "$CEILINGS" check-file "$TRUTH_TMP/oversized.md"
expect_red "oversized document never reaches a ledger consumer" "exceeds 65536-byte ceiling" \
  assert_agents_ledger "$TRUTH_TMP/oversized.md"

if [[ -z "${PROGRAMME_TRUTH_SELFHOST:-}" && -z "${PROGRAMME_TRUTH_CEILING_CHILD:-}" ]]; then
  active_agents="$TRUTH_TMP/selfhost-agents.md"
  selfhost_log="$TRUTH_TMP/selfhost.log"
  replace_ledger_bullet "$ACTIVE_LEDGER_BULLET" "$ROOT/AGENTS.md" "$active_agents"
  if PROGRAMME_TRUTH_AGENTS="$active_agents" PROGRAMME_TRUTH_SELFHOST=1 \
    bash "${BASH_SOURCE[0]}" >"$selfhost_log" 2>&1; then
    ok "full contract on active canonical AGENTS ledger"
  else
    bad "full contract on active canonical AGENTS ledger: $(tail -n 20 "$selfhost_log")"
  fi

  if child_out="$(
    PROGRAMME_TRUTH_AGENTS="$TRUTH_TMP/oversized.md" \
    PROGRAMME_TRUTH_CEILING_CHILD=1 \
    bash "${BASH_SOURCE[0]}" 2>&1
  )"; then
    bad "oversized AGENTS left the suite green"
  elif grep -Fq 'monitor.rs still exists' <<<"$child_out"; then
    bad "consumers ran after ceiling failure"
  elif grep -Fq 'exceeds 65536-byte ceiling' <<<"$child_out"; then
    ok "oversized AGENTS stops before consumers"
  else
    bad "oversized AGENTS red without ceiling: $child_out"
  fi
fi

if [[ "$FAILURES" -ne 0 ]]; then
  echo "$FAILURES programme-truth case(s) failed"
  exit 1
fi
echo "PASS: ci programme truth"
