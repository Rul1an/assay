#!/usr/bin/env bash
# Workflow wiring and behavioral/mutation tests for the scheduled OSV binder.
# Runtime fixtures exercise alias-group, package and manifest coverage through
# the production CLI. Disabling each enforcement predicate must let its named
# negative escape; the workflow must keep calling the single runtime owner.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-scanner-scheduled-contract.py"
BIND="scripts/ci/bind-osv-json-sarif-counts.py"
WORKFLOW=".github/workflows/osv-scanner-scheduled.yml"
ACTIVE_INVOKE="python3 scripts/ci/bind-osv-json-sarif-counts.py"
ENV_WIRING='OSV_OUTCOME: ${{ steps.osv-scan.outcome }}'

[[ -f "${ROOT}/${CHECKER}" ]] || { echo "FAIL: checker missing" >&2; exit 1; }
[[ -f "${ROOT}/${BIND}" ]] || { echo "FAIL: bind script missing" >&2; exit 1; }
[[ -f "${ROOT}/${WORKFLOW}" ]] || { echo "FAIL: workflow missing" >&2; exit 1; }

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local case_root="$1"
  mkdir -p "$case_root/.github/workflows" "$case_root/scripts/ci"
  cp "${ROOT}/${CHECKER}" "$case_root/${CHECKER}"
  cp "${ROOT}/${WORKFLOW}" "$case_root/${WORKFLOW}"
  cp "${ROOT}/${BIND}" "$case_root/${BIND}"
}

