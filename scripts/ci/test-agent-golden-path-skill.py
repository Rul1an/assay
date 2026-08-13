#!/usr/bin/env python3
"""Pin the shared Codex/Claude project skill to the driven golden-path contract."""

from __future__ import annotations

import ast
from dataclasses import dataclass
import json
import os
import re
from pathlib import Path
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_PATH = ROOT / "docs/generated/agent-golden-path.json"
GUIDE_PATH = ROOT / "docs/guides/agent-golden-path.md"
WORKFLOW_PATH = ROOT / ".github/workflows/kernel-matrix.yml"
PRECOMMIT_PATH = ROOT / ".pre-commit-config.yaml"
MARKETPLACE_PATH = ROOT / ".claude-plugin/marketplace.json"
PLUGIN_MANIFEST_PATH = ROOT / "packaging/claude-plugin/.claude-plugin/plugin.json"
PLUGIN_MCP_PATH = ROOT / "packaging/claude-plugin/.mcp.json"
PROJECT_MCP_PATH = ROOT / ".mcp.json"
PLUGIN_SKILL_PATH = ROOT / "packaging/claude-plugin/skills/assay-golden-path/SKILL.md"
PLUGIN_CONTRACT_PATH = (
    ROOT
    / "packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json"
)
SKILL_PATHS = (
    ROOT / ".agents/skills/assay-golden-path/SKILL.md",
    ROOT / ".claude/skills/assay-golden-path/SKILL.md",
)
CLAUDE_SKILL_SIBLING = ".claude/skills/assay-golden-path/OTHER.md"
GIT_ENV = {
    **os.environ,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CEILING_DIRECTORIES": str(ROOT.parent),
}
for git_selector in (
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_WORK_TREE",
):
    GIT_ENV.pop(git_selector, None)
