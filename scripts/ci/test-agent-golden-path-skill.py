#!/usr/bin/env python3
"""Pin the shared Codex/Claude project skill to the driven golden-path contract."""

from __future__ import annotations

import ast
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
CANONICAL_PRECOMMIT = "pre-commit run --all-files --show-diff-on-failure"
ALLOWED_PREPUSH_PRECOMMIT = "pre-commit run --hook-stage pre-push --all-files"


@dataclass(frozen=True)
class PrecommitHookContract:
    stages: tuple[str, ...] | None
    files_pattern: str


@dataclass(frozen=True)
class WorkflowStepContract:
    condition: str | None
    continue_on_error: bool | None
    shell_lines: tuple[str, ...]


@dataclass(frozen=True)
class WorkflowContract:
    pull_request_branches: tuple[str, ...]
    pull_request_types: tuple[str, ...] | None
    pull_request_paths: tuple[str, ...]
    lint_runner: str
    lint_needs: tuple[str, ...] | None
    lint_condition: str | None
    lint_continue_on_error: bool | None
    lint_steps: tuple[WorkflowStepContract, ...]


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


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def active_lines(text: str) -> list[tuple[int, str]]:
    result = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        prefix = line[: len(line) - len(line.lstrip())]
        if "\t" in prefix:
            fail(f"kernel-matrix workflow uses tab indentation at line {line_number}")
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            result.append((line_number, line))
    return result


def locate_mapping(
    lines: list[tuple[int, str]], start_index: int, parent_indent: int, key: str,
    missing_message: str | None = None,
) -> tuple[int, int]:
    """Locate one direct mapping key and the first line outside its scope."""
    key_indent = parent_indent + 2
    matches = []
    end_index = len(lines)
    for index in range(start_index, len(lines)):
        _, line = lines[index]
        line_indent = indentation(line)
        if line_indent <= parent_indent:
            end_index = index
            break
        if line_indent == key_indent and line.strip() == key:
            matches.append(index)
    if len(matches) != 1:
        fail(missing_message or f"kernel-matrix workflow must declare exactly one {key[:-1]} section")

    key_index = matches[0]
    key_end = end_index
    for index in range(key_index + 1, end_index):
        if indentation(lines[index][1]) <= key_indent:
            key_end = index
            break
    return key_index, key_end


def parse_inline_string_list(raw: str, label: str) -> tuple[str, ...]:
    diagnostic = f"{label} must be a bracketed list of quoted strings"
    if not raw.startswith("[") or not raw.endswith("]"):
        fail(diagnostic)
    try:
        values = ast.literal_eval(raw)
    except (SyntaxError, ValueError):
        fail(diagnostic)
    if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
        fail(diagnostic)
    return tuple(values)


def direct_mapping_values(
    lines: list[tuple[int, str]], start_index: int, end_index: int, mapping_indent: int,
    label: str,
) -> dict[str, str]:
    values: dict[str, str] = {}
    for _, line in lines[start_index:end_index]:
        if indentation(line) != mapping_indent:
            continue
        key, separator, raw = line.strip().partition(":")
        if not separator:
            continue
        if key in values:
            fail(f"{label} duplicates key: {key}")
        values[key] = raw.strip()
    return values


def optional_boolean(raw: str, label: str) -> bool | None:
    if not raw:
        return None
    if raw == "true":
        return True
    if raw == "false":
        return False
    fail(f"{label} must be true or false")


def parse_job_needs(raw: str) -> tuple[str, ...]:
    if raw.startswith("["):
        return parse_inline_string_list(raw, "kernel-matrix lint.needs")
    if not raw:
        fail("kernel-matrix lint.needs must be a string or bracketed list of quoted strings")
    return (raw,)


def parse_lint_step(
    lines: list[tuple[int, str]], start_index: int, end_index: int
) -> WorkflowStepContract:
    fields: dict[str, str] = {}
    field_lines = [(start_index, lines[start_index][1].strip()[2:])]
    field_lines.extend(
        (index, line.strip())
        for index, (_, line) in enumerate(
            lines[start_index + 1 : end_index], start=start_index + 1
        )
        if indentation(line) == 8
    )
    run_index = None
    for index, candidate in field_lines:
        key, separator, raw = candidate.partition(":")
        if not separator:
            continue
        if key in fields:
            if key == "run":
                fail("kernel-matrix lint step duplicates key: run")
            fail(f"kernel-matrix lint step duplicates key: {key}")
        fields[key] = raw.strip()
        if key == "run":
            run_index = index

    shell_lines: tuple[str, ...] = ()
    if "run" in fields:
        run = fields["run"]
        if run == "|":
            if run_index is None:
                fail("kernel-matrix lint step run location is unavailable")
            run_end = next(
                (
                    index
                    for index in range(run_index + 1, end_index)
                    if indentation(lines[index][1]) <= 8
                ),
                end_index,
            )
            shell_lines = tuple(
                line.strip()
                for _, line in lines[run_index + 1 : run_end]
                if indentation(line) > 8
            )
        elif not run.startswith((">", "|")):
            shell_lines = (run,)
    return WorkflowStepContract(
        condition=fields.get("if"),
        continue_on_error=optional_boolean(
            fields.get("continue-on-error", ""),
            "kernel-matrix lint step continue-on-error",
        ),
        shell_lines=shell_lines,
    )


