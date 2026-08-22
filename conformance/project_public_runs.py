#!/usr/bin/env python3
"""Project reviewed, digest-addressed run records into IMPLEMENTATIONS.md."""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "conformance"))
sys.path.insert(0, str(REPO / "conformance/adequacy"))
sys.path.insert(0, str(REPO / "conformance/privileged-mcp-action-v0/scripts"))

import published_rows  # noqa: E402
from implementations import ImplementationRegistryError, load_implementations  # noqa: E402
from artifact_io import content_sha256  # noqa: E402
from strict_json import parse_strict_object  # noqa: E402
from validate_run_record import MAX_RUN_RECORD_BYTES, validate_run_record  # noqa: E402

INDEX_SCHEMA = "assay.conformance.public_runs.v0"
INDEX_FIELDS = ("schema", "runs")
RUN_FIELDS = (
    "implementation_id",
    "suite",
    "record_sha256",
    "image",
    "source",
    "commit",
    "reproduction_mode",
)
MAX_INDEX_BYTES = 64 * 1024
MAX_TEMPLATE_BYTES = 1024 * 1024
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
TABLE_TOKEN = "{{public_runs_table}}"


def _digest(data: bytes) -> str:
    return content_sha256(data)


def index_path(repo: Path) -> Path:
    return repo / "conformance/public-runs.json"


def records_dir(repo: Path) -> Path:
    return repo / "conformance/public-runs"


def record_path(repo: Path, digest: str) -> Path:
    if not SHA256.fullmatch(digest):
        raise ValueError("record_sha256 must be a sha256 content address")
    return records_dir(repo) / digest.removeprefix("sha256:")


def sort_publication_rows(rows: list[dict]) -> list[dict]:
    return sorted(rows, key=lambda row: (row["implementation_id"], row["record_sha256"]))


def escape_markdown_cell(value: object) -> str:
    return (
        str(value)
        .replace("\\", "\\\\")
        .replace("|", "\\|")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    )


def _registry_by_id(repo: Path) -> dict[str, dict]:
    try:
        document = load_implementations(repo / "conformance/implementations.json")
    except ImplementationRegistryError as exc:
        raise ValueError("implementation registry rejected: %s" % exc) from exc
    return {row["id"]: row for row in document["implementations"]}


def _load_index(repo: Path) -> list[dict]:
    data = published_rows.read_regular_file(index_path(repo), MAX_INDEX_BYTES)
    document = published_rows.parse_json_object(data, "public-runs index")
    extra = set(document) - set(INDEX_FIELDS)
    missing = [field for field in INDEX_FIELDS if field not in document]
    if extra or missing:
        raise ValueError("public-runs index has missing or surplus fields")
    if document["schema"] != INDEX_SCHEMA:
        raise ValueError("unsupported public-runs schema")
    runs = document["runs"]
    if not isinstance(runs, list):
        raise ValueError("public-runs runs must be a list")
    seen: set[tuple[str, str]] = set()
    clean: list[dict] = []
    for row in runs:
        if not isinstance(row, dict) or set(row) != set(RUN_FIELDS):
            raise ValueError("public-runs row has missing or surplus fields")
        ident = row["implementation_id"]
        digest = row["record_sha256"]
        if not isinstance(ident, str):
            raise ValueError("implementation_id must be a string")
        if not SHA256.fullmatch(digest):
            raise ValueError("record_sha256 must be a sha256 content address")
        key = (ident, digest)
        if key in seen:
            raise ValueError("duplicate public-run identity: %s %s" % key)
        seen.add(key)
        clean.append(row)
    return clean


def _listed_record_names(repo: Path) -> set[str]:
    directory = records_dir(repo)
    if not directory.exists():
        return set()
    if not directory.is_dir() or directory.is_symlink():
        raise ValueError("%s is not a regular directory" % directory)
    names: set[str] = set()
    for entry in directory.iterdir():
        if entry.name.startswith("."):
            continue
        if entry.is_symlink() or not entry.is_file() or not HEX64.fullmatch(entry.name):
            raise ValueError("surplus record %s" % entry.name)
        names.add(entry.name)
    return names


