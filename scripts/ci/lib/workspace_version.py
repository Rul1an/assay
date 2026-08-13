"""Read the workspace package version without requiring Python 3.11 tomllib."""

from __future__ import annotations

import re
from pathlib import Path


_VERSION = re.compile(r'^version\s*=\s*"([^"]+)"\s*$')


def read_workspace_version(manifest: Path) -> str:
    in_workspace_package = False
    for raw_line in manifest.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line.startswith("["):
            in_workspace_package = line == "[workspace.package]"
            continue
        if in_workspace_package:
            match = _VERSION.fullmatch(line)
            if match:
                return match.group(1)
    raise ValueError(f"missing [workspace.package] version in {manifest}")
