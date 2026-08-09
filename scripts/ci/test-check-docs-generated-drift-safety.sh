#!/usr/bin/env bash
set -euo pipefail

without_git_context() {
  env \
    -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
    -u GIT_COMMON_DIR \
    -u GIT_DIR \
    -u GIT_INDEX_FILE \
    -u GIT_OBJECT_DIRECTORY \
    -u GIT_WORK_TREE \
    "$@"
}

ROOT="$(without_git_context git rev-parse --show-toplevel)"
PROBE="$(mktemp -d)"
EXTERNAL_TMP=""

cleanup_probe() {
  rm -rf "$PROBE"
  if [[ -n "$EXTERNAL_TMP" ]]; then
    rm -rf "$EXTERNAL_TMP"
  fi
}
trap cleanup_probe EXIT

hermetic_git() {
  local root="$1"
  shift
  without_git_context env \
    GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CEILING_DIRECTORIES="$(dirname "$root")" \
    git -C "$root" \
      -c core.excludesFile= \
      -c core.attributesFile= \
      "$@"
}

seed_repo() {
  local destination="$1"
  mkdir -p "$destination"
  hermetic_git "$ROOT" ls-files -z | tar -cf - --null -T - \
    | (cd "$destination" && tar -xf -)
  hermetic_git "$destination" -c init.defaultBranch=main init -q
  hermetic_git "$destination" add -f -- .
}

snapshot_tree() {
  local root="$1"
  hermetic_git "$root" ls-files -z --cached --others --exclude-standard \
    | SNAPSHOT_ROOT="$root" python3 /dev/fd/3 3<<'PY'
import hashlib
import json
import os
import stat
import sys

MAX_LIST_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 200_000
MAX_TOTAL_CONTENT_BYTES = 2 * 1024 * 1024 * 1024

listing = sys.stdin.buffer.read(MAX_LIST_BYTES + 1)
if len(listing) > MAX_LIST_BYTES:
    raise SystemExit("snapshot path list exceeds 64 MiB")

paths = listing.rstrip(b"\0").split(b"\0") if listing else []
if len(paths) > MAX_ENTRIES:
    raise SystemExit("snapshot path list exceeds 200000 entries")

root = os.fsencode(os.environ["SNAPSHOT_ROOT"])
total_content = 0
for path in sorted(paths):
    absolute = os.path.join(root, path)
    metadata = os.lstat(absolute)
    file_type = stat.S_IFMT(metadata.st_mode)
    digest = hashlib.sha256()
    digest.update(len(path).to_bytes(8, "big"))
    digest.update(path)
    digest.update(file_type.to_bytes(4, "big"))

    if stat.S_ISREG(metadata.st_mode):
        total_content += metadata.st_size
        if total_content > MAX_TOTAL_CONTENT_BYTES:
            raise SystemExit("snapshot regular-file content exceeds 2 GiB")
        digest.update(metadata.st_size.to_bytes(8, "big"))
        with open(absolute, "rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
    elif stat.S_ISLNK(metadata.st_mode):
        target = os.readlink(absolute)
        target_bytes = os.fsencode(target)
        digest.update(len(target_bytes).to_bytes(8, "big"))
        digest.update(target_bytes)

    display_path = json.dumps(os.fsdecode(path), ensure_ascii=True)
    print(f"{digest.hexdigest()}\t{display_path}")
PY
}

CASE_ROOT="$PROBE/case/repo"
seed_repo "$CASE_ROOT"
SELF_TEST="$CASE_ROOT/scripts/ci/test-check-docs-generated-drift.sh"

MODE="$(python3 - "$SELF_TEST" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
lines = text.splitlines()
historical = sum(line.strip() == "trap cleanup EXIT" for line in lines)
target = sum(line.strip() == '''trap 'rm -rf "$SCRATCH"' EXIT''' for line in lines)

if (historical, target) == (1, 0):
    print("historical")
    raise SystemExit(0)
if (historical, target) != (0, 1):
    raise SystemExit(
        "FAIL: drift self-test mode detection requires exactly one cleanup trap "
        f"(historical={historical}, target={target})"
    )

for name, pattern in (
    ("cleanup() function", r"(?m)^\s*cleanup\s*\(\s*\)\s*\{"),
    ("*_BACKUP variable", r"\b[A-Za-z_][A-Za-z0-9_]*_BACKUP\b"),
    ("trap cleanup", r"(?m)^\s*trap\b[^\n]*\bcleanup\b"),
):
    if re.search(pattern, text):
        raise SystemExit(f"FAIL: target drift self-test retains forbidden {name}")

outside_seed = []
inside_seed = False
for line in lines:
    if not inside_seed and re.match(r"^seed_repo\s*\(\s*\)\s*\{\s*$", line):
        inside_seed = True
        continue
    if inside_seed and line == "}":
        inside_seed = False
        continue
    if not inside_seed:
        outside_seed.append(line)

if inside_seed:
    raise SystemExit("FAIL: target drift self-test seed_repo() boundary is incomplete")

commands = "\n".join(outside_seed).replace("\\\n", " ")
rooted_copy = re.compile(
    r"(?m)^\s*(?:command\s+)?(?:cp|mv)\b[^\n]*"
    r"(?:\$ROOT|\$\{ROOT\})(?:/|\b)[^\n]*$"
)
if rooted_copy.search(commands):
    raise SystemExit(
        "FAIL: target drift self-test has cp/mv rooted under $ROOT outside seed_repo()"
    )

print("target")
PY
)"
echo "drift safety probe mode: $MODE"

