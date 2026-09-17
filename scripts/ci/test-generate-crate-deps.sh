#!/usr/bin/env bash
# Pin that generate-crate-deps.sh cannot drop edges via grep -q + pipefail (#3084).
#
# The drift gate only round-trips a generator against its own output, so a race that
# sometimes omits an edge can still look green. This pins the membership check itself.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
DRIFT="$ROOT/scripts/ci/check-docs-generated-drift.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# Static check over the drift-gate generator list: a piped early-exit reader under
# pipefail is the #3084 shape. Chosen as the primary pin because it fails on the old
# membership line every time; a retry loop is scheduling-dependent (20 unmodified
# runs on this host produced one hash even before the fix).
python3 - "$DRIFT" "$ROOT" <<'PY'
from pathlib import Path
import re
import sys

drift = Path(sys.argv[1])
root = Path(sys.argv[2])
text = drift.read_text(encoding="utf-8")
block = re.search(r"^GENERATORS=\((.*?)\)", text, re.M | re.S)
if block is None:
    raise SystemExit("FAIL: GENERATORS array not found in check-docs-generated-drift.sh")
generators = re.findall(r"scripts/[A-Za-z0-9_./-]+", block.group(1))
if not generators:
    raise SystemExit("FAIL: GENERATORS array is empty")

called = re.compile(
    r"""(?:^|[\s;])(?:source|\.|bash)\s+["']?([A-Za-z0-9_./-]+\.sh)"""
)
early_exit = re.compile(r"\|\s*(?:grep\s+-\S*q|head)\b")


def code_lines(path: Path):
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        yield lineno, line.split("#", 1)[0]


def collect_scripts(start: Path, seen: set[Path]) -> None:
    if start in seen or not start.is_file():
        return
    seen.add(start)
    for _, line in code_lines(start):
        for match in called.finditer(line):
            raw = match.group(1)
            candidates = [start.parent / raw, root / raw]
            for candidate in candidates:
                if candidate.is_file():
                    collect_scripts(candidate.resolve(), seen)
                    break


violations = []
scanned = []
for rel in generators:
    path = (root / rel).resolve()
    if not path.is_file():
        raise SystemExit(f"FAIL: generator listed but missing: {rel}")
    if path.suffix == ".py":
        scanned.append(f"{rel} (python; no pipefail)")
        continue
    scripts: set[Path] = set()
    collect_scripts(path, scripts)
    for script in sorted(scripts):
        rel_script = script.relative_to(root).as_posix()
        body = script.read_text(encoding="utf-8")
        if "pipefail" not in body:
            scanned.append(f"{rel_script} (no pipefail)")
            continue
        for lineno, line in code_lines(script):
            if early_exit.search(line):
                violations.append(f"{rel_script}:{lineno}:{line.strip()}")
        scanned.append(rel_script)

if violations:
    print("FAIL: piped early-exit reader under pipefail in a drift generator:", file=sys.stderr)
    for row in violations:
        print(f"  {row}", file=sys.stderr)
    sys.exit(1)

print("ok    no piped grep -q/head in drift generators")
for row in scanned:
    print(f"      scanned {row}")
PY

echo "ok    static membership-shape scan"

# 10 identical runs in a scratch tree so a race cannot rewrite the reviewable mermaid.
mkdir -p "$SCRATCH/repo"
(cd "$ROOT" && git ls-files -z | tar -cf - --null -T -) | (cd "$SCRATCH/repo" && tar -xf -)
COMMITTED_SHA="$(python3 - "$ROOT/docs/generated/crate-deps.mermaid" <<'PY'
import hashlib
import sys
from pathlib import Path
print(hashlib.sha256(Path(sys.argv[1]).read_bytes()).hexdigest())
PY
)"

hashes="$SCRATCH/hashes.txt"
: >"$hashes"
for _ in $(seq 1 10); do
  (cd "$SCRATCH/repo" && bash scripts/docs/generate-crate-deps.sh >/dev/null)
  python3 - "$SCRATCH/repo/docs/generated/crate-deps.mermaid" "$hashes" <<'PY'
import hashlib
import sys
from pathlib import Path
digest = hashlib.sha256(Path(sys.argv[1]).read_bytes()).hexdigest()
Path(sys.argv[2]).write_text(Path(sys.argv[2]).read_text() + digest + "\n")
print(digest)
PY
done >/dev/null

python3 - "$hashes" "$COMMITTED_SHA" <<'PY'
import sys
from pathlib import Path
lines = [line for line in Path(sys.argv[1]).read_text().splitlines() if line]
unique = sorted(set(lines))
committed = sys.argv[2]
if len(lines) != 10:
    raise SystemExit(f"FAIL: expected 10 runs, got {len(lines)}")
if len(unique) != 1:
    raise SystemExit(f"FAIL: generate-crate-deps.sh produced {len(unique)} distinct outputs: {unique}")
if unique[0] != committed:
    raise SystemExit(
        f"FAIL: generator sha256 {unique[0]} != committed crate-deps.mermaid {committed}"
    )
print(f"ok    10/10 runs sha256={unique[0]} (matches committed)")
PY
