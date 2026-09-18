#!/usr/bin/env python3
"""Refuse a documented CoverageReport key that the serialised type does not have.

`docs/getting-started/python-quickstart.md` named `passed`, `violations`, and
`score` as the `validate()` return (#3095). `validate()` returns
`Coverage.analyze()` unchanged, which `json.loads` of `serde_json::to_string`
on `CoverageReport`. Those three names are not fields, and the struct carries
no `serde(rename)`, so every subscript on that page is a KeyError.

This check is a coverage remap of documented report keys into the
CoverageReport field table, not a second completeness proof of the pages.
It does not see an invented console table, a POSIX-only setup step, or a
prose lie that never writes a subscript or `report dict (...)` list.

The source is the only vocabulary. Field names are the serialised keys when
the struct body has no `serde(rename)`. A rename, a missing struct, or an
unreadable page is a failure, never a pass: this cannot guess a second
mapping.

Pages are listed explicitly. A docs-wide walk of every `report[` / `result[`
subscript would need a map from documented keys onto the serialised type they
belong to: `docs/use-cases/self-correction.md` and `docs/mcp/self-correction.md`
write `result["allowed"]` for `assay_check_args`, a different API. This check
does not carry that map. The listed pages document `validate()` /
`Coverage.analyze()` against CoverageReport. Attribute access such as
`coverage.score` is not extracted; that limit is tracked in #3105.

Usage: check-docs-serialized-keys.py [--root DIR]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SOURCE = "crates/assay-core/src/coverage_next/types.rs"
PAGES = (
    "docs/getting-started/python-quickstart.md",
    "docs/python-sdk/index.md",
    "docs/AIcontext/entry-points.md",
    "docs/AIcontext/quick-reference.md",
)

STRUCT_HEAD = re.compile(r"^pub struct CoverageReport \{", re.MULTILINE)
FIELD = re.compile(r"^\s*pub ([a-z][A-Za-z0-9_]*):", re.MULTILINE)
SUBSCRIPT = re.compile(
    r"""\b(report|result)\s*\[\s*['\"]([A-Za-z_][A-Za-z0-9_]*)['\"]\s*\]"""
)
GET = re.compile(
    r"""\b(report|result)\s*\.\s*get\s*\(\s*['\"]([A-Za-z_][A-Za-z0-9_]*)['\"]"""
)
# The historical lie was a parenthetical on the validate() return line.
PROSE_LIST = re.compile(r"report dict\s*\(([^)]*)\)")
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


class CheckError(Exception):
    pass


def coverage_report_fields(source: str) -> set[str]:
    heads = list(STRUCT_HEAD.finditer(source))
    if len(heads) != 1:
        raise CheckError(
            f"{SOURCE}: expected exactly one `pub struct CoverageReport`, found {len(heads)}"
        )
    start = heads[0].end()
    close = re.search(r"^\}", source[start:], re.MULTILINE)
    if close is None:
        raise CheckError(f"{SOURCE}: `CoverageReport` has no closing brace at column 0")
    body = source[start : start + close.start()]
    if "serde(rename" in body:
        raise CheckError(
            f"{SOURCE}: `CoverageReport` carries serde(rename); this check reads "
            "field names as serialised keys and cannot interpret a rename"
        )
    fields = set(FIELD.findall(body))
    if not fields:
        raise CheckError(f"{SOURCE}: `CoverageReport` has no readable pub fields")
    return fields


def documented_keys(page: str) -> set[str]:
    keys = {match.group(2) for match in SUBSCRIPT.finditer(page)}
    keys.update(match.group(2) for match in GET.finditer(page))
    for match in PROSE_LIST.finditer(page):
        keys.update(IDENT.findall(match.group(1)))
    return keys


def problems(root: Path) -> list[str]:
    found: list[str] = []
    try:
        fields = coverage_report_fields((root / SOURCE).read_text(encoding="utf-8"))
    except (OSError, CheckError) as exc:
        return [str(exc)]

    for rel in PAGES:
        path = root / rel
        try:
            page = path.read_text(encoding="utf-8")
        except OSError as exc:
            found.append(f"{rel}: {exc}")
            continue
        unknown = sorted(documented_keys(page) - fields)
        for key in unknown:
            found.append(
                f"{rel}: documents {key!r}, which CoverageReport does not serialise "
                f"(fields: {', '.join(sorted(fields))})"
            )
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    found = problems(args.root)
    for problem in found:
        print(f"FAIL: {problem}", file=sys.stderr)
    if found:
        return 1
    print(
        "documented coverage-report keys: agree with CoverageReport field names"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
