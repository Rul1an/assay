#!/usr/bin/env python3
"""Fail closed unless ToolError::result publishes exactly ToolError itself."""

from __future__ import annotations

import re
import sys
from pathlib import Path


DEFAULT_TARGET = Path("crates/assay-mcp-server/src/tools/mod.rs")
MAX_SOURCE_CHARS = 1_000_000
EXPECTED_BODY = (
    'Ok(serde_json::to_value(serde_json::json!({'
    '"allowed":false,"error":self}))?)'
)


def extract_result_body(source: str) -> str | None:
    impl_match = re.search(r"impl\s+ToolError\s*\{", source)
    if not impl_match:
        return None
    result_match = re.search(
        r"pub\s+fn\s+result\s*\(\s*self\s*\)\s*->[^{\r\n]+{",
        source[impl_match.end() :],
    )
    if not result_match:
        return None

    start = impl_match.end() + result_match.end()
    depth = 1
    state = "code"
    block_depth = 0
    escaped = False
    index = start
    while index < len(source):
        char = source[index]
        pair = source[index : index + 2]
        if state == "line_comment":
            if char == "\n":
                state = "code"
        elif state == "block_comment":
            if pair == "/*":
                block_depth += 1
                index += 1
            elif pair == "*/":
                block_depth -= 1
                index += 1
                if block_depth == 0:
                    state = "code"
        elif state in {"string", "char"}:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif (state == "string" and char == '"') or (
                state == "char" and char == "'"
            ):
                state = "code"
        elif pair == "//":
            state = "line_comment"
            index += 1
        elif pair == "/*":
            state = "block_comment"
            block_depth = 1
            index += 1
        elif char == '"':
            state = "string"
        elif char == "'":
            state = "char"
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start:index]
        index += 1
    return None


def compact_rust(body: str) -> str:
    """Remove comments and code whitespace while preserving literals."""
    output: list[str] = []
    state = "code"
    block_depth = 0
    escaped = False
    index = 0
    while index < len(body):
        char = body[index]
        pair = body[index : index + 2]
        if state == "line_comment":
            if char == "\n":
                state = "code"
        elif state == "block_comment":
            if pair == "/*":
                block_depth += 1
                index += 1
            elif pair == "*/":
                block_depth -= 1
                index += 1
                if block_depth == 0:
                    state = "code"
        elif state in {"string", "char"}:
            output.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif (state == "string" and char == '"') or (
                state == "char" and char == "'"
            ):
                state = "code"
        elif pair == "//":
            state = "line_comment"
            index += 1
        elif pair == "/*":
            state = "block_comment"
            block_depth = 1
            index += 1
        elif char == '"':
            output.append(char)
            state = "string"
        elif char == "'":
            output.append(char)
            state = "char"
        elif not char.isspace():
            output.append(char)
        index += 1
    return "".join(output)


def check(source: str) -> bool:
    body = extract_result_body(source)
    if body is None:
        print("FAIL: ToolError::result(self) body not found", file=sys.stderr)
        return False
    actual = compact_rust(body)
    if actual != EXPECTED_BODY:
        print(
            "FAIL: ToolError::result must consist only of the approved direct-self "
            "publication expression",
            file=sys.stderr,
        )
        print(f"actual: {actual}", file=sys.stderr)
        return False
    print("OK: ToolError::result publishes self through ToolError::Serialize")
    return True


if __name__ == "__main__":
    if len(sys.argv) == 1:
        source = DEFAULT_TARGET.read_text()
    elif sys.argv[1:] == ["--stdin"]:
        source = sys.stdin.read(MAX_SOURCE_CHARS + 1)
        if len(source) > MAX_SOURCE_CHARS:
            print("FAIL: source exceeds guard input limit", file=sys.stderr)
            raise SystemExit(2)
    else:
        print(f"usage: {Path(sys.argv[0]).name} [--stdin]", file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(0 if check(source) else 1)
