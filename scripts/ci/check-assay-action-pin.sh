#!/usr/bin/env bash
# One-rule gate for the Assay consumer Action pin.
#
# One process reads the vendored action.yml once, derives digest and declared
# inputs from those bytes, validates every owner-listed snippet against them,
# and --published-compares the same retained bytes to the live file.
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
import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

MODE = sys.argv[1]
PIN = sys.argv[2]
FIXTURE = Path(sys.argv[3]) if sys.argv[3] else Path()
PROVENANCE = Path(sys.argv[4]) if sys.argv[4] else Path()
TREE = Path(sys.argv[5])
READ_LIMIT = 1048576
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
    "packs/open/cicd-starter/README.md",
)
OWNED = frozenset(WORKFLOWS + SNIPPETS)
SKIP_PREFIXES = (
    "docs/archive/",
    "docs/architecture/",
    "scripts/ci/fixtures/",
    "demo/",
    "third_party/",
    "crates/assay-ebpf/",
)
SCAN_SUFFIXES = {".md", ".yml", ".yaml", ".rs"}
DOC_FLOATING = "Rul1an/assay-action@v3"
FENCE = re.compile(
    r"^[ \t]*```(?P<lang>ya?ml)[^\n`]*\n(?P<body>.*?)^[ \t]*```",
    re.IGNORECASE | re.MULTILINE | re.DOTALL,
)
RAW_STRING = re.compile(r'r#"(.*?)"#', re.DOTALL)
RUBY_SAFE_LOAD = r"""
require "json"
require "yaml"
begin
  val = YAML.safe_load(STDIN.read, aliases: false)
  STDOUT.write(JSON.generate(val))
rescue Psych::SyntaxError, Psych::BadAlias, Psych::DisallowedClass => e
  STDERR.write(e.message)
  exit 2
end
"""


def fail(message: str) -> None:
    raise SystemExit(message)


if MODE == "--list-paths":
    for path in WORKFLOWS + SNIPPETS:
        print(path)
    raise SystemExit(0)


EXPECTED_USES = f"Rul1an/assay-action@{PIN}"


def bounded_read(path: Path, *, allow_empty: bool = False) -> bytes:
    try:
        with path.open("rb") as handle:
            data = handle.read(READ_LIMIT + 1)
    except OSError as exc:
        fail(f"{path}: read failed: {exc}")
    if len(data) > READ_LIMIT:
        fail(f"{path}: exceeds {READ_LIMIT}-byte limit")
    if not data and not allow_empty:
        fail(f"{path}: file is empty")
    return data


def decode_utf8(path: Path, data: bytes) -> str:
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as exc:
        fail(f"{path}: not valid UTF-8: {exc}")


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


