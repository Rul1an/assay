#!/usr/bin/env python3
"""Render published adequacy prose from the typed measured-row index."""

from __future__ import annotations

import argparse
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
    "corpus",
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


def number_word(value: int, *, title: bool = False) -> str:
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value < 100:
        raise ValueError("word formatting requires an integer from 0 through 99")
    if value < 20:
        rendered = _SMALL_WORDS[value]
    else:
        tens, units = divmod(value, 10)
        rendered = _TENS_WORDS[tens] + ("-" + _SMALL_WORDS[units] if units else "")
    return rendered.capitalize() if title else rendered


def table_result(row: dict) -> str:
    if row["score_percent"] is None:
        return "control %s" % row["control"]
    in_scope = row["killed"] + row["survived"] + row["silent"]
    rendered = "%d of %d in scope (%s%%), %d survivors" % (
        row["killed"],
        in_scope,
        str(row["score_percent"]),
        row["survived"],
    )
    for field, label in (
        ("equivalent", "declared equivalent"),
        ("out_of_scope", "out of scope"),
        ("known_holes", "known holes"),
    ):
        if row[field]:
            rendered += ", %d %s" % (row[field], label)
    return rendered


def document_summary(row: dict) -> str:
    in_scope = row["killed"] + row["survived"] + row["silent"]
    return (
        "%d of %d DECLARED in-scope rules killed (%s%%). "
        "%d declared out of scope, %d rules declared.\n"
        "control-%s. %d mutant(s) survived. %d KNOWN HOLES."
    ) % (
        row["killed"],
        in_scope,
        str(row["score_percent"]),
        row["out_of_scope"],
        row["declared_total"],
        row["control"],
        row["survived"],
        row["known_holes"],
    )


def publication_values(rows: list[dict] | tuple[dict, ...]) -> dict[str, object]:
    values: dict[str, object] = {}
    for row in rows:
        prefix = row["corpus"] + "."
        for field in PUBLIC_FIELDS:
            values[prefix + field] = row[field]
        values[prefix + "in_scope"] = row["killed"] + row["survived"] + row["silent"]
        score = row["score_percent"]
        values[prefix + "score_percent"] = score
        values[prefix + "table_result"] = table_result(row)
        if score is not None:
            values[prefix + "document_summary"] = document_summary(row)
    values["aggregate.measured"] = sum(1 for row in rows if row["score_percent"] is not None)
    values["aggregate.control_only"] = sum(
        1 for row in rows if row["score_percent"] is None and row["control_status"] == "killed"
    )
    values["aggregate.summary"] = "%s measured, %s control-only" % (
        number_word(values["aggregate.measured"], title=True),
        number_word(values["aggregate.control_only"]),
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
        return number_word(value, title=style == "word-title")
    raise ValueError("unsupported template formatter %s" % style)


def render_template(template: Path, values: dict[str, object]) -> str:
    source, _metadata, _body = template_parts(template)
    corpus_names = {
        str(value) for name, value in values.items() if name.endswith(".corpus")
    }
    document_corpus = template.parent.name if template.parent.name in corpus_names else None
    table_bindings: list[str] = []
    for number, line in enumerate(source.splitlines(), 1):
        tokens = list(TOKEN.finditer(line))
        if not tokens:
            continue
        bindings = {token.group(1).rsplit(".", 1)[0] for token in tokens}
        if len(bindings) != 1:
            raise ValueError(
                "%s:%d token line must use exactly one corpus namespace"
                % (template.name, number)
            )
        bound = next(iter(bindings))
        if line.startswith("|"):
            first_cell = line.split("|", 2)[1]
            identity = bound + ".corpus"
            if [token.group(1) for token in TOKEN.finditer(first_cell)] != [identity]:
                raise ValueError(
                    "%s:%d corpus table row must render its identity from %s"
                    % (template.name, number, identity)
                )
            if [token.group(1) for token in tokens] != [identity, bound + ".table_result"]:
                raise ValueError(
                    "%s:%d corpus table result role must be %s.table_result"
                    % (template.name, number, bound)
                )
            table_bindings.append(bound)
        elif bound == "aggregate":
            if [token.group(1) for token in tokens] != ["aggregate.summary"]:
                raise ValueError(
                    "%s:%d aggregate summary role must be aggregate.summary"
                    % (template.name, number)
                )
        else:
            if document_corpus is None:
                raise ValueError(
                    "%s:%d corpus token %s appears outside a corpus table"
                    % (template.name, number, bound)
                )
            if bound != document_corpus:
                raise ValueError(
                    "%s:%d corpus token %s conflicts with document context %s"
                    % (template.name, number, bound, document_corpus)
                )
            if [token.group(1) for token in tokens] != [bound + ".document_summary"]:
                raise ValueError(
                    "%s:%d document summary role must be %s.document_summary"
                    % (template.name, number, bound)
                )

    if table_bindings:
        expected = sorted(
            str(value) for name, value in values.items() if name.endswith(".corpus")
        )
        if table_bindings != expected:
            raise ValueError(
                "%s: corpus table must bind each measured corpus exactly once in measured corpus order"
                % template.name
            )

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
    fence_start = "```json\n"
    fence_end = "\n```"
    if block.count(fence_start) != 1:
        raise ValueError(
            "%s: checked-numbers block needs exactly one JSON metadata fence" % template.name
        )
    payload_start = block.index(fence_start) + len(fence_start)
    payload_end = block.find(fence_end, payload_start)
    if payload_end < 0:
        raise ValueError("%s: checked-numbers JSON metadata fence is not closed" % template.name)
    metadata = published_rows.parse_json_object(
        block[payload_start:payload_end].encode("utf-8"),
        "%s checked-numbers metadata" % template.name,
    )
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
