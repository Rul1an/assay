#!/usr/bin/env python3
"""Fail-closed structural contract for the published-release golden path."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]


def require(text: str, needle: str, message: str, problems: list[str]) -> None:
    if needle not in text:
        problems.append(message)


def validate_manifest(path: Path, source_root: Path, problems: list[str]) -> None:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"harness manifest is unreadable: {error}")
        return
    if manifest.get("schema") != "assay.published_release_golden_path.harness.v1":
        problems.append("harness manifest schema drifted")
    files = manifest.get("files")
    if not isinstance(files, list) or not files:
        problems.append("harness manifest has no files")
        return
    paths: list[str] = []
    for row in files:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            problems.append("harness manifest row has no path")
            continue
        relative = PurePosixPath(row["path"])
        paths.append(str(relative))
        if relative.is_absolute() or "." in relative.parts or ".." in relative.parts:
            problems.append(f"unsafe harness manifest path: {relative}")
            continue
        source = source_root.joinpath(*relative.parts)
        try:
            actual = hashlib.sha256(source.read_bytes()).hexdigest()
        except OSError as error:
            problems.append(f"harness file is unreadable: {relative}: {error}")
            continue
        if row.get("sha256") != actual:
            problems.append(f"harness digest drifted: {relative}")
    expected = [
        "examples/privileged-action-gate/mock_github_mcp.py",
        "examples/privileged-action-gate/policies/no-allowance.yaml",
        "examples/privileged-action-gate/baseline-approved.json",
    ]
    if paths != expected:
        problems.append("harness manifest must list exactly the bounded denied-call fixture")


def validate_contract(
    workflow: Path,
    release_workflow: Path,
    driver: Path,
    manifest: Path,
    source_root: Path,
) -> list[str]:
    problems: list[str] = []
    try:
        workflow_text = workflow.read_text(encoding="utf-8")
        release_text = release_workflow.read_text(encoding="utf-8")
        driver_text = driver.read_text(encoding="utf-8")
    except OSError as error:
        return [f"contract input is missing: {error}"]

    require(workflow_text, "workflow_call:", "workflow must be reusable from release.yml", problems)
    require(workflow_text, "workflow_dispatch:", "workflow must support an explicit replay", problems)
    require(workflow_text, "release_tag:", "workflow must require an exact release tag input", problems)
    require(workflow_text, "timeout-minutes: 20", "live job must have a bounded timeout", problems)
    require(
        workflow_text,
        "bash scripts/ci/published-release-golden-path.sh",
        "workflow must execute the reviewed driver",
        problems,
    )
    require(workflow_text, "--harness-sha \"$GITHUB_SHA\"", "workflow must bind the harness head", problems)
    require(workflow_text, "if: always()", "workflow must retain partial failure evidence", problems)
    require(workflow_text, "retention-days: 30", "workflow must pin artifact retention", problems)
    require(workflow_text, "if-no-files-found: error", "missing replay evidence must fail closed", problems)
    require(
        workflow_text,
        "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
        "checkout action must be SHA-pinned",
        problems,
    )
    require(
        workflow_text,
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "artifact upload action must be SHA-pinned",
        problems,
    )
    if "continue-on-error:" in workflow_text:
        problems.append("published-release journey must not continue on error")
    if "releases/latest" in workflow_text.lower() or "release_tag: latest" in workflow_text.lower():
        problems.append("workflow must not resolve a moving latest release")

    require(
        release_text,
        "uses: ./.github/workflows/published-release-golden-path.yml",
        "release transaction must call the published-release gate",
        problems,
    )
    require(
        release_text,
        "release_tag: ${{ needs.release-contract.outputs.version }}",
        "release transaction must pass its validated version",
        problems,
    )
    require(
        release_text,
        "needs: [release-contract, release]",
        "published-release gate must wait until release publication completes",
        problems,
    )

    required_driver_fragments = {
        "exact stable tag": "release tag must be an exact stable vX.Y.Z tag",
        "fresh run root": "run root already exists; refusing to reuse prior evidence",
        "proof-kit API digest": "proof-kit digest differs from the release API",
        "canonical attestation verifier": (
            '"$kit_root/verify-offline.sh" --assets-dir "$downloads" '
            '>"$results/attestation-verify.log" 2>&1'
        ),
        "disposable HOME": 'export HOME="$run_root/home"',
        "restricted PATH": 'export PATH="$install_root/bin:/usr/bin:/bin"',
        "installed CLI resolution": '"$(command -v assay)" == "$install_root/bin/assay"',
        "installed MCP resolution": '"$(command -v assay-mcp-server)" == "$install_root/bin/assay-mcp-server"',
        "harness head": '"head_sha": harness_sha',
        "separate release provenance": '"release": {',
        "separate harness provenance": '"harness": {',
        "produced bundle": 'bundle="$results/produced.bundle.tar.gz"',
        "inspect produced bundle": 'assay evidence show --format json -- "$bundle"',
        "verify produced bundle": 'assay evidence verify-privileged-mcp-action "$bundle" --format json',
        "tamper failure code": 'reason_code == "E_EVIDENCE_INTEGRITY"',
        "artifact manifest": '"assay.published_release_golden_path.artifacts.v1"',
    }
    for label, fragment in required_driver_fragments.items():
        require(driver_text, fragment, f"driver lost {label}", problems)
    if "|| true" in driver_text or "set +e" in driver_text:
        problems.append("driver suppresses a failure instead of recording its exact status")
    verifier = driver_text.find(
        '"$kit_root/verify-offline.sh" --assets-dir "$downloads" '
        '>"$results/attestation-verify.log" 2>&1'
    )
    first_product_call = driver_text.find('run_capture "assay-version"')
    if verifier < 0 or first_product_call < 0 or verifier > first_product_call:
        problems.append("release attestations must verify before the first Assay invocation")
    required_artifacts = [
        "run-pin.json",
        "commands.ndjson",
        "produced.bundle.tar.gz",
        "decisions.ndjson",
        "inspect.json",
        "verify.json",
        "tamper-verify.json",
        "enforcement.sarif",
    ]
    for name in required_artifacts:
        if f'"{name}"' not in driver_text:
            problems.append(f"driver no longer requires retained artifact: {name}")

    validate_manifest(manifest, source_root, problems)
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workflow", type=Path, default=ROOT / ".github/workflows/published-release-golden-path.yml")
    parser.add_argument("--release-workflow", type=Path, default=ROOT / ".github/workflows/release.yml")
    parser.add_argument("--driver", type=Path, default=ROOT / "scripts/ci/published-release-golden-path.sh")
    parser.add_argument(
        "--manifest",
        type=Path,
        default=ROOT / "scripts/ci/fixtures/published-release-golden-path/v1/harness-manifest.json",
    )
    parser.add_argument("--source-root", type=Path, default=ROOT)
    args = parser.parse_args()
    problems = validate_contract(
        args.workflow, args.release_workflow, args.driver, args.manifest, args.source_root
    )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("ok: published-release golden-path contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
