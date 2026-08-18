#!/usr/bin/env python3
"""Validate and atomically extract a bounded release tar archive."""

from __future__ import annotations

import pathlib
import shutil
import tarfile


class ArchiveRejected(ValueError):
    pass


def extract_archive(
    archive: pathlib.Path,
    destination: pathlib.Path,
    *,
    max_decoded_bytes: int,
    max_members: int = 32,
) -> None:
    if max_decoded_bytes <= 0 or max_members <= 0:
        raise ValueError("archive ceilings must be positive")
    if destination.exists():
        raise ArchiveRejected("archive destination already exists")

    with tarfile.open(archive, "r:gz") as handle:
        members = handle.getmembers()
        if not 1 <= len(members) <= max_members:
            raise ArchiveRejected("archive member-count ceiling exceeded")

        total = 0
        seen: set[str] = set()
        for member in members:
            relative = pathlib.PurePosixPath(member.name)
            if (
                relative.is_absolute()
                or not relative.parts
                or "." in relative.parts
                or ".." in relative.parts
                or "\\" in member.name
                or member.name in seen
                or not (member.isdir() or member.isreg())
            ):
                raise ArchiveRejected(f"unsafe archive member: {member.name!r}")
            seen.add(member.name)
            if member.isreg():
                if member.size < 0 or member.size > max_decoded_bytes:
                    raise ArchiveRejected(f"archive member exceeds size ceiling: {member.name}")
                total += member.size
                if total > max_decoded_bytes:
                    raise ArchiveRejected("archive decoded-size ceiling exceeded")

        scratch = destination.with_name(f".{destination.name}.extracting")
        if scratch.exists():
            raise ArchiveRejected("archive scratch destination already exists")
        scratch.mkdir(parents=True)
        try:
            handle.extractall(path=scratch, members=members, filter="data")
            extracted_size = sum(path.stat().st_size for path in scratch.rglob("*") if path.is_file())
            if extracted_size != total:
                raise ArchiveRejected("archive decoded size changed while extracting")
            scratch.replace(destination)
        except BaseException:
            shutil.rmtree(scratch, ignore_errors=True)
            raise
