#!/usr/bin/env python3
"""Generate the machine and Markdown views of the #2154 golden-path contract."""

from __future__ import annotations

import json
import stat
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
JSON_OUTPUT = ROOT / "docs/generated/agent-golden-path.json"
MARKDOWN_OUTPUT = ROOT / "docs/guides/agent-golden-path.md"
SKILL_OUTPUTS = (
    ROOT / ".agents/skills/assay-golden-path/SKILL.md",
    ROOT / ".claude/skills/assay-golden-path/SKILL.md",
)
PLUGIN_SKILL_OUTPUTS = (
    ROOT / "packaging/claude-plugin/skills/assay-golden-path/SKILL.md",
)
PLUGIN_CONTRACT_OUTPUT = (
    ROOT
    / "packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json"
)
PLUGIN_ASSET_ROOT = (
    "${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/assets/privileged-action-gate"
)
PLUGIN_RESOURCE_COPIES = (
    (JSON_OUTPUT, PLUGIN_CONTRACT_OUTPUT),
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
MAX_PLUGIN_RESOURCE_BYTES = 1024 * 1024
TABLE_START = "<!-- agent-golden-path-table:start -->"
TABLE_END = "<!-- agent-golden-path-table:end -->"
DISCOVERY_START = "<!-- agent-golden-path-discovery:start -->"
DISCOVERY_END = "<!-- agent-golden-path-discovery:end -->"
CURSOR_DOCS_URL = "https://cursor.com/docs/skills"
CURSOR_DOCS_ACCESSED = "2026-08-09"
SKILL_DESCRIPTION = (
    "Drive Assay's install-to-evidence golden path and interpret its stdout and exit "
    "codes. Use when an agent must operate or diagnose Assay; do not use it to infer "
    "provider execution, external side effects, or a clean result from missing output."
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


def stdout(kind: str, document: str | None = None) -> dict[str, object]:
    return {"kind": kind, "document": document}


def outcome(
    name: str,
    label: str,
    exit_code: int,
    stdout_contract: dict[str, object],
    argv: list[str],
    *,
    gap_issue: int | None = None,
    **details: object,
) -> dict[str, object]:
    return {
        "name": name,
        "label": label,
        "exit_code": exit_code,
        "argv": argv,
        "stdout": stdout_contract,
        "reason_code": None,
        "next_step": None,
        "gap_issue": gap_issue,
        **details,
    }


def require_type(value: object, expected: type, label: str):
    if not isinstance(value, expected):
        raise SystemExit(f"{label} must be {expected.__name__}")
    return value


STEPS: list[dict[str, object]] = [
    {
        "step": 1,
        "id": "install-check",
        "label": "Install check",
        "binary": "assay",
        "outcomes": [outcome("success", "Success", 0, stdout("text"), ["version"])],
        "stdout_summary": "One `MAJOR.MINOR.PATCH` line.",
        "failure_summary": (
            "A missing or unstartable binary is a host spawn failure: no Assay process "
            "runs, so Assay produces no stdout or exit code."
        ),
    },
    {
        "step": 2,
        "id": "preflight",
        "label": "Preflight",
        "binary": "assay",
        "outcomes": [
            outcome(
                "success",
                "Success",
                0,
                stdout("json", "assay.doctor_report.v0"),
                ["doctor", "--format", "json"],
            ),
            outcome(
                "invalid-config",
                "invalid explicit config",
                1,
                stdout("json", "assay.doctor_report.v0"),
                ["doctor", "--format", "json", "--config", "<config>"],
                gap_issue=2160,
                config_error_code="E_CFG_PARSE",
            ),
        ],
        "stdout_summary": (
            "Parses as `assay.doctor_report.v0`. A config failure remains JSON and "
            "carries `config_error.code: E_CFG_PARSE`."
        ),
        "failure_summary": (
            "`reason_code` and `next_step` are absent, and the exit is outside the frozen "
            "config/usage class: [gap #2160](https://github.com/Rul1an/assay/issues/2160)."
        ),
    },
    {
        "step": 3,
        "id": "starter-files",
        "label": "Starter files",
        "binary": "assay",
        "outcomes": [
            outcome(
                "success",
                "Success",
                0,
                stdout("text"),
                ["init", "--preset", "dev", "--hello-trace"],
            ),
            outcome(
                "unknown-preset",
                "unknown preset",
                2,
                stdout("text"),
                ["init", "--preset", "not-a-preset"],
                gap_issue=2161,
            ),
        ],
        "stdout_summary": (
            "Human progress text; success ends with `Next: assay validate --config "
            "eval.yaml --trace-file traces/hello.jsonl`. A failing run writes partial "
            "progress text, not the fatal diagnosis."
        ),
        "failure_summary": (
            "No machine report, `reason_code`, or `next_step` on stdout: "
            "[gap #2161](https://github.com/Rul1an/assay/issues/2161)."
        ),
    },
    {
        "step": 4,
        "id": "policy-validation",
        "label": "Policy validation",
        "binary": "assay",
        "outcomes": [
            outcome(
                "valid",
                "Valid",
                0,
                stdout("empty"),
                ["policy", "validate", "--input", "<policy>"],
            ),
            outcome(
                "malformed",
                "malformed",
                2,
                stdout("empty"),
                ["policy", "validate", "--input", "<policy>"],
                gap_issue=2162,
            ),
        ],
        "stdout_summary": "Empty on both paths.",
        "failure_summary": (
            "The diagnosis, registered reason, and next step are absent from stdout: "
            "[gap #2162](https://github.com/Rul1an/assay/issues/2162)."
        ),
    },
    {
        "step": 5,
        "id": "evaluation-result",
        "label": "Evaluation result",
        "binary": "assay",
        "outcomes": [
            outcome(
                "success",
                "All tests pass",
                0,
                stdout("json", "assay.run_report.v1"),
                [
                    "run",
                    "--config",
                    "eval.yaml",
                    "--trace-file",
                    "traces/hello.jsonl",
                    "--format",
                    "json",
                ],
            ),
            outcome(
                "completed-test-failure",
                "completed run with failed tests",
                1,
                stdout("json", "assay.run_report.v1"),
                ["run", "--config", "<config>", "--format", "json"],
                classification="completed_test_failure",
            ),
        ],
        "stdout_summary": (
            "Both completed outcomes parse as `assay.run_report.v1`; failed results carry "
            "`status: fail` inside `results`."
        ),
        "failure_summary": (
            "Exit `1` is a completed results report, not an early-failure diagnosis; "
            "`reason_code` and `next_step` are absent by design."
        ),
    },
    {
        "step": 6,
        "id": "protected-action",
        "label": "Protected action",
        "binary": "assay-mcp-server",
        "working_directory": "examples/privileged-action-gate",
        "outcomes": [
            outcome(
                "policy-denied",
                "Policy-denied call after stdin closes",
                0,
                stdout("json_lines", "jsonrpc-2.0"),
                [
                    "proxy-enforce",
                    "--upstream-command",
                    "<python>",
                    "--upstream-arg",
                    "-u",
                    "--upstream-arg",
                    "mock_github_mcp.py",
                    "--enforce-policy",
                    "policies/no-allowance.yaml",
                    "--declared-mcp-manifest",
                    "baseline-approved.json",
                ],
                jsonrpc_error_code=-32042,
                origin="assay-proxy",
                reason="no_declared_allowance",
            ),
            outcome(
                "startup-failure",
                "startup input failure",
                1,
                stdout("empty"),
                [
                    "proxy-enforce",
                    "--upstream-command",
                    "<python>",
                    "--enforce-policy",
                    "missing.yaml",
                    "--declared-mcp-manifest",
                    "missing.json",
                ],
                gap_issue=2163,
            ),
        ],
        "stdout_summary": (
            "The denied `tools/call` response pins `error.code: -32042`, "
            "`error.data.origin: assay-proxy`, and `error.data.reason: "
            "no_declared_allowance`."
        ),
        "failure_summary": (
            "Policy denial is not a process failure. A missing enforcement policy fails "
            "startup with empty stdout and no stable reason/next-step object: "
            "[gap #2163](https://github.com/Rul1an/assay/issues/2163)."
        ),
    },
    {
        "step": 7,
        "id": "evidence-inspection",
        "label": "Evidence inspection",
        "binary": "assay",
        "outcomes": [
            outcome(
                "valid",
                "Valid",
                0,
                stdout("json"),
                ["evidence", "show", "<bundle>", "--format", "json"],
            ),
            outcome(
                "tampered",
                "integrity failure",
                2,
                stdout("empty"),
                ["evidence", "show", "<bundle>", "--format", "json"],
                gap_issue=2164,
            ),
        ],
        "stdout_summary": (
            "Success parses as an object containing `manifest` and `events`. Integrity "
            "failure produces no stdout."
        ),
        "failure_summary": (
            "No JSON failure report, `reason_code`, or `next_step`: "
            "[gap #2164](https://github.com/Rul1an/assay/issues/2164)."
        ),
    },
    {
        "step": 8,
        "id": "offline-profile-verification",
        "label": "Offline profile verification",
        "binary": "assay",
        "outcomes": [
            outcome(
                "valid",
                "Valid",
                0,
                stdout("json", "assay.privileged_mcp_action.verify.report.v0"),
                [
                    "evidence",
                    "verify-privileged-mcp-action",
                    "<bundle>",
                    "--format",
                    "json",
                ],
            ),
            outcome(
                "tampered",
                "integrity or profile failure",
                2,
                stdout("json", "assay.privileged_mcp_action.verify.report.v0"),
                [
                    "evidence",
                    "verify-privileged-mcp-action",
                    "<bundle>",
                    "--format",
                    "json",
                ],
                gap_issue=2165,
            ),
        ],
        "stdout_summary": (
            "Both paths parse as `assay.privileged_mcp_action.verify.report.v0`. Success "
            "has `bundle_integrity: pass` and `verdict: valid`; tamper has "
            "`bundle_integrity: fail`, a bounded finding, and no verdict."
        ),
        "failure_summary": (
            "The failure report has no registered `reason_code` or actionable `next_step`: "
            "[gap #2165](https://github.com/Rul1an/assay/issues/2165)."
        ),
    },
    {
        "step": 9,
        "id": "sarif-projection",
        "label": "SARIF projection",
        "binary": "assay-mcp-server",
        "outcomes": [
            outcome(
                "valid",
                "Valid input",
                0,
                stdout("json", "sarif-2.1.0"),
                ["enforcement-sarif", "--input", "-", "--output", "-"],
            ),
            outcome(
                "malformed",
                "malformed non-empty NDJSON",
                0,
                stdout("json", "sarif-2.1.0"),
                ["enforcement-sarif", "--input", "-", "--output", "-"],
                gap_issue=2166,
            ),
        ],
        "stdout_summary": (
            "Valid input produces SARIF `2.1.0`. Malformed lines are currently discarded "
            "and can produce a clean report with zero results."
        ),
        "failure_summary": (
            "This is fail-open, not a successful empty projection: "
            "[gap #2166](https://github.com/Rul1an/assay/issues/2166). No reason or next "
            "step is emitted."
        ),
    },
]

for step in STEPS:
    outcomes = require_type(step["outcomes"], list, "golden-path outcomes")
    primary_outcome = require_type(
        outcomes[0], dict, "primary golden-path outcome"
    )
    primary_argv = require_type(
        primary_outcome["argv"], list, "primary golden-path argv"
    )
    step["command"] = " ".join([str(step["binary"]), *map(str, primary_argv)])


CONTRACT: dict[str, object] = {
    "schema": "assay.agent_golden_path.v1",
    "schema_version": 1,
    "generated_by": "scripts/docs/generate-agent-golden-path.py",
    "source_issue": 2154,
    "journey_issue": 1975,
    "non_claims": [
        "The contract records current behavior; gap rows are not clean results.",
        "Schema identity conventions outside this narrow contract remain owned by issue #2167.",
        "A passing evidence integrity check does not prove an external side effect.",
    ],
    "steps": STEPS,
}


def exit_summary(step: dict[str, object]) -> str:
    outcomes = require_type(step["outcomes"], list, "golden-path outcomes")
    return "; ".join(
        f"{item['label']} `{item['exit_code']}`" for item in outcomes
    ) + "."


def render_table() -> str:
    rows = [
        "| step | working directory | command | exit code | stdout | on failure |",
        "|---|---|---|---|---|---|",
    ]
    for step in STEPS:
        working_directory = step.get("working_directory") or "invocation cwd"
        rows.append(
            "| {step}. {label} | `{working_directory}` | `{command}` | {exit_codes} | "
            "{stdout_summary} | {failure_summary} |".format(
                step=step["step"],
                label=step["label"],
                working_directory=working_directory,
                command=step["command"],
                exit_codes=exit_summary(step),
                stdout_summary=step["stdout_summary"],
                failure_summary=step["failure_summary"],
            )
        )
    return "\n".join(rows)


def render_discovery() -> str:
    return "\n".join(
        (
            "## Project Skill Discovery",
            "",
            "The generated `assay-golden-path` project skill is available at both:",
            "",
            "- `.agents/skills/assay-golden-path/SKILL.md`",
            "- `.claude/skills/assay-golden-path/SKILL.md`",
            "",
            INVOCATION_CWD_RULE,
            SOURCE_REPO_CWD_RULE,
            PYTHON_PLACEHOLDER_RULE,
            "",
            f"Source: [Cursor's skill documentation]({CURSOR_DOCS_URL}), accessed on "
            f"`{CURSOR_DOCS_ACCESSED}`.",
            "The documentation describes `.agents/skills/` as a project-level location "
            "and `.claude/skills/` as a compatibility location. This repository does not "
            "exercise Cursor runtime discovery.",
        )
    )


def replace_generated_block(
    current: str, start: str, end: str, rendered: str, label: str
) -> str:
    if current.count(start) != 1 or current.count(end) != 1:
        raise SystemExit(
            f"agent golden-path guide must contain exactly one {label} marker pair"
        )
    if current.index(start) > current.index(end):
        raise SystemExit(f"agent golden-path guide {label} markers are out of order")
    before, remainder = current.split(start, 1)
    _, after = remainder.split(end, 1)
    return f"{before}{start}\n{rendered}\n{end}{after}"


def render_markdown(current: str) -> str:
    with_discovery = replace_generated_block(
        current,
        DISCOVERY_START,
        DISCOVERY_END,
        render_discovery(),
        "discovery",
    )
    return replace_generated_block(
        with_discovery, TABLE_START, TABLE_END, render_table(), "table"
    )


def render_skill(*, plugin: bool = False) -> str:
    lines = [
        "---",
        "name: assay-golden-path",
        f"description: {SKILL_DESCRIPTION}",
        "---",
        "",
        "# Assay Golden Path",
        "",
        "Drive the nine steps below in order. Read stdout and the process exit code",
        "separately; a policy denial can be a successful JSON-RPC exchange rather than",
        "a process failure.",
        "",
    ]
    if plugin:
        lines.extend(
            [
                "`${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/references/agent-golden-path.json` is the authoritative machine contract.",
                "Read it when exact argv, fields, or per-outcome metadata are needed.",
                "",
                INVOCATION_CWD_RULE,
                "Fixtures named by the contract under `examples/privileged-action-gate` are bundled at "
                f"`{PLUGIN_ASSET_ROOT}`.",
                "A present `working_directory` in the contract resolves through that mapping.",
                PYTHON_PLACEHOLDER_RULE,
                "",
                f"{EMPTY_STDOUT_RULE}",
                "Do not replace a linked gap with an inferred clean result.",
                "",
                "Claude Code loads this skill from the installed Assay plugin.",
                "",
            ]
        )
    else:
        lines.extend(
            [
                "`docs/generated/agent-golden-path.json` is the authoritative machine contract.",
                "Read it when exact argv, fields, or per-outcome metadata are needed. Edit and",
                "run `scripts/docs/generate-agent-golden-path.py` instead of editing this file.",
                "",
                INVOCATION_CWD_RULE,
                SOURCE_REPO_CWD_RULE,
                PYTHON_PLACEHOLDER_RULE,
                "",
                f"{EMPTY_STDOUT_RULE}",
                "Do not replace a linked gap with an inferred clean result.",
                "",
                "Codex and Claude Code are the project-skill hosts exercised here.",
                f"Cursor's skill documentation ({CURSOR_DOCS_URL}), accessed on "
                f"{CURSOR_DOCS_ACCESSED}, describes .agents/skills as a project-level location "
                "and .claude/skills as a compatibility location. This repository does not "
                "exercise Cursor runtime discovery.",
                "",
            ]
        )
    lines.extend(["## Journey", ""])
    for step in STEPS:
        lines.extend([f"### {step['step']}. {step['label']}", ""])
        working_directory = step.get("working_directory")
        if working_directory is not None:
            rendered_working_directory = PLUGIN_ASSET_ROOT if plugin else working_directory
            lines.extend([f"Working directory: `{rendered_working_directory}`", ""])
        lines.extend(
            [
                f"Run: `{step['command']}`",
                "",
                f"Exit: {exit_summary(step)}",
                "",
                f"Stdout: {step['stdout_summary']}",
                "",
                f"On failure: {step['failure_summary']}",
                "",
            ]
        )

    lines.extend(["## Non-claims", ""])
    non_claims = require_type(CONTRACT["non_claims"], list, "golden-path non-claims")
    lines.extend(f"- {claim}" for claim in non_claims)
    lines.append("")
    return "\n".join(lines)


def read_plugin_resource(source: Path) -> bytes:
    if source.is_symlink():
        raise RuntimeError(f"plugin resource source must not be a symlink: {source}")
    try:
        metadata = source.stat()
    except FileNotFoundError as error:
        raise RuntimeError(f"plugin resource source is missing: {source}") from error
    if not stat.S_ISREG(metadata.st_mode):
        raise RuntimeError(f"plugin resource source must be a regular file: {source}")
    if metadata.st_size > MAX_PLUGIN_RESOURCE_BYTES:
        raise RuntimeError(
            f"plugin resource source exceeds {MAX_PLUGIN_RESOURCE_BYTES} bytes: {source}"
        )
    with source.open("rb") as resource:
        payload = resource.read(MAX_PLUGIN_RESOURCE_BYTES + 1)
    if len(payload) > MAX_PLUGIN_RESOURCE_BYTES:
        raise RuntimeError(
            f"plugin resource source grew beyond {MAX_PLUGIN_RESOURCE_BYTES} bytes: {source}"
        )
    return payload


def main() -> None:
    current = MARKDOWN_OUTPUT.read_text(encoding="utf-8")
    rendered_markdown = render_markdown(current)
    rendered_skill = render_skill()
    rendered_plugin_skill = render_skill(plugin=True)
    rendered_contract = (json.dumps(CONTRACT, indent=2, ensure_ascii=True) + "\n").encode(
        "ascii"
    )
    plugin_resources = [
        (rendered_contract if source == JSON_OUTPUT else read_plugin_resource(source), output)
        for source, output in PLUGIN_RESOURCE_COPIES
    ]

    JSON_OUTPUT.write_bytes(rendered_contract)
    MARKDOWN_OUTPUT.write_text(rendered_markdown, encoding="utf-8")
    for output in SKILL_OUTPUTS:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered_skill, encoding="ascii")
    for output in PLUGIN_SKILL_OUTPUTS:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered_plugin_skill, encoding="ascii")
    for payload, output in plugin_resources:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)


if __name__ == "__main__":
    main()
