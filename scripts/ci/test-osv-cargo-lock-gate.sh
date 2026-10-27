#!/usr/bin/env bash
# Mutation battery for the PR OSV Cargo.lock gate.
#
# Control must be green on the live tree. Swapping to the PR-diff reusable,
# floating the pin, widening scan-args, dropping security-events, or deleting
# the cmov rationale must turn the checker red. So must a quoted ignoreUntil,
# a missing or empty reason, a missing ignoreUntil, or a misspelled key in
# osv-scanner.toml: those rules used to live only in a comment.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-cargo-lock-gate.py"
WORKFLOW=".github/workflows/ci.yml"
OSV_TOML="osv-scanner.toml"

[[ -f "${ROOT}/${CHECKER}" ]] || { echo "FAIL: checker missing" >&2; exit 1; }
[[ -f "${ROOT}/${WORKFLOW}" ]] || { echo "FAIL: workflow missing" >&2; exit 1; }
[[ -f "${ROOT}/${OSV_TOML}" ]] || { echo "FAIL: osv-scanner.toml missing" >&2; exit 1; }

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local case_root="$1"
  mkdir -p "$case_root/.github/workflows" "$case_root/scripts/ci"
  cp "${ROOT}/${CHECKER}" "$case_root/${CHECKER}"
  cp "${ROOT}/${WORKFLOW}" "$case_root/${WORKFLOW}"
  cp "${ROOT}/${OSV_TOML}" "$case_root/${OSV_TOML}"
  mkdir -p "$case_root/fuzz"
  cp "$ROOT/Cargo.lock" "$case_root/Cargo.lock"
  cp "$ROOT/fuzz/Cargo.lock" "$case_root/fuzz/Cargo.lock"
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

c="$scratch/control"
seed "$c"
run_checker "control-is-green" "$c" 0

c="$scratch/pr-diff"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "osv-scanner-reusable.yml@",
    "osv-scanner-reusable-pr.yml@",
    1,
)
path.write_text(text)
PY
run_checker "pr-diff-reusable" "$c" 1
if ! grep -F "osv-scanner-reusable-pr.yml" "$scratch/pr-diff-reusable.log" >/dev/null; then
  cat "$scratch/pr-diff-reusable.log" >&2
  echo "FAIL: pr-diff mutation did not name the PR-diff reusable" >&2
  exit 1
fi

c="$scratch/sha-drift"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "b1d267aea5211fa648d9d442fa64b90dbdcbdd36",
    "0000000000000000000000000000000000000000",
)
path.write_text(text)
PY
run_checker "sha-drift" "$c" 1

c="$scratch/recursive"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "        --lockfile=Cargo.lock\n        --lockfile=fuzz/Cargo.lock\n",
    "        --recursive\n        ./\n",
    1,
)
path.write_text(text)
PY
run_checker "recursive-or-dot" "$c" 1

c="$scratch/no-security-events"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace("      security-events: write\n", "", 1)
path.write_text(text)
PY
run_checker "security-events-dropped" "$c" 1

c="$scratch/fail-open"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      scan-args: |-\n"
insert = needle + "      fail-on-vuln: false\n"
if needle not in text:
    raise SystemExit("scan-args block missing; cannot insert fail-on-vuln")
path.write_text(text.replace(needle, insert, 1))
PY
run_checker "fail-on-vuln-false" "$c" 1

c="$scratch/rationale-dropped"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
lines = path.read_text().splitlines(keepends=True)
out = []
skip = False
for line in lines:
    if line.startswith("  osv-cargo-lock:"):
        skip = True
        out.append(line)
        continue
    if skip and line.startswith("    #"):
        continue
    skip = False
    out.append(line)
path.write_text("".join(out))
PY
run_checker "rationale-comment-dropped" "$c" 1
if ! grep -F "GHSA-3rjw-m598-pq24" "$scratch/rationale-comment-dropped.log" >/dev/null; then
  cat "$scratch/rationale-comment-dropped.log" >&2
  echo "FAIL: rationale mutation did not name GHSA-3rjw-m598-pq24" >&2
  exit 1
fi

