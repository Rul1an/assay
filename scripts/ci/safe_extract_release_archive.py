#!/usr/bin/env python3
"""Validate and atomically extract a bounded release tar or ZIP archive."""

from __future__ import annotations

import pathlib
import shutil
import stat
import struct
import tarfile
import zipfile
import zlib
from dataclasses import dataclass
from typing import BinaryIO


class ArchiveRejected(ValueError):
    pass


_ZIP_EOCD_SIGNATURE = b"PK\x05\x06"
_ZIP64_LOCATOR_SIGNATURE = b"PK\x06\x07"
_ZIP_CENTRAL_SIGNATURE = b"PK\x01\x02"
_ZIP_LOCAL_SIGNATURE = b"PK\x03\x04"
_ZIP_EOCD_MAX_BYTES = 22 + 65535
_ZIP_MAX_NAME_BYTES = 1024
_ZIP_DIRECTORY_BYTES_PER_MEMBER = 46 + _ZIP_MAX_NAME_BYTES
_ZIP_SUPPORTED_METHODS = frozenset((zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED))
_WINDOWS_RESERVED_STEMS = {
    "aux",
    "con",
    "conin$",
    "conout$",
    "nul",
    "prn",
    *(f"com{index}" for index in range(1, 10)),
    "com¹",
    "com²",
    "com³",
    *(f"lpt{index}" for index in range(1, 10)),
    "lpt¹",
    "lpt²",
    "lpt³",
}
_WINDOWS_RESERVED_CHARS = frozenset('<>:"\\|?*') | frozenset(
    chr(value) for value in range(32)
)


@dataclass(frozen=True)
class _ZipMember:
    raw_name: bytes
    name: str
    parts: tuple[str, ...]
    is_dir: bool
    method: int
    crc: int
    compressed_size: int
    decoded_size: int
    local_offset: int


def _validate_raw_zip_name(raw_name: bytes, seen: set[str]) -> tuple[str, tuple[str, ...], bool]:
    if not raw_name or len(raw_name) > _ZIP_MAX_NAME_BYTES:
        raise ArchiveRejected("unsafe ZIP member name")
    try:
        name = raw_name.decode("ascii")
    except UnicodeDecodeError as error:
        raise ArchiveRejected("ZIP member names must be ASCII") from error
    is_dir = name.endswith("/")
    body = name[:-1] if is_dir else name
    parts = tuple(body.split("/"))
    windows_key = "/".join(part.casefold() for part in parts)
    if (
        not parts
        or any(not part or part in (".", "..") for part in parts)
        or any(_WINDOWS_RESERVED_CHARS.intersection(part) for part in parts)
        or any(part.endswith((" ", ".")) for part in parts)
        or any(
            part.split(".", 1)[0].rstrip(" .").casefold()
            in _WINDOWS_RESERVED_STEMS
            for part in parts
        )
        or windows_key in seen
    ):
        raise ArchiveRejected(f"unsafe archive member: {name!r}")
    seen.add(windows_key)
    return name, parts, is_dir


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


def _read_exact(raw: BinaryIO, size: int, description: str) -> bytes:
    value = raw.read(size)
    if len(value) != size:
        raise ArchiveRejected(f"ZIP {description} is truncated")
    return value


