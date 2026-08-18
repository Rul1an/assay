#!/usr/bin/env python3
"""Fail-closed resource ceilings for CI inventories and programme-truth docs.

Measured live max among programme-truth inputs: 26008 bytes
(``.github/workflows/kernel-matrix.yml``). The document ceiling is 65536
bytes (64 KiB). Inventories are bounded before unique-sort materialization.
Exceeding a ceiling is an error; nothing is silently truncated.
"""

from __future__ import annotations

import os
import sys

MAX_DOC_BYTES = 65536
MAX_INVENTORY_PATHS = 8192
MAX_INVENTORY_BYTES = 524288


def require_bounded_bytes(
    data: bytes, label: str, limit: int = MAX_DOC_BYTES
) -> bytes:
    n = len(data)
    if n > limit:
        raise SystemExit(f"{label} exceeds {limit}-byte ceiling ({n} bytes)")
    return data


def require_bounded_file(
    path: str, label: str | None = None, limit: int = MAX_DOC_BYTES
) -> None:
    label = label or path
    try:
        n = os.path.getsize(path)
    except OSError as exc:
        raise SystemExit(f"{label} could not be sized: {exc}") from exc
    if n > limit:
        raise SystemExit(f"{label} exceeds {limit}-byte ceiling ({n} bytes)")


def read_bounded_stream(
    fp, label: str, limit: int = MAX_DOC_BYTES
) -> bytes:
    buf = bytearray()
    while True:
        chunk = fp.read(8192)
        if not chunk:
            break
        if len(buf) + len(chunk) > limit:
            raise SystemExit(f"{label} exceeds {limit}-byte ceiling")
        buf.extend(chunk)
    return bytes(buf)


def bound_inventory(
    lines,
    max_paths: int = MAX_INVENTORY_PATHS,
    max_bytes: int = MAX_INVENTORY_BYTES,
) -> list[str]:
    kept: list[str] = []
    total = 0
    count = 0
    for raw in lines:
        line = raw.rstrip("\n")
        if not line:
            continue
        encoded = (line + "\n").encode("utf-8")
        count += 1
        total += len(encoded)
        if count > max_paths:
            raise SystemExit(
                f"inventory exceeds max path count {max_paths} ({count} paths)"
            )
        if total > max_bytes:
            raise SystemExit(
                f"inventory exceeds max byte budget {max_bytes} ({total} bytes)"
            )
        kept.append(line)
    return sorted(set(kept))


def main(argv: list[str]) -> None:
    if not argv:
        raise SystemExit(
            "usage: resource_ceilings.py "
            "check-file|check-stdin|inventory|max-doc-bytes [path]"
        )
    cmd = argv[0]
    if cmd == "max-doc-bytes":
        print(MAX_DOC_BYTES)
        return
    if cmd == "check-file":
        require_bounded_file(argv[1])
        return
    if cmd == "check-stdin":
        label = argv[1] if len(argv) > 1 else "input"
        read_bounded_stream(sys.stdin.buffer, label)
        return
    if cmd == "inventory":
        max_paths = int(
            os.environ.get("BOUNDED_INVENTORY_MAX_PATHS", MAX_INVENTORY_PATHS)
        )
        max_bytes = int(
            os.environ.get("BOUNDED_INVENTORY_MAX_BYTES", MAX_INVENTORY_BYTES)
        )
        for path in bound_inventory(
            sys.stdin, max_paths=max_paths, max_bytes=max_bytes
        ):
            print(path)
        return
    raise SystemExit(f"unknown command: {cmd}")


if __name__ == "__main__":
    main(sys.argv[1:])
