#!/usr/bin/env python3
"""Fail-closed structural contract for the published-release golden path."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
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


def lines_between(text: str, start_marker: str, end_marker: str, problems: list[str]) -> list[str]:
    if text.count(start_marker) != 1 or text.count(end_marker) != 1:
        problems.append(f"expected unique contract block markers: {start_marker!r}, {end_marker!r}")
        return []
    start = text.index(start_marker)
    end = text.index(end_marker, start)
    return active_lines(text[start:end])


def shell_control_depth_before(text: str, marker: str, problems: list[str]) -> int:
    """Return lexical shell control depth before a unique executable marker."""
    if text.count(marker) != 1:
        problems.append(f"expected unique shell control marker: {marker!r}")
        return -1

    target_line = text[: text.index(marker)].count("\n")
    stack: list[str] = []
    heredoc_end: str | None = None
    for raw_line in text.splitlines()[:target_line]:
        stripped = raw_line.strip()
        if heredoc_end is not None:
            if stripped == heredoc_end:
                heredoc_end = None
            continue
        if not stripped or stripped.startswith("#"):
            continue

        if re.match(r"^(fi|done|esac|\})(?:\s|$)", stripped):
            if stack:
                stack.pop()
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*\(\)\s*\{$", stripped):
            stack.append("function")
        elif re.match(r"^(if|for|while|until|case)\b", stripped):
            stack.append(stripped.split(maxsplit=1)[0])
        elif re.search(r"(?:^|\|\||&&)\s*\{\s*$", stripped):
            stack.append("group")

        heredoc = re.search(r"<<-?\s*['\"]?([A-Za-z_][A-Za-z0-9_]*)['\"]?", stripped)
        if heredoc:
            heredoc_end = heredoc.group(1)

    return len(stack)


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
    executable_paths = [
        row.get("path")
        for row in files
        if isinstance(row, dict) and row.get("executable") is True
    ]
    if executable_paths != ["scripts/ci/release_archive_inventory.sh"]:
        problems.append("harness executable surface drifted")


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
        attestation_text = (source_root / "scripts/ci/release_attestation_enforce.sh").read_text(
            encoding="utf-8"
        )
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
    expected_exercise_step = [
        "- name: Exercise the attested published release",
        "shell: bash",
        "env:",
        "GH_TOKEN: ${{ github.token }}",
        "RELEASE_TAG: ${{ inputs.release_tag }}",
        "RUN_ROOT: ${{ runner.temp }}/assay-published-release-golden-path",
        "run: |",
        "set -euo pipefail",
        "bash scripts/ci/published-release-golden-path.sh \\",
        '--release-tag "$RELEASE_TAG" \\',
        '--harness-sha "$GITHUB_SHA" \\',
        '--workflow-run-id "$GITHUB_RUN_ID" \\',
        '--workflow-run-attempt "$GITHUB_RUN_ATTEMPT" \\',
        '--run-root "$RUN_ROOT"',
    ]
    if named_step_lines(workflow_text, "Exercise the attested published release", problems) != expected_exercise_step:
        problems.append("workflow must execute only the exact reviewed driver invocation")
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
    expected_attestation_block = [
        'signer_workflow="$REPO/.github/workflows/release.yml"',
        'if ! GH_BIN="$GH_BIN" JQ_BIN="$JQ_BIN" \\',
        'ASSETS_DIR="$downloads" \\',
        'OUT_SUMMARY="$results/attestation-summary.json" \\',
        'OUT_RAW_DIR="$results/attestation-raw" \\',
        'REPO="$REPO" \\',
        'SIGNER_WORKFLOW="$signer_workflow" \\',
        'SOURCE_REF="" \\',
        'SOURCE_DIGEST="$source_digest" \\',
        'bash "$harness_root/scripts/ci/release_attestation_enforce.sh" \\',
        '>"$results/attestation-verify.log" 2>&1; then',
        'cat "$results/attestation-verify.log" >&2',
        'fail "reviewed release attestation verifier rejected the published assets"',
        "fi",
        'record_command "verify-release-attestations" 0 "$harness_root/scripts/ci/release_attestation_enforce.sh"',
    ]
    attestation_block = lines_between(
        driver_text,
        "# Execute reviewed harness code, not a script carried inside a mutable release asset.",
        'cli_extract="$run_root/cli-extract"',
        problems,
    )
    if attestation_block != expected_attestation_block:
        problems.append("driver attestation execution block drifted")
    expected_version_block = [
        'run_capture "mcp-version" 0 "$results/mcp-version.txt" "$results/mcp-version.stderr" assay-mcp-server --version',
        '[[ "$(tr -d \'\\r\\n\' <"$results/mcp-version.txt")" == "assay-mcp-server $version" ]] \\',
        '|| fail "assay-mcp-server version differs from pinned release"',
    ]
    version_block = lines_between(
        driver_text,
        'run_capture "mcp-version"',
        'pushd "$session_root"',
        problems,
    )
    if version_block != expected_version_block:
        problems.append("exact MCP version execution block drifted")
    expected_proxy_block = [
        "proxy_status=0",
        "proxy_argv=(",
        "assay-mcp-server proxy-enforce",
        '--upstream-command "$PYTHON_BIN" --upstream-arg -u --upstream-arg "$fixture_dir/mock_github_mcp.py"',
        '--enforce-policy "$fixture_dir/policies/no-allowance.yaml"',
        '--declared-mcp-manifest "$fixture_dir/baseline-approved.json"',
        '--enforcement-decision-out "$decisions"',
        '--denied-call-observation-out "$observations"',
        ")",
        'printf \'%s\\n%s\\n\' "$init_request" "$call_request" \\',
        '| "${proxy_argv[@]}" \\',
        '>"$results/proxy.jsonl" 2>"$results/proxy.stderr" || proxy_status=$?',
        'record_command "proxy-enforce" "$proxy_status" "${proxy_argv[@]}"',
    ]
    proxy_block = lines_between(
        driver_text,
        "proxy_status=0",
        '[[ "$proxy_status"',
        problems,
    )
    if proxy_block != expected_proxy_block:
        problems.append("proxy execution and provenance block drifted")
    if shell_control_depth_before(driver_text, "proxy_status=0", problems) != 0:
        problems.append("proxy execution block must run at top level")
    proxy_tokens = ("proxy-enforce", "proxy_argv", "proxy_status")
    expected_proxy_surface = [
        line
        for line in expected_proxy_block
        + [
            '[[ "$proxy_status" -eq 0 ]] || fail "proxy-enforce exited $proxy_status, expected 0 for a policy denial"',
            '[[ -s "$decisions" ]] || fail "proxy-enforce produced no enforcement decision"',
            '[[ -s "$observations" ]] || fail "proxy-enforce produced no denied-call observation"',
        ]
        if any(token in line for token in proxy_tokens)
    ]
    proxy_surface = [
        line
        for line in driver_lines
        if any(token in line for token in proxy_tokens)
    ]
    if proxy_surface != expected_proxy_surface:
        problems.append("driver has an alternate proxy execution or provenance path")
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
    semantic_driver_lines = {
        'for pattern, expected in (("release-assets/*.tar.gz", 2), ("attestation-raw/*.json", 2)):': (
            "retained trust-input count enforcement drifted"
        ),
        '[[ "$asset_url" == "https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}" ]] \\': (
            "release asset URL binding drifted"
        ),
    }
    for line, message in semantic_driver_lines.items():
        if driver_lines.count(line) != 1:
            problems.append(message)
    expected_verify_args = [
        "verify_args=(",
        'attestation verify "$asset"',
        '--repo "$REPO"',
        '--signer-workflow "$SIGNER_WORKFLOW"',
        '--cert-oidc-issuer "$CERT_OIDC_ISSUER"',
        '--predicate-type "$PREDICATE_TYPE"',
        '--source-digest "$SOURCE_DIGEST"',
        "--deny-self-hosted-runners",
        "--format json",
        ")",
    ]
    verify_args = lines_between(
        attestation_text,
        "  verify_args=(",
        '  if [ -n "$SOURCE_REF" ]',
        problems,
    )
    if verify_args != expected_verify_args:
        problems.append("attestation verification argv binding drifted")
    expected_subject_digest_block = [
        'if ! printf \'%s\\n\' "$verify_json" | "$JQ_BIN" -e --arg digest "$asset_sha256" \'',
        'any(.[]; any((.verificationResult.statement.subject // [])[]?; .digest.sha256? == $digest))',
        "' >/dev/null; then",
        'echo "Verified attestation for ${asset_name} does not match the local subject digest" >&2',
        "exit 1",
        "fi",
    ]
    subject_digest_block = lines_between(
        attestation_text,
        '  if ! printf \'%s\\n\' "$verify_json" | "$JQ_BIN" -e --arg digest "$asset_sha256"',
        '  printf \'%s\\n\' "$verify_json" | "$JQ_BIN" -c',
        problems,
    )
    if subject_digest_block != expected_subject_digest_block:
        problems.append("attestation local-subject digest execution block drifted")
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
