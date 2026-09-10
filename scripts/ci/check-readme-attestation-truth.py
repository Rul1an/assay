#!/usr/bin/env python3
"""Refuse a README attestation row that names a statement or predicate the source does not emit.

The README's "What ships" table described attestation as an "in-toto / DSSE statement (v0)" on
v6.0.0 and v6.1.0 while `crates/assay-evidence/src/attestation.rs` emitted an in-toto v1
Statement with the evidence-bundle/v1 predicate (#2859). Every release gate passed it, because
none of them read the README (#2875).

The source is the only vocabulary, read from two sides with this one rule:

- On the release path, where no Rust toolchain exists, the names come from the source text: what
  `statement_from_parts` puts in `type_` and `predicate_type`, resolved through the constants it
  names. Every public constructor goes through that function; reading it rather than a fixed
  constant name means switching it to another constant is caught. The deprecated
  `statement_from_manifest` also assembles a statement, with the v0 predicate the verifier refuses,
  and is deliberately not read.
- Under required CI, `readme_attestation_row_names_what_the_shipped_constructor_emits` in
  `attestation.rs` runs the constructor the CLI calls and passes the URIs it returns through
  `--statement-type` and `--predicate-type`. That covers a change on the ship path the source
  reading cannot see, such as a type reassigned after `statement_from_parts` returns.

The README row is the projection and must state both names, and no other version. Anything this
cannot read is a failure, never a pass: a renamed emitter, a constant that is not a plain string,
an assignment in an unfamiliar form, a URI outside the expected shape, or not exactly one row.

Usage: check-readme-attestation-truth.py [--root DIR] [--statement-type URI --predicate-type URI]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

README = "README.md"
SOURCE = "crates/assay-evidence/src/attestation.rs"
EMITTER = "statement_from_parts"
ROW_PREFIX = "| **Attestation** | "

STATEMENT_URI = re.compile(r"https://in-toto\.io/Statement/(v\d+(?:\.\d+)?)")
PREDICATE_URI = re.compile(r"https://[^\s\"]+/attestation/([a-z0-9-]+/(v\d+(?:\.\d+)?))")
VERSION_TOKEN = re.compile(r"(?<![\w.])v\d+(?:\.\d+)*(?![\w])")


class CheckError(Exception):
    pass


def emitter_body(source: str) -> str:
    starts = [m.start() for m in re.finditer(rf"^fn {EMITTER}\(", source, re.MULTILINE)]
    if len(starts) != 1:
        raise CheckError(f"{SOURCE}: expected exactly one `fn {EMITTER}(`, found {len(starts)}")
    end = re.compile(r"^\}", re.MULTILINE).search(source, starts[0])
    if end is None:
        raise CheckError(f"{SOURCE}: `fn {EMITTER}` has no closing brace at column 0")
    return source[starts[0]:end.end()]


def emitted_constant(body: str, field: str) -> str:
    names = re.findall(rf"^\s*{field}: ([A-Z][A-Z0-9_]*)\.to_string\(\),$", body, re.MULTILINE)
    if len(names) != 1:
        raise CheckError(
            f"{SOURCE}: `{EMITTER}` must set `{field}` from exactly one named constant, "
            f"found {len(names)}"
        )
    return names[0]


def constant_value(source: str, name: str) -> str:
    values = re.findall(rf"^(?:pub )?const {name}: &str =\s*\"([^\"\\]+)\";", source, re.MULTILINE)
    if len(values) != 1:
        raise CheckError(f"{SOURCE}: expected one plain string constant `{name}`, found {len(values)}")
    return values[0]


def source_uris(source: str) -> tuple[str, str]:
    """Return (statement type, predicate type) as the source text says `statement_from_parts` emits."""
    body = emitter_body(source)
    return (constant_value(source, emitted_constant(body, "type_")),
            constant_value(source, emitted_constant(body, "predicate_type")))


def names(statement: str, predicate: str) -> tuple[str, str, str]:
    """Return (statement version, predicate name, predicate version) from the two type URIs."""
    statement_match = STATEMENT_URI.fullmatch(statement)
    if statement_match is None:
        raise CheckError(f"{SOURCE}: statement type {statement!r} is not an in-toto Statement URI")
    predicate_match = PREDICATE_URI.fullmatch(predicate)
    if predicate_match is None:
        raise CheckError(f"{SOURCE}: predicate type {predicate!r} is not an attestation URI")
    return statement_match.group(1), predicate_match.group(1), predicate_match.group(2)


def readme_row(readme: str) -> str:
    rows = [line for line in readme.splitlines() if line.startswith(ROW_PREFIX)]
    if len(rows) != 1:
        raise CheckError(f"{README}: expected exactly one README attestation row, found {len(rows)}")
    return rows[0]


def problems(root: Path, emitted: tuple[str, str] | None = None) -> list[str]:
    try:
        readme = (root / README).read_text(encoding="utf-8")
        if emitted is None:
            emitted = source_uris((root / SOURCE).read_text(encoding="utf-8"))
        statement_version, predicate_name, predicate_version = names(*emitted)
        row = readme_row(readme)
    except (OSError, CheckError) as exc:
        return [str(exc)]

    found = []
    for phrase in (f"in-toto {statement_version} Statement", f"{predicate_name} predicate"):
        if phrase not in row:
            found.append(f"{README}: attestation row does not state \"{phrase}\", which "
                         f"the statement emits: {row}")
    allowed = {statement_version, predicate_version}
    for token in sorted(set(VERSION_TOKEN.findall(row)) - allowed):
        found.append(f"{README}: attestation row names {token}, which the emitted statement "
                     f"does not carry: {row}")
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--statement-type", help="statement type URI a running constructor emitted")
    parser.add_argument("--predicate-type", help="predicate type URI a running constructor emitted")
    args = parser.parse_args()
    if (args.statement_type is None) != (args.predicate_type is None):
        parser.error("--statement-type and --predicate-type go together")
    emitted = None if args.statement_type is None else (args.statement_type, args.predicate_type)
    found = problems(args.root, emitted)
    for problem in found:
        print(f"FAIL: {problem}", file=sys.stderr)
    if found:
        return 1
    print("README attestation row: agrees with the statement and predicate the source emits")
    return 0


if __name__ == "__main__":
    sys.exit(main())
