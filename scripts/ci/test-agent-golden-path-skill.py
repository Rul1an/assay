#!/usr/bin/env python3
"""Pin the shared Codex/Claude project skill to the driven golden-path contract."""

from __future__ import annotations

from dataclasses import dataclass
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_PATH = ROOT / "docs/generated/agent-golden-path.json"
GUIDE_PATH = ROOT / "docs/guides/agent-golden-path.md"
WORKFLOW_PATH = ROOT / ".github/workflows/kernel-matrix.yml"
PRECOMMIT_PATH = ROOT / ".pre-commit-config.yaml"
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
MAX_EVIDENCE_BYTES = 1024 * 1024


@dataclass(frozen=True)
class PrecommitHookContract:
    stages: tuple[str, ...] | None
    files_pattern: str


def fail(message: str) -> None:
    raise AssertionError(message)


def read_bounded_evidence(path: Path, label: str) -> bytes:
    if not path.is_file():
        fail(f"{label} is missing: {path.relative_to(ROOT)}")
    if path.is_symlink():
        fail(f"{label} must be a regular tracked file, not a symlink: {path}")
    if path.stat().st_size > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    with path.open("rb") as handle:
        payload = handle.read(MAX_EVIDENCE_BYTES + 1)
    if len(payload) > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    return payload


def workflow_pull_request_paths(text: str) -> set[str]:
    """Read active pull-request paths without accepting comments or sibling keys."""
    lines = text.splitlines()
    section_start = None
    search_from = 0
    for indentation, key in ((0, "on:"), (2, "pull_request:"), (4, "paths:")):
        matches = []
        for index in range(search_from, len(lines)):
            line = lines[index]
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            current_indentation = len(line) - len(line.lstrip(" "))
            if section_start is not None and current_indentation < indentation:
                break
            if current_indentation == indentation and stripped == key:
                matches.append(index)
        if len(matches) != 1:
            fail(f"kernel-matrix workflow must declare exactly one {key[:-1]} section")
        section_start = matches[0]
        search_from = section_start + 1

    paths = set()
    item_pattern = re.compile(r'^\s{6}-\s+"([^"]+)"\s*(?:#.*)?$')
    for line in lines[search_from:]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indentation = len(line) - len(line.lstrip(" "))
        if indentation <= 4:
            break
        match = item_pattern.fullmatch(line)
        if not match:
            fail("kernel-matrix pull_request.paths contains an unsupported entry")
        path = match.group(1)
        if path in paths:
            fail(f"kernel-matrix pull_request.paths duplicates entry: {path}")
        paths.add(path)
    return paths


