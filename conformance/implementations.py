#!/usr/bin/env python3
"""Load the static implementation registry. Standard library only.

    python3 conformance/implementations.py

A digest addresses image bytes. It does not authenticate a publisher, and a
row here does not prove safety, reproducibility, independence, or conformance.
This module does not pull, run, or fetch an image.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import urlparse

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent / "adequacy"))
import published_rows  # noqa: E402

REGISTRY_PATH = Path(__file__).resolve().parent / "implementations.json"
SCHEMA = "assay.conformance.implementations.v0"
ALLOWED_SUITES = frozenset(("privileged-mcp-action-v0",))
REPRODUCTION_MODES = frozenset((
    "blind_from_spec",
    "from_spec_then_conformance",
    "commissioned_clean_room",
    "other_disclosed",
))
AUTHORSHIP_KINDS = frozenset(("human", "agent-assisted", "agent-generated"))
AGENT_KINDS = frozenset(("agent-assisted", "agent-generated"))
HUMAN_AUTHORSHIP_FIELDS = ("kind",)
AGENT_AUTHORSHIP_FIELDS = ("kind", "model", "prompt_strategy")
SOURCE_PATTERN = r"^https?://[^\s]+$"
IMAGE_PATTERN = r"^[^:@/\s]+(?:/[^:@/\s]+)*@sha256:[0-9a-f]{64}$"
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
IMAGE_RE = re.compile(IMAGE_PATTERN)
COMMIT_RE = re.compile(r"\A[0-9a-f]{40}\Z")
SOURCE_RE = re.compile(SOURCE_PATTERN)


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


def _validate_authorship(value: object, ident: str) -> dict:
    if not isinstance(value, dict):
        raise ImplementationRegistryError("%s: authorship must be an object" % ident)
    kind = value.get("kind")
    if kind == "human":
        _reject_unknown_fields(value, HUMAN_AUTHORSHIP_FIELDS, what="%s authorship" % ident)
        return value
    if kind in AGENT_KINDS:
        _reject_unknown_fields(
            value, AGENT_AUTHORSHIP_FIELDS, what="%s authorship" % ident
        )
        _require_text(value.get("model"), "model", ident)
        _require_text(value.get("prompt_strategy"), "prompt_strategy", ident)
        return value
    raise ImplementationRegistryError("%s: unknown authorship kind %r" % (ident, kind))


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
    if not SOURCE_RE.fullmatch(source) or parsed.scheme not in {"http", "https"} or not parsed.netloc:
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
    _validate_authorship(row["authorship"], ident)
    return row


def implementation_schema() -> dict:
    """JSON Schema rendered from the same vocabulary the validator uses."""
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://raw.githubusercontent.com/Rul1an/assay/main/conformance/implementations.schema.json",
        "title": "Assay static implementation registry",
        "type": "object",
        "additionalProperties": False,
        "required": list(DOC_FIELDS),
        "properties": {
            "schema": {"const": SCHEMA},
            "implementations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": list(ROW_FIELDS),
                    "properties": {
                        "id": {
                            "type": "string",
                            "pattern": "^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$",
                        },
                        "name": {"type": "string", "minLength": 1},
                        "suite": {"const": next(iter(ALLOWED_SUITES))},
                        "image": {
                            "type": "string",
                            "pattern": IMAGE_PATTERN,
                            "description": (
                                "Exact name@sha256:<64 hex>. A digest addresses bytes; "
                                "it does not authenticate the publisher."
                            ),
                        },
                        "source": {"type": "string", "pattern": SOURCE_PATTERN},
                        "commit": {"type": "string", "pattern": "^[0-9a-f]{40}$"},
                        "language": {"type": "string", "minLength": 1},
                        "reproduction_mode": {"enum": sorted(REPRODUCTION_MODES)},
                        "authorship": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "additionalProperties": False,
                                    "required": list(HUMAN_AUTHORSHIP_FIELDS),
                                    "properties": {"kind": {"const": "human"}},
                                },
                                {
                                    "type": "object",
                                    "additionalProperties": False,
                                    "required": list(AGENT_AUTHORSHIP_FIELDS),
                                    "properties": {
                                        "kind": {"enum": sorted(AGENT_KINDS)},
                                        "model": {"type": "string", "minLength": 1},
                                        "prompt_strategy": {"type": "string", "minLength": 1},
                                    },
                                },
                            ]
                        },
                    },
                },
            },
        },
    }


def load_implementations(path: Path | None = None) -> dict:
    path = Path(path) if path is not None else REGISTRY_PATH
    try:
        data = published_rows.read_regular_file(path)
        doc = published_rows.parse_json_object(data, "implementation registry")
    except ValueError as exc:
        raise ImplementationRegistryError(str(exc)) from exc
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
