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

PACKAGE_DIR="$(mktemp -d)"
cleanup() {
  find "$PACKAGE_DIR" -depth -delete
}
trap cleanup EXIT

# Build the publish artifact without compiling it. The packaged manifest and
# README are authoritative; never substitute checkout metadata or paths.
if ! CARGO_TARGET_DIR="$PACKAGE_DIR" cargo package -p assay-cli --no-verify --allow-dirty >/dev/null; then
  echo "ERROR: cargo package -p assay-cli --no-verify --allow-dirty failed" >&2
  exit 1
fi

shopt -s nullglob
archives=("$PACKAGE_DIR"/package/assay-cli-*.crate)
if [[ "${#archives[@]}" -ne 1 ]]; then
  echo "ERROR: expected one built assay-cli .crate, found ${#archives[@]}" >&2
  exit 1
fi

python3 -u - "$ROOT" "${archives[0]}" <<'PY'
from __future__ import annotations

import gzip
import re
import shlex
import sys
import tarfile
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(sys.argv[1])
ARCHIVE_PATH = Path(sys.argv[2])

ADR042_SENTENCES = (
    "Assay ships no single safety score and never claims more than it can prove.",
    "A deny is fail-closed caution, not a verdict on intent; an allow is the decision to forward, never proof the action happened.",
)
FORBIDDEN_PREFIXES = ("docs/", "examples/", "demo/")
MAX_README_BYTES = 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_CRATE_BYTES = 64 * 1024 * 1024
MAX_DECOMPRESSED_STREAM_BYTES = 16 * 1024 * 1024
MAX_ARCHIVE_UNCOMPRESSED_BYTES = 64 * 1024 * 1024
MAX_ARCHIVE_MEMBERS = 4096
MAX_MEMBER_NAME = 4096
MAX_LINK_LABEL = 512
MAX_LINK_URL = 2048


def fail(message: str) -> None:
    print(f"assay-cli crate README check failed: {message}", file=sys.stderr)
    raise SystemExit(1)


class BoundedReader:
    def __init__(self, stream: gzip.GzipFile, ceiling: int) -> None:
        self.stream = stream
        self.ceiling = ceiling
        self.consumed = 0

    def read(self, size: int = -1) -> bytes:
        remaining = self.ceiling - self.consumed
        requested = remaining + 1 if size < 0 else min(size, remaining + 1)
        data = self.stream.read(requested)
        self.consumed += len(data)
        if self.consumed > self.ceiling:
            fail(f"packaged crate decompressed stream exceeds {self.ceiling} bytes")
        return data


def decode_bounded_utf8(data: bytes, ceiling: int, label: str) -> str:
    if len(data) > ceiling:
        fail(f"{label} exceeds {ceiling} bytes")
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"{label} is not UTF-8: {error}")


def read_bounded_utf8(path: Path, label: str) -> str:
    with path.open("rb") as stream:
        data = stream.read(MAX_README_BYTES + 1)
    return decode_bounded_utf8(data, MAX_README_BYTES, label)


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


def archive_member_bytes(
    archive: tarfile.TarFile, member: tarfile.TarInfo, ceiling: int, label: str
) -> bytes:
    if not member.isfile():
        fail(f"{label} is not a regular file in packaged crate")
    if member.size > ceiling:
        fail(f"{label} exceeds {ceiling} bytes")
    stream = archive.extractfile(member)
    if stream is None:
        fail(f"could not read {label} from packaged crate")
    data = stream.read(ceiling + 1)
    if len(data) > ceiling:
        fail(f"{label} exceeds {ceiling} bytes")
    return data


