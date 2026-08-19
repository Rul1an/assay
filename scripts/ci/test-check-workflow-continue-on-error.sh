#!/usr/bin/env bash
# Mutation battery for check-workflow-continue-on-error.py.
#
# The checker is a policy, so the only thing worth testing is what it REFUSES. Each case below is a
# mutation that must bite; a case that passes here means the policy has a hole of that shape.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-workflow-continue-on-error.py"
[[ -f "${ROOT}/${CHECKER}" ]] || { echo "FAIL: checker missing" >&2; exit 1; }

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# A tree the checker can run in: the real checker, a minimal workflow set.
seed() {
  local case_root="$1"
  mkdir -p "$case_root/.github/workflows" "$case_root/scripts/ci"
  # The sandbox carries one synthetic workflow, so the real allowlists would all read as stale --
  # correctly, since none of those workflows are here. Blanking them keeps each case about the one
  # mutation it seeds, and case 6 puts an entry back to exercise the stale rule itself.
  python3 - "${ROOT}/${CHECKER}" "$case_root/scripts/ci/$(basename "${CHECKER}")" <<'PY'
import pathlib, re, sys
src, dst = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
t = src.read_text()
t = re.sub(r"ALLOWED_JOB_LEVEL: dict\[str, str\] = \{.*?\n\}",
           "ALLOWED_JOB_LEVEL: dict[str, str] = {}", t, count=1, flags=re.S)
t = re.sub(r"ALLOWED_STEP_LEVEL: dict\[tuple\[str, str\], str\] = \{.*?\n\}",
           "ALLOWED_STEP_LEVEL: dict[tuple[str, str], str] = {}", t, count=1, flags=re.S)
dst.write_text(t)
PY
  cat > "$case_root/.github/workflows/clean.yml" <<'YAML'
name: Clean
on: [push]
jobs:
  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - name: Compile
        run: cargo build
YAML
}

run_case() {
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

# 0. The clean tree passes, or every case below proves nothing.
c="$scratch/clean"; seed "$c"
run_case "a-clean-tree-passes" "$c" 0

# 1. Job-level on an un-allowlisted workflow.
c="$scratch/job-level"; seed "$c"
cat >> "$c/.github/workflows/clean.yml" <<'YAML'
  advisory:
    name: Advisory
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - name: Thing
        run: true
YAML
run_case "job-level-is-refused" "$c" 1

# 2. Step-level not on the allowlist.
c="$scratch/step-level"; seed "$c"
cat >> "$c/.github/workflows/clean.yml" <<'YAML'
      - name: Upload something
        run: true
        continue-on-error: true
YAML
run_case "unlisted-step-is-refused" "$c" 1

# 3. An alternate YAML boolean. GitHub accepts spellings this parser does not read, so it must
#    refuse rather than silently treat `yes` as absent and pass.
c="$scratch/alt-bool"; seed "$c"
cat >> "$c/.github/workflows/clean.yml" <<'YAML'
      - name: Sneaky
        run: true
        continue-on-error: yes
YAML
run_case "alternate-boolean-is-refused-not-ignored" "$c" 2

# 4. A commented decoy sets nothing and must not be reported.
c="$scratch/decoy"; seed "$c"
cat >> "$c/.github/workflows/clean.yml" <<'YAML'
      - name: Honest
        run: true
        # continue-on-error: true
YAML
run_case "commented-decoy-is-not-a-violation" "$c" 0

# 5. A brand-new workflow cannot bring its own exemption.
c="$scratch/new-workflow"; seed "$c"
cat > "$c/.github/workflows/rogue.yml" <<'YAML'
name: Rogue
on: [push]
jobs:
  rogue:
    name: Rogue
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - name: Thing
        run: true
YAML
run_case "unauthorized-new-workflow-is-refused" "$c" 1

# 6. A stale allowlist entry is a permission for something that no longer exists.
c="$scratch/stale"; seed "$c"
python3 - "$c/scripts/ci/$(basename "$CHECKER")" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); t = p.read_text()
t = t.replace('ALLOWED_JOB_LEVEL: dict[str, str] = {}',
              'ALLOWED_JOB_LEVEL: dict[str, str] = {"gone.yml": "workflow deleted long ago"}', 1)
p.write_text(t)
PY
run_case "stale-allowlist-entry-is-refused" "$c" 1

# 7. An unattributable flag: present, but no name to bind it to.
c="$scratch/orphan"; seed "$c"
cat > "$c/.github/workflows/orphan.yml" <<'YAML'
continue-on-error: true
YAML
run_case "unattributable-flag-fails-closed" "$c" 2

printf 'PASS: workflow continue-on-error policy mutation battery (8 cases)\n'
