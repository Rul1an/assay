#!/usr/bin/env python3
"""Shared byte rendering and atomic replacement for JSON artifacts."""

from __future__ import annotations

import hashlib
import json
import os
import tempfile
from pathlib import Path
from typing import Any

ARTIFACT_TEMP_PREFIX = ".assay-artifact-"


def content_sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def render_deterministic_json_bytes(value: Any) -> bytes:
    """Render the programme's stable pretty-JSON form, including one trailing LF."""
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_regular_file_atomically(path: Path, data: bytes) -> None:
    """Replace an artifact without exposing a partial destination.

    The caller must keep the parent path stable while this function runs. Repointing a symlinked
    parent concurrently can make replacement fail and can prevent best-effort temp-file cleanup.
    """
    destination = Path(path)
    parent = destination.parent
    # The destination is the caller's explicit output path. This helper writes no data read from
    # that path and executes nothing from it; selecting any writable destination is the API.
    parent.mkdir(parents=True, exist_ok=True)
    # The temporary file stays beside that explicit destination so replacement is atomic.
    fd, tmp_name = tempfile.mkstemp(prefix=ARTIFACT_TEMP_PREFIX, dir=str(parent))
    tmp = Path(tmp_name)
    try:
        written = 0
        while written < len(data):
            count = os.write(fd, data[written:])
            if count <= 0:
                raise OSError("artifact write made no progress")
            written += count
        os.fsync(fd)
        os.close(fd)
        fd = -1
        # Replacing the caller-selected destination is the intended output capability.
        os.replace(tmp, destination)
    finally:
        if fd >= 0:
            os.close(fd)
        # `tmp` is the exact path returned by mkstemp above, never caller-provided.
        tmp.unlink(missing_ok=True)