def load_packaged_crate(path: Path) -> tuple[list[str], str, str, str]:
    if path.stat().st_size > MAX_CRATE_BYTES:
        fail(f"built .crate exceeds {MAX_CRATE_BYTES} bytes")
    with path.open("rb") as compressed:
        decompressed = gzip.GzipFile(fileobj=compressed)
        bounded = BoundedReader(decompressed, MAX_DECOMPRESSED_STREAM_BYTES)
        archive = tarfile.open(fileobj=bounded, mode="r|")
        roots: set[str] = set()
        members: set[str] = set()
        selected: dict[str, bytes] = {}
        member_count = 0
        uncompressed_bytes = 0
        with archive:
            for member in archive:
                member_count += 1
                if member_count > MAX_ARCHIVE_MEMBERS:
                    fail(f"packaged crate has more than {MAX_ARCHIVE_MEMBERS} members")
                if len(member.name) > MAX_MEMBER_NAME:
                    fail("packaged crate member name exceeds ceiling")
                if member.size < 0:
                    fail("packaged crate member has a negative size")
                uncompressed_bytes += member.size
                if uncompressed_bytes > MAX_ARCHIVE_UNCOMPRESSED_BYTES:
                    fail(
                        "packaged crate declared uncompressed size exceeds "
                        f"{MAX_ARCHIVE_UNCOMPRESSED_BYTES} bytes"
                    )
                parts = Path(member.name).parts
                if not parts or member.name.startswith("/") or ".." in parts:
                    fail(f"unsafe packaged crate member: {member.name!r}")
                roots.add(parts[0])
                if len(parts) == 1 or not member.isfile():
                    continue
                relative = "/".join(parts[1:])
                if relative in members:
                    fail(f"duplicate packaged crate member: {relative}")
                members.add(relative)
                if relative == "Cargo.toml.orig":
                    selected[relative] = archive_member_bytes(
                        archive, member, MAX_MANIFEST_BYTES, "packaged Cargo.toml.orig"
                    )
                elif relative == "README.md":
                    selected[relative] = archive_member_bytes(
                        archive, member, MAX_README_BYTES, "packaged README"
                    )
        if member_count == 0:
            fail("packaged crate has no members")
        if len(roots) != 1:
            fail(f"packaged crate must have one root, found {sorted(roots)!r}")
        manifest_bytes = selected.get("Cargo.toml.orig")
        if manifest_bytes is None:
            fail("packaged crate is missing Cargo.toml.orig")
        manifest_text = decode_bounded_utf8(
            manifest_bytes,
            MAX_MANIFEST_BYTES,
            "packaged Cargo.toml.orig",
        )
        readme_name = crate_readme_selection(manifest_text)
        readme_bytes = selected.get(readme_name)
        if readme_bytes is None:
            fail(f"member-list miss: packaged {readme_name} is absent")
        readme_text = decode_bounded_utf8(
            readme_bytes,
            MAX_README_BYTES,
            "packaged README",
        )
        return sorted(members), manifest_text, readme_name, readme_text


def reject_mutable_github_content_link(raw: str) -> None:
    parsed = urlsplit(raw)
    hostname = (parsed.hostname or "").lower().rstrip(".")
    if hostname not in ("github.com", "www.github.com"):
        return
    parts = parsed.path.split("/")
    if len(parts) < 4 or not parts[1] or not parts[2]:
        return
    if parts[3] not in ("blob", "tree"):
        return
    if (
        parsed.scheme != "https"
        or parsed.username is not None
        or parsed.password is not None
        or parsed.port is not None
        or len(parts) < 5
        or re.fullmatch(r"[0-9a-fA-F]{40}", parts[4]) is None
    ):
        fail(f"mutable git ref in {raw}")


def _scan_delimited(source: str, start: int, closer: str, ceiling: int) -> int | None:
    found = source.find(closer, start, start + ceiling)
    return None if found == -1 else found


