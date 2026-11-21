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
    r"serde_ignored::deserialize|assay_common::ignored_keys::decode|assay_common::yaml_input::parse_tree)"
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


def check() -> bool:
    guarded_roots = [SERVER, CORE]
    rust_files = sorted(path for base in guarded_roots for path in base.rglob("*.rs"))
    if not rust_files:
        print("FAIL: guarded Rust sources not found", file=sys.stderr)
        return False

    sources = {path: path.read_text() for path in rust_files}
    errors: list[str] = []
    allowed_parser_calls = {
        SERVER / "mod.rs": [],
        SERVER / "policy_decide.rs": [],
        SERVER / "check_sequence.rs": [],
        # Tests build raw trees with the shared syntax layer. A section is decoded by the shared
        # reader's checked source, so no policy file decodes one itself.
        CORE / "legacy.rs": [
            "assay_common::yaml_input::parse_tree",
        ],
    }

    for path, source in sources.items():
        effective = code_only(source)
        if re.search(
            r"\buse\s+(?:::\s*)?(?:serde_(?:yaml|ignored)\b|"
            r"assay_common\s*(?:as\b|::\s*\*|::[^;]*\b(?:ignored_keys|yaml_input)\b))",
            effective,
        ):
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

    require(errors, shared, "letinput=Input::parse_bytes(bytes)", "shared YAML input")
    require(errors, shared, "matches!(input.source(),serde_yaml::Value::Mapping(_))", "shared mapping root")
    require(errors, shared, "Policy::from_tool_text(text)", "eval policy text-key route")
    require(errors, shared, "policy_mapping(bytes)?;Err(ToolError::policy_parse(PolicyParseFailure::Structure,None))", "eval policy refusal classification")
    require(errors, decide, "letinput=policy_mapping(bytes)?;", "policy_decide route")
    require(errors, decide, "rootstructPolicyDecisionDocument", "closed blocklist root")
    require(errors, decide, "input.read_json_projection::<PolicyDecisionDocument>()", "blocklist JSON-compatible typed stage")
    require(errors, coverage, "super::parse_tool_policy(&policy_bytes)", "check_coverage route")
    require(errors, explain, "super::parse_tool_policy(&policy_bytes)", "explain_trace route")
    require(errors, sources[SERVER / "check_sequence.rs"], "SequenceDocument::parse(s)", "shared sequence document route")
    wrap = Path("crates/assay-cli/src/cli/commands/mcp.rs").read_text()
    require(errors, wrap, "McpPolicy::from_slice_with_source(&file)", "wrap full policy route")
    require(errors, wrap, "source.wrap_diagnostics()", "wrap loaded-source diagnostics")
    require(errors, args, "McpPolicy::from_slice_with_source(&policy_bytes)", "check_args full parser route")
    require(errors, args, "source.check_args_notes(&policy,tool_name)", "check_args loaded-source diagnostics")
    require(errors, legacy, "letcontent=policy_text(bytes)?;assay_common::yaml_input::parse_tree(content.as_bytes())", "core syntax stage")
    document = sources[CORE / "document.rs"]
    require(errors, legacy, "super::document::read(policy_text(bytes)?)?", "full policy delegation")
    require(errors, document, "root(yaml_types,x_extensions)structPolicyDocument", "closed policy root")
    require(errors, document, "load::<PolicyDocument>(text).map_err(refusal)?.into_current()", "closed policy typed stage")
    require(errors, document, "let(document,source)=load_with_checked_source::<PolicyDocument>(text).map_err(refusal)?", "closed policy with source")
    require(errors, document, "Ok((document.into_current()?,super::PolicySource(source)))", "current policy validation with source")
    require(errors, document, "Refusal::NotAMapping{path,..}ifpath.is_empty()", "root refusal classification")
    require(errors, sources[CORE / "section.rs"], "self.0.section::<T>(key)", "checked-source section projection")

    for path in (
        SERVER / "policy_decide.rs",
        SERVER / "check_args.rs",
        SERVER / "check_coverage.rs",
        SERVER / "explain_trace.rs",
    ):
        effective = code_only(sources[path])
        patterns = [r"\.is_mapping\s*\(", r"\.as_mapping"]
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
    if len(sys.argv) != 1:
        print(f"usage: {Path(sys.argv[0]).name}", file=sys.stderr)
        raise SystemExit(1)
    raise SystemExit(0 if check() else 1)