run_checker() {
  local name="$1" case_root="$2" expected="$3"
  local status=0
  ( cd "$case_root" && python3 "$CHECKER" ) >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

# --- checker cases ---

c="$scratch/control"
seed "$c"
run_checker "control-is-green" "$c" 0

c="$scratch/sha-drift"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import re
import sys

p = Path(sys.argv[1])
text = p.read_text()
text, n = re.subn(
    r"(osv-reporter-action@)[0-9a-f]{40}",
    r"\g<1>9a498708959aeaef5ef730655706c5a1df1edbc2",
    text,
    count=1,
)
if n != 1:
    raise SystemExit("failed to mutate reporter SHA")
p.write_text(text)
PY
run_checker "sha-drift-is-refused" "$c" 1

c="$scratch/invoke-bypass"
seed "$c"
python3 - "$c/${WORKFLOW}" "$ACTIVE_INVOKE" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
old = sys.argv[2]
text = p.read_text()
if old not in text:
    raise SystemExit("active invocation missing from seed; cannot mutate")
p.write_text(text.replace(old, "echo 'bypassed'", 1))
PY
run_checker "invoke-bypass-is-refused" "$c" 1

c="$scratch/env-unwired"
seed "$c"
python3 - "$c/${WORKFLOW}" "$ENV_WIRING" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
old = sys.argv[2]
text = p.read_text()
if old not in text:
    raise SystemExit("env wiring missing from seed; cannot mutate")
p.write_text(text.replace(old, "OSV_SCAN_RESULT: ${{ steps.osv-scan.outcome }}", 1))
PY
run_checker "env-wiring-mutation-is-refused" "$c" 1

c="$scratch/shared-teller"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
text = p.read_text()
walk = (
    "          def walk(value):\n"
    "              if isinstance(value, dict):\n"
    "                  for key, child in value.items():\n"
    '                      if key == "vulnerabilities" and isinstance(child, list):\n'
    "                          pass\n"
)
if 'key == "vulnerabilities"' in text:
    text = text.replace('key == "vulnerabilities"', 'key == "packages"', 1)
p.write_text(text + "\n" + walk)
PY
run_checker "shared-teller-walk-is-refused" "$c" 1

c="$scratch/leftover-validate"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
text = p.read_text()
needle = "      - name: Build advisory SARIF\n"
step = (
    "      - name: Validate OSV scanner result\n"
    "        shell: bash\n"
    "        run: |\n"
    "          set -euo pipefail\n"
    "\n"
    "          if [ ! -s osv-results.json ]; then\n"
    "            echo \"::error::OSV scanner did not produce a JSON result\"\n"
    "            exit 1\n"
    "          fi\n"
    "\n"
)
if needle not in text:
    raise SystemExit("Build advisory SARIF step missing from seed; cannot mutate")
if "Validate OSV scanner result" in text:
    raise SystemExit("Validate step already present; mutation would be a no-op")
p.write_text(text.replace(needle, step + needle, 1))
PY
run_checker "leftover-validate-step-is-refused" "$c" 1

for mutation in retention-condition retention-json retention-sarif; do
  c="$scratch/$mutation"
  seed "$c"
  python3 - "$c/${WORKFLOW}" "$mutation" <<'PYMUT'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
old = {"retention-condition": "always() && steps.osv-targets.outcome == 'success'",
       "retention-json": "            osv-results.json\n",
       "retention-sarif": "            osv-results.sarif\n"}[sys.argv[2]]
assert s.count(old) == 1
p.write_text(s.replace(old, "false" if sys.argv[2] == "retention-condition" else "", 1))
PYMUT
  run_checker "$mutation-is-refused" "$c" 1
done

# Behavioral fixtures execute the production CLI, including its outcome gate.
python3 - "${ROOT}/${BIND}" "$scratch" <<'PYTEST'
import copy
import json
import os
from pathlib import Path
import subprocess
import sys

binder = Path(sys.argv[1])
scratch = Path(sys.argv[2])
failures = []

def check(name, scanner, reporter, want, outcome="success", script=binder):
    directory = scratch / name
    directory.mkdir()
    for filename, data in (("osv-results.json", scanner), ("osv-results.sarif", reporter)):
        if data is not None:
            (directory / filename).write_text(data if isinstance(data, str) else json.dumps(data))
    env = dict(os.environ)
    env.pop("OSV_OUTCOME", None)
    if outcome is not None:
        env["OSV_OUTCOME"] = outcome
    run = subprocess.run([sys.executable, str(script)], cwd=directory, env=env,
                         capture_output=True, text=True)
    if run.returncode != want:
        failures.append(name)
        print(f"FAIL {name}: exit {run.returncode}, wanted {want}\n{run.stdout}{run.stderr}")
    else:
        print(f"ok    {name} (exit {want})")

def package(name, ids):
    return {"package": {"name": name, "version": "1.0", "ecosystem": "PyPI"},
            "vulnerabilities": [{"id": item, "aliases": ["CVE-2026-1234"]} for item in ids],
            "groups": [{"ids": ids, "aliases": ids + ["CVE-2026-1234"]}]}

def result(name, rule="CVE-2026-1234", path="requirements.txt"):
    return {"ruleId": rule,
            "message": {"text": f"Package '{name}@1.0' is vulnerable to '{rule}'."},
            "locations": [{"physicalLocation": {"artifactLocation": {"uri": path}}}]}

def sarif(rows):
    return {"runs": [{"results": rows}]}

scanner = {"results": [{"source": {"path": "/github/workspace/requirements.txt", "type": "lockfile"},
                       "packages": [package("alpha", ["GHSA-shared", "PYSEC-alpha"]),
                                    package("beta", ["GHSA-shared", "PYSEC-beta"])]}]}
reporter = sarif([result("alpha"), result("beta")] * 3)
check("alias-overlap-4-json-6-sarif", scanner, reporter, 0)
# Equal counts do not prove association: both bad inputs used to exit zero.
equal = sarif([result("alpha"), result("beta")] * 2)
check("equal-count-control", scanner, equal, 0)
wrong = copy.deepcopy(equal)
wrong["runs"][0]["results"][0]["ruleId"] = "UNRELATED"
wrong["runs"][0]["results"][0]["message"]["text"] = "Package 'alpha@1.0' is vulnerable to 'UNRELATED'."
check("wrong-advisory-equal-count", scanner, wrong, 1)
missing = sarif([result("alpha")] * 4)
check("missing-package-padded-by-duplicate", scanner, missing, 1)
wrong_path = sarif([result("alpha"), result("beta", path="other.txt")] * 2)
check("wrong-manifest-equal-count", scanner, wrong_path, 1)
check("findings-with-empty-sarif", scanner, sarif([]), 1)
check("non-success-with-findings", scanner, reporter, 0, outcome="failure")
check("upstream-empty-source-success-without-json", None, sarif([]), 1)
check("clean-scan", {"results": []}, sarif([]), 0)
check("execution-failure", {"results": []}, sarif([]), 1, outcome="failure")
check("missing-outcome", scanner, reporter, 1, outcome=None)
for name, j, r in (("missing-json", None, reporter), ("empty-json", "", reporter),
                   ("malformed-json", "{broken", reporter), ("missing-sarif", scanner, None),
                   ("empty-sarif", scanner, ""), ("malformed-sarif", scanner, "{broken"),
                   ("missing-runs", scanner, {}), ("missing-scanner-results", {}, sarif([]))):
    check(name, j, r, 1)

# Metadata nested inside an advisory is not an additional finding.
metadata = copy.deepcopy(scanner)
metadata["results"][0]["packages"][0]["vulnerabilities"][0]["database_specific"] = {"vulnerabilities": [1, 2]}
check("advisory-metadata-is-not-a-finding", metadata, reporter, 0)
changed_copy = json.loads(json.dumps(reporter))
changed_copy["runs"][0]["results"][2]["level"] = "error"
check("non-identical-duplicate", scanner, changed_copy, 1)
bad_message = copy.deepcopy(equal)
bad_message["runs"][0]["results"][0]["message"]["text"] = "Unrecognized package layout"
check("unrecognized-package-message", scanner, bad_message, 1)
for value in (None, {}, "wrong"):
    malformed = copy.deepcopy(scanner)
    malformed["results"][0]["packages"] = value
    check("invalid-packages-" + type(value).__name__, malformed, sarif([]), 1)

# An alias of another package must not be borrowed to justify this finding.
scoped = copy.deepcopy(scanner)
alpha = scoped["results"][0]["packages"][0]
beta = scoped["results"][0]["packages"][1]
alpha["groups"] = [{"ids": ["ALPHA"], "aliases": ["CVE-ALPHA"]}]
alpha["vulnerabilities"] = [{"id": "ALPHA"}]
beta["groups"] = [{"ids": ["BETA"], "aliases": ["CVE-BETA"]}]
beta["vulnerabilities"] = [{"id": "BETA"}]
check("package-scoped-alias-control", scoped,
      sarif([result("alpha", "CVE-ALPHA"), result("beta", "CVE-BETA")]), 0)
check("alias-borrowed-from-another-package", scoped,
      sarif([result("alpha", "CVE-BETA"), result("beta", "CVE-BETA")]), 1)

# Named mutations retain all parsing and execute the same negative fixtures.
def mutate(name, old):
    source = binder.read_text()
    if source.count(old) != 1:
        raise SystemExit(f"mutation target not unique: {old}")
    target = scratch / name
    target.write_text(source.replace(old, "if False:", 1))
    return target

set_mutant = mutate("set-mutant.py", "if expected != actual:")
for name, artifact in (("wrong-id", wrong), ("missing-package", missing),
                       ("wrong-path", wrong_path), ("empty-report", sarif([]))):
    check("set-mutation-escapes-" + name, scanner, artifact, 0, script=set_mutant)
outcome_mutant = mutate("outcome-mutant.py", 'if outcome != "success" and json_count == 0:')
check("outcome-mutation-escapes-crash", {"results": []}, sarif([]), 0,
      outcome="failure", script=outcome_mutant)

if failures:
    raise SystemExit(f"FAIL: {len(failures)} behavioral cases: {', '.join(failures)}")
PYTEST