c="$scratch/quoted-ignore-until"
seed "$c"
python3 - "$c/${OSV_TOML}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
path.write_text(text.replace("ignoreUntil = 2026-12-31", 'ignoreUntil = "2026-12-31"', 1))
PY
run_checker "quoted-ignoreUntil" "$c" 1
if ! grep -F "unquoted" "$scratch/quoted-ignoreUntil.log" >/dev/null; then
  cat "$scratch/quoted-ignoreUntil.log" >&2
  echo "FAIL: quoted ignoreUntil mutation did not say to write it unquoted" >&2
  exit 1
fi

c="$scratch/missing-reason"
seed "$c"
python3 - "$c/${OSV_TOML}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
lines = [line for line in path.read_text().splitlines(keepends=True) if not line.startswith("reason =")]
path.write_text("".join(lines))
PY
run_checker "missing-reason" "$c" 1
if ! grep -F "reason" "$scratch/missing-reason.log" >/dev/null; then
  cat "$scratch/missing-reason.log" >&2
  echo "FAIL: missing-reason mutation did not name reason" >&2
  exit 1
fi

c="$scratch/empty-reason"
seed "$c"
python3 - "$c/${OSV_TOML}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
# Keep the key so this is empty, not missing.
path.write_text(text.replace('reason = "Informational', 'reason = ""\n# Informational', 1))
PY
run_checker "empty-reason" "$c" 1
if ! grep -F "reason" "$scratch/empty-reason.log" >/dev/null; then
  cat "$scratch/empty-reason.log" >&2
  echo "FAIL: empty-reason mutation did not name reason" >&2
  exit 1
fi

c="$scratch/missing-ignore-until"
seed "$c"
python3 - "$c/${OSV_TOML}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
lines = [line for line in path.read_text().splitlines(keepends=True) if not line.startswith("ignoreUntil =")]
path.write_text("".join(lines))
PY
run_checker "missing-ignoreUntil" "$c" 1
if ! grep -F "ignoreUntil" "$scratch/missing-ignoreUntil.log" >/dev/null; then
  cat "$scratch/missing-ignoreUntil.log" >&2
  echo "FAIL: missing-ignoreUntil mutation did not name ignoreUntil" >&2
  exit 1
fi

c="$scratch/misspelled-key"
seed "$c"
python3 - "$c/${OSV_TOML}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
path.write_text(text.replace("ignoreUntil =", "ignoreUtil =", 1))
PY
run_checker "misspelled-key" "$c" 1
if ! grep -F "ignoreUtil" "$scratch/misspelled-key.log" >/dev/null; then
  cat "$scratch/misspelled-key.log" >&2
  echo "FAIL: misspelled-key mutation did not name ignoreUtil" >&2
  exit 1
fi

c="$scratch/runs-on-deleted"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      runs-on: ubuntu-24.04\n      scan-args: |-"
assert needle in text, "osv with.runs-on block missing"
path.write_text(text.replace(needle, "      scan-args: |-", 1))
PY
run_checker "runs-on-deleted" "$c" 1
if ! grep -F "with.runs-on" "$scratch/runs-on-deleted.log" >/dev/null; then
  cat "$scratch/runs-on-deleted.log" >&2
  echo "FAIL: runs-on-deleted mutation did not name with.runs-on" >&2
  exit 1
fi

c="$scratch/runs-on-latest"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      runs-on: ubuntu-24.04\n      scan-args: |-"
assert needle in text, "osv with.runs-on block missing"
path.write_text(text.replace(needle, "      runs-on: ubuntu-latest\n      scan-args: |-", 1))
PY
run_checker "runs-on-latest" "$c" 1
if ! grep -F "with.runs-on" "$scratch/runs-on-latest.log" >/dev/null; then
  cat "$scratch/runs-on-latest.log" >&2
  echo "FAIL: runs-on-latest mutation did not name with.runs-on" >&2
  exit 1
fi

c="$scratch/runs-on-comment-lookalike"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      runs-on: ubuntu-24.04\n      scan-args: |-"
assert needle in text, "osv with.runs-on block missing"
replacement = "      # runs-on: ubuntu-24.04 (inert comment, not effective)\n      scan-args: |-"
path.write_text(text.replace(needle, replacement, 1))
PY
run_checker "runs-on-comment-lookalike" "$c" 1
if ! grep -F "with.runs-on" "$scratch/runs-on-comment-lookalike.log" >/dev/null; then
  cat "$scratch/runs-on-comment-lookalike.log" >&2
  echo "FAIL: comment lookalike mutation did not name with.runs-on" >&2
  exit 1