def parse_precommit_self_test(text: str) -> PrecommitHookContract:
    """Read the unique active drift self-test hook from local pre-commit hooks."""
    lines = text.splitlines()
    hook_starts: list[int] = []

    for index, line in enumerate(lines):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indentation = len(line) - len(line.lstrip(" "))
        if indentation != 2 or stripped != "- repo: local":
            continue

        local_end = len(lines)
        for cursor in range(index + 1, len(lines)):
            candidate = lines[cursor]
            candidate_stripped = candidate.strip()
            if not candidate_stripped or candidate_stripped.startswith("#"):
                continue
            if len(candidate) - len(candidate.lstrip(" ")) <= 2:
                local_end = cursor
                break

        for cursor in range(index + 1, local_end):
            candidate = lines[cursor]
            candidate_stripped = candidate.strip()
            if not candidate_stripped or candidate_stripped.startswith("#"):
                continue
            if (
                len(candidate) - len(candidate.lstrip(" ")) == 4
                and candidate_stripped == "hooks:"
            ):
                hooks_end = local_end
                for hook_cursor in range(cursor + 1, local_end):
                    hook_line = lines[hook_cursor]
                    hook_stripped = hook_line.strip()
                    if not hook_stripped or hook_stripped.startswith("#"):
                        continue
                    if len(hook_line) - len(hook_line.lstrip(" ")) <= 4:
                        hooks_end = hook_cursor
                        break
                for hook_cursor in range(cursor + 1, hooks_end):
                    hook_line = lines[hook_cursor]
                    hook_stripped = hook_line.strip()
                    if hook_stripped.startswith("#"):
                        continue
                    if (
                        len(hook_line) - len(hook_line.lstrip(" ")) == 6
                        and hook_stripped == "- id: docs-generated-drift-self-test"
                    ):
                        hook_starts.append(hook_cursor)

    if len(hook_starts) != 1:
        fail("pre-commit config must declare exactly one active generated-docs drift self-test hook")

    fields: dict[str, list[str]] = {"stages": [], "files": []}
    hook_start = hook_starts[0]
    for line in lines[hook_start + 1 :]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indentation = len(line) - len(line.lstrip(" "))
        if indentation <= 6:
            break
        if indentation != 8:
            continue
        key, separator, value = stripped.partition(":")
        if separator and key in fields:
            fields[key].append(value.strip())

    if len(fields["stages"]) > 1:
        fail("generated-docs drift self-test duplicates stages")
    if len(fields["files"]) != 1:
        fail("generated-docs drift self-test must declare exactly one files entry")

    stages = None
    if fields["stages"]:
        match = re.fullmatch(r"\[([^]]*)\](?:\s+#.*)?", fields["stages"][0])
        if not match:
            fail("generated-docs drift self-test stages must be an inline list")
        raw_entries = match.group(1).strip()
        entries = (
            ()
            if not raw_entries
            else tuple(entry.strip() for entry in raw_entries.split(","))
        )
        if any(not entry for entry in entries):
            fail("generated-docs drift self-test stages must not contain empty entries")
        stages = entries

    files_pattern = fields["files"][0]
    if not files_pattern:
        fail("generated-docs drift self-test files entry must be a scalar")
    return PrecommitHookContract(stages=stages, files_pattern=files_pattern)


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
    contract = json.loads(read_bounded_evidence(CONTRACT_PATH, "contract evidence"))
    if not isinstance(contract, dict):
        fail("golden-path contract root must be an object")
    if contract.get("schema") != "assay.agent_golden_path.v1":
        fail("unexpected golden-path contract schema")
    if contract.get("schema_version") != 1:
        fail("unexpected golden-path contract schema version")

    payloads: list[bytes] = []
    for path in SKILL_PATHS:
        payloads.append(read_bounded_evidence(path, "skill evidence"))

    if payloads[0] != payloads[1]:
        fail("Codex and Claude project skills are not byte-identical")

    text = payloads[0].decode("ascii")
    guide = read_bounded_evidence(GUIDE_PATH, "guide evidence").decode("utf-8")
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
        guide_working_directory = working_directory or "."
        guide_row = (
            f"| {step['step']}. {step['label']} | `{guide_working_directory}` | "
            f"`{step['command']}` |"
        )
        if guide_row not in guide:
            fail(f"guide omits working directory for {step['id']}")

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

    workflow = read_bounded_evidence(WORKFLOW_PATH, "workflow evidence").decode("utf-8")
    workflow_paths = workflow_pull_request_paths(workflow)
    required_workflow_paths = (
        'scripts/**',
        '.agents/**',
        '.claude/**',
        '.gitignore',
        '.pre-commit-config.yaml',
        'docs/generated/**',
        'docs/guides/agent-golden-path.md',
        '.github/workflows/kernel-matrix.yml',
    )
    for path in required_workflow_paths:
        if path not in workflow_paths:
            fail(f"kernel-matrix workflow does not cover skill contract path: {path}")

    precommit_text = read_bounded_evidence(
        PRECOMMIT_PATH, "pre-commit config evidence"
    ).decode("utf-8")
    hook = parse_precommit_self_test(precommit_text)
    if hook.stages is not None and "pre-commit" not in hook.stages:
        fail("generated-docs drift self-test must run at the default pre-commit stage")
    if (
        re.search(
            hook.files_pattern, "scripts/docs/generate-agent-golden-path.py"
        )
        is None
    ):
        fail("generated-docs drift self-test does not cover its golden-path generator")

    print("agent golden-path skill: portable, byte-identical, and contract-complete")


if __name__ == "__main__":
    main()
