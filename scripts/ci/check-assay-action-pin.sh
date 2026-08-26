#!/usr/bin/env bash
# One-rule gate for the Assay consumer Action pin.
#
# Parses the pin, the vendored published action.yml inputs, and every owner-listed
# snippet in one check. Offline by default. --published fetches the pin's
# action.yml and requires byte identity with the fixture.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-}"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"
TREE="${ASSAY_ACTION_TREE:-${ROOT}}"

if [[ -n "${MODE}" && "${MODE}" != "--published" && "${MODE}" != "--list-paths" ]]; then
  echo "usage: $0 [--published|--list-paths]" >&2
  exit 2
fi

PIN=""
FIXTURE=""
PROVENANCE=""
if [[ "${MODE}" != "--list-paths" ]]; then
  PIN_FILE="${ASSAY_ACTION_PIN_FILE:-${TREE}/.github/assay-action-pin}"
  PIN="$(ASSAY_ACTION_PIN_FILE="${PIN_FILE}" "${READER}")"
  FIXTURE="${ASSAY_ACTION_FIXTURE_FILE:-${TREE}/scripts/ci/fixtures/assay-action-pin/action.yml}"
  PROVENANCE="${ASSAY_ACTION_PROVENANCE_FILE:-${TREE}/scripts/ci/fixtures/assay-action-pin/PROVENANCE}"
fi

python3 - "${MODE}" "${PIN}" "${FIXTURE}" "${PROVENANCE}" "${TREE}" <<'PY'
from __future__ import annotations

import hashlib
import os
import re
import sys
from pathlib import Path

MODE = sys.argv[1]
PIN = sys.argv[2]
FIXTURE = Path(sys.argv[3]) if sys.argv[3] else Path()
PROVENANCE = Path(sys.argv[4]) if sys.argv[4] else Path()
TREE = Path(sys.argv[5])
WORKFLOWS = (
    ".github/workflows/assay.yml",
    ".github/workflows/action-v2-test.yml",
)
SNIPPETS = (
    "docs/AIcontext/user-flows.md",
    "docs/AIcontext/quick-reference.md",
    "docs/AIcontext/entry-points.md",
    "docs/AIcontext/code-map.md",
    "docs/AIcontext/codebase-overview.md",
    "docs/getting-started/ci-integration.md",
    "docs/getting-started/quickstart.md",
    "docs/guides/github-action.md",
    "docs/guides/rollout-template.md",
    "docs/index.md",
    "docs/README.md",
    "docs/mcp/quickstart.md",
    "docs/use-cases/ci-gate.md",
    "docs/DISTRIBUTION-SUBMISSION-GUIDE.md",
    "crates/assay-cli/src/templates.rs",
    ".github/workflows/release.yml",
)
OWNED = frozenset(WORKFLOWS + SNIPPETS)
SKIP_PREFIXES = (
    "docs/archive/",
    "scripts/ci/fixtures/",
    "demo/",
    "packs/",
    "third_party/",
)
SCAN_SUFFIXES = {".md", ".yml", ".yaml", ".rs"}
DOC_FLOATING = "Rul1an/assay-action@v3"
USES_LINE = re.compile(
    r"""^[ \t]*(?:-[ \t]+)?["']?uses["']?[ \t]*:[ \t]*(?P<rest>.+)$"""
)


def fail(message: str) -> None:
    raise SystemExit(message)


if MODE == "--list-paths":
    for path in WORKFLOWS + SNIPPETS:
        print(path)
    raise SystemExit(0)


EXPECTED_USES = f"Rul1an/assay-action@{PIN}"


def parse_provenance(text: str) -> tuple[str, str]:
    commit = None
    digest = None
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("commit="):
            commit = line.split("=", 1)[1].strip()
        elif line.startswith("sha256="):
            digest = line.split("=", 1)[1].strip()
        elif line.startswith("tag="):
            continue
        else:
            fail(f"unrecognized provenance line: {line}")
    if commit is None or digest is None:
        fail("provenance must declare commit= and sha256=")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        fail(f"provenance commit is not 40-hex: {commit}")
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        fail(f"provenance sha256 is not 64-hex: {digest}")
    return commit, digest


