#!/usr/bin/env python3
"""Refuse a documented CoverageReport key that the serialised type does not have.

`docs/getting-started/python-quickstart.md` named `passed`, `violations`, and
`score` as the `validate()` return (#3095). `validate()` returns
`Coverage.analyze()` unchanged, which `json.loads` of `serde_json::to_string`
on `CoverageReport`. Those three names are not fields, and the struct carries
no serialization-changing serde directive, so every subscript on that page is a
KeyError.

This check is a coverage remap of documented report keys into the
CoverageReport field table, not a second completeness proof of the pages.
It does not see an invented console table, a POSIX-only setup step, or a
prose lie that never writes a subscript or `report dict (...)` list.

The source is the only vocabulary. Field names are the serialised keys when
CoverageReport has no serialization-changing serde directive on the container
or its fields. `serde(default)` is accepted. A rename, rename_all, skip,
flatten, unrecognized serde directive, missing struct, missing configured
method, unreadable page, or unreadable source is a failure, never a pass:
this cannot guess a second mapping.

Pages are listed explicitly. A docs-wide walk of every `report[` / `result[`
subscript would need a map from documented keys onto the serialised type they
belong to: `docs/use-cases/self-correction.md` and `docs/mcp/self-correction.md`
write `result["allowed"]` for `assay_check_args`, a different API. This check
does not carry that map. The listed pages document `validate()` /
`Coverage.analyze()` against CoverageReport. Attribute access such as
`coverage.score` is not extracted; that limit is tracked in #3105.

`METHODS` is a closed singleton for `Coverage.analyze`. Stdlib `ast` isolates
that one method. Only backticked ident bullets under that method's own
`Returns:` section are compared to `coverage_report_fields()`. An empty
extract is not a completeness proof. Adding the Python file to `PAGES` is not
this extract: `PAGES` reads subscripts, `.get(...)`, and `report dict (...)`.

The harness treats every `PAGES` entry as load-bearing: it plants a fabricated
key on each listed page, then drops each entry in turn and requires that plant
to go unobserved. That is not a second pinned list. A decorative entry that
can be removed without turning the harness red is the defect. The same invert
applies to the `METHODS` singleton and to `coverage_report_fields()`.

Usage: check-docs-serialized-keys.py [--root DIR]
"""

from __future__ import annotations

