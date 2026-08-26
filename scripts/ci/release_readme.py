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
ROOT = Path(__file__).resolve().parents[2]


def render_release_readme(source: str, version: str) -> str:
    if VERSION_RE.fullmatch(version) is None:
        raise ValueError("release version must be exact X.Y.Z")
    installs = INSTALL_RE.findall(source)
    if len(installs) != 1:
        raise ValueError("README must carry exactly one release-pinned install command")
    releases = RELEASE_RE.findall(source)
    if len(releases) != 1:
        raise ValueError("README must carry exactly one current-release link")

    rendered = INSTALL_RE.sub(
        f"cargo install assay-cli --version {version} --locked", source, count=1
    )
    rendered = RELEASE_RE.sub(
        f"Current release: [`v{version}`](https://github.com/Rul1an/assay/releases/tag/v{version}).",
        rendered,
        count=1,
    )
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
