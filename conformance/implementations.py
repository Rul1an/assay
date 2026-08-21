#!/usr/bin/env python3
"""Load the static implementation registry. Standard library only.

    python3 conformance/implementations.py

A digest addresses image bytes. It does not authenticate a publisher, and a
row here does not prove safety, reproducibility, independence, or conformance.
This module does not pull, run, or fetch an image.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

REPO = Path(__file__).resolve().parent.parent
REGISTRY_PATH = Path(__file__).resolve().parent / "implementations.json"
SCHEMA = "assay.conformance.implementations.v0"
MAX_REGISTRY_BYTES = 256 * 1024
ALLOWED_SUITES = frozenset(("privileged-mcp-action-v0",))
REPRODUCTION_MODES = frozenset((
    "blind_from_spec",
    "from_spec_then_conformance",
    "commissioned_clean_room",
    "other_disclosed",
))
DOC_FIELDS = ("schema", "implementations")
ROW_FIELDS = (
    "id",
    "name",
    "suite",
    "image",
    "source",
    "commit",
    "language",
    "reproduction_mode",
    "authorship",
)
ID_RE = re.compile(r"\A[a-z][a-z0-9]*(?:-[a-z0-9]+)*\Z")
IMAGE_RE = re.compile(r"\A[^:@\s]+(?:/[^:@\s]+)*@sha256:[0-9a-f]{64}\Z")
COMMIT_RE = re.compile(r"\A[0-9a-f]{40}\Z")
AUTHORSHIP_RE = re.compile(
    r"\A(?:Authored-By: human|Assisted-By: \S+ \S+|Generated-By: \S+ \S+)\Z"
)


class ImplementationRegistryError(Exception):
    """The implementation registry cannot be used. Absence is never a pass."""


def _reject_unknown_fields(obj: dict, allowed: tuple[str, ...], *, what: str) -> None:
    extra = set(obj) - set(allowed)
    if extra:
        raise ImplementationRegistryError(
            "%s has unknown field(s): %s" % (what, ", ".join(sorted(extra)))
        )
    missing = [field for field in allowed if field not in obj]
    if missing:
        raise ImplementationRegistryError(
            "%s missing %s" % (what, ", ".join(missing))
        )


def _require_text(value: object, field: str, ident: str) -> str:
    if not isinstance(value, str) or not value:
        raise ImplementationRegistryError("%s: %s must be a non-empty string" % (ident, field))
    return value


def _validate_row(row: object, seen: set[str]) -> dict:
    if not isinstance(row, dict):
        raise ImplementationRegistryError(
            "implementation is %s, not an object" % type(row).__name__
        )
    _reject_unknown_fields(row, ROW_FIELDS, what="implementation")
    ident = _require_text(row["id"], "id", "implementation")
    if not ID_RE.fullmatch(ident):
        raise ImplementationRegistryError("implementation id is malformed: %s" % ident)
    if ident in seen:
        raise ImplementationRegistryError("duplicate implementation id: %s" % ident)
    seen.add(ident)
    _require_text(row["name"], "name", ident)
    suite = _require_text(row["suite"], "suite", ident)
    if suite not in ALLOWED_SUITES:
        raise ImplementationRegistryError("%s: unknown suite %r" % (ident, suite))
    image = _require_text(row["image"], "image", ident)
    if not IMAGE_RE.fullmatch(image):
        raise ImplementationRegistryError(
            "%s: image must be name@sha256:<64 hex digest>, not a tag" % ident
        )
    source = _require_text(row["source"], "source", ident)
    parsed = urlparse(source)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ImplementationRegistryError(
            "%s: source must be an absolute HTTP(S) URL" % ident
        )
    commit = _require_text(row["commit"], "commit", ident)
    if not COMMIT_RE.fullmatch(commit):
        raise ImplementationRegistryError("%s: commit must be a full 40-hex SHA" % ident)
    _require_text(row["language"], "language", ident)
    mode = _require_text(row["reproduction_mode"], "reproduction_mode", ident)
    if mode not in REPRODUCTION_MODES:
        raise ImplementationRegistryError("%s: unknown reproduction_mode %r" % (ident, mode))
    authorship = _require_text(row["authorship"], "authorship", ident)
    if not AUTHORSHIP_RE.fullmatch(authorship):
        raise ImplementationRegistryError("%s: authorship disclosure is invalid" % ident)
    return row


def load_implementations(path: Path | None = None) -> dict:
    path = Path(path) if path is not None else REGISTRY_PATH
    if path.is_symlink():
        try:
            path.resolve().relative_to(REPO.resolve())
        except ValueError:
            raise ImplementationRegistryError(
                "registry symlink escapes the repository: %s" % path
            )
    if not path.is_file():
        raise ImplementationRegistryError("implementation registry missing: %s" % path)
    data = path.read_bytes()
    if len(data) > MAX_REGISTRY_BYTES:
        raise ImplementationRegistryError(
            "implementation registry exceeds %d bytes" % MAX_REGISTRY_BYTES
        )
    try:
        doc = json.loads(data)
    except json.JSONDecodeError as exc:
        raise ImplementationRegistryError(
            "implementation registry is not JSON: %s" % exc
        ) from exc
    if not isinstance(doc, dict):
        raise ImplementationRegistryError(
            "implementation registry is %s, not an object" % type(doc).__name__
        )
    _reject_unknown_fields(doc, DOC_FIELDS, what="implementation registry")
    if doc.get("schema") != SCHEMA:
        raise ImplementationRegistryError(
            "implementation registry schema must be %s" % SCHEMA
        )
    rows = doc.get("implementations")
    if not isinstance(rows, list):
        raise ImplementationRegistryError("implementations must be a list")
    seen: set[str] = set()
    doc["implementations"] = [_validate_row(row, seen) for row in rows]
    return doc


def main(argv: list[str] | None = None) -> int:
    if argv:
        sys.stderr.write(
            "implementations.py takes no flags; it only validates the registry\n"
        )
        return 2
    try:
        load_implementations()
    except ImplementationRegistryError as exc:
        sys.stderr.write("implementation registry rejected: %s\n" % exc)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
