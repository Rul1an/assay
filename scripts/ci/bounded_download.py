#!/usr/bin/env python3
"""Atomically download a public artifact without exceeding a byte ceiling."""

from __future__ import annotations

import pathlib
import urllib.request


class DownloadRejected(ValueError):
    pass


def download(
    url: str, destination: pathlib.Path, *, max_bytes: int,
    accept: str = "application/octet-stream", token: str | None = None,
) -> None:
    if max_bytes <= 0:
        raise ValueError("download ceiling must be positive")
    if destination.exists():
        raise DownloadRejected("download destination already exists")
    scratch = destination.with_name(f".{destination.name}.downloading")
    if scratch.exists():
        raise DownloadRejected("download scratch destination already exists")

    headers = {"Accept": accept, "User-Agent": "assay-release-verifier"}
    request = urllib.request.Request(url, headers=headers)
    if token:
        # Only the post-publication metadata re-read passes a token: anonymous API calls share a
        # 60-per-hour budget per runner IP, which failed three release runs. Assets stay anonymous.
        # Unredirected: urllib copies ordinary headers onto a redirect's request for its target,
        # so the token is attached to this first request only and never follows a redirect.
        request.add_unredirected_header("Authorization", f"Bearer {token}")
    try:
        # The driver validates the exact GitHub release URL before this call.
        with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
            declared = response.headers.get("Content-Length")
            if declared is not None and (not declared.isdigit() or int(declared) > max_bytes):
                raise DownloadRejected("download content length exceeds ceiling")
            total = 0
            with scratch.open("xb") as output:
                while chunk := response.read(min(65536, max_bytes - total + 1)):
                    total += len(chunk)
                    if total > max_bytes:
                        raise DownloadRejected("download stream exceeds ceiling")
                    output.write(chunk)
            if total == 0:
                raise DownloadRejected("download yielded no bytes")
        scratch.replace(destination)
    except BaseException:
        scratch.unlink(missing_ok=True)
        raise
