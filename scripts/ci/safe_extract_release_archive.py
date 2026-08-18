#!/usr/bin/env python3
"""Validate and atomically extract a bounded release tar archive."""

from __future__ import annotations

import pathlib
import shutil
import tarfile


class ArchiveRejected(ValueError):
    pass


def _validate_members(
    handle: tarfile.TarFile, *, max_decoded_bytes: int, max_members: int
) -> tuple[int, int]:
    count = 0
    file_count = 0
    total = 0
    seen: set[str] = set()
    for member in handle:
        count += 1
        if count > max_members:
            raise ArchiveRejected("archive member-count ceiling exceeded")
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
            file_count += 1
            if member.size < 0 or member.size > max_decoded_bytes:
                raise ArchiveRejected(f"archive member exceeds size ceiling: {member.name}")
            total += member.size
            if total > max_decoded_bytes:
                raise ArchiveRejected("archive decoded-size ceiling exceeded")
    if count == 0:
        raise ArchiveRejected("archive contains no members")
    return file_count, total


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

    with archive.open("rb") as raw:
        with tarfile.open(fileobj=raw, mode="r|gz") as handle:
            expected_count, expected_size = _validate_members(
                handle,
                max_decoded_bytes=max_decoded_bytes,
                max_members=max_members,
            )
        raw.seek(0)
        scratch = destination.with_name(f".{destination.name}.extracting")
        if scratch.exists():
            raise ArchiveRejected("archive scratch destination already exists")
        scratch.mkdir(parents=True)
        try:
            with tarfile.open(fileobj=raw, mode="r|gz") as handle:
                handle.extractall(path=scratch, members=handle, filter="data")
            extracted_count = sum(1 for path in scratch.rglob("*") if path.is_file())
            extracted_size = sum(path.stat().st_size for path in scratch.rglob("*") if path.is_file())
            if extracted_count != expected_count or extracted_size != expected_size:
                raise ArchiveRejected("archive contents changed while extracting")
            scratch.replace(destination)
        except BaseException:
            shutil.rmtree(scratch, ignore_errors=True)
            raise