import argparse
import ast
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
# Closed singleton: Coverage.analyze Returns bullets vs CoverageReport.
# Adding a Python file to PAGES is not this extract; PAGES reads subscripts.
METHODS = (
    ("assay-python-sdk/python/assay/coverage.py", "Coverage", "analyze"),
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
SERDE_CALL = re.compile(r"(?<![A-Za-z0-9_])serde\s*\(")
RETURNS_HEADER = re.compile(r"^(\s*)Returns:\s*$")
KEY_BULLET = re.compile(r"^(\s*)-\s+`([^`]+)`")
SECTION = re.compile(r"^(\s*)([A-Za-z][A-Za-z0-9_ ]*):")
SIMPLE_KEY = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
ALLOWED_SERDE_DIRECTIVES = frozenset({"default"})


class CheckError(Exception):
    pass


def _balanced(text: str, start: int, opener: str, closer: str) -> tuple[str, int]:
    if start >= len(text) or text[start] != opener:
        raise CheckError(f"{SOURCE}: unbalanced {opener}{closer} on CoverageReport")
    depth = 0
    i = start
    quote = None
    escape = False
    while i < len(text):
        ch = text[i]
        if quote is not None:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == quote:
                quote = None
        elif ch in "\"'":
            quote = ch
        elif ch == opener:
            depth += 1
        elif ch == closer:
            depth -= 1
            if depth == 0:
                return text[start + 1 : i], i + 1
        i += 1
    raise CheckError(f"{SOURCE}: unbalanced {opener}{closer} on CoverageReport")


def _top_level_parts(inner: str) -> list[str]:
    parts: list[str] = []
    buf: list[str] = []
    depth = 0
    quote = None
    escape = False
    for ch in inner:
        if quote is not None:
            buf.append(ch)
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == quote:
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            buf.append(ch)
            continue
        if ch in "([{":
            depth += 1
            buf.append(ch)
            continue
        if ch in ")]}":
            depth -= 1
            buf.append(ch)
            continue
        if ch == "," and depth == 0:
            part = "".join(buf).strip()
            if part:
                parts.append(part)
            buf = []
            continue
        buf.append(ch)
    tail = "".join(buf).strip()
    if tail:
        parts.append(tail)
    return parts


def _attribute_bodies(text: str) -> list[str]:
    bodies: list[str] = []
    cursor = 0
    while True:
        start = text.find("#[", cursor)
        if start < 0:
            return bodies
        body, end = _balanced(text, start + 1, "[", "]")
        bodies.append(body)
        cursor = end


def _serde_directives(text: str) -> list[str]:
    names: list[str] = []
    for match in SERDE_CALL.finditer(text):
        inner, _ = _balanced(text, match.end() - 1, "(", ")")
        for part in _top_level_parts(inner):
            name = IDENT.match(part)
            if name is None:
                raise CheckError(
                    f"{SOURCE}: `CoverageReport` carries unreadable serde({part!r})"
                )
            names.append(name.group(0))
    return names


def _container_region(source: str, head_start: int) -> str:
    prev = 0
    for match in re.finditer(r"^\}", source[:head_start], re.MULTILINE):
        prev = match.end()
    return source[prev:head_start]


def coverage_report_fields(source: str) -> set[str]:
    heads = list(STRUCT_HEAD.finditer(source))
    if len(heads) != 1:
        raise CheckError(
            f"{SOURCE}: expected exactly one `pub struct CoverageReport`, found {len(heads)}"
        )
    head = heads[0]
    close = re.search(r"^\}", source[head.end() :], re.MULTILINE)
    if close is None:
        raise CheckError(f"{SOURCE}: `CoverageReport` has no closing brace at column 0")
    body = source[head.end() : head.end() + close.start()]
    region = _container_region(source, head.start()) + body
    for attr in _attribute_bodies(region):
        for name in _serde_directives(attr):
            if name not in ALLOWED_SERDE_DIRECTIVES:
                raise CheckError(
                    f"{SOURCE}: `CoverageReport` carries serde({name}); this check reads "
                    "field names as serialised keys and cannot interpret a "
                    "serialization-changing or unrecognized serde directive"
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


def _method_def(
    tree: ast.AST, class_name: str, method_name: str
) -> ast.FunctionDef:
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == class_name:
            for item in node.body:
                if isinstance(item, ast.FunctionDef) and item.name == method_name:
                    return item
    raise CheckError(f"missing {class_name}.{method_name}")


def documented_return_keys(
    source: str, class_name: str, method_name: str
) -> list[tuple[int, str]]:
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        raise CheckError(f"syntax error: {exc}") from exc
    func = _method_def(tree, class_name, method_name)
    if not func.body:
        return []
    first = func.body[0]
    if not (
        isinstance(first, ast.Expr)
        and isinstance(first.value, ast.Constant)
        and isinstance(first.value.value, str)
    ):
        return []
    lines = source.splitlines()
    end = first.end_lineno or first.lineno
    in_returns = False
    returns_indent: int | None = None
    found: list[tuple[int, str]] = []
    for lineno in range(first.lineno, end + 1):
        line = lines[lineno - 1]
        header = RETURNS_HEADER.match(line)
        if header:
            in_returns = True
            returns_indent = len(header.group(1))
            continue
        if not in_returns:
            continue
        section = SECTION.match(line)
        if (
            section
            and returns_indent is not None
            and len(section.group(1)) <= returns_indent
            and section.group(2) != "Returns"
        ):
            break
        bullet = KEY_BULLET.match(line)
        if bullet is None:
            continue
        key = bullet.group(2)
        if SIMPLE_KEY.fullmatch(key) is None:
            continue
        found.append((lineno, key))
    return found


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

    for item in METHODS:
        if not isinstance(item, tuple) or len(item) != 3 or not all(item):
            found.append("METHODS entry is malformed; expected (path, class, method)")
            continue
        rel, cls, meth = item
        path = root / rel
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            found.append(f"{rel}: {exc}")
            continue
        try:
            keys = documented_return_keys(text, cls, meth)
        except CheckError as exc:
            found.append(f"{rel}: {exc}")
            continue
        for lineno, key in keys:
            if key not in fields:
                found.append(
                    f"{rel}:{lineno}: documents {key!r}, which CoverageReport does not serialise"
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