EXPECTED_DESCRIPTION = (
    "Drive Assay's install-to-evidence golden path and interpret its stdout and exit "
    "codes. Use when an agent must operate or diagnose Assay; do not use it to infer "
    "provider execution, external side effects, or a clean result from missing output."
)
EXPECTED_PLUGIN_DESCRIPTION = (
    "Connect Claude Code to Assay's MCP policy and evidence tools and golden-path skill."
)
PLUGIN_CONTRACT_REFERENCE = (
    "${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/references/agent-golden-path.json"
)
PLUGIN_ASSET_ROOT = (
    "${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/assets/privileged-action-gate"
)
PLUGIN_FIXTURE_MAPPING = (
    "Fixtures named by the contract under `examples/privileged-action-gate` are bundled at "
    f"`{PLUGIN_ASSET_ROOT}`."
)
PLUGIN_RESOURCE_COPIES = (
    (CONTRACT_PATH, PLUGIN_CONTRACT_PATH),
    (
        ROOT / "examples/privileged-action-gate/mock_github_mcp.py",
        ROOT
        / "packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/mock_github_mcp.py",
    ),
    (
        ROOT / "examples/privileged-action-gate/baseline-approved.json",
        ROOT
        / "packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/baseline-approved.json",
    ),
    (
        ROOT / "examples/privileged-action-gate/policies/no-allowance.yaml",
        ROOT
        / "packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies/no-allowance.yaml",
    ),
)
EMPTY_STDOUT_RULE = (
    "Empty stdout in a gap row is an observed limitation, not permission for a caller "
    "to infer success from missing evidence."
)
INVOCATION_CWD_RULE = (
    "When a step has no `working_directory`, run it from the invocation cwd."
)
SOURCE_REPO_CWD_RULE = (
    "A present `working_directory` is a POSIX path relative to the source repository."
)
PYTHON_PLACEHOLDER_RULE = (
    "Replace `<python>` with `python3` on Unix-like hosts or `python` on Windows."
)
CURSOR_SCOPE = (
    "Cursor's skill documentation (https://cursor.com/docs/skills), accessed on "
    "2026-08-09, describes .agents/skills as a project-level location and "
    ".claude/skills as a compatibility location. This repository does not exercise "
    "Cursor runtime discovery."
)
DISCOVERY_START = "<!-- agent-golden-path-discovery:start -->"
DISCOVERY_END = "<!-- agent-golden-path-discovery:end -->"
RELEASE_START = "<!-- agent-golden-path-release:start -->"
RELEASE_END = "<!-- agent-golden-path-release:end -->"
DISCOVERY_SKILL_ROOTS = (
    ".agents/skills/assay-golden-path/SKILL.md",
    ".claude/skills/assay-golden-path/SKILL.md",
)
CURSOR_DOCS_URL = "https://cursor.com/docs/skills"
CURSOR_DOCS_ACCESSED = "2026-08-09"
PROTECTED_ACTION_CWD = "examples/privileged-action-gate"
MAX_EVIDENCE_BYTES = 1024 * 1024
CANONICAL_PRECOMMIT = "pre-commit run --all-files --show-diff-on-failure"
ALLOWED_PREPUSH_PRECOMMIT = "pre-commit run --hook-stage pre-push --all-files"
SELF_TEST_TRIGGER_PATH = "scripts/docs/generate-agent-golden-path.py"
SELF_TEST_TRIGGER_MODE = "100644"
SELF_TEST_TRIGGER_TYPES = frozenset({"file", "non-executable", "python", "text"})
MAPPING_ENTRY_PATTERN = re.compile(r"^(?P<key>[A-Za-z0-9_-]+) *:(?P<value>.*)$")
ISSUE_REFERENCE_PATTERN = re.compile(
    r"(?<!\]\()#(?P<short>[0-9]+)(?![A-Za-z0-9_-])"
    r"|/issues/(?P<url>[0-9]+)(?![A-Za-z0-9_-])"
)
PUBLIC_TOKEN_PATTERN = re.compile(r"[a-z0-9]+")
PRIVATE_PHRASE_FAMILIES = (
    (
        "roadmap ownership",
        (("roadmap", "ownership"), ("road", "map", "ownership")),
    ),
    (
        "next slice",
        (
            ("next", "slice"),
            ("future", "slice"),
            ("next", "pr"),
            ("future", "pr"),
        ),
    ),
    (
        "future marketplace",
        (
            ("future", "marketplace"),
            ("future", "marketplaces"),
            ("future", "market", "place"),
            ("marketplace", "packaging"),
            ("marketplaces", "packaging"),
            ("market", "place", "packaging"),
            ("future", "plugin"),
            ("future", "plugins"),
            ("future", "plug", "in"),
            ("plugin", "packaging"),
            ("plugins", "packaging"),
            ("plug", "in", "packaging"),
        ),
    ),
)
NUMBER_WORDS = {
    "one": "1",
    "two": "2",
    "three": "3",
    "four": "4",
    "five": "5",
    "six": "6",
    "seven": "7",
    "eight": "8",
    "nine": "9",
}
STEP_NUMBERS = frozenset(NUMBER_WORDS.values())
# This closed vocabulary intentionally covers only the nine implementation-step
# labels in the generated golden path. It is not a general number normalizer.

# Both bounded YAML readers intentionally model the repository's two-space
# layout rather than arbitrary YAML indentation or scalar forms.
YAML_INDENT_STEP = 2
YAML_ROOT_INDENT = 0
WORKFLOW_ROOT_PARENT_INDENT = -YAML_INDENT_STEP
WORKFLOW_SECTION_INDENT = 2
WORKFLOW_FIELD_INDENT = 4
WORKFLOW_SEQUENCE_INDENT = 6
WORKFLOW_STEP_FIELD_INDENT = 8
PRECOMMIT_REPO_INDENT = 2
PRECOMMIT_HOOKS_INDENT = 4
PRECOMMIT_HOOK_INDENT = 6
PRECOMMIT_HOOK_FIELD_INDENT = 8


@dataclass(frozen=True)
class PrecommitHookContract:
    stages: tuple[str, ...] | None
    files_pattern: str
    root_files_pattern: str | None
    root_exclude_pattern: str | None
    exclude_pattern: str | None
    types: tuple[str, ...] | None
    types_or: tuple[str, ...] | None
    exclude_types: tuple[str, ...] | None


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


def contains_token_sequence(tokens: tuple[str, ...], sequence: tuple[str, ...]) -> bool:
    width = len(sequence)
    return any(
        tokens[index : index + width] == sequence
        for index in range(len(tokens) - width + 1)
    )


def issue_references(text: str) -> set[int]:
    return {
        int(match.group("short") or match.group("url"))
        for match in ISSUE_REFERENCE_PATTERN.finditer(text)
    }


