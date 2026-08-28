#!/usr/bin/env bash
# Guardrail: assay-cli must ship a crate-owned README whose links resolve
# against `cargo package -p assay-cli --list`, not the workspace README.
#
# Archive README (release tarball / #2677) must pin a version. This crate
# page must not. The release archive scanner rewrites to tag-bound pins;
# do not import that different contract here. Local extract-only linear
# scan (README 1 MiB / label 512 / URL 2048) for crate README links.
# Relative targets ⊆ package members;
# reject mutable git refs and version pins including package-id syntax.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

LIST_FILE="$(mktemp)"
META_FILE="$(mktemp)"
trap 'rm -f "$LIST_FILE" "$META_FILE"' EXIT

# --list --no-verify is the membership contract. Do not resolve link
# targets against the checkout, Path.exists(), or git ls-files.
if ! cargo package -p assay-cli --list --no-verify --allow-dirty >"$LIST_FILE"; then
  echo "ERROR: cargo package -p assay-cli --list --no-verify --allow-dirty failed" >&2
  exit 1
fi

if ! cargo metadata --format-version 1 --no-deps --offline --manifest-path crates/assay-cli/Cargo.toml >"$META_FILE"; then
  echo "ERROR: cargo metadata for assay-cli failed" >&2
  exit 1
fi

echo "assay-cli cargo package --list:"
cat "$LIST_FILE"

python3 -u - "$ROOT" "$LIST_FILE" "$META_FILE" <<'PY'
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
LIST_PATH = Path(sys.argv[2])
META_PATH = Path(sys.argv[3])

ADR042_SENTENCES = (
    "Assay ships no single safety score and never claims more than it can prove.",
    "A deny is fail-closed caution, not a verdict on intent; an allow is the decision to forward, never proof the action happened.",
)
FORBIDDEN_PREFIXES = ("docs/", "examples/", "demo/")
MUTABLE_GIT_REF = re.compile(r"/(?:blob|tree)/(?:HEAD|main|master|refs/heads/[^/?#]+)(?:/|$)")
VERSION_PIN = re.compile(r"cargo\s+install\s+assay-cli(?:\s+--version\b|@[0-9])")
MAX_README_BYTES = 1024 * 1024
MAX_LINK_LABEL = 512
MAX_LINK_URL = 2048


def fail(message: str) -> None:
    print(f"assay-cli crate README check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_bounded_utf8(path: Path, label: str) -> str:
    with path.open("rb") as stream:
        data = stream.read(MAX_README_BYTES + 1)
    if len(data) > MAX_README_BYTES:
        fail(f"{label} exceeds {MAX_README_BYTES} bytes")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"{label} is not UTF-8: {error}")


