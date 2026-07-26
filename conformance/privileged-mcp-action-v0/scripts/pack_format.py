#!/usr/bin/env python3
"""Deterministic clean-room rendering helpers shared by the pack builder and scorer."""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import tarfile
from typing import Any, BinaryIO, Iterable

MAX_SOURCE_BUNDLE_BYTES = 16 * 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_EVENTS_BYTES = 8 * 1024 * 1024
EXPECTED_BUNDLE_MEMBERS = ("manifest.json", "events.ndjson")
MAX_SOURCE_ARCHIVE_BYTES = 12 * 1024 * 1024
MAX_MEMBER_NAME_BYTES = 512


class BoundedReader:
    """Limit bytes returned by a binary stream, including one overflow probe."""

    def __init__(self, source: BinaryIO, limit: int) -> None:
        self.source = source
        self.limit = limit
        self.consumed = 0

    def read(self, size: int = -1) -> bytes:
        remaining_with_probe = self.limit + 1 - self.consumed
        if remaining_with_probe <= 0:
            raise ValueError(f"decoded stream exceeds {self.limit} bytes")
        requested = remaining_with_probe if size < 0 else min(size, remaining_with_probe)
        data = self.source.read(requested)
        self.consumed += len(data)
        if self.consumed > self.limit:
            raise ValueError(f"decoded stream exceeds {self.limit} bytes")
        return data


def ordered_vectors(vectors: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(vectors, key=lambda vector: vector["sha256"])


def opaque_case_id(index: int) -> str:
    return f"case-{index:03d}"


def opaque_run_id(index: int) -> str:
    return f"pmav0-{opaque_case_id(index)}"


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def add_tar_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = 0o644
    info.mtime = 0
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    archive.addfile(info, io.BytesIO(data))


def deterministic_tar_gz(
    files: dict[str, bytes],
    *,
    preserve_order: bool = False,
) -> bytes:
    tar_buffer = io.BytesIO()
    with tarfile.open(fileobj=tar_buffer, mode="w", format=tarfile.PAX_FORMAT) as archive:
        names = files if preserve_order else sorted(files)
        for name in names:
            add_tar_file(archive, name, files[name])
    output = io.BytesIO()
    with gzip.GzipFile(fileobj=output, mode="wb", filename="", mtime=0) as stream:
        stream.write(tar_buffer.getvalue())
    return output.getvalue()


def bundle_files(bundle: bytes) -> tuple[dict[str, Any], bytes]:
    if len(bundle) > MAX_SOURCE_BUNDLE_BYTES:
        raise ValueError(f"source bundle exceeds {MAX_SOURCE_BUNDLE_BYTES} bytes")
    values: dict[str, bytes] = {}
    with gzip.GzipFile(fileobj=io.BytesIO(bundle)) as decoded:
        bounded = BoundedReader(decoded, MAX_SOURCE_ARCHIVE_BYTES)
        with tarfile.open(fileobj=bounded, mode="r|") as archive:
            for index, member in enumerate(archive):
                if index >= len(EXPECTED_BUNDLE_MEMBERS):
                    raise ValueError("source bundle contains surplus members")
                if len(member.name.encode("utf-8")) > MAX_MEMBER_NAME_BYTES:
                    raise ValueError("source bundle member name is too long")
                expected_name = EXPECTED_BUNDLE_MEMBERS[index]
                if member.name != expected_name or not member.isfile():
                    raise ValueError(
                        "source bundle must contain manifest.json then events.ndjson"
                    )
                limit = (
                    MAX_MANIFEST_BYTES
                    if member.name == "manifest.json"
                    else MAX_EVENTS_BYTES
                )
                if member.size > limit:
                    raise ValueError(f"{member.name} exceeds {limit} bytes")
                source = archive.extractfile(member)
                if source is None:
                    raise ValueError(f"cannot read source bundle member {member.name}")
                data = source.read(limit + 1)
                if len(data) > limit:
                    raise ValueError(f"{member.name} expands past {limit} bytes")
                values[member.name] = data
    if tuple(values) != EXPECTED_BUNDLE_MEMBERS:
        raise ValueError("source bundle is missing required members")
    return json.loads(values["manifest.json"]), values["events.ndjson"]


def rewrite_bundle_stream_identity(bundle: bytes, opaque_run_id: str) -> bytes:
    """Replace answer-bearing stream ids without changing non-identity event fields."""
    manifest, source_events = bundle_files(bundle)
    declared = manifest["files"]["events.ndjson"]
    source_integrity_clean = (
        declared["sha256"] == sha256(source_events)
        and declared["bytes"] == len(source_events)
    )

    source_without_identity = []
    rewritten = []
    seen_ids: set[str] = set()
    for line in source_events.splitlines():
        event = json.loads(line)
        source_event = dict(event)
        source_event.pop("id", None)
        source_event.pop("assayrunid", None)
        source_without_identity.append(source_event)
        sequence = event["assayseq"]
        if isinstance(sequence, bool) or not isinstance(sequence, int):
            raise ValueError("stream-identity rewrite requires integer assayseq values")
        rewritten_id = f"{opaque_run_id}:{sequence}"
        if rewritten_id in seen_ids:
            raise ValueError(
                f"stream-identity rewrite would collide on assayseq {sequence!r}"
            )
        seen_ids.add(rewritten_id)
        event["assayrunid"] = opaque_run_id
        event["id"] = rewritten_id
        rewritten.append(
            json.dumps(event, separators=(",", ":"), sort_keys=True).encode() + b"\n"
        )
    events = b"".join(rewritten)
    rewritten_without_identity = []
    for line in events.splitlines():
        event = json.loads(line)
        event.pop("id", None)
        event.pop("assayrunid", None)
        rewritten_without_identity.append(event)
    if rewritten_without_identity != source_without_identity:
        raise ValueError("stream-identity rewrite changed a non-identity event field")

    manifest["run_id"] = opaque_run_id
    if source_integrity_clean:
        manifest["files"]["events.ndjson"]["sha256"] = sha256(events)
        manifest["files"]["events.ndjson"]["bytes"] = len(events)
    manifest_bytes = (
        json.dumps(manifest, separators=(",", ":"), sort_keys=True).encode() + b"\n"
    )
    return deterministic_tar_gz(
        {
            "manifest.json": manifest_bytes,
            "events.ndjson": events,
        },
        preserve_order=True,
    )


def clean_room_spec(spec: bytes) -> bytes:
    text = spec.decode("utf-8")
    start_heading = "## 8. Conformance corpus"
    end_heading = "## 9. Versioning and maturity"
    start = text.find(start_heading)
    end = text.find(end_heading)
    if start < 0 or end < 0 or end <= start:
        raise ValueError(
            "canonical spec must contain ordered sections "
            f"{start_heading!r} and {end_heading!r}"
        )
    replacement = """\
## 8. Clean-room rendering note

The canonical document's informative conformance-corpus section is omitted from this pack because it
names answer-bearing corpus surfaces. Sections 1 through 7 and 9 onward are unchanged.

"""
    return (text[:start] + replacement + text[end:]).encode()


def clean_room_descriptor(descriptor: bytes) -> bytes:
    value = json.loads(descriptor)
    value.pop("corpus", None)
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
