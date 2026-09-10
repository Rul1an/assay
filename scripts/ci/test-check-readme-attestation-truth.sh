#!/usr/bin/env bash
# Prove the README attestation row is checked against the statement the source emits (#2875).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CHECK="$ROOT/scripts/ci/check-readme-attestation-truth.py"
README=README.md
SOURCE=crates/assay-evidence/src/attestation.rs

# The real files, not a hand-written fixture: the positive control has to be the tree that ships.
reset_tree() {
  rm -rf "$TMP/tree"
  mkdir -p "$TMP/tree/$(dirname "$SOURCE")"
  cp "$ROOT/$README" "$TMP/tree/$README"
  cp "$ROOT/$SOURCE" "$TMP/tree/$SOURCE"
}

run_check() {
  python3 "$CHECK" --root "$TMP/tree"
}

# Replace exactly one occurrence, so a mutation that silently matches nothing cannot pass as red.
replace_once() {
  python3 - "$TMP/tree/$1" "$2" "$3" <<'PY'
from pathlib import Path
import sys

path, old, new = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor must occur exactly once in {path}: {old!r}")
path.write_text(text.replace(old, new), encoding="utf-8")
PY
}

reset_tree
run_check >"$TMP/control.out" 2>&1 || {
  cat "$TMP/control.out" >&2
  echo "FAIL: the shipped README and source must agree" >&2
  exit 1
}
printf 'PASS: shipped README row agrees with the emitted statement\n'

mutations=0
expect_red() {
  local name="$1" diagnostic="$2"
  if run_check >"$TMP/$name.out" 2>&1; then
    echo "FAIL: mutation $name was not observed" >&2
    exit 1
  fi
  grep -Fq "$diagnostic" "$TMP/$name.out" || {
    cat "$TMP/$name.out" >&2
    echo "FAIL: mutation $name missed diagnostic: $diagnostic" >&2
    exit 1
  }
  mutations=$((mutations + 1))
  printf 'PASS: %s\n' "$name"
  reset_tree
}

row_prefix='| **Attestation** | '

# The row as it shipped on v6.0.0 and v6.1.0 (#2859): the historical defect must be red.
python3 - "$TMP/tree/$README" "$row_prefix" <<'PY'
from pathlib import Path
import sys

path, prefix = Path(sys.argv[1]), sys.argv[2]
lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
hits = [i for i, line in enumerate(lines) if line.startswith(prefix)]
if len(hits) != 1:
    raise SystemExit("fixture needs exactly one attestation row")
lines[hits[0]] = prefix + "Export a bundle as an in-toto / DSSE statement (v0), anchor-pluggable. |\n"
path.write_text("".join(lines), encoding="utf-8")
PY
expect_red historical-v6.1.0-row 'does not state "in-toto v1 Statement"'

# README side, one name at a time.
replace_once "$README" 'in-toto v1 Statement' 'in-toto v2 Statement'
expect_red readme-statement-version 'does not state "in-toto v1 Statement"'
replace_once "$README" 'evidence-bundle/v1 predicate' 'evidence-bundle/v0 predicate'
expect_red readme-predicate-version 'does not state "evidence-bundle/v1 predicate"'
replace_once "$README" 'evidence-bundle/v1 predicate.' 'evidence-bundle/v1 predicate (v0).'
expect_red readme-stray-version 'names v0'

# Source side, one constant at a time, with the README left as it ships.
replace_once "$SOURCE" '"https://in-toto.io/Statement/v1"' '"https://in-toto.io/Statement/v2"'
expect_red source-statement-constant 'does not state "in-toto v2 Statement"'
# Anchored on the declaration; a unit test further down repeats the URI as its expected value.
replace_once "$SOURCE" '"https://docs.getassay.dev/attestation/evidence-bundle/v1";' \
  '"https://docs.getassay.dev/attestation/evidence-bundle/v2";'
expect_red source-predicate-constant 'does not state "evidence-bundle/v2 predicate"'

# The emitter, not the constant's name, decides what ships: switching it is a change too.
replace_once "$SOURCE" '        predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE_V1.to_string(),' \
  '        predicate_type: EVIDENCE_BUNDLE_PREDICATE_TYPE_V0.to_string(),'
expect_red source-emitter-switch 'does not state "evidence-bundle/v0 predicate"'
# Same for the statement type. Only one statement constant exists today, so the mutation adds a
# second and points the emitter at it; a check that read `STATEMENT_TYPE` by name stays green.
python3 - "$TMP/tree/$SOURCE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
declaration = 'const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";\n'
emitter = text.index("fn statement_from_parts(")
field = "        type_: STATEMENT_TYPE.to_string(),\n"
at = text.index(field, emitter)
if text.count(declaration) != 1:
    raise SystemExit("statement constant declaration must occur exactly once")
text = text[:at] + "        type_: STATEMENT_TYPE_V2.to_string(),\n" + text[at + len(field):]
text = text.replace(declaration, declaration
                    + 'const STATEMENT_TYPE_V2: &str = "https://in-toto.io/Statement/v2";\n')
path.write_text(text, encoding="utf-8")
PY
expect_red source-statement-emitter-switch 'does not state "in-toto v2 Statement"'

# Anything the check cannot read is a failure, never a pass.
replace_once "$SOURCE" 'fn statement_from_parts(' 'fn statement_assembled_from_parts('
expect_red emitter-not-found 'statement_from_parts'
python3 - "$TMP/tree/$README" "$row_prefix" <<'PY'
from pathlib import Path
import sys

path, prefix = Path(sys.argv[1]), sys.argv[2]
path.write_text("".join(
    line for line in path.read_text(encoding="utf-8").splitlines(keepends=True)
    if not line.startswith(prefix)
), encoding="utf-8")
PY
expect_red row-missing 'exactly one README attestation row'
python3 - "$TMP/tree/$README" "$row_prefix" <<'PY'
from pathlib import Path
import sys

path, prefix = Path(sys.argv[1]), sys.argv[2]
lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
row = next(line for line in lines if line.startswith(prefix))
path.write_text("".join(lines) + row, encoding="utf-8")
PY
expect_red row-duplicated 'exactly one README attestation row'

if [ "$mutations" -ne 11 ]; then
  echo "FAIL: expected 11 observed mutations, got $mutations" >&2
  exit 1
fi

# The check pins names, not prose: a reworded row that keeps both names stays green.
python3 - "$TMP/tree/$README" "$row_prefix" <<'PY'
from pathlib import Path
import sys

path, prefix = Path(sys.argv[1]), sys.argv[2]
lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
hits = [i for i, line in enumerate(lines) if line.startswith(prefix)]
lines[hits[0]] = (prefix + "Signed evidence: an in-toto v1 Statement, DSSE-wrapped, "
                  "carrying the evidence-bundle/v1 predicate. |\n")
path.write_text("".join(lines), encoding="utf-8")
PY
run_check >"$TMP/reworded.out" 2>&1 || {
  cat "$TMP/reworded.out" >&2
  echo "FAIL: a reworded row naming the same statement and predicate was refused" >&2
  exit 1
}
printf 'PASS: reworded row with the same names stays green\n'
printf 'README attestation truth mutations: %s observed\n' "$mutations"