def _preflight_zip_directory(
    raw: BinaryIO, *, max_decoded_bytes: int, max_members: int
) -> tuple[list[_ZipMember], int]:
    raw.seek(0, 2)
    size = raw.tell()
    if size < 22:
        raise ArchiveRejected("ZIP end-of-directory record is missing")
    tail_size = min(size, _ZIP_EOCD_MAX_BYTES)
    raw.seek(size - tail_size)
    tail = _read_exact(raw, tail_size, "end-of-directory window")
    eocd_index = tail.rfind(_ZIP_EOCD_SIGNATURE)
    if eocd_index < 0 or eocd_index + 22 != len(tail):
        raise ArchiveRejected("ZIP end-of-directory record is missing or not at EOF")
    eocd_offset = size - tail_size + eocd_index
    (
        _,
        disk_number,
        directory_disk,
        entries_on_disk,
        entry_count,
        directory_size,
        directory_offset,
        comment_size,
    ) = struct.unpack_from("<4s4H2LH", tail, eocd_index)
    if comment_size != 0:
        raise ArchiveRejected("ZIP archive comments are unsupported")
    if (
        entries_on_disk == 0xFFFF
        or entry_count == 0xFFFF
        or directory_size == 0xFFFFFFFF
        or directory_offset == 0xFFFFFFFF
        or (
            eocd_offset >= 20
            and tail[eocd_index - 20 : eocd_index - 16] == _ZIP64_LOCATOR_SIGNATURE
        )
    ):
        raise ArchiveRejected("ZIP64 archives are unsupported")
    if disk_number != 0 or directory_disk != 0 or entries_on_disk != entry_count:
        raise ArchiveRejected("multi-disk ZIP archives are unsupported")
    if entry_count == 0:
        raise ArchiveRejected("archive contains no members")
    if entry_count > max_members:
        raise ArchiveRejected("archive member-count ceiling exceeded")
    if directory_size > max_members * _ZIP_DIRECTORY_BYTES_PER_MEMBER:
        raise ArchiveRejected("ZIP central-directory size ceiling exceeded")
    if directory_offset + directory_size != eocd_offset:
        raise ArchiveRejected("ZIP central-directory bounds are inconsistent")

    raw.seek(directory_offset)
    directory = _read_exact(raw, directory_size, "central directory")
    cursor = 0
    seen: set[str] = set()
    members: list[_ZipMember] = []
    expected_size = 0
    while cursor < len(directory):
        if len(members) >= max_members:
            raise ArchiveRejected("archive member-count ceiling exceeded")
        if cursor + 46 > len(directory):
            raise ArchiveRejected("ZIP central-directory record is truncated")
        (
            signature,
            _,
            version_needed,
            flags,
            method,
            _,
            _,
            crc,
            compressed_size,
            decoded_size,
            name_size,
            extra_size,
            member_comment_size,
            member_disk,
            _,
            external_attr,
            local_offset,
        ) = struct.unpack_from("<4s6H3L5H2L", directory, cursor)
        if signature != _ZIP_CENTRAL_SIGNATURE:
            raise ArchiveRejected("ZIP central-directory record has an invalid signature")
        record_end = cursor + 46 + name_size + extra_size + member_comment_size
        if record_end > len(directory):
            raise ArchiveRejected("ZIP central-directory record is truncated")
        raw_name = directory[cursor + 46 : cursor + 46 + name_size]
        if (
            version_needed != 20
            or flags != 0
            or method not in _ZIP_SUPPORTED_METHODS
            or extra_size != 0
            or member_comment_size != 0
            or member_disk != 0
            or compressed_size == 0xFFFFFFFF
            or decoded_size == 0xFFFFFFFF
            or local_offset == 0xFFFFFFFF
        ):
            raise ArchiveRejected("ZIP member uses an unsupported feature")
        name, parts, is_dir = _validate_raw_zip_name(raw_name, seen)
        mode_type = stat.S_IFMT(external_attr >> 16)
        if mode_type not in (0, stat.S_IFREG, stat.S_IFDIR):
            raise ArchiveRejected(f"unsafe archive member: {name!r}")
        if mode_type == stat.S_IFDIR and not is_dir:
            raise ArchiveRejected(f"unsafe archive member: {name!r}")
        if is_dir:
            if method != zipfile.ZIP_STORED or compressed_size or decoded_size:
                raise ArchiveRejected(f"invalid ZIP directory member: {name!r}")
        else:
            if decoded_size > max_decoded_bytes:
                raise ArchiveRejected(f"archive member exceeds size ceiling: {name}")
            expected_size += decoded_size
            if expected_size > max_decoded_bytes:
                raise ArchiveRejected("archive decoded-size ceiling exceeded")
        if local_offset >= directory_offset:
            raise ArchiveRejected("ZIP local-header offset is outside the data region")
        members.append(
            _ZipMember(
                raw_name=raw_name,
                name=name,
                parts=parts,
                is_dir=is_dir,
                method=method,
                crc=crc,
                compressed_size=compressed_size,
                decoded_size=decoded_size,
                local_offset=local_offset,
            )
        )
        cursor = record_end
    if cursor != len(directory) or len(members) != entry_count:
        raise ArchiveRejected("ZIP central-directory member count is inconsistent")
    keys = [(tuple(part.casefold() for part in member.parts), member.is_dir) for member in members]
    for key, is_dir in keys:
        if not is_dir and any(
            len(other) > len(key) and other[: len(key)] == key for other, _ in keys
        ):
            raise ArchiveRejected("ZIP file member conflicts with a child path")

    next_offset = 0
    for member in sorted(members, key=lambda item: item.local_offset):
        if member.local_offset != next_offset:
            raise ArchiveRejected("ZIP local records contain a gap or overlap")
        raw.seek(member.local_offset)
        local = _read_exact(raw, 30, "local header")
        (
            signature,
            version_needed,
            flags,
            method,
            _,
            _,
            crc,
            compressed_size,
            decoded_size,
            name_size,
            extra_size,
        ) = struct.unpack("<4s5H3L2H", local)
        raw_name = _read_exact(raw, name_size, "local member name")
        if (
            signature != _ZIP_LOCAL_SIGNATURE
            or version_needed != 20
            or flags != 0
            or method != member.method
            or crc != member.crc
            or compressed_size != member.compressed_size
            or decoded_size != member.decoded_size
            or extra_size != 0
            or raw_name != member.raw_name
        ):
            raise ArchiveRejected("ZIP local header differs from central directory")
        next_offset = member.local_offset + 30 + name_size + member.compressed_size
        if next_offset > directory_offset:
            raise ArchiveRejected("ZIP local record overlaps the central directory")
    if next_offset != directory_offset:
        raise ArchiveRejected("ZIP local records do not end at the central directory")
    return members, expected_size