def _bind_row(row: dict, report: dict, registry: dict[str, dict]) -> None:
    implementation = report["implementation"]
    if "id" not in implementation or "image" not in implementation:
        raise ValueError("publication requires implementation id and image")
    registered = registry.get(row["implementation_id"])
    if registered is None:
        raise ValueError("implementation_id mismatch: unknown %s" % row["implementation_id"])
    checks = (
        ("implementation_id", row["implementation_id"], implementation["id"], registered["id"]),
        ("suite", row["suite"], report["suite"], registered["suite"]),
        ("image", row["image"], implementation["image"], registered["image"]),
        ("source", row["source"], implementation["source"], registered["source"]),
        ("commit", row["commit"], implementation["commit"], registered["commit"]),
        (
            "reproduction_mode",
            row["reproduction_mode"],
            implementation["reproduction_mode"],
            registered["reproduction_mode"],
        ),
    )
    for name, indexed, recorded, expected in checks:
        if not indexed == recorded == expected:
            raise ValueError("%s mismatch" % name)


def load_publication(repo: Path) -> list[dict]:
    rows = _load_index(repo)
    present = _listed_record_names(repo)
    expected = {row["record_sha256"].removeprefix("sha256:") for row in rows}
    missing = expected - present
    surplus = present - expected
    if missing:
        raise ValueError("missing record %s" % sorted(missing)[0])
    if surplus:
        raise ValueError("surplus record %s" % sorted(surplus)[0])
    registry = _registry_by_id(repo)
    loaded: list[dict] = []
    for row in rows:
        path = record_path(repo, row["record_sha256"])
        data = published_rows.read_regular_file(path, MAX_RUN_RECORD_BYTES)
        digest = _digest(data)
        if digest != row["record_sha256"]:
            raise ValueError("record digest mismatch: %s" % digest)
        report = parse_strict_object(data, label="run record")
        validate_run_record(report)
        _bind_row(row, report, registry)
        loaded.append(
            {
                "implementation_id": row["implementation_id"],
                "suite": row["suite"],
                "record_sha256": row["record_sha256"],
                "image": row["image"],
                "source": row["source"],
                "commit": row["commit"],
                "reproduction_mode": row["reproduction_mode"],
                "summary": report["summary"],
            }
        )
    return sort_publication_rows(loaded)


def render_table(rows: list[dict]) -> str:
    lines = []
    for row in rows:
        summary = row["summary"]
        digest = row["record_sha256"]
        record_cell = "[%s](%s)" % (
            escape_markdown_cell(digest),
            escape_markdown_cell("public-runs/" + digest.removeprefix("sha256:")),
        )
        lines.append(
            "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |"
            % (
                escape_markdown_cell(row["implementation_id"]),
                escape_markdown_cell(row["suite"]),
                record_cell,
                escape_markdown_cell(row["image"]),
                escape_markdown_cell(row["commit"]),
                escape_markdown_cell(row["reproduction_mode"]),
                escape_markdown_cell(summary["match"]),
                escape_markdown_cell(summary["mismatch"]),
                escape_markdown_cell(summary["execution_error"]),
                escape_markdown_cell(summary["harness_error"]),
                escape_markdown_cell(summary["review_warnings"]),
            )
        )
    return "\n".join(lines)


def render_document(repo: Path, rows: list[dict]) -> str:
    template = repo / "conformance/IMPLEMENTATIONS.md.in"
    source = published_rows.read_regular_file(template, MAX_TEMPLATE_BYTES).decode("utf-8")
    if source.count(TABLE_TOKEN) != 1:
        raise ValueError("IMPLEMENTATIONS.md.in must contain exactly one table token")
    rendered = source.replace(TABLE_TOKEN, render_table(rows))
    if "{{" in rendered or "}}" in rendered:
        raise ValueError("template carries an unresolved or malformed token")
    return rendered


def projection_findings(repo: Path) -> list[str]:
    try:
        rows = load_publication(repo)
        expected = render_document(repo, rows)
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        return [str(exc)]
    output = repo / "conformance/IMPLEMENTATIONS.md"
    try:
        actual = published_rows.read_regular_file(output, MAX_TEMPLATE_BYTES).decode("utf-8")
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        return [str(exc)]
    if actual != expected:
        return ["conformance/IMPLEMENTATIONS.md differs from its fresh deterministic projection"]
    return []


def write_document(repo: Path) -> None:
    rendered = render_document(repo, load_publication(repo))
    output = repo / "conformance/IMPLEMENTATIONS.md"
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=output.parent,
            prefix=output.name + ".",
            delete=False,
        ) as handle:
            temporary = Path(handle.name)
            handle.write(rendered)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, output)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--repo", type=Path, default=REPO)
    args = parser.parse_args(argv)
    try:
        if args.check:
            findings = projection_findings(args.repo)
            if findings:
                for finding in findings:
                    print(finding, file=sys.stderr)
                return 1
        else:
            write_document(args.repo)
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        print("public-run projection failed: %s" % exc, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
