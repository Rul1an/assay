#!/usr/bin/env python3
"""Fail-closed consumer for the historical-retention harness.

The driver records observations. This checker independently recomputes
exact-once classes, pairwise byte continuity, canary continuity, required
boundaries, and activation targets. A driver-written match field is not a verdict.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_REL = "scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json"
PRECOMMIT_REL = ".pre-commit-config.yaml"
HOOK_ID = "published-release-historical-retention-contract"
SCHEMA = "assay.published_release_historical_retention.harness.v1"
PIN_SCHEMA = "assay.published_release_historical_retention.run_pin.v1"
INITIAL_ACTIVATION_REF = "initial"
SAFE_RELEASE_TAG = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")
HEAD_SHA = re.compile(r"^[0-9a-f]{40}$")
DRIVER_REL = "scripts/ci/published-release-historical-retention.sh"
V1_EXECUTABLE_PATHS = ["scripts/ci/release_archive_inventory.sh"]
V1_RELEASE_PAIR = ("v5.3.0", "v5.4.0")
V1_COMMAND_CLASSES = {
    "state_producing": [
        "create-journey-canary",
        "init",
        "export-v0-bundle",
        "import-v1-bundle",
        "stage-prefix-v5.3.0",
        "stage-prefix-v5.4.0",
    ],
    "observe": [
        "migrate-check-v5.3",
        "migrate-check-v5.4",
        "explicit-v1-v5.3",
        "verify-v0-under-v5.4",
        "verify-v1-under-v5.4",
        "assay-version-v5.4",
        "post-failed-activation-active",
        "post-reactivation-active",
    ],
    "activate": [
        "failed-activate-v5.4",
        "activate-v5.4",
        "reactivate-v5.3",
    ],
}
FORBIDDEN_VERDICT_KEYS = (
    "continuity_matched",
    "exact_once_ok",
    "bytes_continuous",
    "canary_matched",
    "activation_ok",
    "upgrade_ok",
    "rollback_ok",
)
FORBIDDEN_CLAIM_WORDS = (
    "tamper-proof",
    "tamper-evident against",
    "physical retention",
    "filesystem immutability",
    "shipped updater",
    "automatic rollback",
    "transactional rollback",
    "non-Linux",
    "retroactive",
)
SELF_ATTEST_NEEDLE = "continuity_matched"


def require(text: str, needle: str, message: str, problems: list[str]) -> None:
    if needle not in text:
        problems.append(message)


def active_lines(text: str) -> list[str]:
    return [line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]


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


def load_manifest(path: Path, problems: list[str]) -> dict:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"harness manifest is unreadable: {error}")
        return {}
    if manifest.get("schema") != SCHEMA:
        problems.append("harness manifest schema drifted")
    for key in (
        "from_tag",
        "to_tag",
        "boundaries",
        "activation_targets",
        "boundary_activation_refs",
        "command_classes",
        "required_retained_artifacts",
        "files",
    ):
        if key not in manifest:
            problems.append(f"harness manifest missing {key}")
    return manifest


def unsafe_release_tag_reason(value: object, field: str) -> str | None:
    if not isinstance(value, str) or not value.strip():
        return None
    if SAFE_RELEASE_TAG.fullmatch(value) is None:
        return f"unsafe {field} path component"
    return None


def release_pair(manifest: dict, problems: list[str]) -> tuple[str | None, str | None]:
    from_tag = manifest.get("from_tag")
    to_tag = manifest.get("to_tag")
    if not isinstance(from_tag, str) or not from_tag.strip():
        problems.append("harness manifest missing from_tag")
        from_tag = None
    else:
        reason = unsafe_release_tag_reason(from_tag, "from_tag")
        if reason:
            problems.append(reason)
            from_tag = None
    if not isinstance(to_tag, str) or not to_tag.strip():
        problems.append("harness manifest missing to_tag")
        to_tag = None
    else:
        reason = unsafe_release_tag_reason(to_tag, "to_tag")
        if reason:
            problems.append(reason)
            to_tag = None
    return from_tag, to_tag


def require_v1_release_pair(from_tag: str | None, to_tag: str | None, problems: list[str]) -> None:
    if from_tag is None or to_tag is None:
        return
    if (from_tag, to_tag) != V1_RELEASE_PAIR:
        problems.append("harness manifest release pair drifted from the v1 denominator")


def require_expected_head_sha(value: object, problems: list[str]) -> str | None:
    if not isinstance(value, str) or HEAD_SHA.fullmatch(value) is None:
        problems.append("--expected-head-sha must be exactly 40 lowercase hex characters")
        return None
    return value


def manifest_driver_digest(manifest: dict, problems: list[str]) -> str | None:
    for row in manifest.get("files") or []:
        if not isinstance(row, dict) or row.get("path") != DRIVER_REL:
            continue
        digest = row.get("sha256")
        if isinstance(digest, str) and len(digest) == 64:
            return digest
        problems.append("harness manifest driver digest is unusable")
        return None
    problems.append("harness manifest missing driver digest")
    return None


def bind_run_pin_harness(
    pin: dict,
    expected_head_sha: str,
    driver_digest: str,
    problems: list[str],
) -> None:
    harness = pin.get("harness")
    if not isinstance(harness, dict):
        problems.append("run-pin harness is missing")
        return
    if harness.get("head_sha") != expected_head_sha:
        problems.append("run-pin harness.head_sha must match --expected-head-sha")
    if harness.get("driver_sha256") != driver_digest:
        problems.append("run-pin harness.driver_sha256 must match the harness manifest driver digest")
    run_id = harness.get("workflow_run_id")
    if not isinstance(run_id, str) or not run_id.strip():
        problems.append("run-pin harness.workflow_run_id must be a nonempty string")
    attempt = harness.get("workflow_run_attempt")
    if type(attempt) is not int or attempt < 1:
        problems.append("run-pin harness.workflow_run_attempt must be a positive integer")


def resolve_release_ref(from_tag: str | None, to_tag: str | None, ref: str, problems: list[str], where: str) -> str | None:
    if ref == "from_tag":
        if from_tag is None:
            problems.append(f"{where}: from_tag is missing")
        return from_tag
    if ref == "to_tag":
        if to_tag is None:
            problems.append(f"{where}: to_tag is missing")
        return to_tag
    problems.append(f"{where}: unknown release pair reference {ref!r}")
    return None


def command_name_classes(classes: object, problems: list[str]) -> dict[str, str]:
    mapping: dict[str, str] = {}
    if not isinstance(classes, dict):
        problems.append("harness manifest vocabulary is unusable")
        return mapping
    for class_name, names in classes.items():
        if not isinstance(names, list):
            problems.append(f"command class {class_name} must be a list")
            continue
        for name in names:
            if not isinstance(name, str):
                problems.append(f"command class {class_name} has a non-string name")
                continue
            if name in mapping:
                problems.append(f"command {name} declared in multiple classes")
                continue
            mapping[name] = class_name
    return mapping


def require_v1_command_classes(manifest: dict, problems: list[str]) -> dict[str, str]:
    if manifest.get("command_classes") != V1_COMMAND_CLASSES:
        problems.append("harness manifest command_classes drifted from the v1 denominator")
    return command_name_classes(V1_COMMAND_CLASSES, problems)


def hook_files_regex(text: str, hook_id: str) -> str | None:
    in_hook = False
    for line in text.splitlines():
        if re.match(rf"^\s+- id: {re.escape(hook_id)}\s*$", line):
            in_hook = True
            continue
        if in_hook and re.match(r"^\s+- id:", line):
            break
        if in_hook:
            match = re.match(r"^\s+files:\s+(\S+)\s*$", line)
            if match:
                return match.group(1)
    return None


def validate_hook_selects_manifest_files(precommit_text: str, manifest: dict, problems: list[str]) -> None:
    selector = hook_files_regex(precommit_text, HOOK_ID)
    if not selector:
        problems.append("historical-retention pre-commit files selector is missing")
        return
    try:
        compiled = re.compile(selector)
    except re.error as error:
        problems.append(f"historical-retention pre-commit files selector is invalid: {error}")
        return
    for row in manifest.get("files") or []:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            continue
        path = row["path"]
        if compiled.search(path) is None:
            problems.append(f"historical-retention pre-commit files selector must match {path!r}")


def boundary_specs(manifest: dict, problems: list[str]) -> list[tuple[str, str, str]]:
    names = manifest.get("boundaries")
    targets = manifest.get("activation_targets")
    refs = manifest.get("boundary_activation_refs")
    if not isinstance(names, list) or not names:
        problems.append("harness manifest missing boundaries")
        return []
    if not isinstance(targets, dict) or not isinstance(refs, dict):
        problems.append("harness manifest missing boundary activation maps")
        return []
    if list(targets) != names or list(refs) != names:
        problems.append("boundary activation refs must list every boundary exactly once")
        return []
    specs: list[tuple[str, str, str]] = []
    for name in names:
        if not isinstance(name, str):
            problems.append("boundary name must be a string")
            return []
        target_ref = targets.get(name)
        last_ref = refs.get(name)
        if not isinstance(target_ref, str) or not target_ref or not isinstance(last_ref, str) or not last_ref:
            problems.append(f"boundary {name} missing activation reference")
            return []
        specs.append((name, target_ref, last_ref))
    return specs


def last_successful_activation(
    ref: str,
    after_activate: dict[str, str],
    from_tag: str | None,
    problems: list[str],
    boundary: str,
) -> str | None:
    if ref == INITIAL_ACTIVATION_REF:
        if not from_tag:
            problems.append(f"initial activation reference missing from_tag at {boundary}")
            return None
        return from_tag
    if ref not in after_activate:
        problems.append(f"unknown activation reference {ref!r} at {boundary}")
        return None
    return after_activate[ref]


def unsafe_required_artifact_path_reason(item: str) -> str | None:
    """Reject anything that is not a canonical POSIX relative file path.

    Backslash is refused explicitly so a Windows path cannot be a second
    encoding of the same artifact. PurePosixPath(".") has empty parts, so a
    parts-only denylist of "." never sees it.
    """
    if "\\" in item:
        return f"unsafe required retained artifact path: {item}"
    relative = PurePosixPath(item)
    if (
        not relative.parts
        or relative.is_absolute()
        or ".." in relative.parts
        or relative.as_posix() != item
    ):
        return f"unsafe required retained artifact path: {item}"
    return None


def required_retained_artifacts(manifest: dict, problems: list[str]) -> list[str]:
    items = manifest.get("required_retained_artifacts")
    if not isinstance(items, list) or not items:
        problems.append("harness manifest missing required_retained_artifacts")
        return []
    paths: list[str] = []
    for item in items:
        if not isinstance(item, str) or not item:
            problems.append("required retained artifact path is invalid")
            continue
        if item in paths:
            problems.append(f"required retained artifact path is duplicated: {item}")
            continue
        reason = unsafe_required_artifact_path_reason(item)
        if reason:
            problems.append(reason)
            continue
        paths.append(item)
    return paths


def workflow_on_mapping(text: str) -> str:
    if text.startswith("on:"):
        start = 0
    else:
        idx = text.find("\non:")
        if idx < 0:
            return ""
        start = idx + 1
    end = text.find("\npermissions:", start)
    if end < 0:
        return text[start:]
    return text[start:end]


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ndjson_rows(path: Path, problems: list[str], label: str) -> list[dict]:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        problems.append(f"{label} is unreadable: {error}")
        return []
    rows: list[dict] = []
    for index, line in enumerate(lines, start=1):
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            problems.append(f"{label} line {index} is not JSON: {error}")
            continue
        if not isinstance(row, dict):
            problems.append(f"{label} line {index} is not an object")
            continue
        rows.append(row)
    return rows



def harness_files_snapshot(
    files,
    problems: list[str],
    *,
    origin: str,
    schema: object = None,
    allowed_paths: list[str] | None = None,
    require_boolean: bool = False,
) -> tuple[object, tuple[tuple[str, str, bool], ...]] | None:
    """Normalize files[] to (schema, ((path, sha256, executable), ...)).

    One snapshot for harness manifest files[] and results/harness-files.json.
    Comparison is exact path, sha256, executable, schema, count, and order.
    """
    if not isinstance(files, list) or not files:
        problems.append(f"{origin} has no files")
        return None
    rows: list[tuple[str, str, bool]] = []
    seen: dict[str, int] = {}
    for row in files:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            problems.append(f"{origin} row has no path")
            continue
        path = row["path"]
        if path in seen:
            problems.append(f"{origin} path is duplicated: {path}")
            continue
        seen[path] = len(rows)
        if allowed_paths is not None and path not in allowed_paths:
            problems.append(f"{origin} path is unknown: {path}")
            continue
        digest = row.get("sha256")
        if not isinstance(digest, str) or len(digest) != 64:
            problems.append(f"{origin} digest is unusable: {path}")
            continue
        flag = row.get("executable", False)
        if "executable" in row and not isinstance(row.get("executable"), bool):
            problems.append(f"invalid executable flag: {path}")
            continue
        if require_boolean and "executable" not in row:
            problems.append(f"invalid executable flag: {path}")
            continue
        if require_boolean and not isinstance(flag, bool):
            problems.append(f"invalid executable flag: {path}")
            continue
        rows.append((path, digest, flag is True))
    if allowed_paths is not None:
        for path in allowed_paths:
            if path not in seen:
                problems.append(f"{origin} missing path: {path}")
    return (schema, tuple(rows))


def require_declared_executable_surface(
    rows: tuple[tuple[str, str, bool], ...],
    problems: list[str],
) -> None:
    observed = [path for path, _digest, flag in rows if flag]
    if observed != V1_EXECUTABLE_PATHS:
        problems.append("harness executable surface drifted")


def validate_harness_files_observation(results: Path, manifest: dict, problems: list[str]) -> None:
    files = manifest.get("files")
    declared = harness_files_snapshot(files, problems, origin="harness manifest", schema=manifest.get("schema"))
    allowed = [
        row["path"]
        for row in files
        if isinstance(files, list) and isinstance(row, dict) and isinstance(row.get("path"), str)
    ] if isinstance(files, list) else []
    report_path = results / "harness-files.json"
    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"harness-files.json is unreadable: {error}")
        return
    if not isinstance(report, dict):
        problems.append("harness-files.json is not an object")
        return
    observed = harness_files_snapshot(
        report.get("files"),
        problems,
        origin="harness-files.json",
        schema=report.get("schema"),
        allowed_paths=allowed or None,
        require_boolean=True,
    )
    if declared is not None and observed is not None and declared != observed:
        problems.append("harness files observation drifted")


def validate_manifest_files(
    manifest: dict,
    source_root: Path,
    workflow: Path,
    driver: Path,
    problems: list[str],
) -> None:
    files = manifest.get("files")
    if not isinstance(files, list) or not files:
        problems.append("harness manifest has no files")
        return
    overrides = {
        ".github/workflows/published-release-historical-retention.yml": workflow,
        "scripts/ci/published-release-historical-retention.sh": driver,
    }
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
        source = overrides.get(str(relative), source_root.joinpath(*relative.parts))
        try:
            actual = sha256_file(source)
        except OSError as error:
            problems.append(f"harness file is unreadable: {relative}: {error}")
            continue
        if row.get("sha256") != actual:
            problems.append(f"harness digest drifted: {relative}")
    expected = [
        ".github/workflows/published-release-historical-retention.yml",
        DRIVER_REL,
        "scripts/ci/release_attestation_enforce.sh",
        "scripts/ci/release_archive_inventory.sh",
        "scripts/ci/safe_extract_release_archive.py",
        "scripts/ci/bounded_download.py",
    ]
    if paths != expected:
        problems.append("harness manifest must list exactly the reviewed harness inputs")
    declared = harness_files_snapshot(files, problems, origin="harness manifest", schema=manifest.get("schema"))
    if declared is not None:
        require_declared_executable_surface(declared[1], problems)


def validate_source_contract(
    workflow: Path,
    driver: Path,
    manifest_path: Path,
    source_root: Path,
    pre_commit: Path,
) -> list[str]:
    problems: list[str] = []
    try:
        workflow_text = workflow.read_text(encoding="utf-8")
        driver_text = driver.read_text(encoding="utf-8")
        precommit_text = pre_commit.read_text(encoding="utf-8")
    except OSError as error:
        return [f"contract input is missing: {error}"]
    manifest = load_manifest(manifest_path, problems)
    if not manifest:
        return problems

    from_tag, to_tag = release_pair(manifest, problems)
    require_v1_release_pair(from_tag, to_tag, problems)
    require(workflow_text, "workflow_dispatch:", "workflow must support an explicit replay", problems)
    if "workflow_call:" in workflow_text:
        problems.append("historical workflow must not be a reusable caller surface")
    if workflow_on_mapping(workflow_text).rstrip("\n") != "on:\n  workflow_dispatch:":
        problems.append("historical workflow must be workflow_dispatch only")
    require(
        workflow_text,
        f"--manifest {MANIFEST_REL}",
        "workflow must pass the harness manifest path",
        problems,
    )
    if "--from-tag" in workflow_text or "--to-tag" in workflow_text:
        problems.append("workflow must not pass a second release-pair source")
    require(driver_text, 'manifest["from_tag"]', "driver must load from_tag from the harness manifest", problems)
    require(driver_text, 'manifest["to_tag"]', "driver must load to_tag from the harness manifest", problems)
    require(
        driver_text,
        'manifest["required_retained_artifacts"]',
        "driver must record required retained artifacts from the harness manifest",
        problems,
    )
    require(
        driver_text,
        "unsafe required retained artifact path",
        "driver must reject unsafe required retained artifact paths before materialization",
        problems,
    )
    require(
        driver_text,
        "as_posix() != relative",
        "driver must require authored artifact paths to be canonical POSIX",
        problems,
    )
    require(
        driver_text,
        '"\\\\" in relative',
        "driver must refuse backslash artifact paths",
        problems,
    )
    require(
        driver_text,
        "unsafe from_tag path component",
        "driver must reject an unsafe from_tag before materialization",
        problems,
    )
    require(
        driver_text,
        "unsafe to_tag path component",
        "driver must reject an unsafe to_tag before materialization",
        problems,
    )
    require(
        driver_text,
        'V1_RELEASE_PAIR = ("v5.3.0", "v5.4.0")',
        "driver must embed the v1 release pair before materialization",
        problems,
    )
    require(
        driver_text,
        "harness manifest release pair drifted from the v1 denominator",
        "driver must reject a drifted v1 release pair before materialization",
        problems,
    )
    require(
        driver_text,
        "destination.chmod(0o755)",
        "driver must chmod executable harness files",
        problems,
    )
    require(
        driver_text,
        "invalid executable flag",
        "driver must reject a non-boolean executable flag",
        problems,
    )
    require(
        driver_text,
        "destination.stat().st_mode",
        "driver must observe materialized executable mode",
        problems,
    )
    require(
        driver_text,
        "stat.S_IXUSR",
        "driver must use the user-execute bit",
        problems,
    )
    require(
        driver_text,
        "harness executable observation drifted",
        "driver must fail closed when observed mode differs from the declaration",
        problems,
    )
    require(
        driver_text,
        '"executable": observed_executable',
        "driver must report observed executable in harness-files.json",
        problems,
    )
    required_retained_artifacts(manifest, problems)
    name_classes = require_v1_command_classes(manifest, problems)
    for _, target_ref, last_ref in boundary_specs(manifest, problems):
        resolve_release_ref(from_tag, to_tag, target_ref, problems, "activation_targets")
        if last_ref != INITIAL_ACTIVATION_REF and last_ref not in name_classes:
            problems.append(f"unknown activation reference {last_ref!r}")
    require(workflow_text, "timeout-minutes: 30", "live job must have a bounded timeout", problems)
    require(workflow_text, "runs-on: ubuntu-latest", "historical job must be Linux x86_64", problems)
    require(
        workflow_text,
        'if [[ "$GITHUB_REF" != "refs/heads/main" ]]',
        "workflow must fail closed unless dispatched from main",
        problems,
    )
    if "if: github.ref == 'refs/heads/main'" in workflow_text:
        problems.append("main-only must fail closed, not skip the job")
    require(
        workflow_text,
        "bash scripts/ci/published-release-historical-retention.sh",
        "workflow must execute the reviewed driver",
        problems,
    )
    require(workflow_text, 'if: always()', "workflow must retain partial failure evidence", problems)
    require(workflow_text, "retention-days: 90", "workflow must retain hosted artifacts for 90 days", problems)
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
    require(
        workflow_text,
        'IMAGE_OS="${ImageOS:?image os provenance is empty}"',
        "workflow must bind non-empty ImageOS at runtime",
        problems,
    )
    require(
        workflow_text,
        'IMAGE_VERSION="${ImageVersion:?image version provenance is empty}"',
        "workflow must bind non-empty ImageVersion at runtime",
        problems,
    )
    if "IMAGE_OS: ${{ env.ImageOS }}" in workflow_text:
        problems.append("workflow must not bind ImageOS through the empty env context")
    if "IMAGE_VERSION: ${{ env.ImageVersion }}" in workflow_text:
        problems.append("workflow must not bind ImageVersion through the empty env context")
    if "continue-on-error:" in workflow_text:
        problems.append("historical journey must not continue on error")
    if "releases/latest" in workflow_text.lower():
        problems.append("workflow must not resolve a moving latest release")
    if "--fixture-root" in workflow_text:
        problems.append("hosted workflow must not pass fixture-root")

    expected_exercise = [
        "- name: Exercise historical retention",
        "shell: bash",
        "env:",
        "GH_TOKEN: ${{ github.token }}",
        "RUN_ROOT: ${{ runner.temp }}/assay-published-release-historical-retention",
        "RUNNER_OS: ${{ runner.os }}",
        "RUNNER_ARCH: ${{ runner.arch }}",
        "run: |",
        "set -euo pipefail",
        'IMAGE_OS="${ImageOS:?image os provenance is empty}"',
        'IMAGE_VERSION="${ImageVersion:?image version provenance is empty}"',
        "export IMAGE_OS IMAGE_VERSION",
        "bash scripts/ci/published-release-historical-retention.sh \\",
        f"--manifest {MANIFEST_REL} \\",
        '--harness-sha "$GITHUB_SHA" \\',
        '--workflow-run-id "$GITHUB_RUN_ID" \\',
        '--workflow-run-attempt "$GITHUB_RUN_ATTEMPT" \\',
        '--run-root "$RUN_ROOT"',
    ]
    if named_step_lines(workflow_text, "Exercise historical retention", problems) != expected_exercise:
        problems.append("workflow must execute only the exact reviewed driver invocation")
    expected_check = [
        "- name: Check retained historical retention",
        "shell: bash",
        "env:",
        "RUN_ROOT: ${{ runner.temp }}/assay-published-release-historical-retention",
        "run: |",
        "set -euo pipefail",
        "python3 scripts/ci/check-published-release-historical-retention-contract.py \\",
        '--results "$RUN_ROOT/results" \\',
        "--manifest scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json \\",
        '--expected-head-sha "$GITHUB_SHA"',
    ]
    check_lines = named_step_lines(workflow_text, "Check retained historical retention", problems)
    if check_lines != expected_check:
        problems.append("workflow must execute only the exact reviewed consumer-checker invocation")
    exercise_at = workflow_text.find("      - name: Exercise historical retention")
    check_at = workflow_text.find("      - name: Check retained historical retention")
    retain_at = workflow_text.find("      - name: Retain the replayable journey evidence")
    if not (0 <= exercise_at < check_at < retain_at):
        problems.append("consumer checker must run after the driver and before artifact upload")
    if "if: always()" in "\n".join(check_lines):
        problems.append("consumer checker must not use if: always()")

    driver_lines = active_lines(driver_text)
    require(driver_text, "from bounded_download import download", "driver must reuse bounded download", problems)
    require(
        driver_text,
        "from safe_extract_release_archive import extract_archive",
        "driver must reuse bounded archive extraction",
        problems,
    )
    require(
        driver_text,
        'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"',
        "driver must reuse the reviewed attestation verifier",
        problems,
    )
    require(driver_text, "flag_unavailable", "driver must classify v5.3 explicit v1 as flag_unavailable", problems)
    if "value_rejected" in driver_text:
        problems.append("v5.3 explicit v1 must not be classified as value_rejected")
    require(driver_text, '"migration_required": False', "driver must record migration_required false", problems)
    require(driver_text, '"disposition": "unmeasured"', "driver must not decide the unmeasured v0 cross-version verify", problems)
    require(driver_text, "journey/.journey-canary", "driver must create a once-only journey canary", problems)
    require(driver_text, "commands.ndjson", "driver must retain command provenance", problems)
    require(driver_text, "journey-ledger.ndjson", "driver must retain the journey ledger", problems)
    require(driver_text, "run-pin.json", "driver must retain the run pin", problems)
    require(driver_text, "stdout_sha256", "driver must record stdout digests", problems)
    require(driver_text, "stderr_sha256", "driver must record stderr digests", problems)
    require(driver_text, "executed_binary_sha256", "driver must record executed binary digests", problems)
    require(driver_text, "subject_binary_sha256", "driver must record subject binary digests for staging and activation", problems)
    require(driver_text, 'record_activate "failed-activate-v5.4"', "driver must record semantic activation rows", problems)
    require(driver_text, 'record_activate "activate-v5.4"', "driver must record successful v5.4 activation", problems)
    require(driver_text, 'record_activate "reactivate-v5.3"', "driver must record v5.3 reactivation", problems)
    require(driver_text, "record_stage", "driver must record semantic staging rows", problems)
    require(
        driver_text,
        "pathlib.Path(sys.argv[1]).write_bytes(os.urandom(32))",
        "canary row must record the argv that actually ran",
        problems,
    )
    require(driver_text, "selected_profile", "driver must record selected profile", problems)
    require(workflow_text, "RUNNER_OS: ${{ runner.os }}", "workflow must bind runner OS provenance", problems)
    require(workflow_text, "RUNNER_ARCH: ${{ runner.arch }}", "workflow must bind runner arch provenance", problems)
    require(driver_text, "active_binary_sha256", "driver must record the active binary digest at each boundary", problems)
    require(driver_text, "st_ino", "driver must record inode as a tripwire only", problems)
    require(driver_text, "st_birthtime", "driver must record birth-time as a tripwire only", problems)
    require(driver_text, "prefix digest mismatch before activation", "activation must check digest before switching", problems)
    require(driver_text, "prefix version mismatch before activation", "activation must check version before switching", problems)
    require(driver_text, "ln -sfn", "driver must switch a harness-owned active symlink", problems)
    require(driver_text, "$active_link/bin/assay", "post-fail and later commands must execute through the active symlink", problems)
    require(driver_text, "refusing to reuse prior evidence", "driver must refuse a reused run root", problems)
    require(driver_text, "x86_64-unknown-linux-gnu", "driver must pin Linux x86_64 product assets", problems)
    require(
        driver_text,
        "the harness observed single-creation byte continuity",
        "driver lost claim ceiling",
        problems,
    )
    if SELF_ATTEST_NEEDLE in driver_text:
        problems.append("driver must not write a self-attested continuity verdict")
    if "|| true" in driver_text or "set +e" in driver_text:
        problems.append("driver suppresses a failure instead of recording its exact status")

    activate_idx = next((i for i, line in enumerate(driver_lines) if line.startswith("activate_prefix()")), -1)
    if activate_idx < 0:
        problems.append("driver must define activate_prefix")
    else:
        digest_idx = next(
            (i for i, line in enumerate(driver_lines) if "prefix digest mismatch before activation" in line),
            -1,
        )
        version_idx = next(
            (i for i, line in enumerate(driver_lines) if "prefix version mismatch before activation" in line),
            -1,
        )
        switch_idx = next((i for i, line in enumerate(driver_lines) if "ln -sfn" in line), -1)
        if not (activate_idx < digest_idx < version_idx < switch_idx):
            problems.append("activation must check digest and version before switching the symlink")

    validate_manifest_files(manifest, source_root, workflow, driver, problems)
    validate_hook_selects_manifest_files(precommit_text, manifest, problems)
    return problems


def _file_map(entry: dict) -> dict[str, str]:
    files = entry.get("files")
    if not isinstance(files, list):
        return {}
    mapping: dict[str, str] = {}
    for row in files:
        if isinstance(row, dict) and isinstance(row.get("path"), str) and isinstance(row.get("sha256"), str):
            mapping[row["path"]] = row["sha256"]
    return mapping


def validate_results(results: Path, manifest_path: Path, expected_head_sha: str) -> list[str]:
    problems: list[str] = []
    if require_expected_head_sha(expected_head_sha, problems) is None:
        return problems
    manifest = load_manifest(manifest_path, problems)
    if not manifest:
        return problems
    from_tag, to_tag = release_pair(manifest, problems)
    require_v1_release_pair(from_tag, to_tag, problems)
    driver_digest = manifest_driver_digest(manifest, problems)
    specs = boundary_specs(manifest, problems)
    name_classes = require_v1_command_classes(manifest, problems)
    required_files = required_retained_artifacts(manifest, problems)
    try:
        pin = json.loads((results / "run-pin.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        problems.append(f"run-pin.json is unreadable: {error}")
        return problems
    if not isinstance(pin, dict):
        problems.append("run-pin.json is not an object")
        return problems
    binding_problems = len(problems)
    if driver_digest is not None:
        bind_run_pin_harness(pin, expected_head_sha, driver_digest, problems)
    if driver_digest is None or len(problems) > binding_problems:
        return problems

    validate_harness_files_observation(results, manifest, problems)

    if not specs or from_tag is None or to_tag is None:
        return problems
    classes = V1_COMMAND_CLASSES
    boundaries = [name for name, _, _ in specs]

    commands = ndjson_rows(results / "commands.ndjson", problems, "commands.ndjson")
    ledger = ndjson_rows(results / "journey-ledger.ndjson", problems, "journey-ledger.ndjson")

    if pin.get("schema") != PIN_SCHEMA:
        problems.append("run-pin schema drifted")
    if pin.get("from_tag") != from_tag:
        problems.append("run-pin from_tag must match the harness manifest")
    if pin.get("to_tag") != to_tag:
        problems.append("run-pin to_tag must match the harness manifest")
    for key in FORBIDDEN_VERDICT_KEYS:
        if key in pin:
            problems.append(f"run-pin contains self-attested verdict {key}")
    if pin.get("migration_required") is not False:
        problems.append("checker requires migration_required false")
    verify_meta = pin.get("v0_cross_version_verify")
    if not isinstance(verify_meta, dict) or verify_meta.get("disposition") != "unmeasured":
        problems.append("v0 cross-version verify disposition must remain unmeasured")
    recorded_exit = verify_meta.get("recorded_exit") if isinstance(verify_meta, dict) else None
    if type(recorded_exit) is not int:
        problems.append("v0 cross-version verify recorded_exit must be an integer")
    ceiling = pin.get("claim_ceiling")
    if not isinstance(ceiling, str) or "the harness observed single-creation byte continuity" not in ceiling:
        problems.append("run-pin lost claim ceiling")
    if isinstance(ceiling, str):
        lowered = ceiling.lower()
        for word in FORBIDDEN_CLAIM_WORDS:
            if word.lower() in lowered:
                problems.append(f"claim ceiling overstated: {word}")
    for key in ("image_os", "image_version", "runner_os", "runner_arch"):
        value = pin.get(key)
        if not isinstance(value, str) or not value.strip():
            problems.append(f"run-pin provenance must be non-empty: {key}")
    if pin.get("runner_os") != "Linux" or pin.get("runner_arch") != "X64":
        problems.append("run-pin must bind Linux/X64 runner truth")

    activate_names = classes["activate"]

    names = [row.get("name") for row in commands]
    for name, class_name in name_classes.items():
        count = names.count(name)
        if count != 1:
            problems.append(f"exact-once class {name} occurred {count} times")

    executed_fields = (
        "name",
        "class",
        "exit_code",
        "argv",
        "stdout_sha256",
        "stderr_sha256",
        "executed_binary_sha256",
        "selected_profile",
    )
    last_successful = from_tag
    after_activate: dict[str, str] = {}
    for row in commands:
        name = row.get("name")
        if isinstance(name, str) and name not in name_classes:
            problems.append(f"undeclared command: {name}")
            continue
        if isinstance(name, str) and row.get("class") != name_classes[name]:
            problems.append(
                f"command {name} class {row.get('class')} does not match manifest class {name_classes[name]}"
            )
        if isinstance(name, str) and (name in activate_names or row.get("class") == "activate"):
            if row.get("class") != "activate":
                problems.append(f"{name} must have class activate")
            if "executed_binary_sha256" in row:
                problems.append(f"{name} must not claim executed_binary_sha256")
            if not isinstance(row.get("subject_binary_sha256"), str) or len(row.get("subject_binary_sha256", "")) != 64:
                problems.append(f"{name} missing subject_binary_sha256")
            if "activation_target" not in row:
                problems.append(f"{name} missing activation_target")
            if name == "failed-activate-v5.4":
                if row.get("exit_code") == 0:
                    problems.append("failed-activate-v5.4 must be nonzero")
                if not row.get("rejection_reason"):
                    problems.append("failed-activate-v5.4 must record rejection_reason")
            else:
                if row.get("exit_code") != 0:
                    problems.append(f"{name} must succeed")
                if "rejection_reason" in row:
                    problems.append(f"{name} must not carry rejection_reason")
            if row.get("exit_code") == 0 and isinstance(row.get("activation_target"), str):
                last_successful = row["activation_target"]
            after_activate[name] = last_successful
        elif isinstance(name, str) and name.startswith("stage-prefix-"):
            if "executed_binary_sha256" in row:
                problems.append("staging row must not claim executed_binary_sha256")
            if not isinstance(row.get("subject_binary_sha256"), str) or len(row.get("subject_binary_sha256", "")) != 64:
                problems.append(f"{name} missing subject_binary_sha256")
        else:
            for field in executed_fields:
                if field not in row:
                    problems.append(f"command {row.get('name')} missing {field}")
                    break

    canary_cmd = [row for row in commands if row.get("name") == "create-journey-canary"]
    if len(canary_cmd) != 1:
        problems.append("create-journey-canary must be recorded exactly once")
    elif "pathlib.Path(sys.argv[1]).write_bytes(os.urandom(32))" not in " ".join(
        str(part) for part in canary_cmd[0].get("argv", [])
    ):
        problems.append("canary row must record the argv that actually ran")

    explicit = [row for row in commands if row.get("name") == "explicit-v1-v5.3"]
    if len(explicit) != 1:
        problems.append("explicit v5.3 v1 command must be recorded exactly once")
    elif explicit[0].get("flag_status") != "flag_unavailable":
        problems.append("v5.3 explicit v1 must be recorded as flag_unavailable")

    verify_v0 = [row for row in commands if row.get("name") == "verify-v0-under-v5.4"]
    if len(verify_v0) != 1:
        problems.append("v5.4 retained-v0 verify command must be recorded exactly once")

    post_fail = [row for row in commands if row.get("name") == "post-failed-activation-active"]
    if len(post_fail) != 1:
        problems.append("post-failed-activation must execute through the active symlink")

    observed = [row.get("boundary") for row in ledger]
    if observed != boundaries:
        problems.append(f"required boundaries drifted: {observed}")
    canaries = [row.get("canary_sha256") for row in ledger]
    if not canaries or any(item != canaries[0] or not isinstance(item, str) or len(item) != 64 for item in canaries):
        problems.append("canary continuity failed")
    seen_files: dict[str, str] = {}
    created_binary = None
    target_by_name = {name: target_ref for name, target_ref, _ in specs}
    last_ref_by_name = {name: last_ref for name, _, last_ref in specs}
    for row in ledger:
        boundary = row.get("boundary")
        if not isinstance(boundary, str) or boundary not in target_by_name:
            problems.append(f"required boundaries drifted: {observed}")
            continue
        expected_target = resolve_release_ref(
            from_tag, to_tag, target_by_name[boundary], problems, f"activation_targets.{boundary}"
        )
        last_before = last_successful_activation(
            last_ref_by_name[boundary], after_activate, from_tag, problems, boundary
        )
        if row.get("activation_target") != expected_target:
            problems.append(f"activation target drifted at {boundary}")
        if last_before is None or row.get("activation_target") != last_before:
            problems.append(f"ledger activation_target must match last successful activation at {boundary}")
        files = _file_map(row)
        if boundary == "v5.3-created":
            created_binary = row.get("active_binary_sha256")
        if boundary == "failed-v5.4-activation":
            if row.get("activation_target") != from_tag:
                problems.append(f"failed-v5.4-activation must keep active on {from_tag}")
            if row.get("active_binary_sha256") != created_binary:
                problems.append("failed activation must keep the staged v5.3 binary digest")
        for path, digest in seen_files.items():
            if path not in files:
                problems.append(f"later boundary omitted retained file {path} at {boundary}")
            elif files[path] != digest:
                problems.append(f"pairwise byte continuity failed for {path} at {boundary}")
        for path, digest in files.items():
            if path not in seen_files:
                seen_files[path] = digest
            elif seen_files[path] != digest:
                problems.append(f"pairwise byte continuity failed for {path} at {boundary}")
    if post_fail and created_binary and post_fail[0].get("executed_binary_sha256") != created_binary:
        problems.append("post-failed-activation executed binary must match staged v5.3")
    if ledger:
        last_files = _file_map(ledger[-1])
        for path in required_files:
            if path not in last_files:
                problems.append(f"required retained artifact missing: {path}")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workflow",
        type=Path,
        default=ROOT / ".github/workflows/published-release-historical-retention.yml",
    )
    parser.add_argument(
        "--driver",
        type=Path,
        default=ROOT / "scripts/ci/published-release-historical-retention.sh",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=ROOT / MANIFEST_REL,
    )
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--pre-commit", type=Path, default=ROOT / PRECOMMIT_REL)
    parser.add_argument("--results", type=Path)
    parser.add_argument("--expected-head-sha")
    args = parser.parse_args()
    if args.results is not None:
        if args.expected_head_sha is None:
            print("FAIL: --expected-head-sha is required for --results")
            return 1
        problems = validate_results(args.results, args.manifest, args.expected_head_sha)
    else:
        problems = validate_source_contract(
            args.workflow, args.driver, args.manifest, args.source_root, args.pre_commit
        )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("ok: published-release historical-retention contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
