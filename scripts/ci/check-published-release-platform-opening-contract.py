#!/usr/bin/env python3
"""Fail-closed contract for the published-archive Windows/macOS opening.

Linux full journeys share one matrixed driver invocation (x86_64 + arm64).
This checker requires both Linux matrix rows to call that driver with --target
and keeps opening legs downloading their own published archive by tag.
"""

from __future__ import annotations

import argparse
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


EXPECTED_OPENING_STEP = [
    "- name: Exercise the published CLI opening",
    "shell: bash",
    "env:",
    "GH_TOKEN: ${{ github.token }}",
    "RELEASE_TAG: ${{ inputs.release_tag }}",
    "RELEASE_TARGET: ${{ matrix.target }}",
    "RUN_ROOT: ${{ runner.temp }}/assay-published-release-opening",
    "run: |",
    "set -euo pipefail",
    "bash scripts/ci/published-release-platform-opening.sh \\",
    '--release-tag "$RELEASE_TAG" \\',
    '--target "$RELEASE_TARGET" \\',
    '--run-root "$RUN_ROOT"',
]


def require(text: str, needle: str, message: str, problems: list[str]) -> None:
    if needle not in text:
        problems.append(message)


def active_lines(text: str) -> list[str]:
    return [line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]


def mapping_block(text: str, key: str, indent: int, problems: list[str]) -> str:
    marker = f"{' ' * indent}{key}:"
    lines = text.splitlines()
    starts = [index for index, line in enumerate(lines) if line == marker]
    if len(starts) != 1:
        problems.append(f"expected exactly one {key} mapping, found {len(starts)}")
        return ""
    start = starts[0]
    end = len(lines)
    for index in range(start + 1, len(lines)):
        line = lines[index]
        if line and not line.startswith(" "):
            end = index
            break
        if line.startswith(" " * indent) and not line.startswith(" " * (indent + 1)):
            end = index
            break
    return "\n".join(lines[start:end])


def named_step_lines(text: str, name: str, problems: list[str]) -> list[str]:
    marker = f"      - name: {name}"
    lines = text.splitlines()
    starts = [index for index, line in enumerate(lines) if line == marker]
    if len(starts) != 1:
        problems.append(f"expected exactly one workflow step named {name!r}")
        return []
    start = starts[0]
    end = len(lines)
    for index in range(start + 1, len(lines)):
        if lines[index].startswith("      - name:"):
            end = index
            break
    return active_lines("\n".join(lines[start:end]))


def validate_linux_journey_matrix(workflow_text: str, problems: list[str]) -> None:
    require(
        workflow_text,
        "published-linux-journey:",
        "workflow must define the shared published-linux-journey matrix job",
        problems,
    )
    require(
        workflow_text,
        "bash scripts/ci/published-release-golden-path.sh",
        "Linux journey matrix must execute the reviewed golden-path driver",
        problems,
    )
    require(
        workflow_text,
        '--target "$RELEASE_TARGET"',
        "Linux journey matrix must pass --target from the matrix",
        problems,
    )
    require(
        workflow_text,
        "x86_64-unknown-linux-gnu",
        "Linux journey matrix must include x86_64-unknown-linux-gnu",
        problems,
    )
    require(
        workflow_text,
        "aarch64-unknown-linux-gnu",
        "Linux journey matrix must include aarch64-unknown-linux-gnu",
        problems,
    )
    require(
        workflow_text,
        "ubuntu-24.04-arm",
        "Linux arm64 journey must use official ubuntu-24.04-arm runners",
        problems,
    )
    require(
        workflow_text,
        "published-release-golden-path-${{ matrix.target }}-${{ inputs.release_tag }}-${{ github.sha }}",
        "Linux journey artifacts must be named per matrix.target to avoid collision",
        problems,
    )
    if workflow_text.count("bash scripts/ci/published-release-golden-path.sh") != 1:
        problems.append("exactly one golden-path driver invocation must exist in the workflow")