def declared_inputs(action_yml: str) -> set[str]:
    names: set[str] = set()
    in_inputs = False
    for line in action_yml.splitlines():
        if line.startswith("inputs:"):
            in_inputs = True
            continue
        if not in_inputs:
            continue
        if line and not line[0].isspace() and not line.lstrip().startswith("#"):
            break
        match = re.match(r"^  ([A-Za-z0-9_-]+):(?:\s|$)", line)
        if match:
            names.add(match.group(1))
    if not names:
        fail("vendored action.yml declares no inputs")
    return names


def strip_inline_comment(text: str) -> str:
    in_single = False
    in_double = False
    for index, char in enumerate(text):
        if char == "'" and not in_double:
            in_single = not in_single
        elif char == '"' and not in_single:
            in_double = not in_double
        elif char == "#" and not in_single and not in_double:
            return text[:index].rstrip()
    return text.rstrip()


def parse_uses_value(line: str) -> str | None:
    match = USES_LINE.match(strip_inline_comment(line))
    if match is None:
        return None
    rest = match.group("rest").strip()
    if len(rest) >= 2 and rest[0] in {"'", '"'} and rest[-1] == rest[0]:
        return rest[1:-1]
    return rest


def is_assay_action_ref(ref: str) -> bool:
    lowered = ref.casefold()
    return (
        lowered == "./assay-action"
        or lowered.startswith("./assay-action/")
        or "assay-action@" in lowered
    )


def is_active_consumer_ref(ref: str) -> bool:
    lowered = ref.casefold()
    return (
        lowered == "./assay-action"
        or lowered.startswith("./assay-action/")
        or lowered.startswith("rul1an/assay-action@")
    )


def with_keys_after(lines: list[str], start: int, uses_indent: int) -> list[str]:
    keys: list[str] = []
    in_with = False
    with_indent: int | None = None
    for line in lines[start + 1 :]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        if indent < uses_indent:
            break
        stripped = strip_inline_comment(line)
        if not in_with and re.match(r"""^[ \t]*["']?with["']?[ \t]*:""", stripped):
            in_with = True
            with_indent = indent
            continue
        if indent == uses_indent:
            break
        if not in_with:
            continue
        if with_indent is not None and indent <= with_indent:
            break
        key = re.match(r"""^[ \t]+["']?([A-Za-z0-9_-]+)["']?[ \t]*:""", stripped)
        if key and with_indent is not None and indent == with_indent + 2:
            keys.append(key.group(1))
    return keys


def check_ref(path: Path, ref: str, *, require_pin: bool) -> None:
    if ref == "./assay-action" or ref.startswith("./assay-action/"):
        fail(f"{path}: uses: ./assay-action is not the published consumer pin")
    if "${{" in ref:
        fail(f"{path}: uses must be a literal published pin, not a variable")
    if ref.casefold().startswith("rul1an/assay/assay-action@"):
        fail(f"{path}: uses {ref!r} is the monorepo path, not Rul1an/assay-action")
    if require_pin:
        if ref != EXPECTED_USES:
            fail(f"{path}: uses {ref!r} does not equal pin {PIN}")
        return
    if ref == DOC_FLOATING:
        return
    if ref != EXPECTED_USES:
        fail(f"{path}: uses {ref!r} does not equal pin {PIN} or {DOC_FLOATING}")


def check_file(path: Path, allowed_inputs: set[str], *, require_pin: bool) -> int:
    if not path.is_file():
        fail(f"in-scope file missing: {path}")
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    found = 0
    for index, line in enumerate(lines):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        ref = parse_uses_value(line)
        if ref is None or not is_assay_action_ref(ref):
            continue
        found += 1
        check_ref(path, ref, require_pin=require_pin)
        for key in with_keys_after(lines, index, len(line) - len(line.lstrip())):
            if key not in allowed_inputs:
                fail(
                    f"{path}: undeclared input {key!r} is not in the pinned action.yml"
                )
    return found


def rel_posix(path: Path) -> str:
    return path.relative_to(TREE).as_posix()


def should_skip(rel: str) -> bool:
    return any(rel.startswith(prefix) for prefix in SKIP_PREFIXES)