def _extract_zip_archive(
    archive: pathlib.Path,
    destination: pathlib.Path,
    *,
    max_decoded_bytes: int,
    max_members: int,
) -> None:
    scratch = destination.with_name(f".{destination.name}.extracting")
    if scratch.exists():
        raise ArchiveRejected("archive scratch destination already exists")
    with archive.open("rb") as raw:
        members, expected_size = _preflight_zip_directory(
            raw,
            max_decoded_bytes=max_decoded_bytes,
            max_members=max_members,
        )
        raw.seek(0)
        handle: zipfile.ZipFile | None = None
        try:
            handle = zipfile.ZipFile(raw)
            infos = handle.infolist()
        except (zipfile.BadZipFile, NotImplementedError, RuntimeError) as error:
            if handle is not None:
                handle.close()
            raise ArchiveRejected("ZIP decoder rejected the validated archive") from error
        with handle:
            if len(infos) != len(members):
                raise ArchiveRejected("ZIP decoder observed a different member count")
            for info, member in zip(infos, members, strict=True):
                if (
                    info.filename != member.name
                    or info.header_offset != member.local_offset
                    or info.flag_bits != 0
                    or info.compress_type != member.method
                    or info.CRC != member.crc
                    or info.compress_size != member.compressed_size
                    or info.file_size != member.decoded_size
                ):
                    raise ArchiveRejected("ZIP decoder metadata differs from raw records")

            scratch.mkdir(parents=True)
            actual_size = 0
            try:
                for info, member in zip(infos, members, strict=True):
                    output = scratch.joinpath(*member.parts)
                    if member.is_dir:
                        output.mkdir(parents=True, exist_ok=True)
                        continue
                    output.parent.mkdir(parents=True, exist_ok=True)
                    member_size = 0
                    with handle.open(info) as source, output.open("xb") as target:
                        while chunk := source.read(1024 * 1024):
                            member_size += len(chunk)
                            actual_size += len(chunk)
                            if (
                                member_size > member.decoded_size
                                or actual_size > max_decoded_bytes
                            ):
                                raise ArchiveRejected(
                                    "archive decoded-size ceiling exceeded"
                                )
                            target.write(chunk)
                    if member_size != member.decoded_size:
                        raise ArchiveRejected(
                            "archive member size differs from its declaration"
                        )
                if actual_size != expected_size:
                    raise ArchiveRejected("archive contents changed while extracting")
                scratch.replace(destination)
            except BaseException as error:
                shutil.rmtree(scratch, ignore_errors=True)
                if isinstance(
                    error,
                    (
                        zipfile.BadZipFile,
                        NotImplementedError,
                        RuntimeError,
                        EOFError,
                        zlib.error,
                    ),
                ):
                    raise ArchiveRejected("ZIP decoder rejected member data") from error
                raise


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

    if archive.name.endswith(".zip"):
        _extract_zip_archive(
            archive,
            destination,
            max_decoded_bytes=max_decoded_bytes,
            max_members=max_members,
        )
        return

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
