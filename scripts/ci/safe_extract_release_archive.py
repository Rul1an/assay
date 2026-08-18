#!/usr/bin/env python3
"""Validate and atomically extract a bounded release tar archive."""

from __future__ import annotations

import argparse
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
            for member in members:
                target = scratch.joinpath(*pathlib.PurePosixPath(member.name).parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                source = handle.extractfile(member)
                if source is None:
                    raise ArchiveRejected(f"archive member could not be read: {member.name}")
                with source, target.open("wb") as output:
                    shutil.copyfileobj(source, output, length=1024 * 1024)
                if target.stat().st_size != member.size:
                    raise ArchiveRejected(f"archive member size changed while extracting: {member.name}")
                target.chmod(member.mode & 0o777)
            scratch.replace(destination)
        except BaseException:
            shutil.rmtree(scratch, ignore_errors=True)
            raise


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    parser.add_argument("--max-decoded-bytes", type=int, required=True)
    parser.add_argument("--max-members", type=int, default=32)
    args = parser.parse_args()
    try:
        extract_archive(
            args.archive,
            args.destination,
            max_decoded_bytes=args.max_decoded_bytes,
            max_members=args.max_members,
        )
    except (ArchiveRejected, OSError, tarfile.TarError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