fi

c="$scratch/runs-on-string-lookalike"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      runs-on: ubuntu-24.04\n      scan-args: |-"
assert needle in text, "osv with.runs-on block missing"
replacement = '      decoy-note: "runs-on: ubuntu-24.04"\n      scan-args: |-'
path.write_text(text.replace(needle, replacement, 1))
PY
run_checker "runs-on-string-lookalike" "$c" 1
if ! grep -F "with.runs-on" "$scratch/runs-on-string-lookalike.log" >/dev/null; then
  cat "$scratch/runs-on-string-lookalike.log" >&2
  echo "FAIL: string lookalike mutation did not name with.runs-on" >&2
  exit 1
fi


# Exercise the real checker under an active, narrowly evidenced GHSA disposition.
# A vulnerable version, substituted artifact, missing/malformed lock, or duplicate
# vulnerable package must still refuse the merge, despite the scanner ID ignore.
python3 - "$ROOT" "$scratch/control" <<'PYTEST'
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root, seed = map(Path, sys.argv[1:])
source = "registry+https://github.com/rust-lang/crates.io-index"
old_hash = "3078c7629b62d3f0439517fa394996acacc5cbc91c5a20d8c658e77abd503a71"
fixed_hash = "d2f6c7dbe95a6ed67ad9f18e57daf93a2f034c524b99fd2b76d18fdfeb6660aa"
def package(version, checksum, registry=source):
    return f'[[package]]\nname = "block-buffer"\nversion = "{version}"\nsource = "{registry}"\nchecksum = "{checksum}"\n'
control = package("0.10.4", old_hash) + package("0.12.1", fixed_hash)
cases = [
    ("reviewed-artifacts", control, 0),
    ("vulnerable-0.12.0", package("0.12.0", "cdd35008169921d80bc60d3d0ab416eecb028c4cd653352907921d95084790be"), 1),
    ("vulnerable-0.11.0", package("0.11.0", fixed_hash), 1),
    ("unreviewed-old-version", package("0.10.3", old_hash), 1),
    ("unreviewed-new-version", package("0.12.2", fixed_hash), 1),
    ("prerelease", package("0.12.1-rc.1", fixed_hash), 1),
    ("wrong-registry", package("0.10.4", old_hash, "registry+https://example.invalid/index"), 1),
    ("wrong-checksum", package("0.10.4", "0" * 64), 1),
    ("duplicate-vulnerable-version", control + package("0.12.0", fixed_hash), 1),
    ("malformed-lock", "[[package]", 1),
    ("missing-lock", None, 1),
]
failures = []
for lock in ("Cargo.lock", "fuzz/Cargo.lock"):
    for name, payload, expected in cases:
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp)
            shutil.copytree(seed, dest, dirs_exist_ok=True)
            config = dest / "osv-scanner.toml"
            if "GHSA-qwgh-2vcv-g2f7" not in config.read_text():
                with config.open("a") as f:
                    f.write('\n[[IgnoredVulns]]\nid = "GHSA-qwgh-2vcv-g2f7"\nignoreUntil = 2026-11-10\nreason = "test disposition"\n')
            target = dest / lock
            if payload is None:
                target.unlink()
            else:
                target.write_text(payload)
            result = subprocess.run([sys.executable, "scripts/ci/check-osv-cargo-lock-gate.py"], cwd=dest, capture_output=True, text=True)
            if result.returncode != expected or (expected and "block-buffer disposition" not in result.stderr):
                failures.append(f"{lock}/{name}: got {result.returncode}, expected {expected}: {result.stderr}")
            else:
                print(f"ok    {lock}/{name} (exit {expected})")
if failures:
    raise SystemExit("FAIL: block-buffer disposition boundary\n" + "\n".join(failures))
PYTEST

echo "PASS: osv-cargo-lock gate contract (control, pr-diff, sha-drift, recursive, permissions, fail-open, rationale, toml-ignore, runs-on-effective)"