def parse_kernel_matrix_workflow(text: str) -> WorkflowContract:
    lines = active_lines(text)
    on_index, _ = locate_mapping(lines, 0, -2, "on:")
    pull_request_index, pull_request_end = locate_mapping(
        lines, on_index + 1, 0, "pull_request:"
    )
    pull_request = direct_mapping_values(
        lines, pull_request_index + 1, pull_request_end, 4, "kernel-matrix pull_request"
    )
    if "paths" not in pull_request:
        fail("kernel-matrix pull_request is missing required key: paths")
    if "branches" not in pull_request:
        fail("kernel-matrix pull_request is missing required key: branches")
    if "paths-ignore" in pull_request and "paths" in pull_request:
        fail("kernel-matrix pull_request cannot combine paths and paths-ignore")
    if "branches-ignore" in pull_request and "branches" in pull_request:
        fail("kernel-matrix pull_request cannot combine branches and branches-ignore")

    branches = parse_inline_string_list(
        pull_request["branches"], "kernel-matrix pull_request.branches"
    )
    pull_request_types = (
        parse_inline_string_list(
            pull_request["types"], "kernel-matrix pull_request.types"
        )
        if "types" in pull_request
        else None
    )

    paths = []
    path_pattern = re.compile(r'^ {6}- "([^"]+)"\s*(?:#.*)?$')
    paths_index = next(
        index
        for index in range(pull_request_index + 1, pull_request_end)
        if indentation(lines[index][1]) == 4 and lines[index][1].strip().startswith("paths:")
    )
    for _, line in lines[paths_index + 1 : pull_request_end]:
        if indentation(line) <= 4:
            break
        match = path_pattern.fullmatch(line)
        if not match:
            fail("kernel-matrix pull_request.paths contains an unsupported entry")
        path = match.group(1)
        if path in paths:
            fail(f"kernel-matrix pull_request.paths duplicates entry: {path}")
        paths.append(path)

    jobs_index, _ = locate_mapping(lines, 0, -2, "jobs:")
    lint_index, lint_end = locate_mapping(
        lines,
        jobs_index + 1,
        0,
        "lint:",
        "kernel-matrix workflow must declare exactly one active lint job",
    )
    lint = direct_mapping_values(lines, lint_index + 1, lint_end, 4, "kernel-matrix lint job")
    if "runs-on" not in lint:
        fail("kernel-matrix lint job is missing required key: runs-on")
    if "steps" not in lint:
        fail("kernel-matrix lint job is missing required key: steps")
    steps_index, steps_end = locate_mapping(lines, lint_index + 1, 2, "steps:")

    step_starts = [
        index
        for index in range(steps_index + 1, steps_end)
        if indentation(lines[index][1]) == 6 and lines[index][1].strip().startswith("- ")
    ]
    lint_steps = tuple(
        parse_lint_step(
            lines,
            step_start,
            step_starts[position + 1] if position + 1 < len(step_starts) else steps_end,
        )
        for position, step_start in enumerate(step_starts)
    )
    lint_needs = (
        parse_job_needs(lint["needs"])
        if "needs" in lint
        else None
    )
    return WorkflowContract(
        pull_request_branches=branches,
        pull_request_types=pull_request_types,
        pull_request_paths=tuple(paths),
        lint_runner=lint["runs-on"],
        lint_needs=lint_needs,
        lint_condition=lint.get("if"),
        lint_continue_on_error=optional_boolean(
            lint.get("continue-on-error", ""),
            "kernel-matrix lint job continue-on-error",
        ),
        lint_steps=lint_steps,
    )


def validate_lint_executor(contract: WorkflowContract) -> None:
    # This maintenance pin requires updating this contract and mutation proof together.
    if contract.lint_runner != "ubuntu-latest":
        fail("kernel-matrix lint job must run on ubuntu-latest")
    if contract.lint_needs is not None:
        fail("kernel-matrix lint job must not depend on another job")
    if contract.lint_condition is not None:
        fail("kernel-matrix lint executor must not be conditional")
    if contract.lint_continue_on_error is True:
        fail("kernel-matrix lint executor must fail closed")

    invocation_steps = []
    canonical_steps = []
    for step in contract.lint_steps:
        active = tuple(line.strip() for line in step.shell_lines if line.strip())
        invocations = tuple(line for line in active if line.startswith("pre-commit "))
        if invocations:
            invocation_steps.append((step, invocations))
        if CANONICAL_PRECOMMIT in invocations:
            canonical_steps.append((step, invocations))

    if not invocation_steps:
        fail("kernel-matrix lint job has no canonical pre-commit executor")
    if len(canonical_steps) != 1:
        fail("kernel-matrix lint pre-commit command is noncanonical")

    step, invocations = canonical_steps[0]
    if step.condition is not None:
        fail("kernel-matrix lint executor must not be conditional")
    if step.continue_on_error is True:
        fail("kernel-matrix lint executor must fail closed")
    if invocations != (CANONICAL_PRECOMMIT,):
        fail("kernel-matrix lint pre-commit command is noncanonical")
    for other_step, other_invocations in invocation_steps:
        if other_step is step:
            continue
        if other_invocations != (ALLOWED_PREPUSH_PRECOMMIT,):
            fail("kernel-matrix lint pre-commit command is noncanonical")


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
    workflow_contract = parse_kernel_matrix_workflow(workflow)
    validate_lint_executor(workflow_contract)
    workflow_paths = set(workflow_contract.pull_request_paths)
    if "main" not in workflow_contract.pull_request_branches:
        fail("kernel-matrix pull_request does not cover main")
    if workflow_contract.pull_request_types is not None:
        fail("kernel-matrix pull_request must not declare types")
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
