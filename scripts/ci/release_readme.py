#!/usr/bin/env python3
"""Render README claims for the immutable release archive being assembled."""

from __future__ import annotations

import re
import sys
from pathlib import Path

INSTALL_RE = re.compile(r"cargo install assay-cli --version ([0-9]+\.[0-9]+\.[0-9]+) --locked")
RELEASE_RE = re.compile(
    r"Current release: \[`v([0-9]+\.[0-9]+\.[0-9]+)`\]"
    r"\(https://github\.com/Rul1an/assay/releases/tag/v\1\)\."
)
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
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
        print("usage: release_readme.py VERSION", file=sys.stderr)
        return 2
    rendered = render_release_readme(
        (ROOT / "README.md").read_text(encoding="utf-8"), args[0]
    )
    sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
