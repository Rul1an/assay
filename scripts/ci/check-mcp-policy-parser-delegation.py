#!/usr/bin/env python3
"""Architecture drift guard for the one full-policy bytes parser (#2389)."""

from __future__ import annotations

import re
import sys
from pathlib import Path


MOD = Path("crates/assay-core/src/mcp/policy/mod.rs")
LEGACY = Path("crates/assay-core/src/mcp/policy/legacy.rs")
PARSER_RE = re.compile(
    r"(?:serde_yaml::(?:from_slice|from_str|from_reader|from_value|Deserializer)|"
    r"serde_ignored::deserialize)"
)


def code_only(source: str) -> str:
    out = list(source)
    state = "code"
    block_depth = 0
    escaped = False
    raw_hashes = 0
    index = 0
    while index < len(source):
        char = source[index]
        pair = source[index : index + 2]
        if state == "code":
            raw = re.match(r'r(#{0,16})"', source[index:])
            if raw:
                raw_hashes = len(raw.group(1))
                end = index + len(raw.group(0))
                for pos in range(index, end):
                    out[pos] = " "
                index = end
                state = "raw"
                continue
            if pair == "//":
                out[index] = out[index + 1] = " "
                index += 2
                state = "line"
                continue
            if pair == "/*":
                out[index] = out[index + 1] = " "
                index += 2
                block_depth = 1
                state = "block"
                continue
            if char == '"':
                out[index] = " "
                state = "string"
        elif state == "line":
            if char == "\n":
                state = "code"
            else:
                out[index] = " "
        elif state == "block":
            out[index] = " "
            if pair == "/*":
                out[index + 1] = " "
                block_depth += 1
                index += 1
            elif pair == "*/":
                out[index + 1] = " "
                block_depth -= 1
                index += 1
                if block_depth == 0:
                    state = "code"
        elif state == "string":
            out[index] = " "
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                state = "code"
        elif state == "raw":
            out[index] = " "
            closing = '"' + ("#" * raw_hashes)
            if source.startswith(closing, index):
                for pos in range(index, index + len(closing)):
                    out[pos] = " "
                index += len(closing) - 1
                state = "code"
        index += 1
    return "".join(out)


def compact(source: str) -> str:
    return re.sub(r"\s+", "", code_only(source))


def impl_method(source: str, name: str) -> str | None:
    pattern = re.compile(rf"(?:pub(?:\(super\))?\s+)?fn {name}\s*\(")
    match = pattern.search(source)
    if not match:
        return None
    brace = source.find("{", match.start())
    if brace < 0:
        return None
    depth = 0
    for index, char in enumerate(source[brace:], brace):
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    return None


def check() -> bool:
    errors: list[str] = []
    mod_src = MOD.read_text()
    legacy_src = LEGACY.read_text()

    public_file = impl_method(mod_src, "from_file")
    public_slice = impl_method(mod_src, "from_slice")
    public_str = impl_method(mod_src, "from_str")
    legacy_file = impl_method(legacy_src, "from_file")
    legacy_slice = impl_method(legacy_src, "from_slice")

    hops = [
        (public_file, "legacy::from_file(", "public from_file -> legacy::from_file"),
        (legacy_file, "McpPolicy::from_slice(", "legacy::from_file -> McpPolicy::from_slice"),
        (public_slice, "legacy::from_slice(", "public from_slice -> legacy::from_slice"),
        (public_str, "Self::from_slice(", "public from_str -> from_slice"),
    ]
    for body, fragment, label in hops:
        if body is None:
            errors.append(f"{label}: missing method")
            continue
        if compact(body).count(re.sub(r"\s+", "", fragment)) != 1:
            errors.append(f"{label}: expected one {fragment}")
        if PARSER_RE.search(code_only(body)):
            errors.append(f"{label}: hop must not construct a parser")

    if legacy_slice is None:
        errors.append("legacy::from_slice is missing")
    elif "serde_ignored::deserialize" not in compact(legacy_slice):
        errors.append("legacy::from_slice must own the deserialize sequence")

    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        return False
    print("OK: full-policy parser hops delegate to one bytes-in function")
    return True


if __name__ == "__main__":
    if len(sys.argv) != 1:
        print(f"usage: {Path(sys.argv[0]).name}", file=sys.stderr)
        raise SystemExit(1)
    raise SystemExit(0 if check() else 1)
