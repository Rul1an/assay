#!/usr/bin/env python3
"""Reproduce and reassess the MCP 2026-07-28 error-response id finding."""

from __future__ import annotations

import argparse
import hashlib
import html.parser
import json
import re
import sys
from pathlib import Path
from typing import Any


PINNED_CONSTRAINTS = {
    "mcp_requires_jsonrpc_2": True,
    "mcp_error_id_required": False,
    "mcp_error_id_allows_null": False,
    "jsonrpc_response_id_required": True,
    "jsonrpc_unknown_id_must_be_null": True,
}
HEX_40 = re.compile(r"^[0-9a-f]{40}$")
HEX_64 = re.compile(r"^[0-9a-f]{64}$")
MAX_PACK_FILE_BYTES = 1 << 20
MAX_SUBJECT_BYTES = 2 << 20
MAX_CHECKSUM_BYTES = 64 << 10
MAX_PACK_ENTRIES = 256
MAX_PACK_DEPTH = 8


class PackError(ValueError):
    """The committed pack is malformed or no longer matches its provenance."""


class SubjectError(ValueError):
    """A supplied upstream subject cannot support the bounded extraction."""


class _TextExtractor(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.parts: list[str] = []

    def handle_data(self, data: str) -> None:
        self.parts.append(data)

    def text(self) -> str:
        return " ".join(" ".join(self.parts).split())


class _DefinitionExtractor(html.parser.HTMLParser):
    """Collect normalized HTML definition-list term/description pairs."""

    def __init__(self) -> None:
        super().__init__()
        self._capture: str | None = None
        self._parts: list[str] = []
        self._term: str | None = None
        self._heading_tag: str | None = None
        self._heading_parts: list[str] = []
        self._section: str | None = None
        self.definitions: list[tuple[str | None, str, str]] = []

    def handle_starttag(
        self,
        tag: str,
        attrs: list[tuple[str, str | None]],
    ) -> None:
        del attrs
        if tag in {"h1", "h2", "h3", "h4", "h5", "h6"}:
            self._heading_tag = tag
            self._heading_parts = []
        if tag in {"dt", "dd"}:
            self._capture = tag
            self._parts = []

    def handle_data(self, data: str) -> None:
        if self._heading_tag is not None:
            self._heading_parts.append(data)
        if self._capture is not None:
            self._parts.append(data)

    def handle_endtag(self, tag: str) -> None:
        if tag == self._heading_tag:
            self._section = " ".join(" ".join(self._heading_parts).split())
            self._heading_tag = None
            self._heading_parts = []
        if tag != self._capture:
            return
        text = " ".join(" ".join(self._parts).split())
        if tag == "dt":
            self._term = text
        elif self._term is not None:
            self.definitions.append((self._section, self._term, text))
            self._term = None
        self._capture = None
        self._parts = []


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bounded(
    path: Path,
    limit: int,
    error_type: type[ValueError],
) -> bytes:
    """Read at most limit bytes, probing one byte past an inclusive ceiling."""
    try:
        with path.open("rb") as handle:
            data = handle.read(limit + 1)
    except OSError as exc:
        raise error_type("cannot read bounded input") from exc
    if len(data) > limit:
        raise error_type("input exceeds size limit")
    return data


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _error_shape_is_valid(message: dict[str, Any]) -> bool:
    error = message.get("error")
    return (
        message.get("jsonrpc") == "2.0"
        and "result" not in message
        and isinstance(error, dict)
        and isinstance(error.get("code"), int)
        and not isinstance(error.get("code"), bool)
        and isinstance(error.get("message"), str)
    )


def evaluate_message(message: Any) -> dict[str, bool]:
    """Evaluate only the error-response constraints exercised by this pack."""
    if not isinstance(message, dict) or not _error_shape_is_valid(message):
        return {"mcp": False, "jsonrpc": False}

    has_id = "id" in message
    identifier = message.get("id")
    mcp_id_valid = not has_id or (
        isinstance(identifier, str)
        or (isinstance(identifier, int) and not isinstance(identifier, bool))
    )
    jsonrpc_id_valid = has_id and (
        identifier is None or isinstance(identifier, str) or _is_number(identifier)
    )
    return {"mcp": mcp_id_valid, "jsonrpc": jsonrpc_id_valid}


def _load_json(path: Path) -> Any:
    try:
        return json.loads(read_bounded(path, MAX_PACK_FILE_BYTES, PackError))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise PackError(f"cannot read valid JSON from {path.name}") from exc


def _public_pack_files(root: Path) -> set[str]:
    files: set[str] = set()
    directories = [root]
    entries = 0
    while directories:
        directory = directories.pop()
        try:
            children = directory.iterdir()
            for path in children:
                entries += 1
                if entries > MAX_PACK_ENTRIES:
                    raise PackError("public pack exceeds entry limit")
                relative = path.relative_to(root)
                if len(relative.parts) > MAX_PACK_DEPTH:
                    raise PackError("public pack exceeds depth limit")
                if path.is_symlink():
                    raise PackError("public pack must not contain symbolic links")
                if "__pycache__" in relative.parts:
                    continue
                if path.is_dir():
                    directories.append(path)
                    continue
                if not path.is_file():
                    raise PackError("public pack contains an unsupported file type")
                if path.name == "SHA256SUMS" or path.suffix in {".pyc", ".pyo"}:
                    continue
                files.add(relative.as_posix())
        except OSError as exc:
            raise PackError("cannot enumerate public pack") from exc
    return files


def validate_checksums(root: Path) -> None:
    """Require SHA256SUMS to bind every public file in the pack."""
    checksum_path = root / "SHA256SUMS"
    try:
        lines = read_bounded(
            checksum_path,
            MAX_CHECKSUM_BYTES,
            PackError,
        ).decode("utf-8").splitlines()
    except UnicodeError as exc:
        raise PackError("cannot read SHA256SUMS") from exc
    declared: dict[str, str] = {}
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  ([^\n]+)", line)
        if match is None:
            raise PackError("SHA256SUMS contains an invalid line")
        digest, relative = match.groups()
        path = Path(relative)
        if (
            path.is_absolute()
            or ".." in path.parts
            or relative in declared
            or relative == "SHA256SUMS"
        ):
            raise PackError("SHA256SUMS contains an invalid path")
        declared[relative] = digest
    actual_files = _public_pack_files(root)
    if set(declared) != actual_files:
        raise PackError("SHA256SUMS file set does not match the public pack")
    for relative, expected in declared.items():
        data = read_bounded(root / relative, MAX_PACK_FILE_BYTES, PackError)
        if sha256_bytes(data) != expected:
            raise PackError(f"SHA256SUMS digest mismatch for {Path(relative).name}")


def _validate_provenance(root: Path) -> dict[str, Any]:
    provenance = _load_json(root / "PROVENANCE.json")
    if not isinstance(provenance, dict):
        raise PackError("provenance must be an object")
    if provenance.get("schema") != "assay.mcp-jsonrpc-id-conformance.provenance.v1":
        raise PackError("unsupported provenance schema")
    if provenance.get("finding") != PINNED_CONSTRAINTS:
        raise PackError("provenance finding does not match the pinned contract")

    sources = provenance.get("sources")
    if not isinstance(sources, dict) or set(sources) != {
        "mcp_overview",
        "mcp_schema_json",
        "mcp_schema_typescript",
        "jsonrpc_spec",
    }:
        raise PackError("provenance must name exactly the four upstream subjects")
    for source in sources.values():
        if not isinstance(source, dict):
            raise PackError("source provenance must be an object")
        if not isinstance(source.get("url"), str) or not source["url"].startswith(
            "https://"
        ):
            raise PackError("source URL must use HTTPS")
        if not isinstance(source.get("sha256"), str) or not HEX_64.fullmatch(
            source["sha256"]
        ):
            raise PackError("source digest must be lowercase SHA-256")
    for name in ("mcp_schema_json", "mcp_schema_typescript"):
        commit = sources[name].get("commit")
        if not isinstance(commit, str) or not HEX_40.fullmatch(commit):
            raise PackError("MCP source commit must be a full lowercase commit id")
    if (
        sources["mcp_schema_json"]["commit"]
        != sources["mcp_schema_typescript"]["commit"]
    ):
        raise PackError("MCP source subjects must share one commit")

    vectors = provenance.get("vectors")
    if not isinstance(vectors, dict):
        raise PackError("vector provenance must be an object")
    actual_paths = {
        path.relative_to(root).as_posix() for path in (root / "vectors").glob("*.json")
    }
    if set(vectors) != actual_paths:
        raise PackError("vector provenance does not match the committed vector set")
    for relative, expected_digest in vectors.items():
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts:
            raise PackError("vector provenance path escapes the pack")
        if not isinstance(expected_digest, str) or not HEX_64.fullmatch(
            expected_digest
        ):
            raise PackError("vector digest must be lowercase SHA-256")
        actual_digest = sha256_bytes(
            read_bounded(root / path, MAX_PACK_FILE_BYTES, PackError)
        )
        if actual_digest != expected_digest:
            raise PackError(f"vector digest mismatch for {path.name}")
    return provenance


def verify_source_digests(
    records: dict[str, Any],
    subjects: dict[str, bytes],
) -> None:
    """Bind caller-supplied subject bytes to one provenance source set."""
    if set(records) != set(subjects):
        raise SubjectError("source record and subject sets differ")
    for name, subject in subjects.items():
        record = records[name]
        if not isinstance(record, dict) or not isinstance(record.get("sha256"), str):
            raise SubjectError("source record lacks a digest")
        if sha256_bytes(subject) != record["sha256"]:
            raise SubjectError(f"source digest mismatch for {name}")


def reproduce(root: Path) -> dict[str, Any]:
    """Reproduce the committed vector classifications against the pinned record."""
    validate_checksums(root)
    provenance = _validate_provenance(root)
    counts = {
        "both_valid": 0,
        "mcp_only": 0,
        "jsonrpc_only": 0,
        "neither_valid": 0,
    }
    observations: list[dict[str, Any]] = []
    for relative in sorted(provenance["vectors"]):
        vector = _load_json(root / relative)
        if not isinstance(vector, dict) or not isinstance(vector.get("id"), str):
            raise PackError(f"invalid vector shape in {Path(relative).name}")
        observed = evaluate_message(vector.get("message"))
        if vector.get("expected") != observed:
            raise PackError(f"declared outcome drift in {Path(relative).name}")
        bucket = {
            (True, True): "both_valid",
            (True, False): "mcp_only",
            (False, True): "jsonrpc_only",
            (False, False): "neither_valid",
        }[(observed["mcp"], observed["jsonrpc"])]
        counts[bucket] += 1
        observations.append({"id": vector["id"], **observed})

    status = (
        "contradiction"
        if counts["both_valid"] and counts["mcp_only"] and counts["jsonrpc_only"]
        else "not_reproduced"
    )
    return {
        "schema": "assay.mcp-jsonrpc-id-conformance.report.v1",
        "mode": "reproduce",
        "status": status,
        "summary": counts,
        "observations": observations,
    }


def _extract_mcp_json_constraints(subject: bytes) -> dict[str, bool]:
    try:
        schema = json.loads(subject)
        definitions = schema["$defs"]
        error_response = definitions["JSONRPCErrorResponse"]
        request_id = definitions["RequestId"]
        required = error_response["required"]
        identifier_types = request_id["type"]
    except (UnicodeError, json.JSONDecodeError, KeyError, TypeError) as exc:
        raise SubjectError("MCP subject lacks the expected schema definitions") from exc
    if not isinstance(required, list):
        raise SubjectError("MCP error-response required set is not readable")
    if isinstance(identifier_types, str):
        identifier_types = [identifier_types]
    if not isinstance(identifier_types, list) or not all(
        isinstance(value, str) for value in identifier_types
    ):
        raise SubjectError("MCP request-id types are not readable")
    return {
        "mcp_error_id_required": "id" in required,
        "mcp_error_id_allows_null": "null" in identifier_types,
    }


def _extract_mcp_typescript_constraints(subject: bytes) -> dict[str, bool]:
    try:
        text = subject.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SubjectError("MCP TypeScript subject is not UTF-8") from exc
    request_id = re.search(
        r"export\s+type\s+RequestId\s*=\s*([^;]+);",
        text,
    )
    error_response = re.search(
        r"export\s+interface\s+JSONRPCErrorResponse\s*\{(.*?)\}",
        text,
        flags=re.DOTALL,
    )
    if request_id is None or error_response is None:
        raise SubjectError("MCP TypeScript subject lacks the expected declarations")
    identifier = re.search(
        r"\bid\s*(\?)?\s*:\s*RequestId\s*;",
        error_response.group(1),
    )
    if identifier is None:
        raise SubjectError("MCP TypeScript error-response id is not readable")
    request_id_members = {
        member.strip() for member in request_id.group(1).split("|")
    }
    return {
        "mcp_error_id_required": identifier.group(1) is None,
        "mcp_error_id_allows_null": "null" in request_id_members,
    }


def _extract_jsonrpc_constraints(subject: bytes) -> dict[str, bool]:
    try:
        document = subject.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SubjectError("JSON-RPC subject is not UTF-8") from exc
    parser = _DefinitionExtractor()
    parser.feed(document)
    parser.close()
    expected = (
        "this member is required. "
        "it must be the same as the value of the id member in the request object. "
        "if there was an error in detecting the id in the request object "
        "(e.g. parse error/invalid request), it must be null."
    )
    response_id_clause = any(
        section is not None
        and re.fullmatch(
            r"(?:\d+(?:\.\d+)*\s+)?response object",
            section.casefold(),
        )
        and term.casefold() == "id"
        and description.casefold() == expected
        for section, term, description in parser.definitions
    )
    if not response_id_clause:
        raise SubjectError("JSON-RPC subject lacks the expected response-id clauses")
    return {
        "jsonrpc_response_id_required": True,
        "jsonrpc_unknown_id_must_be_null": True,
    }


def _extract_mcp_overview_constraints(subject: bytes) -> dict[str, bool]:
    try:
        document = subject.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SubjectError("MCP overview subject is not UTF-8") from exc
    parser = _TextExtractor()
    parser.feed(document)
    text = parser.text().casefold()
    requires_jsonrpc = (
        "all messages between mcp clients and servers must follow the json-rpc 2.0 "
        "specification"
    ) in text
    if not requires_jsonrpc:
        raise SubjectError("MCP overview lacks the universal JSON-RPC requirement")
    return {"mcp_requires_jsonrpc_2": True}


def reassess_subjects(
    mcp_typescript: bytes,
    mcp_schema: bytes,
    mcp_overview: bytes,
    jsonrpc_spec: bytes,
) -> dict[str, Any]:
    """Extract the bounded constraints from caller-supplied upstream bytes."""
    if any(
        len(subject) > MAX_SUBJECT_BYTES
        for subject in (mcp_typescript, mcp_schema, mcp_overview, jsonrpc_spec)
    ):
        raise SubjectError("input exceeds size limit")
    typescript_constraints = _extract_mcp_typescript_constraints(mcp_typescript)
    json_constraints = _extract_mcp_json_constraints(mcp_schema)
    if typescript_constraints != json_constraints:
        raise SubjectError("MCP source and generated schema disagree on the id boundary")
    constraints = {
        **_extract_mcp_overview_constraints(mcp_overview),
        **typescript_constraints,
        **_extract_jsonrpc_constraints(jsonrpc_spec),
    }
    omission_conflict = (
        constraints["mcp_requires_jsonrpc_2"]
        and not constraints["mcp_error_id_required"]
        and constraints["jsonrpc_response_id_required"]
    )
    null_conflict = (
        constraints["mcp_requires_jsonrpc_2"]
        and not constraints["mcp_error_id_allows_null"]
        and constraints["jsonrpc_unknown_id_must_be_null"]
    )
    return {
        "schema": "assay.mcp-jsonrpc-id-conformance.report.v1",
        "mode": "reassess",
        "status": (
            "contradiction" if omission_conflict or null_conflict else "not_reproduced"
        ),
        "constraints": constraints,
        "arms": {
            "omitted_id": omission_conflict,
            "null_id": null_conflict,
        },
        "subjects": {
            "mcp_schema_typescript_sha256": sha256_bytes(mcp_typescript),
            "mcp_schema_json_sha256": sha256_bytes(mcp_schema),
            "mcp_overview_sha256": sha256_bytes(mcp_overview),
            "jsonrpc_spec_sha256": sha256_bytes(jsonrpc_spec),
        },
    }


def verify_pinned_subjects(
    root: Path,
    mcp_typescript: bytes,
    mcp_schema: bytes,
    mcp_overview: bytes,
    jsonrpc_spec: bytes,
) -> dict[str, Any]:
    """Verify exact pinned bytes before extracting and comparing their constraints."""
    validate_checksums(root)
    provenance = _validate_provenance(root)
    subjects = {
        "mcp_schema_typescript": mcp_typescript,
        "mcp_schema_json": mcp_schema,
        "mcp_overview": mcp_overview,
        "jsonrpc_spec": jsonrpc_spec,
    }
    verify_source_digests(provenance["sources"], subjects)
    report = reassess_subjects(
        mcp_typescript,
        mcp_schema,
        mcp_overview,
        jsonrpc_spec,
    )
    if report["constraints"] != provenance["finding"]:
        raise SubjectError("source extraction differs from the pinned finding")
    report["mode"] = "verify-pinned"
    return report


def _add_subject_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--mcp-typescript", type=Path, required=True)
    parser.add_argument("--mcp-schema", type=Path, required=True)
    parser.add_argument("--mcp-overview", type=Path, required=True)
    parser.add_argument("--jsonrpc-spec", type=Path, required=True)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    reproduce_parser = commands.add_parser(
        "reproduce", help="verify and run the committed vector pack"
    )
    reproduce_parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parent
    )
    reassess_parser = commands.add_parser(
        "reassess", help="evaluate caller-supplied upstream source bytes"
    )
    _add_subject_arguments(reassess_parser)
    pinned_parser = commands.add_parser(
        "verify-pinned",
        help="bind supplied bytes to PROVENANCE.json before reassessment",
    )
    pinned_parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parent
    )
    _add_subject_arguments(pinned_parser)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "reproduce":
            report = reproduce(args.root.resolve())
        elif args.command == "verify-pinned":
            report = verify_pinned_subjects(
                args.root.resolve(),
                read_bounded(args.mcp_typescript, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.mcp_schema, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.mcp_overview, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.jsonrpc_spec, MAX_SUBJECT_BYTES, SubjectError),
            )
        else:
            report = reassess_subjects(
                read_bounded(args.mcp_typescript, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.mcp_schema, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.mcp_overview, MAX_SUBJECT_BYTES, SubjectError),
                read_bounded(args.jsonrpc_spec, MAX_SUBJECT_BYTES, SubjectError),
            )
    except (OSError, PackError, SubjectError) as exc:
        print(
            json.dumps(
                {
                    "schema": "assay.mcp-jsonrpc-id-conformance.report.v1",
                    "status": "error",
                    "error": type(exc).__name__,
                },
                sort_keys=True,
            )
        )
        return 3
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["status"] == "contradiction" else 2


if __name__ == "__main__":
    sys.exit(main())
