#!/usr/bin/env python3
"""Derive publishable workspace library crates for semver checks."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any


def _read_metadata() -> dict[str, Any]:
    env_override = os.environ.get("ASSAY_SEMVER_METADATA_JSON")
    if env_override is not None:
        env_override = env_override.strip()
        if env_override:
            try:
                # CI contract tests inject fixture metadata as JSON text.
                return json.loads(env_override)
            except json.JSONDecodeError as exc:
                raise SystemExit(
                    "ASSAY_SEMVER_METADATA_JSON must contain valid JSON text."
                ) from exc

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
            return candidate.relative_to(cwd).as_posix()
        except ValueError:
            pass

    normalized = manifest_path.replace("\\", "/")
    marker = "/crates/"
    idx = normalized.find(marker)
    if idx != -1:
        return normalized[idx + 1 :]
    return normalized.removeprefix("./")


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
    metadata = _read_metadata()
    crates = derive_published_library_crates(metadata)
    if not crates:
        raise SystemExit("no publishable workspace library crates found")
    for name, manifest in crates:
        print(f"{name}\t{manifest}")


if __name__ == "__main__":
    main()
