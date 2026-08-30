#!/usr/bin/env python3
"""Render README claims for the immutable release archive being assembled."""

from __future__ import annotations

import re
import sys
from pathlib import Path

VERSION_PATTERN = r"[0-9]+\.[0-9]+\.[0-9]+(?:-(?:rc|beta)\.[0-9]+)?"
INSTALL_RE = re.compile(
    rf"cargo install assay-cli --version ({VERSION_PATTERN}) --locked"
)
RELEASE_RE = re.compile(
    rf"Current release: \[`v({VERSION_PATTERN})`\]"
    r"\(https://github\.com/Rul1an/assay/releases/tag/v\1\)\."
)
VERSION_RE = re.compile(rf"^{VERSION_PATTERN}$")
TAG_RE = re.compile(rf"^v({VERSION_PATTERN})$")
CHECKOUT_RE = re.compile(
    r"For v5\.5\.1, run the last command from a source checkout or an extracted published CLI archive\.\s+"
    r"The installer is binary-only and does not carry the bounded\s+"
    r"quickstart assets\."
)
FIRST_PARTY_GITHUB_RE = re.compile(
    r"^https://github\.com/Rul1an/assay/(blob|tree|raw)/([^/]+)(?:/(.*))?$"
)
FIRST_PARTY_RAW_RE = re.compile(
    r"^https://raw\.githubusercontent\.com/Rul1an/assay/([^/]+)(?:/(.*))?$"
)
MUTABLE_FIRST_PARTY_RE = re.compile(
    r"https://(?:github\.com/Rul1an/assay/(?:blob|tree|raw)|"
    r"raw\.githubusercontent\.com/Rul1an/assay)/(?:HEAD|main|master)(?:/|$)"
)
MAX_LINK_LABEL = 512
MAX_LINK_URL = 2048
OVERLONG_PROBE = 4096
MUTABLE_REFS = frozenset({"HEAD", "main", "master"})
FIRST_PARTY_REPO = "Rul1an/assay"
PACKED_SOURCE_PATHS = (
    "LICENSE",
    "examples/mcp-quickstart/policy.yaml",
    "examples/mcp-quickstart/run.py",
    "examples/mcp-quickstart/mock_server.py",
)
ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_QUICKSTART = (
    "From the root of this extracted CLI archive, with `assay` on PATH "
    "(this archive's binary directory), run `python3 examples/mcp-quickstart/run.py`. "
    "This archive packs LICENSE plus examples/mcp-quickstart/policy.yaml, "
    "examples/mcp-quickstart/run.py, and examples/mcp-quickstart/mock_server.py."
)


def immutable_ref(version: str) -> str:
    """Tag-bound ref for first-party URLs. Shared policy for #2676."""
    return f"v{version}"


def expand_archive_members(paths: set[str] | frozenset[str] | tuple[str, ...]) -> frozenset[str]:
    members: set[str] = set()
    for raw in paths:
        path = raw.replace("\\", "/").strip()
        if not path or path == ".":
            continue
        members.add(path)
        trimmed = path.strip("/")
        members.add(trimmed)
        members.add(trimmed + "/")
        acc: list[str] = []
        for part in trimmed.split("/"):
            acc.append(part)
            joined = "/".join(acc)
            members.add(joined)
            members.add(joined + "/")
    return frozenset(members)


def default_packed_members() -> frozenset[str]:
    return expand_archive_members(PACKED_SOURCE_PATHS)


def list_archive_members(root: Path) -> frozenset[str]:
    collected: set[str] = set()
    resolved = root.resolve()
    for path in resolved.rglob("*"):
        relative = path.relative_to(resolved).as_posix()
        if relative == "README.md":
            continue
        collected.add(relative)
    return expand_archive_members(collected)


def peel_dot_slash(path: str) -> str:
    while path.startswith("./"):
        path = path[2:]
    return path


