#!/usr/bin/env bash
# One-rule gate for the Assay consumer Action pin.
#
# Parses the pin, the vendored published action.yml inputs, and the allowlisted
# consumer workflows/docs in one check. Offline: no live GitHub fetch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"
PIN="$(ASSAY_ACTION_PIN_FILE="${ASSAY_ACTION_PIN_FILE:-${ROOT}/.github/assay-action-pin}" "${READER}")"
FIXTURE="${ASSAY_ACTION_FIXTURE_FILE:-${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml}"
PROVENANCE="${ASSAY_ACTION_PROVENANCE_FILE:-${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE}"
WORKFLOW_ASSAY="${ASSAY_ACTION_WORKFLOW_ASSAY:-${ROOT}/.github/workflows/assay.yml}"
WORKFLOW_V2="${ASSAY_ACTION_WORKFLOW_V2:-${ROOT}/.github/workflows/action-v2-test.yml}"
DOC_USER_FLOWS="${ASSAY_ACTION_DOC_USER_FLOWS:-${ROOT}/docs/AIcontext/user-flows.md}"
DOC_CI_INTEGRATION="${ASSAY_ACTION_DOC_CI_INTEGRATION:-${ROOT}/docs/getting-started/ci-integration.md}"

python3 - "${PIN}" "${FIXTURE}" "${PROVENANCE}" \
  "${WORKFLOW_ASSAY}" "${WORKFLOW_V2}" \
  "${DOC_USER_FLOWS}" "${DOC_CI_INTEGRATION}" <<'PY'
from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

PIN = sys.argv[1]
FIXTURE = Path(sys.argv[2])
PROVENANCE = Path(sys.argv[3])
WORKFLOWS = (Path(sys.argv[4]), Path(sys.argv[5]))
DOCS = (Path(sys.argv[6]), Path(sys.argv[7]))
EXPECTED_USES = f"Rul1an/assay-action@{PIN}"
DOC_FLOATING = "Rul1an/assay-action@v3"
USES_LINE = re.compile(
    r"""^[ \t]*(?:-[ \t]+)?["']?uses["']?[ \t]*:[ \t]*(?P<rest>.+)$"""
)
SHA_REF = re.compile(r"^[0-9a-f]{40}$")


def fail(message: str) -> None:
    raise SystemExit(message)


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
        if require_pin:
            for key in with_keys_after(lines, index, len(line) - len(line.lstrip())):
                if key not in allowed_inputs:
                    fail(f"{path}: undeclared input {key!r} is not in the pinned action.yml")
    return found


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
    for workflow in WORKFLOWS:
        found = check_file(workflow, allowed, require_pin=True)
        if found == 0:
            fail(f"{workflow}: no {EXPECTED_USES} uses found")
    for doc in DOCS:
        check_file(doc, allowed, require_pin=False)
    print(
        f"assay action consumer pin: {PIN} "
        f"(fixture sha256 {digest}; {len(WORKFLOWS)} workflows, {len(DOCS)} docs)"
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
