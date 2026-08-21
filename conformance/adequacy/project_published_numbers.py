#!/usr/bin/env python3
"""Render published adequacy prose from the typed measured-row index."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path

import published_rows

MAX_TEMPLATE_BYTES = 1024 * 1024
TOKEN = re.compile(r"\{\{([a-z0-9_.-]+)(?::([a-z-]+))?\}\}")
BEGIN = "<!-- BEGIN CHECKED NUMBERS -->"
END = "<!-- END CHECKED NUMBERS -->"
PUBLIC_FIELDS = (
    "killed",
    "survived",
    "silent",
    "equivalent",
    "out_of_scope",
    "known_holes",
    "declared_total",
    "control",
    "control_status",
)

_SMALL_WORDS = (
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen",
    "eighteen", "nineteen",
)
_TENS_WORDS = ("", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety")


def publication_values(rows: list[dict] | tuple[dict, ...]) -> dict[str, object]:
    values: dict[str, object] = {}
    for row in rows:
        prefix = row["corpus"] + "."
        for field in PUBLIC_FIELDS:
            values[prefix + field] = row[field]
        values[prefix + "in_scope"] = row["killed"] + row["survived"] + row["silent"]
        score = row["score_percent"]
        values[prefix + "score_percent"] = score
    values["aggregate.measured"] = sum(1 for row in rows if row["score_percent"] is not None)
    values["aggregate.control_only"] = sum(
        1 for row in rows if row["score_percent"] is None and row["control_status"] == "killed"
    )
    return values


def format_value(value: object, style: str | None) -> str:
    if style is None:
        return str(value)
    if style == "compact":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ValueError("compact formatting requires a number")
        return format(value, "g")
    if style in ("word", "word-title"):
        if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value < 100:
            raise ValueError("word formatting requires an integer from 0 through 99")
        if value < 20:
            rendered = _SMALL_WORDS[value]
        else:
            tens, units = divmod(value, 10)
            rendered = _TENS_WORDS[tens] + ("-" + _SMALL_WORDS[units] if units else "")
        return rendered.capitalize() if style == "word-title" else rendered
    raise ValueError("unsupported template formatter %s" % style)


def render_template(template: Path, values: dict[str, object]) -> str:
    source, _metadata, _body = template_parts(template)

    def replace(match: re.Match[str]) -> str:
        name = match.group(1)
        if name not in values or values[name] is None:
            raise ValueError("template token %s has no non-null publication value" % name)
        return format_value(values[name], match.group(2))

    rendered = TOKEN.sub(replace, source)
    if "{{" in rendered or "}}" in rendered:
        raise ValueError("template carries an unresolved or malformed token")
    return rendered


def template_parts(template: Path) -> tuple[str, dict, str]:
    source = published_rows.read_regular_file(template, MAX_TEMPLATE_BYTES).decode("utf-8")
    if source.count(BEGIN) != 1 or source.count(END) != 1:
        raise ValueError("%s must contain exactly one checked-numbers block" % template.name)
    start = source.index(BEGIN)
    stop = source.index(END) + len(END)
    if stop < start:
        raise ValueError("%s: END marker precedes BEGIN" % template.name)
    block = source[start:stop]
    fence = re.search(r"```json\n(.*?)\n```", block, re.DOTALL)
    if not fence:
        raise ValueError("%s: checked-numbers block carries no JSON metadata" % template.name)
    try:
        metadata = json.loads(fence.group(1))
    except json.JSONDecodeError as exc:
        raise ValueError("%s: checked-numbers metadata is not valid JSON: %s" % (template.name, exc)) from exc
    if not isinstance(metadata, dict):
        raise ValueError("%s: checked-numbers metadata must be an object" % template.name)
    return source, metadata, source[:start] + source[stop:]


def document_pairs(repo: Path) -> tuple[tuple[Path, Path], ...]:
    return (
        (repo / "conformance/INDEX.md.in", repo / "conformance/INDEX.md"),
        (
            repo / "conformance/privileged-mcp-action-v0/ERRATA.md.in",
            repo / "conformance/privileged-mcp-action-v0/ERRATA.md",
        ),
    )


def render_documents(repo: Path) -> dict[Path, str]:
    loaded = published_rows.load_results(
        repo / "conformance/adequacy/results.json", require_current=True
    )
    values = publication_values(loaded.rows)
    return {output: render_template(template, values) for template, output in document_pairs(repo)}


def projection_findings(repo: Path) -> list[str]:
    findings = []
    for output, expected in render_documents(repo).items():
        try:
            actual = published_rows.read_regular_file(output, MAX_TEMPLATE_BYTES).decode("utf-8")
        except (UnicodeDecodeError, ValueError) as exc:
            findings.append(str(exc))
            continue
        if actual != expected:
            findings.append("%s differs from its fresh deterministic projection" % output.relative_to(repo))
    return findings


def write_documents(repo: Path) -> None:
    for output, rendered in render_documents(repo).items():
        temporary: Path | None = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                dir=output.parent,
                prefix=output.name + ".",
                delete=False,
            ) as handle:
                handle.write(rendered)
                handle.flush()
                os.fsync(handle.fileno())
                temporary = Path(handle.name)
            os.chmod(temporary, 0o644)
            os.replace(temporary, output)
            temporary = None
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args(argv)
    try:
        if args.check:
            findings = projection_findings(args.repo)
            if findings:
                for finding in findings:
                    print(finding, file=sys.stderr)
                return 1
        else:
            write_documents(args.repo)
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        print("published-number projection failed: %s" % exc, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