def validate_opening_workflow(workflow_text: str, problems: list[str]) -> None:
    require(workflow_text, "workflow_call:", "workflow must stay reusable from release.yml", problems)
    require(workflow_text, "workflow_dispatch:", "workflow must stay dispatchable against a published tag", problems)
    require(workflow_text, "windows-latest", "opening job must run on windows-latest", problems)
    require(workflow_text, "macos-latest", "opening job must run on macos-latest", problems)
    require(
        workflow_text,
        "x86_64-pc-windows-msvc",
        "opening job must name the published Windows archive target",
        problems,
    )
    require(
        workflow_text,
        "aarch64-apple-darwin",
        "opening job must name the published macOS arm64 archive target",
        problems,
    )
    require(workflow_text, "timeout-minutes: 20", "opening job must have a bounded timeout", problems)
    require(
        workflow_text,
        "bash scripts/ci/published-release-platform-opening.sh",
        "opening job must execute the reviewed opening driver",
        problems,
    )
    if named_step_lines(workflow_text, "Exercise the published CLI opening", problems) != EXPECTED_OPENING_STEP:
        problems.append("opening job must execute only the exact reviewed opening-driver invocation")
    if "actions/download-artifact" in workflow_text:
        problems.append("published-release workflow must not consume a same-run build artifact")
    if "continue-on-error:" in workflow_text:
        problems.append("published-release journey must not continue on error")
    lowered = workflow_text.lower()
    if "releases/latest" in lowered or "release_tag: latest" in lowered:
        problems.append("workflow must not resolve a moving latest release")


def validate_opening_driver(driver_text: str, problems: list[str]) -> None:
    driver_lines = active_lines(driver_text)
    required = {
        "exact stable tag": 'release tag must be an exact stable vX.Y.Z tag',
        "release download URL": 'https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}',
        "checksum sidecar URL": 'https://github.com/${REPO}/releases/download/${release_tag}/${sidecar_name}',
        "checksum verify": "archive checksum mismatch",
        "attestation verify": "gh attestation verify",
        "signer workflow": "--signer-workflow",
        "source digest": "--source-digest",
        "deny self-hosted": "--deny-self-hosted-runners",
        "version from tag": 'expected_version="${release_tag#v}"',
        "assay version": "assay version",
        "version mismatch": "assay version mismatch",
        "doctor json": "doctor --format json",
        "json parse": "json.load",
        "init hello-trace": "init --preset dev --hello-trace",
        "init files": "eval.yaml",
        "hello trace": "traces/hello.jsonl",
        "windows zip": "x86_64-pc-windows-msvc",
        "macos tarball": "aarch64-apple-darwin",
        "unzip user path": "unzip -q",
        "tar user path": "tar xzkf",
    }
    for label, fragment in required.items():
        require(driver_text, fragment, f"opening driver lost {label}", problems)

    forbidden = {
        "actions/download-artifact": "opening driver must not consume a same-run build artifact",
        "cargo build": "opening driver must not run a tree-built binary",
        "workspace_version": "opening driver must compare version to the release tag, not Cargo.toml",
        "target/debug": "opening driver must not execute a workspace debug binary",
        "target/release": "opening driver must not execute a workspace release binary",
        "releases/latest": "opening driver must not resolve a moving latest release",
    }
    for fragment, message in forbidden.items():
        if fragment in driver_text:
            problems.append(message)

    if "|| true" in driver_text or "set +e" in driver_text:
        problems.append("opening driver suppresses a failure instead of recording its exact status")

    url_line = 'asset_url="https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}"'
    if driver_lines.count(url_line) != 1:
        problems.append("opening driver must bind the public release-by-tag download URL exactly once")


def validate_contract(workflow: Path, driver: Path) -> list[str]:
    problems: list[str] = []
    try:
        workflow_text = workflow.read_text(encoding="utf-8")
        driver_text = driver.read_text(encoding="utf-8")
    except OSError as error:
        return [f"contract input is missing: {error}"]
    validate_linux_journey_matrix(workflow_text, problems)
    validate_opening_workflow(workflow_text, problems)
    validate_opening_driver(driver_text, problems)
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workflow",
        type=Path,
        default=ROOT / ".github/workflows/published-release-golden-path.yml",
    )
    parser.add_argument(
        "--driver",
        type=Path,
        default=ROOT / "scripts/ci/published-release-platform-opening.sh",
    )
    args = parser.parse_args()
    problems = validate_contract(args.workflow, args.driver)
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("ok: published-release platform-opening contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