def load_yaml(text: str, *, source: str, required: bool = True):
    try:
        proc = subprocess.run(
            ["ruby", "-EUTF-8:UTF-8", "-e", RUBY_SAFE_LOAD],
            input=text,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except FileNotFoundError:
        fail("ruby is required to parse GitHub Actions YAML")
    except subprocess.TimeoutExpired:
        fail(f"{source}: YAML parse timed out")
    if proc.returncode != 0:
        if not required:
            return None
        err = (proc.stderr or proc.stdout or "yaml parse failed").strip()
        fail(f"{source}: YAML parse failed: {err}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        if not required:
            return None
        fail(f"{source}: YAML JSON decode failed: {exc}")


def fence_lang(raw: str) -> str:
    token = raw.strip().split()[0].lower() if raw.strip() else ""
    return token


def yaml_texts(path: Path, text: str) -> list[tuple[str, str, bool]]:
    """Return (source, yaml_text, required) documents from one file."""
    items: list[tuple[str, str, bool]] = []
    suffix = path.suffix.lower()
    rel = path.as_posix()
    if suffix in {".yml", ".yaml"}:
        items.append((rel, text, True))
    if suffix == ".rs" and "assay-action" in text.casefold():
        for match in RAW_STRING.finditer(text):
            body = match.group(1)
            if "uses:" in body and "assay-action" in body.casefold():
                items.append((f"{rel} raw-string", body, True))
    for match in FENCE.finditer(text):
        body = match.group("body")
        if "uses:" not in body:
            continue
        lang = fence_lang(match.group("lang"))
        if lang in {"yaml", "yml"}:
            items.append((f"{rel} snippet", body, True))
    if suffix not in {".yml", ".yaml"} and not items and "uses:" in text:
        items.append((rel, text, False))
    seen: set[str] = set()
    unique: list[tuple[str, str, bool]] = []
    for source, body, required in items:
        if body in seen:
            continue
        seen.add(body)
        unique.append((source, body, required))
    return unique


def iter_steps(node: object):
    if isinstance(node, list):
        for item in node:
            yield from iter_steps(item)
        return
    if not isinstance(node, dict):
        return
    uses = node.get("uses")
    if isinstance(uses, str):
        yield uses, node.get("with")
    for value in node.values():
        yield from iter_steps(value)


def declared_inputs(action_yml: str) -> set[str]:
    loaded = load_yaml(action_yml, source="pinned action.yml")
    if not isinstance(loaded, dict):
        fail("pinned action.yml must be a mapping")
    inputs = loaded.get("inputs")
    if not isinstance(inputs, dict) or not inputs:
        fail("vendored action.yml declares no inputs")
    names: set[str] = set()
    for key in inputs:
        if not isinstance(key, str) or not re.fullmatch(r"[A-Za-z0-9_-]+", key):
            fail(f"pinned action.yml input name is invalid: {key!r}")
        names.add(key)
    return names


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


def check_with(path: Path, with_value: object, allowed: set[str]) -> None:
    if with_value is None:
        return
    if not isinstance(with_value, dict):
        fail(f"{path}: with: must be a mapping")
    for key in with_value:
        if key not in allowed:
            fail(
                f"{path}: undeclared input {key!r} is not in the pinned action.yml"
            )


def parsed_docs(path: Path, text: str) -> list[object]:
    docs: list[object] = []
    for source, body, required in yaml_texts(path, text):
        loaded = load_yaml(body, source=source, required=required)
        if loaded is not None:
            docs.append(loaded)
    return docs


def check_file(path: Path, allowed: set[str], *, require_pin: bool) -> int:
    if not path.is_file():
        fail(f"in-scope file missing: {path}")
    text = decode_utf8(path, bounded_read(path))
    found = 0
    for doc in parsed_docs(path, text):
        for ref, with_value in iter_steps(doc):
            if not is_assay_action_ref(ref):
                continue
            found += 1
            check_ref(path, ref, require_pin=require_pin)
            check_with(path, with_value, allowed)
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
            data = bounded_read(path, allow_empty=True)
            if not data:
                continue
            text = decode_utf8(path, data)
            if "assay-action" not in text.casefold():
                continue
            for doc in parsed_docs(path, text):
                for ref, _with_value in iter_steps(doc):
                    if is_active_consumer_ref(ref):
                        fail(f"{rel}: assay-action uses is not on the owner snippet list")


def published_bytes() -> bytes:
    override = os.environ.get("ASSAY_ACTION_PUBLISHED_FILE", "")
    if override:
        published_path = Path(override)
        if not published_path.is_file():
            fail(f"published action.yml override is missing: {published_path}")
        if published_path.resolve() == FIXTURE.resolve():
            fail("published action.yml must not be the fixture file itself")
        return bounded_read(published_path)
    url = f"https://raw.githubusercontent.com/Rul1an/assay-action/{PIN}/action.yml"
    request = urllib.request.Request(url, headers={"User-Agent": "assay-action-pin-live"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(READ_LIMIT + 1)
    except OSError as exc:
        fail(f"published action.yml fetch failed: {exc}")
    if len(data) > READ_LIMIT:
        fail("published action.yml exceeds 1048576-byte limit")
    if not data:
        fail(f"published action.yml fetch was empty: {url}")
    return data


def maybe_replace_fixture_path() -> None:
    swap = os.environ.get("ASSAY_ACTION_FIXTURE_SWAP_FILE", "")
    if not swap:
        return
    if not os.environ.get("ASSAY_ACTION_TREE"):
        fail("ASSAY_ACTION_FIXTURE_SWAP_FILE is test-only and requires ASSAY_ACTION_TREE")
    FIXTURE.write_bytes(bounded_read(Path(swap)))


def main() -> None:
    if not FIXTURE.is_file():
        fail(f"pinned action.yml fixture missing: {FIXTURE}")
    if not PROVENANCE.is_file():
        fail(f"pinned action.yml provenance missing: {PROVENANCE}")

    fixture_bytes = bounded_read(FIXTURE)
    actual = hashlib.sha256(fixture_bytes).hexdigest()
    commit, digest = parse_provenance(decode_utf8(PROVENANCE, bounded_read(PROVENANCE)))
    if commit != PIN:
        fail(f"provenance commit {commit} does not equal pin {PIN}")
    if actual != digest:
        fail(f"pinned fixture digest {actual} != {digest}")

    allowed = declared_inputs(decode_utf8(FIXTURE, fixture_bytes))
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
    if MODE != "--published":
        return
    maybe_replace_fixture_path()
    if fixture_bytes != published_bytes():
        fail(
            f"fixture action.yml does not match published action.yml for {PIN} "
            f"(https://raw.githubusercontent.com/Rul1an/assay-action/{PIN}/action.yml)"
        )
    print(f"published action.yml bytes match pin {PIN}")


if __name__ == "__main__":
    try:
        main()
    except SystemExit as error:
        if error.code not in (0, None):
            print(str(error), file=sys.stderr)
            raise SystemExit(1)
        raise
PY
