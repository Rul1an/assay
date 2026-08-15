#!/usr/bin/env python3
"""Guard: ToolError::result must serialize through ToolError's Serialize impl.

Rejects four bypasses:
  1. Unbounded repack: rebuilding error from public fields without bounding.
  2. Bounded repack: calling bound_public_message but constructing a new struct
     that bypasses ToolError::Serialize.
  3. Commented decoy: a comment containing `"error": self` satisfying a naive
     substring check while the real body repacks fields.
  4. Dead/unrelated decoy: an `"error": self` in a different function or
     dead code satisfying a naive whole-file search.

The guard extracts the result() function body from the ToolError impl block,
strips line comments, and verifies that the body contains `"error": self` as
an expression (not inside a comment or string), and does not access individual
fields or call bound_public_message.

Usage:
    python3 check-mcp-tool-error-publication.py [SOURCE_PATH]

SOURCE_PATH defaults to crates/assay-mcp-server/src/tools/mod.rs.
"""
import re
import sys

DEFAULT_TARGET = "crates/assay-mcp-server/src/tools/mod.rs"


def strip_line_comments(text: str) -> str:
    """Remove // comments from each line, preserving strings naively.

    This is intentionally simple: it handles the patterns that appear in
    tools/mod.rs without trying to be a full Rust lexer. It strips from
    the first `//` that is not inside a quoted string on each line.
    """
    lines = []
    for line in text.split("\n"):
        # Walk the line tracking whether we're inside a string
        in_string = False
        escape_next = False
        i = 0
        while i < len(line):
            ch = line[i]
            if escape_next:
                escape_next = False
                i += 1
                continue
            if ch == "\\":
                escape_next = True
                i += 1
                continue
            if ch == '"' and not in_string:
                in_string = True
                i += 1
                continue
            if ch == '"' and in_string:
                in_string = False
                i += 1
                continue
            if not in_string and i + 1 < len(line) and line[i : i + 2] == "//":
                line = line[:i]
                break
            i += 1
        lines.append(line)
    return "\n".join(lines)


def extract_result_body(source: str) -> str | None:
    """Extract the body of ToolError::result(self) from the impl block.

    Finds `impl ToolError` then `pub fn result(self)` inside it, and
    extracts the brace-delimited body. Returns None if not found.
    """
    # Find the impl ToolError block
    impl_match = re.search(r"impl\s+ToolError\s*\{", source)
    if not impl_match:
        return None

    # From the impl block, find pub fn result(self)
    impl_start = impl_match.start()
    result_match = re.search(
        r"pub\s+fn\s+result\s*\(\s*self\s*\)\s*->\s*[^{]+\{",
        source[impl_start:],
    )
    if not result_match:
        return None

    # Find the matching closing brace by counting braces
    body_start = impl_start + result_match.end()
    depth = 1
    i = body_start
    while i < len(source) and depth > 0:
        if source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
        i += 1

    if depth != 0:
        return None

    return source[body_start : i - 1]


def check(path: str) -> bool:
    with open(path) as f:
        source = f.read()

    errors = []

    raw_body = extract_result_body(source)
    if raw_body is None:
        errors.append(
            "pub fn result(self) not found inside impl ToolError"
        )
        for e in errors:
            print(f"FAIL: {e}", file=sys.stderr)
        return False

    # Strip comments so a `// "error": self` decoy cannot satisfy the check
    body = strip_line_comments(raw_body)

    # 1. The body must contain `"error": self` as an expression.
    #    We require it as a non-comment token in the function body.
    #    The regex ensures it's not inside a surrounding identifier
    #    (e.g. `self.something` would be caught by rule 3 below).
    has_error_self = bool(re.search(r'"error"\s*:\s*self\b(?!\.)', body))
    if not has_error_self:
        errors.append(
            'result() must serialize self directly ("error": self), '
            "not repack from fields"
        )

    # 2. The body must NOT access individual fields, which indicates repacking.
    for field in ("self.code", "self.message", "self.details"):
        if field in body:
            errors.append(
                f"result() must not access {field} — that bypasses Serialize"
            )

    # 3. The body must NOT call bound_public_message — bounding is the
    #    Serialize impl's responsibility, not result()'s.
    if "bound_public_message" in body:
        errors.append(
            "result() must not call bound_public_message — "
            "the Serialize impl handles bounding"
        )

    if errors:
        for e in errors:
            print(f"FAIL: {e}", file=sys.stderr)
        return False

    print("OK: result() serializes self through ToolError::Serialize")
    return True


if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_TARGET
    sys.exit(0 if check(path) else 1)
