#!/usr/bin/env bash
# Discriminating control for assay describe (#2178).
# The mutant is a parent listing that drops a shipping identity still present
# in code. Same fixture, old guard then new guard; both exit codes are recorded.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/cli_describe_contract.py"
BINDINGS="${ROOT}/crates/assay-cli/src/cli/commands/describe/bindings.rs"
DOCTOR_SRC="${ROOT}/crates/assay-cli/src/diagnostics/report.rs"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$CHECKER" ]] || fail "checker missing: $CHECKER"
[[ -f "$BINDINGS" ]] || fail "bindings missing: $BINDINGS"

DOCTOR_SCHEMA="$(
  python3 - "$DOCTOR_SRC" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r'const DOCTOR_REPORT_SCHEMA: &str = "([^"]+)"', text)
if not match:
    raise SystemExit("DOCTOR_REPORT_SCHEMA is not a shipping &str constant")
print(match.group(1))
PY
)" || fail "could not read DOCTOR_REPORT_SCHEMA from its defining file"

grep -q 'identity: DOCTOR_REPORT_SCHEMA' "$BINDINGS" \
  || fail "bindings must reference DOCTOR_REPORT_SCHEMA, not a copied literal"

LISTING="${SCRATCH}/doctor.json"
python3 - "$LISTING" "$DOCTOR_SCHEMA" <<'PY'
import json, sys
path, identity = sys.argv[1], sys.argv[2]
json.dump(
    {
        "schema": "assay.cli.describe.v0",
        "path": ["doctor"],
        "commands": [],
        "identities": [identity],
    },
    open(path, "w", encoding="utf-8"),
)
PY

MUTANT="${SCRATCH}/doctor-omit-shipping-identity.json"
python3 - "$CHECKER" "$LISTING" "$MUTANT" "$DOCTOR_SCHEMA" <<'PY' || fail "could not seed the shipping-identity omission from the doctor constant"
import importlib.util, json, pathlib, sys
spec = importlib.util.spec_from_file_location("cli_describe_contract", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
listing = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
mutated = mod.seed_identity_omission(listing, sys.argv[4])
pathlib.Path(sys.argv[3]).write_text(json.dumps(mutated), encoding="utf-8")
PY

old_exit=0
python3 "$CHECKER" --repo "$ROOT" --listing "$MUTANT" --guard old >/dev/null 2>"$SCRATCH/old.err" \
  || old_exit=$?
[[ "$old_exit" -eq 0 ]] || fail "old guard rejected the identity-omission mutant (exit $old_exit): $(cat "$SCRATCH/old.err")"

new_exit=0
python3 "$CHECKER" --repo "$ROOT" --listing "$MUTANT" --guard new >/dev/null 2>"$SCRATCH/new.err" \
  || new_exit=$?
[[ "$new_exit" -ne 0 ]] || fail "new guard stayed green on the identity-omission mutant"
grep -F "parent listing omitted shipping identity ${DOCTOR_SCHEMA}" "$SCRATCH/new.err" >/dev/null \
  || fail "new guard missed the shipping doctor identity: $(cat "$SCRATCH/new.err")"

echo "identity omission old_guard_exit=${old_exit} new_guard_exit=${new_exit}"
echo "ok: cli describe contract"