def generated_discovery_block(guide: str) -> str:
    if guide.count(DISCOVERY_START) != 1 or guide.count(DISCOVERY_END) != 1:
        fail("agent golden-path guide must contain exactly one discovery marker pair")
    if guide.index(DISCOVERY_START) > guide.index(DISCOVERY_END):
        fail("agent golden-path guide discovery markers are out of order")
    before, remainder = guide.split(DISCOVERY_START, 1)
    block, after = remainder.split(DISCOVERY_END, 1)
    if DISCOVERY_START in before or DISCOVERY_END in before + after:
        fail("agent golden-path guide discovery markers are out of order")
    return block


def generated_release_block(guide: str) -> str:
    if guide.count(RELEASE_START) != 1 or guide.count(RELEASE_END) != 1:
        fail("agent golden-path guide must contain exactly one release marker pair")
    if guide.index(RELEASE_START) > guide.index(RELEASE_END):
        fail("agent golden-path guide release markers are out of order")
    return guide.split(RELEASE_START, 1)[1].split(RELEASE_END, 1)[0]


def contract_issue_numbers(contract: dict[str, object]) -> set[int]:
    allowed: set[int] = set()
    steps = contract.get("steps")
    if not isinstance(steps, list):
        fail("golden-path contract steps must be a list")
    for step in steps:
        if not isinstance(step, dict):
            fail("golden-path steps must be objects")
        outcomes = step.get("outcomes")
        if not isinstance(outcomes, list):
            fail(f"golden-path outcomes must be a list for {step.get('id')}")
        for outcome in outcomes:
            if not isinstance(outcome, dict):
                fail(f"golden-path outcomes must be objects for {step.get('id')}")
            issue = outcome.get("gap_issue")
            if issue is None:
                continue
            if (
                not isinstance(issue, int)
                or isinstance(issue, bool)
                or issue <= 0
            ):
                fail(
                    "golden-path gap_issue must be a positive integer for "
                    f"{step.get('id')}"
                )
            allowed.add(issue)

    non_claims = contract.get("non_claims")
    if not isinstance(non_claims, list):
        fail("golden-path contract non_claims must be a list")
    for claim in non_claims:
        if not isinstance(claim, str):
            fail("golden-path non-claims must be strings")
        allowed.update(issue_references(claim))
    return allowed


def validate_public_vocabulary(text: str, contract: dict[str, object]) -> None:
    allowed_issues = contract_issue_numbers(contract)
    observed_issues = issue_references(text)
    unexpected = sorted(observed_issues - allowed_issues)
    if unexpected:
        fail(f"skill references issue outside contract evidence: #{unexpected[0]}")

    tokens = tuple(PUBLIC_TOKEN_PATTERN.findall(text.lower()))
    for label, sequences in PRIVATE_PHRASE_FAMILIES:
        if any(contains_token_sequence(tokens, sequence) for sequence in sequences):
            fail(f"skill introduces private planning vocabulary: {label}")

    ownership_tokens = frozenset({"belongs", "owned", "ownership", "owns"})
    for index in range(len(tokens) - 2):
        if tokens[index : index + 2] != ("implementation", "step"):
            continue
        step_number = NUMBER_WORDS.get(tokens[index + 2], tokens[index + 2])
        if step_number not in STEP_NUMBERS:
            continue
        if any(token in ownership_tokens for token in tokens[index + 3 : index + 7]):
            fail(
                "skill introduces private planning vocabulary: "
                f"implementation step {step_number} ownership"
            )


def run_git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "git",
            "-c",
            "core.excludesFile=",
            "-c",
            "core.attributesFile=",
            "-c",
            "init.defaultBranch=main",
            *args,
        ],
        cwd=ROOT,
        env=GIT_ENV,
        text=True,
        capture_output=True,
        check=False,
    )


def validate_skill_repository_state() -> None:
    for path in SKILL_PATHS:
        skill = path.relative_to(ROOT).as_posix()
        if run_git("ls-files", "--error-unmatch", "--", skill).returncode != 0:
            fail(f"skill is not tracked: {skill}")
        if run_git("check-ignore", "--no-index", "--", skill).returncode == 0:
            fail(f"tracked skill is ignored: {skill}")
        attributes = run_git("check-attr", "eol", "--", skill)
        if attributes.returncode != 0 or attributes.stdout != f"{skill}: eol: lf\n":
            fail(f"skill does not declare eol=lf: {skill}")

    if run_git("check-ignore", "--no-index", "--", CLAUDE_SKILL_SIBLING).returncode != 0:
        fail(f"Claude skill sibling is not ignored: {CLAUDE_SKILL_SIBLING}")