MODE="$MODE" SELF_TEST="$SELF_TEST" python3 - <<'PY'
from pathlib import Path
import os

path = Path(os.environ["SELF_TEST"])
text = path.read_text(encoding="utf-8")
mode = os.environ["MODE"]

if mode == "historical":
    trap = "trap cleanup EXIT"
    anchor = r'''printf '\n%%%% drift planted by the drift-check self-test\n' >> "$SUBJECT"''' + "\n"
    if text.count(trap) != 1:
        raise SystemExit("historical cleanup trap is not unique")
    if text.count(anchor) != 1:
        raise SystemExit("historical first diagram mutation anchor is not unique")
    text = text.replace(trap, "trap : EXIT", 1)
    text = text.replace(
        anchor,
        anchor + 'echo "test interruption: hand-edited-diagram"; exit 97\n',
        1,
    )
elif mode == "target":
    trap = '''trap 'rm -rf "$SCRATCH"' EXIT'''
    if text.count(trap) != 1:
        raise SystemExit("target scratch cleanup trap is not unique")
    text = text.replace(trap, "trap : EXIT", 1)
else:
    raise SystemExit(f"unknown probe mode: {mode}")

path.write_text(text, encoding="utf-8")
PY

BEFORE="$(snapshot_tree "$CASE_ROOT")"
EXTERNAL_TMP="$(mktemp -d)"
OUTPUT="$EXTERNAL_TMP/self-test.log"

if [[ "$MODE" == "target" ]]; then
  if (cd "$CASE_ROOT" && without_git_context env \
      TMPDIR="$EXTERNAL_TMP" \
      ASSAY_DOCS_DRIFT_SELF_TEST_CASE=hand-edited-diagram \
      ASSAY_DOCS_DRIFT_INTERRUPT_AFTER_MUTATION=hand-edited-diagram \
      bash scripts/ci/test-check-docs-generated-drift.sh >"$OUTPUT" 2>&1); then
    STATUS=0
  else
    STATUS=$?
  fi
else
  if (cd "$CASE_ROOT" && without_git_context env TMPDIR="$EXTERNAL_TMP" \
      bash scripts/ci/test-check-docs-generated-drift.sh >"$OUTPUT" 2>&1); then
    STATUS=0
  else
    STATUS=$?
  fi
fi

cat "$OUTPUT"
if [[ "$STATUS" -ne 97 ]]; then
  echo "FAIL: interrupted drift self-test exited $STATUS, wanted 97" >&2
  exit 1
fi

MARKER_COUNT="$(awk '$0 == "test interruption: hand-edited-diagram" { count++ } END { print count + 0 }' "$OUTPUT")"
if [[ "$MARKER_COUNT" -ne 1 ]]; then
  echo "FAIL: interrupted drift self-test emitted $MARKER_COUNT interruption markers, wanted 1" >&2
  exit 1
fi

rm -rf "$EXTERNAL_TMP"
EXTERNAL_TMP=""
AFTER="$(snapshot_tree "$CASE_ROOT")"
if [[ "$BEFORE" != "$AFTER" ]]; then
  echo "FAIL: drift self-test writes its repository before cleanup" >&2
  diff -u <(printf '%s\n' "$BEFORE") <(printf '%s\n' "$AFTER") >&2 || true
  exit 1
fi

echo "generated-docs drift interruption safety: pass"