def load_members(path: Path) -> list[str]:
    members = [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    if not members:
        fail("empty inventory from cargo package --list")
    return members


def package_section(text: str) -> str:
    match = re.search(r"(?ms)^\[package\]\s*$(.+?)(?=^\[|\Z)", text)
    if not match:
        fail("could not read [package] from crates/assay-cli/Cargo.toml")
    return match.group(1)


def crate_readme_selection(manifest_text: str) -> str:
    section = package_section(manifest_text)
    local = re.search(r'(?m)^\s*readme\s*=\s*"([^"]+)"\s*$', section)
    workspace = re.search(r"(?m)^\s*readme\.workspace\s*=\s*true\s*$", section)
    if workspace and not local:
        fail("not crate-owned README (workspace-README fallback)")
    if not local or local.group(1) != "README.md":
        fail("not crate-owned README (packaged readme must be README.md)")
    return local.group(1)


def metadata_readme() -> str:
    import json

    meta = json.loads(META_PATH.read_text(encoding="utf-8"))
    for package in meta.get("packages", []):
        if package.get("name") == "assay-cli":
            readme = package.get("readme")
            if not readme:
                fail("not crate-owned README (cargo metadata readme is empty)")
            return str(readme)
    fail("missing package assay-cli in cargo metadata")


def _scan_delimited(source: str, start: int, closer: str, ceiling: int) -> int | None:
    found = source.find(closer, start, start + ceiling)
    return None if found == -1 else found


def _html_attr_url(source: str, index: int) -> tuple[str, int] | None:
    prefix = source[index : index + 4].lower()
    name_len = 4 if prefix == "href" else 3 if prefix.startswith("src") else 0
    if name_len == 0:
        return None
    cursor = index + name_len
    length = len(source)
    while cursor < length and source[cursor] in " \t\r\n":
        cursor += 1
    if cursor >= length or source[cursor] != "=":
        return None
    cursor += 1
    while cursor < length and source[cursor] in " \t\r\n":
        cursor += 1
    if cursor >= length or source[cursor] not in "'\"":
        return None
    quote = source[cursor]
    end = _scan_delimited(source, cursor + 1, quote, MAX_LINK_URL)
    if end is None:
        return None
    return source[cursor + 1 : end].strip(), end + 1


def extract_reference_definitions(text: str) -> list[str]:
    found: list[str] = []
    for line in text.splitlines():
        content = line.lstrip(" ")
        if len(line) - len(content) > 3 or not content.startswith("["):
            continue
        label_end = _scan_delimited(content, 1, "]", MAX_LINK_LABEL)
        if label_end is None or content[label_end + 1 : label_end + 2] != ":":
            continue
        cursor = label_end + 2
        while cursor < len(content) and content[cursor] in " \t":
            cursor += 1
        if cursor >= len(content):
            continue
        if content[cursor] == "<":
            end = _scan_delimited(content, cursor + 1, ">", MAX_LINK_URL)
            if end is not None:
                found.append(content[cursor + 1 : end].strip())
            continue
        end = cursor
        ceiling = min(len(content), cursor + MAX_LINK_URL)
        while end < ceiling and content[end] not in " \t\r\n":
            end += 1
        if end > cursor:
            found.append(content[cursor:end].strip())
    return found


def extract_links(text: str) -> list[str]:
    # Bounded single-pass extract for crate README links only.
    # Not Ruley's #2677 archive rewriter: no version argument, no tag rewrite.
    found = extract_reference_definitions(text)
    index = 0
    length = len(text)
    while index < length:
        html = _html_attr_url(text, index)
        if html is not None:
            found.append(html[0])
            index = html[1]
            continue
        bang = text[index] == "!"
        open_at = index + 1 if bang else index
        if open_at < length and text[open_at] == "[":
            label_end = _scan_delimited(text, open_at + 1, "]", MAX_LINK_LABEL)
            if (
                label_end is not None
                and label_end + 1 < length
                and text[label_end + 1] == "("
            ):
                url_end = _scan_delimited(text, label_end + 2, ")", MAX_LINK_URL)
                url = text[label_end + 2 : url_end] if url_end is not None else ""
                if url_end is not None and "[" not in url and "]" not in url:
                    found.append(url.strip())
                    index = url_end + 1
                    continue
        index += 1
    return found


def classify_relative(raw: str) -> str | None:
    target = raw.strip()
    if not target or target.startswith("#"):
        return None
    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
        return None
    if target.startswith("//"):
        return None
    path = target.split("#", 1)[0].split("?", 1)[0]
    if not path or path == ".":
        return None
    if path.startswith("./"):
        path = path[2:]
    return path


def extract_adr042(text: str, source: str) -> list[str]:
    if not text.strip():
        fail(f"ADR-042 parity extractor empty/missing in {source}")
    found = [sentence for sentence in ADR042_SENTENCES if sentence in text]
    if not found:
        fail(f"ADR-042 parity extractor empty/missing in {source}")
    missing = [sentence for sentence in ADR042_SENTENCES if sentence not in text]
    if missing:
        preview = missing[0][:72]
        fail(f"ADR-042 parity extractor empty/missing in {source}: {preview}")
    return found


members = load_members(LIST_PATH)
print(f"package members: {len(members)}")

forbidden = [member for member in members if member.startswith(FORBIDDEN_PREFIXES)]
if forbidden:
    shown = ", ".join(forbidden[:8])
    fail(f"forbidden prefix in cargo package --list: {shown}")

manifest_text = (ROOT / "crates" / "assay-cli" / "Cargo.toml").read_text(encoding="utf-8")
readme_name = crate_readme_selection(manifest_text)
resolved = metadata_readme()
if resolved != readme_name:
    fail(f"not crate-owned README (cargo metadata readme={resolved!r})")
if "README.md" not in members:
    fail("member-list miss: packaged README.md is not in cargo package --list")

readme_text = read_bounded_utf8(ROOT / "crates" / "assay-cli" / readme_name, "crate README")
if not readme_text.strip():
    fail("crate-owned README is empty")

root_readme = read_bounded_utf8(ROOT / "README.md", "workspace README")
crate_sentences = extract_adr042(readme_text, "crate README")
root_sentences = extract_adr042(root_readme, "workspace README")
if crate_sentences != root_sentences:
    fail("ADR-042 sentence parity mismatch between crate README and workspace README")

if VERSION_PIN.search(readme_text):
    fail("version pin in crate README install command")

links = extract_links(readme_text)
for raw in links:
    if MUTABLE_GIT_REF.search(raw):
        fail(f"mutable git ref in {raw}")

packaged_relatives: list[str] = []
for raw in links:
    relative = classify_relative(raw)
    if relative is None:
        continue
    if relative not in members:
        fail(f"member-list miss: relative target {relative!r} is not in cargo package --list")
    packaged_relatives.append(relative)

positive = [path for path in packaged_relatives if path != "LICENSE" and not path.startswith("#")]
if not positive:
    fail("member-list miss: crate README has no relative link to a packaged member")

print("assay-cli crate-owned README OK")
print(f"readme={readme_name} relatives={positive}")
PY