def read_bounded_evidence(path: Path, label: str) -> bytes:
    if path.is_symlink():
        fail(f"{label} must be a regular tracked file, not a symlink: {path}")
    if not path.is_file():
        fail(f"{label} is missing: {path.relative_to(ROOT)}")
    if path.stat().st_size > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    with path.open("rb") as handle:
        payload = handle.read(MAX_EVIDENCE_BYTES + 1)
    if len(payload) > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    return payload


def validate_plugin_manifests() -> None:
    marketplace = json.loads(
        read_bounded_evidence(MARKETPLACE_PATH, "Claude marketplace manifest")
    )
    expected_marketplace = {
        "name": "assay",
        "owner": {"name": "Assay"},
        "plugins": [
            {
                "name": "assay",
                "description": EXPECTED_PLUGIN_DESCRIPTION,
                "source": "./packaging/claude-plugin",
            }
        ],
    }
    if marketplace != expected_marketplace:
        fail("Claude marketplace identity or local source drifted")

    plugin = json.loads(
        read_bounded_evidence(PLUGIN_MANIFEST_PATH, "Claude plugin manifest")
    )
    expected_plugin = {
        "name": "assay",
        "description": EXPECTED_PLUGIN_DESCRIPTION,
        "author": {"name": "Assay"},
    }
    if plugin != expected_plugin:
        fail("Claude plugin identity, fields, or unversioned contract drifted")

    mcp = json.loads(read_bounded_evidence(PLUGIN_MCP_PATH, "Claude plugin MCP manifest"))
    expected_mcp = {
        "mcpServers": {
            "assay": {
                "command": "assay-mcp-server",
                "args": ["--policy-root", "."],
            }
        }
    }
    if mcp != expected_mcp:
        fail("Claude plugin MCP command or project-root arguments drifted")
    project_mcp = json.loads(
        read_bounded_evidence(PROJECT_MCP_PATH, "project MCP manifest")
    )
    if project_mcp != mcp:
        fail("project and plugin MCP manifests no longer share the cwd-relative invocation")


def validate_plugin_skill(contract: dict[str, object]) -> None:
    payload = read_bounded_evidence(PLUGIN_SKILL_PATH, "Claude plugin skill")
    try:
        text = payload.decode("ascii")
    except UnicodeDecodeError:
        fail("Claude plugin skill must be ASCII")
    fields, body = parse_frontmatter(text)
    expected_fields = {
        "name": "assay-golden-path",
        "description": EXPECTED_DESCRIPTION,
    }
    if fields != expected_fields:
        fail("Claude plugin skill frontmatter or trigger boundaries drifted")
    if f"`{PLUGIN_CONTRACT_REFERENCE}` is the authoritative machine contract." not in body:
        fail("Claude plugin skill omits its bundled authoritative contract")
    if body.count(PLUGIN_FIXTURE_MAPPING) != 1:
        fail("Claude plugin skill must contain exactly one source-to-bundle fixture mapping")
    without_mapping = body.replace(PLUGIN_FIXTURE_MAPPING, "", 1)
    for source_only in (
        "docs/generated/agent-golden-path.json",
        "scripts/docs/generate-agent-golden-path.py",
        "examples/privileged-action-gate",
    ):
        if source_only in without_mapping:
            fail(f"Claude plugin skill contains source-only path outside mapping: {source_only}")
    if f"Working directory: `{PLUGIN_ASSET_ROOT}`" not in body:
        fail("Claude plugin skill does not resolve protected-action assets through plugin root")
    validate_public_vocabulary(text, contract)

    for source, packaged in PLUGIN_RESOURCE_COPIES:
        if read_bounded_evidence(source, "canonical plugin resource") != read_bounded_evidence(
            packaged, "packaged plugin resource"
        ):
            fail(f"packaged plugin resource drifted: {packaged.relative_to(ROOT)}")


