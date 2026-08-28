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
    r"For v5\.4\.0, run the last command from a source checkout\.\s+"
    r"The installer and the\s+published v5\.4\.0 CLI archive install the binary "
    r"but do not carry the bounded\s+quickstart assets\."
)
MARKDOWN_LINK_RE = re.compile(r"(!?\[[^\]]*\]\()([^)]+)(\))")
HREF_RE = re.compile(r'(href=")([^"]+)(")')
ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_QUICKSTART = (
    "From the root of this extracted CLI archive, with `assay` on PATH "
    "(this archive's binary directory), run `python3 examples/mcp-quickstart/run.py`. "
    "This archive packs LICENSE plus examples/mcp-quickstart/policy.yaml, "
    "examples/mcp-quickstart/run.py, and examples/mcp-quickstart/mock_server.py."
)


def _is_external_or_fragment(url: str) -> bool:
    return url.startswith(("http://", "https://", "mailto:", "#", "//"))


def _is_archive_relative(url: str) -> bool:
    path = url.split("#", 1)[0]
    if path == "LICENSE":
        return True
    return path == "examples/mcp-quickstart" or path.startswith(
        "examples/mcp-quickstart/"
    )


def _github_url(url: str, version: str) -> str:
    path, _, fragment = url.partition("#")
    path = path.lstrip("./")
    last = path.rstrip("/").rsplit("/", 1)[-1]
    kind = "tree" if path.endswith("/") or "." not in last else "blob"
    rewritten = f"https://github.com/Rul1an/assay/{kind}/v{version}/{path}"
    return f"{rewritten}#{fragment}" if fragment else rewritten


def _rewrite_target(url: str, version: str) -> str:
    if _is_external_or_fragment(url) or _is_archive_relative(url):
        return url
    return _github_url(url, version)


def _rewrite_repo_links(source: str, version: str) -> str:
    rewritten = MARKDOWN_LINK_RE.sub(
        lambda match: match.group(1)
        + _rewrite_target(match.group(2), version)
        + match.group(3),
        source,
    )
    return HREF_RE.sub(
        lambda match: match.group(1)
        + _rewrite_target(match.group(2), version)
        + match.group(3),
        rewritten,
    )


def render_release_readme(source: str, version: str) -> str:
    if VERSION_RE.fullmatch(version) is None:
        raise ValueError("release version must be exact X.Y.Z")
    installs = INSTALL_RE.findall(source)
    if len(installs) != 1:
        raise ValueError("README must carry exactly one release-pinned install command")
    releases = RELEASE_RE.findall(source)
    if len(releases) != 1:
        raise ValueError("README must carry exactly one current-release link")
    checkouts = CHECKOUT_RE.findall(source)
    if len(checkouts) > 1:
        raise ValueError("README must carry exactly one published-checkout sentence")

    rendered = INSTALL_RE.sub(
        f"cargo install assay-cli --version {version} --locked", source, count=1
    )
    rendered = RELEASE_RE.sub(
        f"Current release: [`v{version}`](https://github.com/Rul1an/assay/releases/tag/v{version}).",
        rendered,
        count=1,
    )
    if checkouts:
        rendered = CHECKOUT_RE.sub(ARCHIVE_QUICKSTART, rendered, count=1)
    rendered = _rewrite_repo_links(rendered, version)
    if INSTALL_RE.findall(rendered) != [version] or RELEASE_RE.findall(rendered) != [version]:
        raise ValueError("rendered README release claims did not converge")
    return rendered


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 1:
        print("usage: release_readme.py TAG", file=sys.stderr)
        return 2
    tag = TAG_RE.fullmatch(args[0])
    if tag is None:
        print(
            "release tag must be vX.Y.Z, vX.Y.Z-rc.N, or vX.Y.Z-beta.N",
            file=sys.stderr,
        )
        return 2
    rendered = render_release_readme(
        (ROOT / "README.md").read_text(encoding="utf-8"), tag.group(1)
    )
    sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
