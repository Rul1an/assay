#!/usr/bin/env python3
"""Pin Assay's two direct actions/attest producers in lockstep."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import NamedTuple

EXPECTED_SHA = "1e69f48acb82d1966a394da916b4c1698aa569d6"
EXPECTED_TAG = "v4.2.2"
WORKFLOW_DIR = Path(".github/workflows")
ATTEST_ACTION = "actions/attest@"
YAML_HEX_ESCAPE_RE = re.compile(
    r"\\(?:x([0-9a-fA-F]{2})|u([0-9a-fA-F]{4})|U([0-9a-fA-F]{8}))"
)
USES_RE = re.compile(
    r"^[ \t]*(?:-[ \t]+)?uses:[ \t]+"
    r"(?P<quote>['\"]?)actions/attest@"
    r"(?P<sha>[0-9a-f]{40})(?P=quote)[ \t]+"
    r"#[ \t]+(?P<tag>v\d+\.\d+\.\d+)[ \t]*$"
)
DIRECT_USES_SHA_RE = re.compile(r"^actions/attest@([0-9a-f]{40})$")


class Producer(NamedTuple):
    path: Path
    step_id: str
    subject_checksums: str
    bundle_output: str


class WorkflowParseError(Exception):
    """A producer workflow could not be loaded, so the contract cannot pass."""


PRODUCERS = (
    Producer(
        path=Path(".github/workflows/runner-spike-delegated.yml"),
        step_id="attest-proof-pack",
        subject_checksums="assay-runner-proof-upload/subject-checksums.txt",
        bundle_output="steps.attest-proof-pack.outputs.bundle-path",
    ),
    Producer(
        path=Path(".github/workflows/privileged-mcp-action-pack-release.yml"),
        step_id="attest",
        subject_checksums="release/SHA256SUMS",
        bundle_output="steps.attest.outputs.bundle-path",
    ),
)


def decode_hex_escape(match: re.Match[str]) -> str:
    codepoint = next(group for group in match.groups() if group is not None)
    try:
        return chr(int(codepoint, 16))
    except ValueError:
        return "\N{REPLACEMENT CHARACTER}"


def active_attest_pins(text: str) -> list[tuple[str, str]]:
    pins: list[tuple[str, str]] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = USES_RE.match(line)
        if match:
            pins.append((match.group("sha"), match.group("tag")))
    return pins


def active_attest_references(text: str) -> int:
    active_text = "\n".join(
        line for line in text.splitlines() if not line.lstrip().startswith("#")
    )
    # GitHub resolves repository names case-insensitively, and YAML double-quoted
    # scalars decode hexadecimal escapes and escaped line breaks before dispatch.
    # Needle is actions/attest@, which does not match attest-build-provenance@.
    normalized = re.sub(r"\\\r?\n[ \t]*", "", active_text)
    normalized = YAML_HEX_ESCAPE_RE.sub(decode_hex_escape, normalized)
    normalized = normalized.casefold()
    return normalized.count(ATTEST_ACTION)


def has_active_attest_callsite(text: str) -> bool:
    return active_attest_references(text) > 0


def load_workflow_mapping(path: Path) -> object:
    try:
        completed = subprocess.run(
            [
                "ruby",
                "-ryaml",
                "-rjson",
                "-e",
                "puts JSON.generate(YAML.load_file(ARGV[0]))",
                str(path),
            ],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError as error:
        raise WorkflowParseError(f"{path}: yaml parser unavailable: {error}") from error
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip() or (
            f"exit {completed.returncode}"
        )
        raise WorkflowParseError(f"{path}: malformed workflow YAML: {detail}")
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise WorkflowParseError(
            f"{path}: yaml parser returned non-JSON: {error}"
        ) from error


def iter_steps(document: object) -> list[dict[str, object]]:
    if not isinstance(document, dict):
        raise WorkflowParseError("workflow root is not a mapping")
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        raise WorkflowParseError("workflow jobs is not a mapping")
    steps: list[dict[str, object]] = []
    for job in jobs.values():
        if not isinstance(job, dict):
            continue
        raw_steps = job.get("steps") or []
        if not isinstance(raw_steps, list):
            continue
        for step in raw_steps:
            if isinstance(step, dict):
                steps.append(step)
    return steps


def direct_attest_steps(document: object) -> list[dict[str, object]]:
    found: list[dict[str, object]] = []
    for step in iter_steps(document):
        uses = step.get("uses")
        if isinstance(uses, str) and DIRECT_USES_SHA_RE.fullmatch(uses.strip()):
            found.append(step)
    return found


def producer_contract_errors(producer: Producer, text: str, document: object) -> list[str]:
    errors: list[str] = []
    steps = direct_attest_steps(document)
    if len(steps) != 1:
        errors.append(
            f"{producer.path}: want exactly one direct actions/attest step, "
            f"found {len(steps)}"
        )
        return errors
    step = steps[0]
    if step.get("id") != producer.step_id:
        errors.append(
            f"{producer.path}: attest step id {step.get('id')!r}, "
            f"want {producer.step_id!r}"
        )
    with_block = step.get("with")
    if not isinstance(with_block, dict):
        errors.append(
            f"{producer.path}: SHA pin is unwired; missing with.subject-checksums "
            f"{producer.subject_checksums}"
        )
    elif with_block.get("subject-checksums") != producer.subject_checksums:
        errors.append(
            f"{producer.path}: subject-checksums {with_block.get('subject-checksums')!r}, "
            f"want {producer.subject_checksums!r}"
        )
    if producer.bundle_output not in text:
        errors.append(
            f"{producer.path}: missing bundle consumer {producer.bundle_output}"
        )
    return errors


def check() -> list[str]:
    errors: list[str] = []
    expected = (EXPECTED_SHA, EXPECTED_TAG)
    expected_paths = {producer.path for producer in PRODUCERS}

    discovered: set[Path] = set()
    for pattern in ("*.yml", "*.yaml"):
        for workflow in WORKFLOW_DIR.glob(pattern):
            if has_active_attest_callsite(workflow.read_text(encoding="utf-8")):
                discovered.add(workflow)
    for workflow in sorted(discovered - expected_paths):
        errors.append(f"unexpected actions/attest workflow callsite: {workflow}")

    for producer in PRODUCERS:
        workflow = producer.path
        if not workflow.is_file():
            errors.append(f"workflow missing: {workflow}")
            continue
        text = workflow.read_text(encoding="utf-8")
        pins = active_attest_pins(text)
        references = active_attest_references(text)
        if references != len(pins):
            errors.append(
                f"{workflow}: found {references} actions/attest reference(s), "
                f"but only {len(pins)} canonical pin(s)"
            )
        if len(pins) != 1:
            errors.append(
                f"{workflow}: want exactly one active actions/attest SHA pin, "
                f"found {len(pins)}"
            )
        elif pins[0] != expected:
            errors.append(
                f"{workflow}: pin @{pins[0][0]} # {pins[0][1]}, "
                f"want @{EXPECTED_SHA} # {EXPECTED_TAG}"
            )
        try:
            document = load_workflow_mapping(workflow)
        except WorkflowParseError as error:
            errors.append(str(error))
            continue
        errors.extend(producer_contract_errors(producer, text, document))
    return errors


def main() -> int:
    errors = check()
    if errors:
        print("FAIL: actions/attest workflow pins", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print(
        f"ok    {len(PRODUCERS)} actions/attest callsites "
        f"@{EXPECTED_SHA} # {EXPECTED_TAG}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
