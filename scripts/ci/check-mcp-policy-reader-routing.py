#!/usr/bin/env python3
"""Architecture drift guard for the MCP bounded policy reader (#2389).

Behavioral safety is proved by real-stdio and unit tests. This guard pins the
one generic Read helper, the spawn_blocking File open, and the five tool
callsites. It is not Rust data-flow analysis.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


SERVER_TOOLS = Path("crates/assay-mcp-server/src/tools")
READER = SERVER_TOOLS / "policy_read.rs"
TOOL_FILES = [
    SERVER_TOOLS / "policy_decide.rs",
    SERVER_TOOLS / "check_sequence.rs",
    SERVER_TOOLS / "check_coverage.rs",
    SERVER_TOOLS / "explain_trace.rs",
    SERVER_TOOLS / "check_args.rs",
]


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


def function_body(source: str, signature: str) -> str | None:
    start = source.find(signature)
    if start < 0:
        return None
    brace = source.find("{", start)
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
    if not READER.is_file():
        print("FAIL: policy reader module not found", file=sys.stderr)
        return False
    reader = READER.read_text()
    bounded = function_body(reader, "fn read_bounded")
    async_entry = function_body(reader, "async fn read_policy_bounded")
    if bounded is None:
        errors.append("read_bounded is missing")
    else:
        body = compact(bounded)
        if "LimitReader::new(" not in body:
            errors.append("read_bounded must construct LimitReader")
        if "metadata(" in body or ".len()" in body and "metadata" in body:
            errors.append("read_bounded must not accept via metadata")
        if "read_to_end" not in body:
            errors.append("read_bounded must accumulate through the LimitReader")
    if async_entry is None:
        errors.append("read_policy_bounded is missing")
    else:
        body = compact(async_entry)
        if "spawn_blocking" not in body:
            errors.append("read_policy_bounded must read inside spawn_blocking")
        if "File::open(" not in body:
            errors.append("read_policy_bounded must open std::fs::File")
        if "read_bounded(" not in body:
            errors.append("read_policy_bounded must call read_bounded")
        if "metadata(" in body:
            errors.append("read_policy_bounded must not consult metadata")
        if "read_to_end" in body:
            errors.append("read_policy_bounded must not duplicate the read")

    for rel in TOOL_FILES:
        if not rel.is_file():
            errors.append(f"{rel}: missing")
            continue
        source = rel.read_text()
        effective = code_only(source)
        if "read_policy_bounded(" not in compact(source):
            errors.append(f"{rel}: must call read_policy_bounded")
        if re.search(r"tokio::fs::read(?:_to_string)?\s*\(", effective):
            errors.append(f"{rel}: direct tokio::fs::read bypass")
        if rel.name == "check_args.rs":
            if "McpPolicy::from_file(" in compact(source):
                errors.append("check_args must not call McpPolicy::from_file")
            if "McpPolicy::from_slice(" not in compact(source):
                errors.append("check_args must parse through McpPolicy::from_slice")

    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        return False
    print("OK: MCP policy reader routes through one LimitReader helper")
    return True


if __name__ == "__main__":
    if len(sys.argv) != 1:
        print(f"usage: {Path(sys.argv[0]).name}", file=sys.stderr)
        raise SystemExit(1)
    raise SystemExit(0 if check() else 1)
