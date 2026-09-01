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
USES_RE = re.compile(
    r"^[ \t]*(?:-[ \t]+)?uses:[ \t]+"
    r"(?P<quote>['\"]?)actions/attest@"
    r"(?P<sha>[0-9a-f]{40})(?P=quote)[ \t]+"
    r"#[ \t]+(?P<tag>v\d+\.\d+\.\d+)[ \t]*$"
)
DIRECT_USES_SHA_RE = re.compile(r"^actions/attest@([0-9a-f]{40})$", re.IGNORECASE)


class Producer(NamedTuple):
    path: Path
    job_id: str
    step_id: str
    subject_checksums: str
    subject_producer: str
    bundle_output: str
    consumer_if: str | None
    consumer_env: tuple[str, str] | None


class WorkflowParseError(Exception):
    """A producer workflow could not be loaded, so the contract cannot pass."""


PRODUCERS = (
    Producer(
        path=Path(".github/workflows/runner-spike-delegated.yml"),
        job_id="phase1-delegated-gates",
        step_id="attest-proof-pack",
        subject_checksums="assay-runner-proof-upload/subject-checksums.txt",
        subject_producer="python3 scripts/ci/assay_runner_delegated_proof_pack.py",
        bundle_output="steps.attest-proof-pack.outputs.bundle-path",
        consumer_if="always() && steps.attest-proof-pack.outputs.bundle-path != ''",
        consumer_env=None,
    ),
    Producer(
        path=Path(".github/workflows/privileged-mcp-action-pack-release.yml"),
        job_id="release-pack",
        step_id="attest",
        subject_checksums="release/SHA256SUMS",
        subject_producer=(
            "sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS"
        ),
        bundle_output="steps.attest.outputs.bundle-path",
        consumer_if=None,
        consumer_env=(
            "ATTESTATION_BUNDLE",
            "${{ steps.attest.outputs.bundle-path }}",
        ),
    ),
)


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


def mapping(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise WorkflowParseError(f"{label} is not a mapping")
    return value


def job_steps(job: dict[str, object]) -> list[dict[str, object]]:
    raw_steps = job.get("steps") or []
    if not isinstance(raw_steps, list):
        return []
    return [step for step in raw_steps if isinstance(step, dict)]


def iter_jobs(document: object) -> list[tuple[str, dict[str, object]]]:
    root = mapping(document, "workflow root")
    jobs = mapping(root.get("jobs"), "workflow jobs")
    named: list[tuple[str, dict[str, object]]] = []
    for name, job in jobs.items():
        if isinstance(name, str) and isinstance(job, dict):
            named.append((name, job))
    return named


def is_direct_attest_uses(uses: object) -> bool:
    return isinstance(uses, str) and DIRECT_USES_SHA_RE.fullmatch(uses.strip()) is not None


def structural_attest_steps(
    document: object,
) -> list[tuple[str, int, dict[str, object]]]:
    found: list[tuple[str, int, dict[str, object]]] = []
    for job_id, job in iter_jobs(document):
        for index, step in enumerate(job_steps(job)):
            if is_direct_attest_uses(step.get("uses")):
                found.append((job_id, index, step))
    return found


def condition_is_false(value: object) -> bool:
    return value is False or (
        isinstance(value, str) and value.strip().casefold() in {"false", "${{ false }}"}
    )


def is_reachable(node: dict[str, object]) -> bool:
    return not condition_is_false(node.get("if"))


def step_run_text(step: dict[str, object]) -> str:
    run = step.get("run")
    return run if isinstance(run, str) else ""


def step_has_bundle_expression(step: dict[str, object], expression: str) -> bool:
    if expression in step_run_text(step):
        return True
    env = step.get("env")
    if not isinstance(env, dict):
        return False
    return any(isinstance(value, str) and expression in value for value in env.values())


def producer_contract_errors(producer: Producer, document: object) -> list[str]:
    errors: list[str] = []
    attest_steps = structural_attest_steps(document)
    if len(attest_steps) != 1:
        errors.append(
            f"{producer.path}: want exactly one direct actions/attest step, "
            f"found {len(attest_steps)}"
        )
        return errors
    job_id, attest_index, step = attest_steps[0]
    jobs = dict(iter_jobs(document))
    job = jobs.get(producer.job_id)
    if job_id != producer.job_id or job is None:
        errors.append(
            f"{producer.path}: attest step is in job {job_id!r}, "
            f"want {producer.job_id!r}"
        )
        return errors
    if not is_reachable(job):
        errors.append(f"{producer.path}: job {producer.job_id!r} is not reachable")
    if not is_reachable(step):
        errors.append(
            f"{producer.path}: attest step {producer.step_id!r} is not reachable"
        )
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
    same_job_steps = job_steps(job)
    earlier = same_job_steps[:attest_index]
    if not any(producer.subject_producer in step_run_text(item) for item in earlier):
        errors.append(
            f"{producer.path}: subject {producer.subject_checksums} is not produced "
            f"earlier in job {producer.job_id!r} by {producer.subject_producer!r}"
        )
    later = same_job_steps[attest_index + 1 :]
    expression = "${{ " + producer.bundle_output + " }}"
    found_consumer = False
    for consumer in later:
        if not is_reachable(consumer):
            continue
        if producer.consumer_if is not None and consumer.get("if") != producer.consumer_if:
            continue
        if producer.consumer_env is not None:
            env = consumer.get("env")
            key, value = producer.consumer_env
            if not isinstance(env, dict) or env.get(key) != value:
                continue
        if not step_has_bundle_expression(consumer, expression):
            continue
        if not step_run_text(consumer):
            continue
        found_consumer = True
        break
    if not found_consumer:
        errors.append(
            f"{producer.path}: missing reachable same-job consumer "
            f"{producer.bundle_output}"
        )
    return errors


def check() -> list[str]:
    errors: list[str] = []
    expected = (EXPECTED_SHA, EXPECTED_TAG)
    expected_paths = {producer.path for producer in PRODUCERS}

    for pattern in ("*.yml", "*.yaml"):
        for workflow in sorted(WORKFLOW_DIR.glob(pattern)):
            if workflow in expected_paths:
                continue
            try:
                document = load_workflow_mapping(workflow)
            except WorkflowParseError:
                continue
            if structural_attest_steps(document):
                errors.append(f"unexpected actions/attest workflow callsite: {workflow}")

    for producer in PRODUCERS:
        workflow = producer.path
        if not workflow.is_file():
            errors.append(f"workflow missing: {workflow}")
            continue
        text = workflow.read_text(encoding="utf-8")
        pins = active_attest_pins(text)
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
        errors.extend(producer_contract_errors(producer, document))
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
