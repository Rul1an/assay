#!/usr/bin/env python3
"""Generate the machine and Markdown views of the #2154 golden-path contract."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
JSON_OUTPUT = ROOT / "docs/generated/agent-golden-path.json"
MARKDOWN_OUTPUT = ROOT / "docs/guides/agent-golden-path.md"
TABLE_START = "<!-- agent-golden-path-table:start -->"
TABLE_END = "<!-- agent-golden-path-table:end -->"


def stdout(kind: str, document: str | None = None) -> dict[str, object]:
    return {"kind": kind, "document": document}


def outcome(
    name: str,
    label: str,
    exit_code: int,
    stdout_contract: dict[str, object],
    *,
    gap_issue: int | None = None,
    **details: object,
) -> dict[str, object]:
    return {
        "name": name,
        "label": label,
        "exit_code": exit_code,
        "stdout": stdout_contract,
        "reason_code": None,
        "next_step": None,
        "gap_issue": gap_issue,
        **details,
    }


STEPS: list[dict[str, object]] = [
    {
        "step": 1,
        "id": "install-check",
        "label": "Install check",
        "command": "assay version",
        "outcomes": [outcome("success", "Success", 0, stdout("text"))],
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
        "command": "assay doctor --format json",
        "outcomes": [
            outcome("success", "Success", 0, stdout("json", "assay.doctor_report.v0")),
            outcome(
                "invalid-config",
                "invalid explicit config",
                1,
                stdout("json", "assay.doctor_report.v0"),
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
        "command": "assay init --preset dev --hello-trace",
        "outcomes": [
            outcome("success", "Success", 0, stdout("text")),
            outcome(
                "unknown-preset",
                "unknown preset",
                2,
                stdout("text"),
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
        "command": "assay policy validate --input policy.yaml",
        "outcomes": [
            outcome("valid", "Valid", 0, stdout("empty")),
            outcome("malformed", "malformed", 2, stdout("empty"), gap_issue=2162),
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
        "command": "assay run --config eval.yaml --format json",
        "outcomes": [
            outcome("success", "All tests pass", 0, stdout("json", "assay.run_report.v1")),
            outcome(
                "completed-test-failure",
                "completed run with failed tests",
                1,
                stdout("json", "assay.run_report.v1"),
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
        "command": "assay-mcp-server proxy-enforce <args>",
        "outcomes": [
            outcome(
                "policy-denied",
                "Policy-denied call after stdin closes",
                0,
                stdout("json_lines", "jsonrpc-2.0"),
                jsonrpc_error_code=-32042,
                origin="assay-proxy",
                reason="no_declared_allowance",
            ),
            outcome(
                "startup-failure",
                "startup input failure",
                1,
                stdout("empty"),
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
        "command": "assay evidence show <bundle> --format json",
        "outcomes": [
            outcome("valid", "Valid", 0, stdout("json")),
            outcome("tampered", "integrity failure", 2, stdout("empty"), gap_issue=2164),
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
        "command": "assay evidence verify-privileged-mcp-action <bundle> --format json",
        "outcomes": [
            outcome(
                "valid",
                "Valid",
                0,
                stdout("json", "assay.privileged_mcp_action.verify.report.v0"),
            ),
            outcome(
                "tampered",
                "integrity or profile failure",
                2,
                stdout("json", "assay.privileged_mcp_action.verify.report.v0"),
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
        "command": "assay-mcp-server enforcement-sarif --input <decisions.ndjson> --output -",
        "outcomes": [
            outcome("valid", "Valid input", 0, stdout("json", "sarif-2.1.0")),
            outcome(
                "malformed",
                "malformed non-empty NDJSON",
                0,
                stdout("json", "sarif-2.1.0"),
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
    outcomes = step["outcomes"]
    assert isinstance(outcomes, list)
    return "; ".join(
        f"{item['label']} `{item['exit_code']}`" for item in outcomes
    ) + "."


def render_table() -> str:
    rows = [
        "| step | command | exit code | stdout | on failure |",
        "|---|---|---|---|---|",
    ]
    for step in STEPS:
        rows.append(
            "| {step}. {label} | `{command}` | {exit_codes} | {stdout_summary} | "
            "{failure_summary} |".format(
                step=step["step"],
                label=step["label"],
                command=step["command"],
                exit_codes=exit_summary(step),
                stdout_summary=step["stdout_summary"],
                failure_summary=step["failure_summary"],
            )
        )
    return "\n".join(rows)


def render_markdown(current: str) -> str:
    if current.count(TABLE_START) != 1 or current.count(TABLE_END) != 1:
        raise SystemExit("agent golden-path guide must contain exactly one table marker pair")
    before, remainder = current.split(TABLE_START, 1)
    _, after = remainder.split(TABLE_END, 1)
    return f"{before}{TABLE_START}\n{render_table()}\n{TABLE_END}{after}"


def main() -> None:
    current = MARKDOWN_OUTPUT.read_text(encoding="utf-8")
    rendered_markdown = render_markdown(current)
    JSON_OUTPUT.write_text(
        json.dumps(CONTRACT, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )
    MARKDOWN_OUTPUT.write_text(rendered_markdown, encoding="utf-8")


if __name__ == "__main__":
    main()
