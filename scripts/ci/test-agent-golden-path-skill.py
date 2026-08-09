#!/usr/bin/env python3
"""Pin the shared Codex/Claude project skill to the driven golden-path contract."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_PATH = ROOT / "docs/generated/agent-golden-path.json"
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
    outcomes = step["outcomes"]
    assert isinstance(outcomes, list)
    return "; ".join(
        f"{outcome['label']} `{outcome['exit_code']}`" for outcome in outcomes
    ) + "."


def main() -> None:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    assert contract["schema"] == "assay.agent_golden_path.v1"
    assert contract["schema_version"] == 1

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
    assert fields == {
        "name": "assay-golden-path",
        "description": EXPECTED_DESCRIPTION,
    }, "portable skill frontmatter or trigger boundaries drifted"

    assert "docs/generated/agent-golden-path.json" in body
    assert EMPTY_STDOUT_RULE in body
    assert "Cursor does not discover this project skill" in body

    cursor = -1
    steps = contract["steps"]
    assert isinstance(steps, list)
    assert len(steps) == 9
    for step in steps:
        assert isinstance(step, dict)
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

    for non_claim in contract["non_claims"]:
        assert non_claim in body, f"skill omits non-claim: {non_claim}"

    forbidden = (
        "assay mcp-server",
        "assay_test_outbound",
        "six production tools",
        "safe agent",
        "compliance claim",
    )
    for phrase in forbidden:
        assert phrase not in text, f"skill introduces forbidden or stale claim: {phrase!r}"

    print("agent golden-path skill: portable, byte-identical, and contract-complete")


if __name__ == "__main__":
    main()
