#!/usr/bin/env python3
"""Architecture drift guard for the MCP policy YAML parsing routes (#2387).

Behavioral safety is proved by real-stdio tests. This guard separately pins the
approved parser-construction sites and the helper calls that keep classification
in one place; it is deliberately not presented as Rust data-flow analysis.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


SERVER = Path("crates/assay-mcp-server/src/tools")
CORE = Path("crates/assay-core/src/mcp/policy")
PARSER_RE = re.compile(
    r"(?:serde_yaml::(?:from_slice|from_str|from_reader|from_value|Deserializer)|"
    r"serde_ignored::deserialize)"
)


def code_only(source: str) -> str:
    """Replace comments and literals with spaces, preserving code positions."""
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
            elif char == "'" and index + 2 < len(source):
                # Lifetimes do not close within two characters; char literals do.
                close = source.find("'", index + 1, min(len(source), index + 8))
                if close != -1:
                    out[index] = " "
                    state = "char"
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
        elif state in {"string", "char"}:
            out[index] = " "
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif (state == "string" and char == '"') or (
                state == "char" and char == "'"
            ):
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


def require(errors: list[str], source: str, fragment: str, label: str) -> None:
    actual = compact(source).count(fragment)
    if actual != 1:
        errors.append(f"{label}: expected one effective {fragment!r}, found {actual}")


def check(root: Path) -> bool:
    guarded_roots = [root / SERVER, root / CORE]
    rust_files = sorted(path for base in guarded_roots for path in base.rglob("*.rs"))
    if not rust_files:
        print("FAIL: guarded Rust sources not found", file=sys.stderr)
        return False

    sources = {path.relative_to(root): path.read_text() for path in rust_files}
    errors: list[str] = []
    allowed_parser_calls = {
        SERVER / "mod.rs": ["serde_yaml::from_slice", "serde_yaml::from_value"],
        SERVER / "policy_decide.rs": [],
        # Fixed and value-free sequence parsing is explicitly outside #2387.
        SERVER / "check_sequence.rs": [
            "serde_yaml::from_slice",
            "serde_yaml::from_slice",
            "serde_yaml::from_slice",
        ],
        CORE / "legacy.rs": ["serde_yaml::from_str", "serde_ignored::deserialize"],
    }

    for path, source in sources.items():
        effective = code_only(source)
        if re.search(r"\buse\s+serde_(?:yaml|ignored)\b", effective):
            errors.append(f"{path}: aliases/imported parser constructors are not approved")
        actual = PARSER_RE.findall(effective)
        expected = allowed_parser_calls.get(path, [])
        if actual != expected:
            errors.append(f"{path}: parser calls {actual!r}, expected {expected!r}")

    shared = sources[SERVER / "mod.rs"]
    decide = sources[SERVER / "policy_decide.rs"]
    args = sources[SERVER / "check_args.rs"]
    coverage = sources[SERVER / "check_coverage.rs"]
    explain = sources[SERVER / "explain_trace.rs"]
    legacy = sources[CORE / "legacy.rs"]

    require(errors, shared, "pub(crate)structMappingStage(pub(crate)serde_yaml::Mapping);", "mapping typestate")
    require(errors, shared, "letvalue:serde_yaml::Value=serde_yaml::from_slice(bytes)", "shared syntax stage")
    require(errors, shared, "serde_yaml::Value::Mapping(m)=>Ok(MappingStage(m))", "shared root stage")
    require(errors, shared, "letMappingStage(mapping)=yaml_mapping_stage(bytes)?;", "shared typed route")
    require(errors, shared, "serde_yaml::from_value::<T>(serde_yaml::Value::Mapping(mapping))", "shared typed stage")
    require(errors, decide, "letsuper::MappingStage(mapping)=yaml_mapping_stage(bytes)?;", "policy_decide route")
    require(errors, decide, "serde_json::to_value(serde_yaml::Value::Mapping(mapping))", "policy_decide JSON-compatible projection")
    require(errors, decide, "serde_json::from_value::<PolicyDecisionDocument>(root)", "policy_decide typed stage")
    require(errors, coverage, "super::parse_tool_policy(&policy_bytes)", "check_coverage route")
    require(errors, explain, "super::parse_tool_policy(&policy_bytes)", "explain_trace route")
    require(errors, args, "McpPolicy::from_file(&policy_path)", "check_args full parser route")
    require(errors, legacy, "letvalue:serde_yaml::Value=matchserde_yaml::from_str(content)", "core syntax stage")
    require(errors, legacy, "if!value.is_mapping()", "core root stage")
    require(errors, legacy, "serde_ignored::deserialize(value,|path|", "core typed stage")

    for path in (
        SERVER / "policy_decide.rs",
        SERVER / "check_args.rs",
        SERVER / "check_coverage.rs",
        SERVER / "explain_trace.rs",
    ):
        effective = code_only(sources[path])
        patterns = [r"\.is_mapping\s*\(", r"\.as_mapping"]
        if path != SERVER / "policy_decide.rs":
            patterns.append(r"Value::Mapping")
        for pattern in patterns:
            if re.search(pattern, effective):
                errors.append(f"{path}: duplicates the shared mapping-root decision")

    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        return False
    print("OK: MCP policy YAML construction and helper routes match the approved architecture")
    return True


if __name__ == "__main__":
    repo_root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".")
    raise SystemExit(0 if check(repo_root) else 1)
