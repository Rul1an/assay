#!/usr/bin/env python3
"""Fail-closed resource ceilings for CI inventories and programme-truth docs.

Measured live max among programme-truth inputs: 26008 bytes
(``.github/workflows/kernel-matrix.yml``). The document ceiling is 65536
bytes (64 KiB). Inventories are bounded before unique-sort materialization.
Exceeding a ceiling is an error; nothing is silently truncated.

Callers may lower inventory caps via the environment. They cannot raise
them above the canonical 8192 / 524288 values. An absent variable uses
the default; an explicit empty override is invalid.
"""

from __future__ import annotations

import os
import stat
import sys

MAX_DOC_BYTES = 65536
MAX_INVENTORY_PATHS = 8192
MAX_INVENTORY_BYTES = 524288
FORBIDDEN_PROGRAMME_OVERRIDES = (
    "PROGRAMME_TRUTH_ROOT",
    "PROGRAMME_TRUTH_AGENTS",
    "PROGRAMME_TRUTH_SELFHOST",
    "PROGRAMME_TRUTH_CEILING_CHILD",
    "PROGRAMME_TRUTH_PREVIEW_MUTANT",
)


def reject_programme_overrides(env: dict[str, str] | None = None) -> None:
    source = os.environ if env is None else env
    for name in FORBIDDEN_PROGRAMME_OVERRIDES:
        if name in source:
            raise SystemExit(f"{name} cannot replace the script worktree")


REJECT_OVERRIDES_CALLER = "python3 " + '"$_TRUTH_LIB"' + " reject-overrides"


def reject_overrides_caller_count(text: str) -> int:
    count = 0
    for line in text.splitlines():
        if line.split("#", 1)[0].strip() == REJECT_OVERRIDES_CALLER:
            count += 1
    return count


def assert_reject_overrides_caller(path: str) -> None:
    count = reject_overrides_caller_count(read_bounded_file(path).decode("utf-8"))
    if count != 1:
        raise SystemExit(f"reject-overrides caller count is {count}, want 1")


def drop_reject_overrides_caller(src: str, dst: str) -> None:
    text = read_bounded_file(src).decode("utf-8")
    count = reject_overrides_caller_count(text)
    if count != 1:
        raise SystemExit(f"reject-overrides caller count is {count}, want 1")
    kept = [
        line
        for line in text.splitlines(keepends=True)
        if line.split("#", 1)[0].strip() != REJECT_OVERRIDES_CALLER
    ]
    out = "".join(kept).encode("utf-8")
    require_bounded_bytes(out, dst)
    with open(dst, "wb") as fh:
        fh.write(out)


def require_bounded_bytes(
    data: bytes, label: str, limit: int = MAX_DOC_BYTES
) -> bytes:
    n = len(data)
    if n > limit:
        raise SystemExit(f"{label} exceeds {limit}-byte ceiling ({n} bytes)")
    return data


def _readonly_open_flags() -> int:
    flags = os.O_RDONLY
    flags |= getattr(os, "O_NOFOLLOW", 0)
    flags |= getattr(os, "O_NONBLOCK", 0)
    return flags


def read_bounded_file(
    path: str, label: str | None = None, limit: int = MAX_DOC_BYTES
) -> bytes:
    """Read at most ``limit`` bytes from a regular file via one descriptor.

    Non-regular inputs fail closed. At most ``limit + 1`` bytes are read
    from the opened fd so an oversize file never reaches a consumer.
    """
    label = label or path
    try:
        fd = os.open(path, _readonly_open_flags())
    except OSError as exc:
        raise SystemExit(f"{label} could not be opened: {exc}") from exc
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            raise SystemExit(f"{label} is not a regular file")
        buf = bytearray()
        want = limit + 1
        while len(buf) < want:
            chunk = os.read(fd, want - len(buf))
            if not chunk:
                break
            buf.extend(chunk)
    finally:
        os.close(fd)
    if len(buf) > limit:
        raise SystemExit(f"{label} exceeds {limit}-byte ceiling ({len(buf)} bytes)")
    return bytes(buf)


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


def caller_cap(env_name: str, canonical: int) -> int:
    if env_name not in os.environ:
        return canonical
    raw = os.environ[env_name]
    if raw == "0" or (raw.startswith("-") and raw[1:].isascii() and raw[1:].isdigit()):
        raise SystemExit(f"{env_name} must be a positive integer")
    if not raw.isascii() or not raw.isdigit() or raw.startswith("0"):
        raise SystemExit(f"{env_name} is not a positive integer")
    value = int(raw, 10)
    if value > canonical:
        raise SystemExit(f"{env_name} cannot exceed canonical {canonical}")
    return value


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
            "check-file|read-file|check-stdin|inventory|"
            "max-doc-bytes|canonical-inventory-limits|"
            "reject-overrides|forbidden-overrides|"
            "assert-reject-caller|drop-reject-caller [path]"
        )
    cmd = argv[0]
    if cmd == "max-doc-bytes":
        print(MAX_DOC_BYTES)
        return
    if cmd == "canonical-inventory-limits":
        print(f"{MAX_INVENTORY_PATHS} {MAX_INVENTORY_BYTES}")
        return
    if cmd == "forbidden-overrides":
        for name in FORBIDDEN_PROGRAMME_OVERRIDES:
            print(name)
        return
    if cmd == "reject-overrides":
        reject_programme_overrides()
        return
    if cmd == "assert-reject-caller":
        assert_reject_overrides_caller(argv[1])
        return
    if cmd == "drop-reject-caller":
        drop_reject_overrides_caller(argv[1], argv[2])
        return
    if cmd == "check-file":
        read_bounded_file(argv[1])
        return
    if cmd == "read-file":
        sys.stdout.buffer.write(read_bounded_file(argv[1]))
        return
    if cmd == "check-stdin":
        label = argv[1] if len(argv) > 1 else "input"
        read_bounded_stream(sys.stdin.buffer, label)
        return
    if cmd == "inventory":
        max_paths = caller_cap("BOUNDED_INVENTORY_MAX_PATHS", MAX_INVENTORY_PATHS)
        max_bytes = caller_cap("BOUNDED_INVENTORY_MAX_BYTES", MAX_INVENTORY_BYTES)
        for path in bound_inventory(
            sys.stdin, max_paths=max_paths, max_bytes=max_bytes
        ):
            print(path)
        return
    raise SystemExit(f"unknown command: {cmd}")


if __name__ == "__main__":
    main(sys.argv[1:])
