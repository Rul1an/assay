#!/usr/bin/env python3
"""Derive publishable workspace library crates for semver checks."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


def _read_metadata(explicit_path: str | None) -> dict[str, Any]:
    metadata_path = explicit_path or None
    if metadata_path is None:
        env_override = os.environ.get("ASSAY_SEMVER_METADATA_JSON", "")
        # CI contract tests set this to inject fixture metadata without
        # modifying the workflow code path.
        if env_override:
            metadata_path = env_override

    if metadata_path:
        with open(metadata_path, "r", encoding="utf-8") as fh:
            return json.load(fh)

    result = subprocess.run(
        ["cargo", "metadata", "--format-version=1", "--no-deps"],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def _manifest_relpath(manifest_path: str) -> str:
    candidate = Path(manifest_path)
    cwd = Path.cwd().resolve()
    if candidate.is_absolute():
        try:
            return candidate.resolve().relative_to(cwd).as_posix()
        except ValueError:
            pass

    normalized = manifest_path.replace("\\", "/")
    marker = "/crates/"
    idx = normalized.find(marker)
    if idx != -1:
        return normalized[idx + 1 :]
    return normalized.lstrip("./")


def _has_lib_target(package: dict[str, Any]) -> bool:
    for target in package.get("targets", []):
        kind = target.get("kind", [])
        if "lib" in kind:
            return True
    return False


def derive_published_library_crates(metadata: dict[str, Any]) -> list[tuple[str, str]]:
    workspace_members = set(metadata.get("workspace_members", []))
    derived: list[tuple[str, str]] = []

    for package in metadata.get("packages", []):
        if package.get("id") not in workspace_members:
            continue
        if package.get("source") is not None:
            continue
        if package.get("publish", ["default"]) == []:
            continue
        if not _has_lib_target(package):
            continue

        name = package.get("name")
        manifest_path = package.get("manifest_path")
        if not isinstance(name, str) or not isinstance(manifest_path, str):
            raise SystemExit("metadata package missing name or manifest_path")
        derived.append((name, _manifest_relpath(manifest_path)))

    return sorted(derived, key=lambda row: row[0])


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--metadata-json",
        help="Read metadata from this JSON file instead of cargo metadata",
    )
    args = parser.parse_args()

    metadata = _read_metadata(args.metadata_json)
    crates = derive_published_library_crates(metadata)
    if not crates:
        raise SystemExit("no publishable workspace library crates found")
    for name, manifest in crates:
        print(f"{name}\t{manifest}")


if __name__ == "__main__":
    main()