def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def mapping_key_value(candidate: str, label: str) -> tuple[str, str] | None:
    """Read one plain bounded-YAML mapping entry, normalizing spaces before `:`."""
    match = MAPPING_ENTRY_PATTERN.fullmatch(candidate)
    if match:
        return match.group("key"), match.group("value").strip()
    if ":" in candidate:
        fail(f"{label} contains unsupported mapping-key syntax")
    return None


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
    key_indent = parent_indent + YAML_INDENT_STEP
    matches = []
    end_index = len(lines)
    for index in range(start_index, len(lines)):
        _, line = lines[index]
        line_indent = indentation(line)
        if line_indent <= parent_indent:
            end_index = index
            break
        if line_indent == key_indent:
            entry = mapping_key_value(line.strip(), "kernel-matrix workflow")
            if entry is not None and entry[0] == key:
                if entry[1] and not entry[1].startswith("#"):
                    fail(f"kernel-matrix {key} section must be a mapping")
                matches.append(index)
    if len(matches) != 1:
        fail(missing_message or f"kernel-matrix workflow must declare exactly one {key} section")

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
        entry = mapping_key_value(line.strip(), label)
        if entry is None:
            continue
        key, raw = entry
        if key in values:
            fail(f"{label} duplicates key: {key}")
        values[key] = raw
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
        if indentation(line) == WORKFLOW_STEP_FIELD_INDENT
    )
    run_index = None
    for index, candidate in field_lines:
        entry = mapping_key_value(candidate, "kernel-matrix lint step")
        if entry is None:
            continue
        key, raw = entry
        if key in fields:
            fail(f"kernel-matrix lint step duplicates key: {key}")
        fields[key] = raw
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
                    if indentation(lines[index][1]) <= WORKFLOW_STEP_FIELD_INDENT
                ),
                end_index,
            )
            shell_lines = tuple(
                line.strip()
                for _, line in lines[run_index + 1 : run_end]
                if indentation(line) > WORKFLOW_STEP_FIELD_INDENT
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
    on_index, _ = locate_mapping(
        lines, 0, WORKFLOW_ROOT_PARENT_INDENT, "on"
    )
    pull_request_index, pull_request_end = locate_mapping(
        lines, on_index + 1, YAML_ROOT_INDENT, "pull_request"
    )
    pull_request = direct_mapping_values(
        lines,
        pull_request_index + 1,
        pull_request_end,
        WORKFLOW_FIELD_INDENT,
        "kernel-matrix pull_request",
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
    path_pattern = re.compile(r'^- "([^"]+)"\s*(?:#.*)?$')
    paths_index = next(
        index
        for index in range(pull_request_index + 1, pull_request_end)
        if indentation(lines[index][1]) == WORKFLOW_FIELD_INDENT
        and (entry := mapping_key_value(
            lines[index][1].strip(), "kernel-matrix pull_request"
        )) is not None
        and entry[0] == "paths"
    )
    for _, line in lines[paths_index + 1 : pull_request_end]:
        if indentation(line) <= WORKFLOW_FIELD_INDENT:
            break
        if indentation(line) != WORKFLOW_SEQUENCE_INDENT:
            fail("kernel-matrix pull_request.paths contains an unsupported entry")
        match = path_pattern.fullmatch(line.strip())
        if not match:
            fail("kernel-matrix pull_request.paths contains an unsupported entry")
        path = match.group(1)
        if path in paths:
            fail(f"kernel-matrix pull_request.paths duplicates entry: {path}")
        paths.append(path)

    jobs_index, _ = locate_mapping(
        lines, 0, WORKFLOW_ROOT_PARENT_INDENT, "jobs"
    )
    lint_index, lint_end = locate_mapping(
        lines,
        jobs_index + 1,
        YAML_ROOT_INDENT,
        "lint",
        "kernel-matrix workflow must declare exactly one active lint job",
    )
    lint = direct_mapping_values(
        lines,
        lint_index + 1,
        lint_end,
        WORKFLOW_FIELD_INDENT,
        "kernel-matrix lint job",
    )
    if "runs-on" not in lint:
        fail("kernel-matrix lint job is missing required key: runs-on")
    if "steps" not in lint:
        fail("kernel-matrix lint job is missing required key: steps")
    steps_index, steps_end = locate_mapping(
        lines, lint_index + 1, WORKFLOW_SECTION_INDENT, "steps"
    )

    step_starts = [
        index
        for index in range(steps_index + 1, steps_end)
        if indentation(lines[index][1]) == WORKFLOW_SEQUENCE_INDENT
        and lines[index][1].strip().startswith("- ")
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


def precommit_line_indentation(line: str, line_number: int) -> int:
    prefix = line[: len(line) - len(line.lstrip())]
    if "\t" in prefix:
        fail(f"pre-commit config uses tab indentation at line {line_number}")
    return indentation(line)


def parse_precommit_inline_list(raw: str, label: str) -> tuple[str, ...]:
    match = re.fullmatch(r"\[(.*)\]\s*(?:#.*)?", raw)
    if not match:
        fail(f"{label} must be an inline list")
    body = match.group(1).strip()
    if not body:
        return ()

    values = []
    for entry in body.split(","):
        entry = entry.strip()
        if not entry:
            fail(f"{label} must not contain empty entries")
        if entry.startswith(("'", '"')):
            try:
                value = ast.literal_eval(entry)
            except (SyntaxError, ValueError):
                fail(f"{label} contains an unsupported entry")
            if not isinstance(value, str):
                fail(f"{label} contains an unsupported entry")
        elif re.fullmatch(r"[A-Za-z0-9_.+-]+", entry):
            value = entry
        else:
            fail(f"{label} contains an unsupported entry")
        values.append(value)
    return tuple(values)


def precommit_scalar(raw: str, label: str) -> str:
    # In a YAML plain scalar, whitespace followed by `#` starts a comment.
    raw = re.split(r"[ \t]+#", raw, maxsplit=1)[0].rstrip()
    if not raw or raw.startswith(("|", ">", "'", '"')):
        fail(f"{label} must be an unquoted inline scalar")
    return raw


def parse_precommit_hook(
    text: str, hook_id: str, label: str
) -> PrecommitHookContract:
    """Read one unique active local pre-commit hook and its effective selectors."""
    lines = text.splitlines()
    hook_starts: list[int] = []
    root_fields: dict[str, str] = {}

    for line_number, line in enumerate(lines, start=1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        line_indent = precommit_line_indentation(line, line_number)
        if line_indent != YAML_ROOT_INDENT:
            continue
        entry = mapping_key_value(stripped, "pre-commit config")
        if entry is None or entry[0] not in {"files", "exclude"}:
            continue
        key, value = entry
        if key in root_fields:
            fail(f"pre-commit config duplicates root selector: {key}")
        root_fields[key] = precommit_scalar(value, f"pre-commit root {key}")

    for index, line in enumerate(lines):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        line_indent = precommit_line_indentation(line, index + 1)
        if line_indent != PRECOMMIT_REPO_INDENT or not stripped.startswith("- "):
            continue
        repo_entry = mapping_key_value(stripped[2:], "pre-commit repository")
        if repo_entry != ("repo", "local"):
            continue

        local_end = len(lines)
        for cursor in range(index + 1, len(lines)):
            candidate = lines[cursor]
            candidate_stripped = candidate.strip()
            if not candidate_stripped or candidate_stripped.startswith("#"):
                continue
            if precommit_line_indentation(candidate, cursor + 1) <= PRECOMMIT_REPO_INDENT:
                local_end = cursor
                break

        for cursor in range(index + 1, local_end):
            candidate = lines[cursor]
            candidate_stripped = candidate.strip()
            if not candidate_stripped or candidate_stripped.startswith("#"):
                continue
            if precommit_line_indentation(candidate, cursor + 1) != PRECOMMIT_HOOKS_INDENT:
                continue
            hooks_entry = mapping_key_value(candidate_stripped, "pre-commit local repository")
            if hooks_entry is None or hooks_entry[0] != "hooks":
                continue
            if hooks_entry[1] and not hooks_entry[1].startswith("#"):
                fail("pre-commit local hooks must be a block sequence")

            hooks_end = local_end
            for hook_cursor in range(cursor + 1, local_end):
                hook_line = lines[hook_cursor]
                hook_stripped = hook_line.strip()
                if not hook_stripped or hook_stripped.startswith("#"):
                    continue
                if (
                    precommit_line_indentation(hook_line, hook_cursor + 1)
                    <= PRECOMMIT_HOOKS_INDENT
                ):
                    hooks_end = hook_cursor
                    break
            for hook_cursor in range(cursor + 1, hooks_end):
                hook_line = lines[hook_cursor]
                hook_stripped = hook_line.strip()
                if not hook_stripped or hook_stripped.startswith("#"):
                    continue
                if (
                    precommit_line_indentation(hook_line, hook_cursor + 1)
                    != PRECOMMIT_HOOK_INDENT
                    or not hook_stripped.startswith("- ")
                ):
                    continue
                id_entry = mapping_key_value(hook_stripped[2:], "pre-commit hook")
                if id_entry == ("id", hook_id):
                    hook_starts.append(hook_cursor)

    if len(hook_starts) != 1:
        fail(f"pre-commit config must declare exactly one active {label} hook")

    selector_keys = (
        "stages",
        "files",
        "exclude",
        "types",
        "types_or",
        "exclude_types",
    )
    fields: dict[str, list[str]] = {key: [] for key in selector_keys}
    hook_start = hook_starts[0]
    for index, line in enumerate(lines[hook_start + 1 :], start=hook_start + 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        line_indent = precommit_line_indentation(line, index + 1)
        if line_indent <= PRECOMMIT_HOOK_INDENT:
            break
        if line_indent != PRECOMMIT_HOOK_FIELD_INDENT:
            continue
        entry = mapping_key_value(stripped, label)
        if entry is not None and entry[0] in fields:
            fields[entry[0]].append(entry[1])

    for key, values in fields.items():
        if len(values) > 1:
            fail(f"{label} duplicates {key}")
    if len(fields["files"]) != 1:
        fail(f"{label} must declare exactly one files entry")

    stages = (
        parse_precommit_inline_list(fields["stages"][0], f"{label} stages")
        if fields["stages"]
        else None
    )
    list_fields = {}
    for key in ("types", "types_or", "exclude_types"):
        list_fields[key] = (
            parse_precommit_inline_list(fields[key][0], f"{label} {key}")
            if fields[key]
            else None
        )

    files_pattern = precommit_scalar(fields["files"][0], f"{label} files entry")
    exclude_pattern = (
        precommit_scalar(fields["exclude"][0], f"{label} exclude entry")
        if fields["exclude"]
        else None
    )
    return PrecommitHookContract(
        stages=stages,
        files_pattern=files_pattern,
        root_files_pattern=root_fields.get("files"),
        root_exclude_pattern=root_fields.get("exclude"),
        exclude_pattern=exclude_pattern,
        types=list_fields["types"],
        types_or=list_fields["types_or"],
        exclude_types=list_fields["exclude_types"],
    )


def precommit_pattern_matches(pattern: str, path: str, label: str) -> bool:
    try:
        return re.search(pattern, path) is not None
    except re.error as error:
        fail(f"{label} is not a supported regular expression: {error.msg}")


def validate_precommit_self_test(hook: PrecommitHookContract) -> None:
    trigger_index = run_git("ls-files", "--stage", "--", SELF_TEST_TRIGGER_PATH)
    expected_prefix = f"{SELF_TEST_TRIGGER_MODE} "
    if (
        trigger_index.returncode != 0
        or len(trigger_index.stdout.splitlines()) != 1
        or not trigger_index.stdout.startswith(expected_prefix)
    ):
        fail(
            "generated-docs drift self-test trigger must remain a tracked "
            f"{SELF_TEST_TRIGGER_MODE} file: {SELF_TEST_TRIGGER_PATH}"
        )

    if hook.stages is not None and "pre-commit" not in hook.stages:
        fail("generated-docs drift self-test must run at the default pre-commit stage")
    if hook.root_files_pattern is not None and not precommit_pattern_matches(
        hook.root_files_pattern,
        SELF_TEST_TRIGGER_PATH,
        "generated-docs drift self-test root files selector",
    ):
        fail(
            "generated-docs drift self-test root files selector excludes its "
            "golden-path generator"
        )
    if hook.root_exclude_pattern is not None and precommit_pattern_matches(
        hook.root_exclude_pattern,
        SELF_TEST_TRIGGER_PATH,
        "generated-docs drift self-test root exclude selector",
    ):
        fail(
            "generated-docs drift self-test root exclude selector excludes its "
            "golden-path generator"
        )
    if not precommit_pattern_matches(
        hook.files_pattern,
        SELF_TEST_TRIGGER_PATH,
        "generated-docs drift self-test files selector",
    ):
        fail("generated-docs drift self-test does not cover its golden-path generator")
    if hook.exclude_pattern is not None and precommit_pattern_matches(
        hook.exclude_pattern,
        SELF_TEST_TRIGGER_PATH,
        "generated-docs drift self-test exclude selector",
    ):
        fail(
            "generated-docs drift self-test exclude selector excludes its golden-path generator"
        )

    effective_types = hook.types if hook.types is not None else ("file",)
    if any(required not in SELF_TEST_TRIGGER_TYPES for required in effective_types):
        fail(
            "generated-docs drift self-test types selector excludes its golden-path generator"
        )
    if hook.types_or and not any(
        candidate in SELF_TEST_TRIGGER_TYPES for candidate in hook.types_or
    ):
        fail(
            "generated-docs drift self-test types_or selector excludes its golden-path generator"
        )
    if hook.exclude_types and any(
        excluded in SELF_TEST_TRIGGER_TYPES for excluded in hook.exclude_types
    ):
        fail(
            "generated-docs drift self-test exclude_types selector excludes its golden-path generator"
        )


def parse_precommit_self_test(text: str) -> PrecommitHookContract:
    return parse_precommit_hook(
        text,
        "docs-generated-drift-self-test",
        "generated-docs drift self-test",
    )


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
    validate_skill_repository_state()
    validate_plugin_manifests()
    contract = json.loads(read_bounded_evidence(CONTRACT_PATH, "contract evidence"))
    if not isinstance(contract, dict):
        fail("golden-path contract root must be an object")
    if contract.get("schema") != "assay.agent_golden_path.v1":
        fail("unexpected golden-path contract schema")
    if contract.get("schema_version") != 1:
        fail("unexpected golden-path contract schema version")
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = workspace["workspace"]["package"]["version"]
    if contract.get("release_version") != version:
        fail("golden-path release_version must match the workspace version")
    if contract.get("release_tag") != f"v{version}":
        fail("golden-path release_tag must match the workspace version")
    validate_plugin_skill(contract)

    payloads: list[bytes] = []
    for path in SKILL_PATHS:
        payloads.append(read_bounded_evidence(path, "skill evidence"))

    if payloads[0] != payloads[1]:
        fail("Codex and Claude project skills are not byte-identical")

    text = payloads[0].decode("ascii")
    guide = read_bounded_evidence(GUIDE_PATH, "guide evidence").decode("utf-8")
    release = generated_release_block(guide)
    for required in (
        f"Assay `{version}`",
        f"`v{version}`",
        "assay version",
        "Upgrade",
        "Roll back",
        "Unreleased",
    ):
        if required not in release:
            fail(f"guide release block omits required wording: {required!r}")
    discovery = generated_discovery_block(guide)
    for root in DISCOVERY_SKILL_ROOTS:
        if root not in discovery:
            fail(f"guide discovery block omits project skill root: {root}")
    for required in (
        "assay-golden-path",
        INVOCATION_CWD_RULE,
        SOURCE_REPO_CWD_RULE,
        PYTHON_PLACEHOLDER_RULE,
        CURSOR_DOCS_URL,
        CURSOR_DOCS_ACCESSED,
        "does not exercise Cursor runtime discovery",
    ):
        if required not in discovery:
            fail(f"guide discovery block omits required wording: {required!r}")
    # Only the generator-owned block is semantic-gated. The surrounding guide
    # remains human-reviewed prose and is still protected by the drift gate.
    validate_public_vocabulary(discovery, contract)
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
        INVOCATION_CWD_RULE,
        SOURCE_REPO_CWD_RULE,
        PYTHON_PLACEHOLDER_RULE,
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
        guide_working_directory = working_directory or "invocation cwd"
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

    validate_public_vocabulary(text, contract)

    forbidden_claims = (
        "assay mcp-server",
        "assay_test_outbound",
        "six production tools",
        "safe agent",
        "compliance claim",
        "Cursor does not discover this project skill",
    )
    for phrase in forbidden_claims:
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
        '.claude-plugin/**',
        'packaging/claude-plugin/**',
        '.gitignore',
        '.gitattributes',
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
    self_test_hook = parse_precommit_self_test(precommit_text)
    validate_precommit_self_test(self_test_hook)
    mutation_hook = parse_precommit_hook(
        precommit_text,
        "agent-golden-path-skill-mutations",
        "agent golden-path skill mutations",
    )
    if mutation_hook.stages != ("pre-push",):
        fail("agent golden-path skill mutations must run only at pre-push")

    required_hook_paths = (
        ".gitattributes",
        ".claude-plugin/marketplace.json",
        "packaging/claude-plugin/.mcp.json",
        "packaging/claude-plugin/skills/assay-golden-path/SKILL.md",
    )
    for hook_id in ("docs-generated-drift", "agent-golden-path-skill-contract"):
        contract_hook = parse_precommit_hook(precommit_text, hook_id, hook_id)
        for required_path in required_hook_paths:
            if not precommit_pattern_matches(
                contract_hook.files_pattern, required_path, f"{hook_id} files selector"
            ):
                fail(f"{hook_id} does not cover {required_path}")

    print("agent golden-path skill: portable, byte-identical, and contract-complete")


if __name__ == "__main__":
    main()