def walk_unlisted() -> None:
    for dirpath, dirnames, filenames in os.walk(TREE):
        dirnames[:] = [name for name in dirnames if name not in {".git", "target"}]
        for name in filenames:
            path = Path(dirpath) / name
            if path.suffix.lower() not in SCAN_SUFFIXES:
                continue
            rel = rel_posix(path)
            if rel in OWNED or should_skip(rel):
                continue
            text = path.read_text(encoding="utf-8")
            for line in text.splitlines():
                if not line.strip() or line.lstrip().startswith("#"):
                    continue
                ref = parse_uses_value(line)
                if ref is None or not is_active_consumer_ref(ref):
                    continue
                fail(f"{rel}: assay-action uses is not on the owner snippet list")


def main() -> None:
    if not FIXTURE.is_file():
        fail(f"pinned action.yml fixture missing: {FIXTURE}")
    if not PROVENANCE.is_file():
        fail(f"pinned action.yml provenance missing: {PROVENANCE}")

    fixture_bytes = FIXTURE.read_bytes()
    actual = hashlib.sha256(fixture_bytes).hexdigest()
    commit, digest = parse_provenance(PROVENANCE.read_text(encoding="utf-8"))
    if commit != PIN:
        fail(f"provenance commit {commit} does not equal pin {PIN}")
    if actual != digest:
        fail(f"pinned fixture digest {actual} != {digest}")

    allowed = declared_inputs(fixture_bytes.decode("utf-8"))
    for rel in WORKFLOWS:
        found = check_file(TREE / rel, allowed, require_pin=True)
        if found == 0:
            fail(f"{TREE / rel}: no {EXPECTED_USES} uses found")
    for rel in SNIPPETS:
        found = check_file(TREE / rel, allowed, require_pin=False)
        if found == 0:
            fail(f"{TREE / rel}: no {DOC_FLOATING} uses found")
    walk_unlisted()
    print(
        f"assay action consumer pin: {PIN} "
        f"(fixture sha256 {digest}; {len(WORKFLOWS)} workflows, {len(SNIPPETS)} snippets)"
    )


if __name__ == "__main__":
    try:
        main()
    except SystemExit as error:
        if error.code not in (0, None):
            print(str(error), file=sys.stderr)
            raise SystemExit(1)
        raise
PY

if [[ "${MODE}" != "--published" ]]; then
  exit 0
fi

PUBLISHED="${ASSAY_ACTION_PUBLISHED_FILE:-}"
cleanup_published=""
if [[ -n "${PUBLISHED}" ]]; then
  if [[ ! -f "${PUBLISHED}" ]]; then
    echo "published action.yml override is missing: ${PUBLISHED}" >&2
    exit 1
  fi
else
  PUBLISHED="$(mktemp)"
  cleanup_published="${PUBLISHED}"
  url="https://raw.githubusercontent.com/Rul1an/assay-action/${PIN}/action.yml"
  python3 - "${url}" "${PUBLISHED}" <<'PY'
from pathlib import Path
import sys
import urllib.request

url, dest = sys.argv[1], sys.argv[2]
request = urllib.request.Request(url, headers={"User-Agent": "assay-action-pin-live"})
with urllib.request.urlopen(request, timeout=30) as response:
    data = response.read(1048576 + 1)
if len(data) > 1048576:
    raise SystemExit("published action.yml exceeds 1048576-byte limit")
if not data:
    raise SystemExit(f"published action.yml fetch was empty: {url}")
Path(dest).write_bytes(data)
PY
fi
if [[ -n "${cleanup_published}" ]]; then
  trap 'rm -f "${cleanup_published}"' EXIT
fi

python3 - "${FIXTURE}" "${PUBLISHED}" "${PIN}" <<'PY'
from pathlib import Path
import sys

fixture = Path(sys.argv[1]).resolve()
published = Path(sys.argv[2]).resolve()
pin = sys.argv[3]
if fixture == published:
    raise SystemExit("published action.yml must not be the fixture file itself")
if fixture.read_bytes() != published.read_bytes():
    raise SystemExit(
        f"fixture action.yml does not match published action.yml for {pin} "
        f"(https://raw.githubusercontent.com/Rul1an/assay-action/{pin}/action.yml)"
    )
print(f"published action.yml bytes match pin {pin}")
PY