def reject_unsupported_link_syntax(text: str) -> None:
    # This crate README intentionally supports one auditable profile: plain
    # inline Markdown links/images. Unsupported CommonMark/HTML forms fail
    # closed instead of becoming invisible to the membership check.
    if "\\" in text:
        fail("unsupported link syntax: backslash escapes are not allowed")
    if "<" in text or ">" in text:
        fail("unsupported link syntax: HTML and autolinks are not allowed")
    for line in text.splitlines():
        content = line.lstrip(" ")
        if len(line) - len(content) > 3 or not content.startswith("["):
            continue
        label_end = _scan_delimited(content, 1, "]", MAX_LINK_LABEL)
        if label_end is not None and content[label_end + 1 : label_end + 2] == ":":
            fail("unsupported link syntax: reference definitions are not allowed")


def extract_links(text: str) -> list[str]:
    # Bounded single-pass extract for the crate README's strict link profile.
    # Not Ruley's #2677 archive rewriter: no version argument, no tag rewrite.
    reject_unsupported_link_syntax(text)
    found: list[str] = []
    index = 0
    length = len(text)
    while index < length:
        bang = text[index] == "!"
        open_at = index + 1 if bang else index
        if open_at < length and text[open_at] == "[":
            label_end = _scan_delimited(text, open_at + 1, "]", MAX_LINK_LABEL)
            if label_end is None:
                fail("unsupported link syntax: label exceeds ceiling or lacks a closing bracket")
            nested = text.find("[", open_at + 1, label_end if label_end is not None else length)
            if nested != -1:
                fail("unsupported link syntax: nested labels are not allowed")
            if label_end is not None and text[label_end + 1 : label_end + 2] == "[":
                fail("unsupported link syntax: reference links are not allowed")
            if (
                label_end is not None
                and label_end + 1 < length
                and text[label_end + 1] == "("
            ):
                url_end = _scan_delimited(text, label_end + 2, ")", MAX_LINK_URL)
                url = text[label_end + 2 : url_end] if url_end is not None else ""
                if url_end is None:
                    fail("unsupported link syntax: URL exceeds ceiling or lacks a closing parenthesis")
                if any(char in url for char in "[]()&%") or any(char.isspace() for char in url):
                    fail("unsupported link syntax: URL escapes, nesting, and titles are not allowed")
                if "[" not in url and "]" not in url:
                    found.append(url.strip())
                    index = url_end + 1
                    continue
        index += 1
    return found


def has_version_pinned_install(text: str) -> bool:
    for line in text.splitlines():
        if "cargo" not in line or "install" not in line or "assay-cli" not in line:
            continue
        try:
            words = shlex.split(line)
        except ValueError as error:
            fail(f"could not parse assay-cli install command: {error}")
        if not words or words[0] != "cargo" or "install" not in words:
            continue
        tail = words[words.index("install") + 1 :]
        package_tokens = [word for word in tail if word == "assay-cli" or word.startswith("assay-cli@")]
        if not package_tokens:
            continue
        if any(word.startswith("assay-cli@") for word in package_tokens):
            return True
        if any(word == "--version" or word.startswith("--version=") for word in tail):
            return True
    return False


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


members, manifest_text, readme_name, readme_text = load_packaged_crate(ARCHIVE_PATH)
print("assay-cli built .crate members:")
print("\n".join(members))
print(f"package members: {len(members)}")

forbidden = [member for member in members if member.startswith(FORBIDDEN_PREFIXES)]
if forbidden:
    shown = ", ".join(forbidden[:8])
    fail(f"forbidden prefix in cargo package --list: {shown}")

if "README.md" not in members:
    fail("member-list miss: packaged README.md is not in built .crate")
if not readme_text.strip():
    fail("crate-owned README is empty")

root_readme = read_bounded_utf8(ROOT / "README.md", "workspace README")
crate_sentences = extract_adr042(readme_text, "crate README")
root_sentences = extract_adr042(root_readme, "workspace README")
if crate_sentences != root_sentences:
    fail("ADR-042 sentence parity mismatch between crate README and workspace README")

if has_version_pinned_install(readme_text):
    fail("version pin in crate README install command")

links = extract_links(readme_text)
for raw in links:
    reject_mutable_github_content_link(raw)

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