def normalize_repo_path(path: str) -> str:
    path = path.replace("\\", "/")
    path, _, fragment = path.partition("#")
    path = peel_dot_slash(path)
    if path.startswith("/") or path == ".." or path.startswith("../") or "/../" in path:
        raise ValueError("path traversal is unclassifiable")
    trimmed = path.rstrip("/")
    if not trimmed:
        raise ValueError("path traversal is unclassifiable")
    parts = trimmed.split("/")
    if any(part in {"", ".", "..", "..."} for part in parts):
        raise ValueError("path traversal is unclassifiable")
    if parts[0] == ".github":
        raise ValueError("unclassifiable repository path")
    return f"{path}#{fragment}" if fragment else path


def _member_keys(path: str) -> set[str]:
    file_path = path.split("#", 1)[0]
    return {file_path, file_path.rstrip("/"), file_path if file_path.endswith("/") else file_path + "/"}


def _github_url(path: str, version: str, *, media: bool) -> str:
    file_path, _, fragment = path.partition("#")
    tag = immutable_ref(version)
    if media:
        rewritten = f"https://raw.githubusercontent.com/{FIRST_PARTY_REPO}/{tag}/{file_path}"
    else:
        last = file_path.rstrip("/").rsplit("/", 1)[-1]
        kind = "tree" if file_path.endswith("/") or "." not in last else "blob"
        rewritten = f"https://github.com/{FIRST_PARTY_REPO}/{kind}/{tag}/{file_path}"
    return f"{rewritten}#{fragment}" if fragment else rewritten


def rewrite_first_party_url(
    url: str, version: str, members: frozenset[str], *, media: bool
) -> str | None:
    """Rewrite repo-owned mutable HEAD/main/master refs. None if not first-party mutable."""
    github = FIRST_PARTY_GITHUB_RE.fullmatch(url)
    raw = FIRST_PARTY_RAW_RE.fullmatch(url)
    if github is None and raw is None:
        return None
    if github is not None:
        kind, ref, path = github.group(1), github.group(2), github.group(3) or ""
    else:
        kind, ref, path = "raw", raw.group(1), raw.group(2) or ""
    if ref not in MUTABLE_REFS:
        return None
    path = normalize_repo_path(path) if path else path
    file_path = path.split("#", 1)[0]
    if file_path and _member_keys(file_path) & members and not media:
        return path
    if kind == "raw" and not media:
        return _github_url(file_path, version, media=False) if file_path else url
    return _github_url(file_path, version, media=media) if file_path else url


def _rewrite_target(
    url: str, version: str, members: frozenset[str], *, media: bool
) -> str:
    url = url.strip()
    if url.startswith(("#", "mailto:")):
        return url
    first_party = rewrite_first_party_url(url, version, members, media=media)
    if first_party is not None:
        return first_party
    if url.startswith(("http://", "https://", "//")):
        return url
    path = normalize_repo_path(url)
    file_path = path.split("#", 1)[0]
    if _member_keys(file_path) & members:
        if media:
            return _github_url(file_path, version, media=True)
        return path
    if file_path.startswith("examples/mcp-quickstart"):
        raise ValueError("not an assembled archive member")
    return _github_url(path, version, media=media)


def _scan_delimited(source: str, start: int, closer: str, ceiling: int) -> int | None:
    found = source.find(closer, start, start + ceiling)
    return None if found == -1 else found


