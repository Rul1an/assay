#!/usr/bin/env python3
"""Pin Assay's two direct actions/attest producers in lockstep."""

from __future__ import annotations

import json
import re
import shlex
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
DIRECT_ATTEST_USES_RE = re.compile(r"^actions/attest@[^\s]+$", re.IGNORECASE)
EXPRESSION_WRAPPER_RE = re.compile(r"^\$\{\{\s*(?P<body>.*?)\s*\}\}$")
NONEMPTY_OUTPUT_TERM_RE = re.compile(
    r"^(?:"
    r"steps\.[A-Za-z0-9_-]+\.outputs\.[A-Za-z0-9_-]+|"
    r"hashFiles\('[^'\r\n]+'\)"
    r")\s*!=\s*''$"
)


class Producer(NamedTuple):
    path: Path
    job_id: str
    step_id: str
    subject_checksums: str
    producer_step_id: str
    producer_working_directory: str | None
    producer_commands: tuple[str, ...]
    bundle_output: str
    consumer_step_id: str
    consumer_if: str | None
    consumer_env: tuple[str, str] | None
    consumer_commands: tuple[str, ...]


class WorkflowParseError(Exception):
    """A producer workflow could not be loaded, so the contract cannot pass."""


PRODUCERS = (
    Producer(
        path=Path(".github/workflows/runner-spike-delegated.yml"),
        job_id="phase1-delegated-gates",
        step_id="attest-proof-pack",
        subject_checksums="assay-runner-proof-upload/subject-checksums.txt",
        producer_step_id="build-proof-pack",
        producer_working_directory=None,
        producer_commands=(
            "set -euo pipefail",
            (
                "python3 scripts/ci/assay_runner_delegated_proof_pack.py "
                "--proof-root $ASSAY_RUNNER_DELEGATED_PROOF_ROOT "
                "--gates '${{ inputs.gates }}' "
                "--build-ebpf '${{ inputs.build_ebpf }}' "
                "--run-id $GITHUB_RUN_ID "
                "--run-attempt $GITHUB_RUN_ATTEMPT "
                "--run-url "
                "'${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/"
                "${GITHUB_RUN_ID}' "
                "--ref $GITHUB_REF "
                "--head-sha $GITHUB_SHA "
                "--workflow-sha '${GITHUB_WORKFLOW_SHA:-$GITHUB_SHA}' "
                "--workflow-name '$GITHUB_WORKFLOW' "
                "--workflow-path .github/workflows/runner-spike-delegated.yml "
                "--repository $GITHUB_REPOSITORY "
                "--retention-days 365"
            ),
        ),
        bundle_output="steps.attest-proof-pack.outputs.bundle-path",
        consumer_step_id="retain-proof-attestation",
        consumer_if="always() && steps.attest-proof-pack.outputs.bundle-path != ''",
        consumer_env=None,
        consumer_commands=(
            "set -euo pipefail",
            (
                "cp '${{ steps.attest-proof-pack.outputs.bundle-path }}' "
                "'$ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD/attestation-bundle.json'"
            ),
        ),
    ),
    Producer(
        path=Path(".github/workflows/privileged-mcp-action-pack-release.yml"),
        job_id="release-pack",
        step_id="attest",
        subject_checksums="release/SHA256SUMS",
        producer_step_id="build-release-checksums",
        producer_working_directory="release",
        producer_commands=(
            "set -euo pipefail",
            "sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS",
        ),
        bundle_output="steps.attest.outputs.bundle-path",
        consumer_step_id="retain-release-attestation",
        consumer_if=None,
        consumer_env=(
            "ATTESTATION_BUNDLE",
            "${{ steps.attest.outputs.bundle-path }}",
        ),
        consumer_commands=(
            "set -euo pipefail",
            "test -n '$ATTESTATION_BUNDLE'",
            "cp '$ATTESTATION_BUNDLE' release/attestation-bundle.json",
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
                (
                    "document = YAML.safe_load(File.read(ARGV[0]), "
                    "permitted_classes: [], permitted_symbols: [], aliases: true); "
                    "puts JSON.generate(document)"
                ),
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
    return (
        isinstance(uses, str)
        and DIRECT_ATTEST_USES_RE.fullmatch(uses.strip()) is not None
    )


def structural_attest_steps(
    document: object,
) -> list[tuple[str, int, dict[str, object]]]:
    found: list[tuple[str, int, dict[str, object]]] = []
    for job_id, job in iter_jobs(document):
        for index, step in enumerate(job_steps(job)):
            if is_direct_attest_uses(step.get("uses")):
                found.append((job_id, index, step))
    return found


def condition_may_run(value: object) -> bool:
    """Recognize only the small condition grammar used by these producers.

    Missing conditions, true, always(), and conjunctions with a non-empty
    step output or hashFiles result may run. False terms and every unsupported
    expression fail closed rather than being mistaken for reachable workflow.
    """
    if value is None or value is True:
        return True
    if value is False or not isinstance(value, str):
        return False
    expression = value.strip()
    wrapper = EXPRESSION_WRAPPER_RE.fullmatch(expression)
    if wrapper is not None:
        expression = wrapper.group("body").strip()
    terms = [term.strip() for term in expression.split("&&")]
    if not terms or any(not term for term in terms):
        return False
    for term in terms:
        folded = term.casefold()
        if folded == "false":
            return False
        if folded in {"true", "always()"}:
            continue
        if NONEMPTY_OUTPUT_TERM_RE.fullmatch(term) is not None:
            continue
        return False
    return True


def is_reachable(node: dict[str, object]) -> bool:
    return condition_may_run(node.get("if"))


def step_run_text(step: dict[str, object]) -> str:
    run = step.get("run")
    return run if isinstance(run, str) else ""


def shell_command_tokens(run: str) -> list[list[str]]:
    """Tokenize simple logical shell lines; this is not a shell evaluator."""
    commands: list[list[str]] = []
    logical_line = ""
    for raw_line in run.splitlines():
        stripped = raw_line.strip()
        if not logical_line and (not stripped or stripped.startswith("#")):
            continue
        continued = stripped.endswith("\\")
        fragment = stripped[:-1].rstrip() if continued else stripped
        logical_line = f"{logical_line} {fragment}".strip()
        if continued:
            continue
        try:
            tokens = shlex.split(logical_line, comments=True, posix=True)
        except ValueError:
            tokens = []
        if tokens:
            commands.append(tokens)
        logical_line = ""
    if logical_line:
        try:
            tokens = shlex.split(logical_line, comments=True, posix=True)
        except ValueError:
            tokens = []
        if tokens:
            commands.append(tokens)
    return commands


def step_runs_exact_commands(
    step: dict[str, object], expected: tuple[str, ...]
) -> bool:
    expected_tokens = [
        shlex.split(command, comments=False, posix=True) for command in expected
    ]
    return shell_command_tokens(step_run_text(step)) == expected_tokens


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
    producer_steps = [
        (index, item)
        for index, item in enumerate(same_job_steps)
        if item.get("id") == producer.producer_step_id
    ]
    if len(producer_steps) != 1:
        errors.append(
            f"{producer.path}: want exactly one producer step "
            f"{producer.producer_step_id!r}, found {len(producer_steps)}"
        )
    else:
        producer_index, producer_step = producer_steps[0]
        if producer_index >= attest_index:
            errors.append(
                f"{producer.path}: producer step {producer.producer_step_id!r} "
                "does not precede attestation"
            )
        if not is_reachable(producer_step):
            errors.append(
                f"{producer.path}: producer step {producer.producer_step_id!r} "
                "is not reachable"
            )
        if (
            producer_step.get("working-directory")
            != producer.producer_working_directory
        ):
            errors.append(
                f"{producer.path}: producer working-directory "
                f"{producer_step.get('working-directory')!r}, "
                f"want {producer.producer_working_directory!r}"
            )
        if not step_runs_exact_commands(producer_step, producer.producer_commands):
            errors.append(
                f"{producer.path}: producer step {producer.producer_step_id!r} "
                "does not run the exact command sequence"
            )

    consumer_steps = [
        (index, item)
        for index, item in enumerate(same_job_steps)
        if item.get("id") == producer.consumer_step_id
    ]
    if len(consumer_steps) != 1:
        errors.append(
            f"{producer.path}: want exactly one consumer step "
            f"{producer.consumer_step_id!r}, found {len(consumer_steps)}"
        )
    else:
        consumer_index, consumer = consumer_steps[0]
        if consumer_index <= attest_index:
            errors.append(
                f"{producer.path}: consumer step {producer.consumer_step_id!r} "
                "does not follow attestation"
            )
        if not is_reachable(consumer):
            errors.append(
                f"{producer.path}: consumer step {producer.consumer_step_id!r} "
                "is not reachable"
            )
        if consumer.get("if") != producer.consumer_if:
            errors.append(
                f"{producer.path}: consumer condition {consumer.get('if')!r}, "
                f"want {producer.consumer_if!r}"
            )
        if producer.consumer_env is not None:
            env = consumer.get("env")
            key, value = producer.consumer_env
            if not isinstance(env, dict) or env.get(key) != value:
                errors.append(
                    f"{producer.path}: consumer env {key!r} is not bound to {value!r}"
                )
        if not step_runs_exact_commands(consumer, producer.consumer_commands):
            errors.append(
                f"{producer.path}: consumer step {producer.consumer_step_id!r} "
                f"does not preserve {producer.bundle_output} with the exact "
                "command sequence"
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
            except WorkflowParseError as error:
                errors.append(str(error))
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
