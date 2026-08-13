#!/usr/bin/env python3
"""Generate the machine and Markdown views of the #2154 golden-path contract."""

from __future__ import annotations

import json
import stat
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/ci/lib"))
from workspace_version import read_workspace_version  # noqa: E402

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
RELEASE_START = "<!-- agent-golden-path-release:start -->"
RELEASE_END = "<!-- agent-golden-path-release:end -->"
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
RELEASE_VERSION = read_workspace_version(ROOT / "Cargo.toml")
RELEASE_TAG = f"v{RELEASE_VERSION}"


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
                ["doctor", "--format", "json", "--config", "<config>"],
                config_check="checked",
            ),
            # Split from `success` because both are exit 0 and only this key separates them. The
            # success row was measured on an empty directory, so the published `Success 0` was
            # established by a run in which no config validation occurred.
            outcome(
                "no-config",
                "no config examined",
                0,
                stdout("json", "assay.doctor_report.v0"),
                ["doctor", "--format", "json"],
                config_check="skipped",
            ),
            # The row that carries this PR's behaviour. Without it the guide asserted an exit in
            # prose that no outcome enumerated, so nothing required a test to drive it.
            outcome(
                "diagnostics-error",
                "config examined, error-severity diagnostic",
                2,
                stdout("json", "assay.doctor_report.v0"),
                [
                    "doctor",
                    "--format",
                    "json",
                    "--config",
                    "<config>",
                    "--trace-file",
                    "<trace>",
                ],
                config_check="checked",
            ),
            outcome(
                "invalid-config",
                "invalid explicit config",
                2,
                stdout("json", "assay.doctor_report.v0"),
                ["doctor", "--format", "json", "--config", "<config>"],
                reason_code="E_CFG_PARSE",
                next_step='Run argv: ["assay","doctor","--config","<config>"]',
                config_error_code="E_CFG_PARSE",
            ),
        ],
        "stdout_summary": (
            "Parses as `assay.doctor_report.v0`. Every report carries "
            "`config_check.status`, one of `checked`, `skipped` or `failed`. Exit `0` on "
            "its own does not mean a config was examined: read `config_check.status` to "
            "tell a clean config from no config. A config that was examined and carries an "
            "error-severity `data_diagnostics[]` entry exits `2`, the class `decide_exit` "
            "gives that diagnostic for `assay validate` and `assay run` too; the text "
            "channel returns the same class for the same tree. A config failure remains "
            "JSON and carries the top-level `reason_code` and `next_step` alongside "
            "`config_error.code`."
        ),
        "failure_summary": (
            "An explicit config that will not load, absent or unreadable alike, is exit `2` "
            "and names the failing file in a concrete JSON argv next step, the same exit "
            "class `assay run` gives the same file. The reason code is not always the same "
            "one, per the non-claim below."
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
            ),
            outcome(
                "success-json",
                "success with `--format json`",
                0,
                stdout("json", "assay.init_report.v0"),
                ["init", "--preset", "dev", "--hello-trace", "--format", "json"],
                reason_code="",
                # A success carries a next step too. Leaving this `null` said the key was absent,
                # which is what `null` means for `policy-validation/valid` in this same file, and
                # the document has carried a concrete argv all along.
                next_step=(
                    'Run argv: ["assay","validate","--config","eval.yaml",'
                    '"--trace-file","traces/hello.jsonl"]'
                ),
            ),
            outcome(
                "unknown-preset-json",
                "unknown preset with `--format json`",
                2,
                stdout("json", "assay.init_report.v0"),
                ["init", "--preset", "not-a-preset", "--format", "json"],
                reason_code="E_INVALID_ARGS",
                next_step="Run: assay --help for usage",
            ),
        ],
        "stdout_summary": (
            "Default `text` is human progress; success ends with `Next: assay validate "
            "--config eval.yaml --trace-file traces/hello.jsonl`, and a failing run "
            "writes partial progress text rather than the fatal diagnosis. `--format "
            "json` replaces that stream with one `assay.init_report.v0` document "
            "naming `reason_code`, `next_step`, and the files created and skipped."
        ),
        "failure_summary": (
            "Under `--format json` a rejected `--preset` publishes `E_INVALID_ARGS` and "
            "a `next_step` on stdout. Failures the reason-code registry does not name, "
            "such as a filesystem write error, still produce no document: stdout is "
            "empty and the diagnosis stays on stderr."
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
                stdout("json", "assay.run_summary.v1"),
                [
                    "policy",
                    "validate",
                    "--input",
                    "<policy>",
                    "--format",
                    "json",
                ],
                reason_code="",
            ),
            outcome(
                "malformed",
                "malformed",
                2,
                stdout("json", "assay.run_summary.v1"),
                [
                    "policy",
                    "validate",
                    "--input",
                    "<policy>",
                    "--format",
                    "json",
                ],
                reason_code="E_POLICY_PARSE",
                next_step=(
                    'Run argv: ["assay","policy","validate","--input","<policy>"]'
                ),
            ),
        ],
        "stdout_summary": (
            "Both paths parse as `assay.run_summary.v1`; valid has exit `0` and an "
            "empty reason, while malformed YAML carries `E_POLICY_PARSE`. Other load or "
            "schema failures remain stderr-only until they receive an honest reason code."
        ),
        "failure_summary": (
            "Malformed policy is exit `2` and names the failing policy in a concrete "
            "JSON argv next step. Missing files and schema failures are not classified as "
            "parse failures."
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
                "verification-disabled",
                "Valid with verification disabled",
                0,
                stdout("json"),
                [
                    "evidence",
                    "show",
                    "<bundle>",
                    "--format",
                    "json",
                    "--no-verify",
                ],
            ),
            outcome(
                "tampered",
                "integrity failure",
                2,
                stdout("json", "assay.run_summary.v1"),
                ["evidence", "show", "<bundle>", "--format", "json"],
                reason_code="E_EVIDENCE_INTEGRITY",
                next_step=(
                    "Obtain an undamaged bundle from its producer; the content this bundle "
                    "carries does not match what it records"
                ),
            ),
            outcome(
                "unreadable",
                "unreadable bundle",
                2,
                stdout("json", "assay.run_summary.v1"),
                ["evidence", "show", "<bundle>", "--format", "json"],
                reason_code="E_EVIDENCE_UNREADABLE",
                next_step=(
                    'Run argv: ["assay","evidence","show","<bundle>",'
                    '"--format","json"]'
                ),
            ),
            outcome(
                "format-contract-failure",
                "format-contract failure",
                2,
                stdout("empty"),
                ["evidence", "show", "<bundle>", "--format", "json"],
                gap_issue=2219,
            ),
        ],
        "stdout_summary": (
            "Success parses as an object containing `manifest`, `events`, and `verify_mode`; "
            "the registered values are `enabled` and `disabled`, with `--no-verify` producing "
            "`disabled`. A recorded-value "
            "mismatch parses as `assay.run_summary.v1` with `E_EVIDENCE_INTEGRITY`; an "
            "unreadable path uses `E_EVIDENCE_UNREADABLE`."
        ),
        "failure_summary": (
            "Only the four verifier codes that establish a recorded-value mismatch map to "
            "`E_EVIDENCE_INTEGRITY`; I/O, gzip, and tar failures use "
            "`E_EVIDENCE_UNREADABLE`. Format-contract failures still exit `2` with empty "
            "stdout: [gap #2219](https://github.com/Rul1an/assay/issues/2219)."
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
                1,
                stdout("empty"),
                ["enforcement-sarif", "--input", "-", "--output", "-"],
            ),
        ],
        "stdout_summary": (
            "Valid input produces SARIF `2.1.0`. A malformed non-empty line produces no "
            "SARIF document."
        ),
        "failure_summary": (
            "Malformed NDJSON fails before projection and names the invalid input line on "
            "stderr. Blank lines remain accepted."
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
    "release_version": RELEASE_VERSION,
    "release_tag": RELEASE_TAG,
    "source_issue": 2154,
    "journey_issue": 1975,
    "non_claims": [
        "The contract records current behavior; gap rows are not clean results.",
        "Schema identity conventions outside this narrow contract remain owned by issue #2167.",
        "A passing evidence integrity check does not prove an external side effect.",
        "A doctor config failure does not distinguish an absent config from an unreadable "
        "one, and its recovery step is the invocation that produced it; both are owned by "
        "issue #2206.",
        "Read config_check.status before reading data_diagnostics: only the value checked "
        "means a config was read, and on skipped the absent data_diagnostics records an "
        "unchecked config rather than a clean one.",
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


def render_release() -> str:
    return "\n".join(
        (
            "## Release-pinned start",
            "",
            f"This journey is pinned to Assay `{RELEASE_VERSION}` "
            f"([`{RELEASE_TAG}`](https://github.com/Rul1an/assay/releases/tag/{RELEASE_TAG})).",
            f"Install the CLI from a verified channel, then require `assay version` to print "
            f"`{RELEASE_VERSION}` before using the table below. Behavior merged after that tag "
            "is `Unreleased` and is not part of this release claim.",
            "",
            "Upgrade by installing a newer explicit release and re-running all nine steps.",
            f"Roll back by reinstalling `{RELEASE_TAG}` from the GitHub release assets and "
            "re-running the same journey.",
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
    with_release = replace_generated_block(
        current,
        RELEASE_START,
        RELEASE_END,
        render_release(),
        "release",
    )
    with_discovery = replace_generated_block(
        with_release,
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