def _rewrite_repo_links(source: str, version: str, members: frozenset[str]) -> str:
    out: list[str] = []
    index = 0
    length = len(source)
    html_forms = (
        ('href="', '"', False),
        ("href='", "'", False),
        ('src="', '"', True),
        ("src='", "'", True),
    )
    while index < length:
        handled = False
        for prefix, quote, media in html_forms:
            if source.startswith(prefix, index):
                url_start = index + len(prefix)
                url_end = _scan_delimited(source, url_start, quote, MAX_LINK_URL)
                if url_end is None:
                    raise ValueError("unclassifiable html attribute")
                out.append(prefix)
                out.append(_rewrite_target(source[url_start:url_end], version, members, media=media))
                out.append(quote)
                index = url_end + 1
                handled = True
                break
        if handled:
            continue
        bang = source[index] == "!"
        open_at = index + 1 if bang else index
        if open_at < length and source[open_at] == "[":
            label_end = _scan_delimited(source, open_at + 1, "]", MAX_LINK_LABEL)
            if label_end is None:
                later = source.find("]", open_at + 1, open_at + 1 + OVERLONG_PROBE)
                if (
                    later != -1
                    and later + 1 < length
                    and source[later + 1] == "("
                    and "[" not in source[open_at + 1 : later]
                ):
                    raise ValueError("unclassifiable markdown link label")
            else:
                nxt = source[label_end + 1] if label_end + 1 < length else ""
                if nxt == "[":
                    raise ValueError("unclassifiable reference-style link")
                if nxt == ":" and (open_at == 0 or source[open_at - 1] == "\n"):
                    raise ValueError("unclassifiable reference-style link")
                if nxt == "(":
                    url_end = _scan_delimited(source, label_end + 2, ")", MAX_LINK_URL)
                    url = source[label_end + 2 : url_end] if url_end is not None else ""
                    if url_end is not None and "[" not in url and "]" not in url:
                        if bang:
                            out.append("!")
                        out.append("[")
                        out.append(source[open_at + 1 : label_end])
                        out.append("](")
                        out.append(
                            _rewrite_target(url, version, members, media=bang)
                        )
                        out.append(")")
                        index = url_end + 1
                        continue
        out.append(source[index])
        index += 1
    return "".join(out)


def render_release_readme(
    source: str, version: str, members: frozenset[str] | None = None
) -> str:
    if VERSION_RE.fullmatch(version) is None:
        raise ValueError("release version must be exact X.Y.Z")
    assembled = default_packed_members() if members is None else frozenset(members)
    installs = INSTALL_RE.findall(source)
    if len(installs) != 1:
        raise ValueError("README must carry exactly one release-pinned install command")
    releases = RELEASE_RE.findall(source)
    if len(releases) != 1:
        raise ValueError("README must carry exactly one current-release link")
    checkouts = CHECKOUT_RE.findall(source)
    if len(checkouts) != 1:
        raise ValueError("README must carry exactly one published-checkout sentence")

    rendered = INSTALL_RE.sub(
        f"cargo install assay-cli --version {version} --locked", source, count=1
    )
    rendered = RELEASE_RE.sub(
        f"Current release: [`v{version}`](https://github.com/Rul1an/assay/releases/tag/v{version}).",
        rendered,
        count=1,
    )
    rendered = CHECKOUT_RE.sub(ARCHIVE_QUICKSTART, rendered, count=1)
    rendered = _rewrite_repo_links(rendered, version, assembled)
    if INSTALL_RE.findall(rendered) != [version] or RELEASE_RE.findall(rendered) != [version]:
        raise ValueError("rendered README release claims did not converge")
    if MUTABLE_FIRST_PARTY_RE.search(rendered):
        raise ValueError("first-party mutable HEAD/main/master ref survived")
    return rendered


def emit_utf8(text: str) -> None:
    """Write complete UTF-8 regardless of inherited stdout encoding."""
    buffer = getattr(sys.stdout, "buffer", None)
    if buffer is None:
        sys.stdout.write(text)
        return
    sys.stdout.flush()
    buffer.write(text.encode("utf-8"))
    buffer.flush()


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if len(args) not in {1, 2}:
        print("usage: release_readme.py TAG [--assembled-cwd]", file=sys.stderr)
        return 2
    tag = TAG_RE.fullmatch(args[0])
    if tag is None:
        print(
            "release tag must be vX.Y.Z, vX.Y.Z-rc.N, or vX.Y.Z-beta.N",
            file=sys.stderr,
        )
        return 2
    if len(args) == 2 and args[1] != "--assembled-cwd":
        print("second argument must be --assembled-cwd", file=sys.stderr)
        return 2
    members = (
        list_archive_members(Path.cwd()) if len(args) == 2 else default_packed_members()
    )
    rendered = render_release_readme(
        (ROOT / "README.md").read_text(encoding="utf-8"), tag.group(1), members=members
    )
    emit_utf8(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
