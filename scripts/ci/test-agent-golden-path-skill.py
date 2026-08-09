#!/usr/bin/env python3
"""Pin the shared Codex/Claude project skill to the driven golden-path contract."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_PATH = ROOT / "docs/generated/agent-golden-path.json"
WORKFLOW_PATH = ROOT / ".github/workflows/kernel-matrix.yml"
SKILL_PATHS = (
    ROOT / ".agents/skills/assay-golden-path/SKILL.md",
    ROOT / ".claude/skills/assay-golden-path/SKILL.md",
)
EXPECTED_DESCRIPTION = (
    "Drive Assay's install-to-evidence golden path and interpret its stdout and exit "
    "codes. Use when an agent must operate or diagnose Assay; do not use it to infer "
    "provider execution, external side effects, or a clean result from missing output."
)
EMPTY_STDOUT_RULE = (
    "Empty stdout in a gap row is an observed limitation, not permission for a caller "
    "to infer success from missing evidence."
)
CURSOR_SCOPE = (
    "Cursor documents compatibility loading for these project roots, but this "
    "repository does not exercise Cursor runtime discovery."
)
PROTECTED_ACTION_CWD = "examples/privileged-action-gate"


def fail(message: str) -> None:
    raise AssertionError(message)


def parse_frontmatter(text: str) -> tuple[dict[str, str], str]:
    if not text.startswith("---\n"):
        fail("SKILL.md must start with YAML frontmatter")
    try:
        raw, body = text[4:].split("\n---\n", 1)
    except ValueError as error:
        raise AssertionError("SKILL.md frontmatter is not terminated") from error

    fields: dict[str, str] = {}
    for line in raw.splitlines():
        key, separator, value = line.partition(": ")
        if not separator or not key or not value:
            fail(f"unsupported frontmatter line: {line!r}")
        fields[key] = value
    return fields, body


def exit_summary(step: dict[str, object]) -> str:
    outcomes = step.get("outcomes")
    if not isinstance(outcomes, list):
        fail("golden-path step outcomes must be a list")
    for outcome in outcomes:
        if not isinstance(outcome, dict):
            fail("golden-path outcomes must be objects")
    return "; ".join(
        f"{outcome['label']} `{outcome['exit_code']}`" for outcome in outcomes
    ) + "."


def main() -> None:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    if not isinstance(contract, dict):
        fail("golden-path contract root must be an object")
    if contract.get("schema") != "assay.agent_golden_path.v1":
        fail("unexpected golden-path contract schema")
    if contract.get("schema_version") != 1:
        fail("unexpected golden-path contract schema version")

    payloads: list[bytes] = []
    for path in SKILL_PATHS:
        if not path.is_file():
            fail(f"project skill is missing: {path.relative_to(ROOT)}")
        if path.is_symlink():
            fail(f"project skill must be a regular tracked file, not a symlink: {path}")
        payloads.append(path.read_bytes())

    if payloads[0] != payloads[1]:
        fail("Codex and Claude project skills are not byte-identical")

    text = payloads[0].decode("ascii")
    fields, body = parse_frontmatter(text)
    expected_fields = {
        "name": "assay-golden-path",
        "description": EXPECTED_DESCRIPTION,
    }
    if fields != expected_fields:
        fail("portable skill frontmatter or trigger boundaries drifted")

    required_body = (
        "docs/generated/agent-golden-path.json",
        EMPTY_STDOUT_RULE,
        CURSOR_SCOPE,
    )
    for fragment in required_body:
        if fragment not in body:
            fail(f"skill omits required guidance: {fragment!r}")

    cursor = -1
    steps = contract.get("steps")
    if not isinstance(steps, list):
        fail("golden-path contract steps must be a list")
    if len(steps) != 9:
        fail(f"golden-path contract must contain 9 steps, found {len(steps)}")
    for step in steps:
        if not isinstance(step, dict):
            fail("golden-path steps must be objects")
        required = (
            f"{step['step']}. {step['label']}",
            f"`{step['command']}`",
            exit_summary(step),
            str(step["stdout_summary"]),
            str(step["failure_summary"]),
        )
        positions = [body.find(fragment) for fragment in required]
        if any(position < 0 for position in positions):
            missing = [
                fragment for fragment, position in zip(required, positions) if position < 0
            ]
            fail(f"skill omits contract content for {step['id']}: {missing}")
        if positions[0] <= cursor:
            fail(f"skill journey is out of order at {step['id']}")
        cursor = positions[0]

        working_directory = step.get("working_directory")
        if step.get("id") == "protected-action":
            if working_directory != PROTECTED_ACTION_CWD:
                fail("protected-action step must pin its working directory")
        if working_directory is not None:
            if not isinstance(working_directory, str) or not working_directory:
                fail(f"invalid working directory for {step['id']}")
            cwd_line = f"Working directory: `{working_directory}`"
            if cwd_line not in body:
                fail(f"skill omits working directory for {step['id']}")

    non_claims = contract.get("non_claims")
    if not isinstance(non_claims, list):
        fail("golden-path contract non_claims must be a list")
    for non_claim in non_claims:
        if not isinstance(non_claim, str):
            fail("golden-path non-claims must be strings")
        if non_claim not in body:
            fail(f"skill omits non-claim: {non_claim}")

    forbidden = (
        "assay mcp-server",
        "assay_test_outbound",
        "six production tools",
        "safe agent",
        "compliance claim",
        "issue #2152 step 3",
        "Plugin and marketplace packaging belongs",
        "Cursor does not discover this project skill",
    )
    for phrase in forbidden:
        if phrase in text:
            fail(f"skill introduces forbidden or stale claim: {phrase!r}")

    workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
    required_workflow_paths = (
        '.agents/**',
        '.claude/**',
        '.gitignore',
        '.pre-commit-config.yaml',
        'docs/generated/**',
        'docs/guides/agent-golden-path.md',
    )
    for path in required_workflow_paths:
        if f'- "{path}"' not in workflow:
            fail(f"kernel-matrix workflow does not cover skill contract path: {path}")

    print("agent golden-path skill: portable, byte-identical, and contract-complete")


if __name__ == "__main__":
    main()
