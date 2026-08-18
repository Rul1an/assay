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


def active_lines(text: str) -> list[str]:
    """Return executable/config lines, excluding blank and comment-only lines."""
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


def validate_manifest(
    path: Path,
    source_root: Path,
    workflow: Path,
    release_workflow: Path,
    driver: Path,
    problems: list[str],
) -> None:
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
    input_overrides = {
        ".github/workflows/published-release-golden-path.yml": workflow,
        ".github/workflows/release.yml": release_workflow,
        "scripts/ci/published-release-golden-path.sh": driver,
    }
    for row in files:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            problems.append("harness manifest row has no path")
            continue
        relative = PurePosixPath(row["path"])
        paths.append(str(relative))
        if relative.is_absolute() or "." in relative.parts or ".." in relative.parts:
            problems.append(f"unsafe harness manifest path: {relative}")
            continue
        source = input_overrides.get(str(relative), source_root.joinpath(*relative.parts))
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
        ".github/workflows/published-release-golden-path.yml",
        ".github/workflows/release.yml",
        "scripts/ci/published-release-golden-path.sh",
        "scripts/ci/release_attestation_enforce.sh",
        "scripts/ci/release_archive_inventory.sh",
        "scripts/ci/safe_extract_release_archive.py",
        "scripts/ci/bounded_download.py",
    ]
    if paths != expected:
        problems.append("harness manifest must list exactly the reviewed harness inputs")


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
    require(workflow_text, "--workflow-run-id \"$GITHUB_RUN_ID\"", "workflow must bind its run id", problems)
    require(
        workflow_text,
        "--workflow-run-attempt \"$GITHUB_RUN_ATTEMPT\"",
        "workflow must bind its run attempt",
        problems,
    )
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
        "release transaction must call published-release verification",
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
        "published-release verification must wait until release publication completes",
        problems,
    )
    caller = mapping_block(release_text, "published-release-golden-path", 2, problems)
    caller_lines = active_lines(caller)
    reusable_call = "uses: ./.github/workflows/published-release-golden-path.yml"
    if active_lines(release_text).count(reusable_call) != 1:
        problems.append("release transaction must contain exactly one published-release workflow caller")
    if reusable_call not in caller_lines:
        problems.append("published-release job must be the reusable workflow caller")
    if caller_lines.count("needs: [release-contract, release]") != 1:
        problems.append("published-release job must uniquely wait for release publication")
    if any(line.startswith("continue-on-error:") for line in caller_lines):
        problems.append("release caller must not ignore failed published-release verification")
    if caller_lines.count("if: >-") != 1:
        problems.append("published-release job must have exactly one stable-release condition")
    for condition in (
        "startsWith(github.ref, 'refs/tags/v')",
        "github.event_name == 'workflow_dispatch'",
        "!contains(github.ref, '-rc')",
        "!contains(github.ref, '-beta')",
        "!contains(github.event.inputs.version, '-rc')",
        "!contains(github.event.inputs.version, '-beta')",
    ):
        if condition not in caller:
            problems.append(f"published-release caller condition lost: {condition}")
    if caller_lines.count("release_tag: ${{ needs.release-contract.outputs.version }}") != 1:
        problems.append("published-release caller must pass the validated version exactly once")
    if caller_lines.count("name: Verify the published release journey") != 1:
        problems.append("release transaction must describe this as post-publication verification")

    driver_lines = active_lines(driver_text)
    required_driver_fragments = {
        "exact stable tag": "release tag must be an exact stable vX.Y.Z tag",
        "fresh run root": "run root already exists; refusing to reuse prior evidence",
        "stable published release": "release tag is still draft or prerelease",
        "external tag source": '"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}"',
        "reviewed attestation verifier": 'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"',
        "compressed asset ceiling": "release asset exceeds compressed-size ceiling",
        "bounded archive extractor": "from safe_extract_release_archive import extract_archive",
        "retained release inputs": 'downloads="$results/release-assets"',
        "disposable HOME": 'export HOME="$run_root/home"',
        "restricted PATH": 'export PATH="$install_root/bin:/usr/bin:/bin"',
        "installed CLI resolution": '"$(command -v assay)" == "$install_root/bin/assay"',
        "installed MCP resolution": '"$(command -v assay-mcp-server)" == "$install_root/bin/assay-mcp-server"',
        "harness head": '"head_sha": harness_sha',
        "workflow run binding": '"workflow_run_id": workflow_run_id',
        "separate release provenance": '"release": {',
        "separate harness provenance": '"harness": {',
        "produced bundle": 'bundle="$results/produced.bundle.tar.gz"',
        "inspect produced bundle": 'assay evidence show --format json -- "$bundle"',
        "verify produced bundle": 'assay evidence verify-privileged-mcp-action "$bundle" --format json',
        "tamper failure code": 'reason_code == "E_EVIDENCE_INTEGRITY"',
        "artifact manifest": '"assay.published_release_golden_path.artifacts.v1"',
        "claim ceiling": "the harness is not a shipped release asset",
    }
    for label, fragment in required_driver_fragments.items():
        require(driver_text, fragment, f"driver lost {label}", problems)
    if "|| true" in driver_text or "set +e" in driver_text:
        problems.append("driver suppresses a failure instead of recording its exact status")
    exact_active_lines = {
        '[[ "$release_tag" =~ ^v[0-9]+\\.[0-9]+\\.[0-9]+$ ]] || fail "release tag must be an exact stable vX.Y.Z tag"': (
            "stable release-tag validation drifted"
        ),
        '"$JQ_BIN" -e \'.draft == false and .prerelease == false\' "$release_api" >/dev/null \\': (
            "published release-state validation drifted"
        ),
        '[[ "$actual_digest" == "$api_digest" ]] || fail "downloaded asset digest differs: $asset_name"': (
            "downloaded asset digest comparison drifted"
        ),
        '"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}" >"$tag_ref"': (
            "external release-tag source binding drifted"
        ),
        'SOURCE_DIGEST="$source_digest" \\': "attestation source digest is not externally bound",
        '[[ "$source_type" == "commit" && "$source_digest" =~ ^[0-9a-f]{40}$ ]] \\': (
            "release tag must peel to a commit before attestation verification"
        ),
        'OUT_RAW_DIR="$results/attestation-raw" \\': "raw attestation inputs must be retained",
        'downloads="$results/release-assets"': "release inputs must be retained with the run artifact",
        'download_release_asset "$cli_asset" 67108864': "CLI compressed-size ceiling drifted",
        'download_release_asset "$mcp_asset" 33554432': "MCP compressed-size ceiling drifted",
        'safe_extract "$downloads/$cli_asset" "$cli_extract" 134217728': "CLI safe-extraction ceiling drifted",
        'safe_extract "$downloads/$mcp_asset" "$mcp_extract" 67108864': "MCP safe-extraction ceiling drifted",
        '--bundle-out "$bundle" --run-id "published-release-${workflow_run_id}-${workflow_run_attempt}" \\': (
            "evidence run id is not bound to the workflow invocation"
        ),
    }
    for line, message in exact_active_lines.items():
        if driver_lines.count(line) != 1:
            problems.append(message)
    if driver_lines.count('PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \\') != 2:
        problems.append("bounded helper execution drifted")
    if any("verify-offline.sh" in line or "release-proof-kit" in line for line in driver_lines):
        problems.append("driver must not execute or trust code carried by the release proof kit")
    verifier_line = 'bash "$harness_root/scripts/ci/release_attestation_enforce.sh" \\'
    if driver_lines.count(verifier_line) != 1:
        problems.append("driver must execute the reviewed attestation verifier exactly once")
    else:
        verifier = driver_lines.index(verifier_line)
        product_boundaries = [
            'safe_extract "$downloads/$cli_asset" "$cli_extract" 134217728',
            'safe_extract "$downloads/$mcp_asset" "$mcp_extract" 67108864',
            'cp "${cli_candidates[0]}" "$install_root/bin/assay"',
            'cp "${mcp_candidates[0]}" "$install_root/bin/assay-mcp-server"',
            'run_capture "assay-version" 0 "$results/assay-version.txt" "$results/assay-version.stderr" assay version',
        ]
        for boundary in product_boundaries:
            if boundary not in driver_lines or verifier > driver_lines.index(boundary):
                problems.append(f"release attestations must precede product use: {boundary}")
    exact_assignments = [
        'cli_asset="assay-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"',
        'mcp_asset="assay-mcp-server-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"',
    ]
    for assignment in exact_assignments:
        if driver_lines.count(assignment) != 1:
            problems.append(f"Linux x86_64 product asset assignment drifted: {assignment}")
    inspect_command = 'assay evidence show --format json -- "$bundle"'
    if driver_lines.count(inspect_command) != 1:
        problems.append("driver must inspect the same bundle it produced exactly once")
    if sum("assay evidence show" in line for line in driver_lines) != 1:
        problems.append("driver contains an alternate evidence-inspection target")
    required_artifacts = [
        "run-pin.json",
        "commands.ndjson",
        "produced.bundle.tar.gz",
        "decisions.ndjson",
        "inspect.json",
        "verify.json",
        "tamper-verify.json",
        "enforcement.sarif",
        "release-api.json",
        "tag-ref.json",
        "attestation-summary.json",
    ]
    for name in required_artifacts:
        if f'"{name}"' not in driver_text:
            problems.append(f"driver no longer requires retained artifact: {name}")

    validate_manifest(manifest, source_root, workflow, release_workflow, driver, problems)
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
