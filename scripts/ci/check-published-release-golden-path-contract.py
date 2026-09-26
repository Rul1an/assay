#!/usr/bin/env python3
"""Fail-closed structural contract for the published-release golden path.

Parse-only: this checker must not execute the driver under review. A pull
request can point it at hostile driver bytes; inherited env/PATH is not a
sandbox. Trusted-repo behavioral probes of selected-archive identity and of
the asset names download_release_asset actually receives live in
test-published-release-golden-path-contract.sh.
"""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import re
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
PROFILE_VERSION_V1 = "--profile-version v1"
DENIED_OBSERVATIONS_FLAG = "--denied-observations"
VERIFY_PRIVILEGED = "verify-privileged-mcp-action"
EXAMPLE_DENIED_VERIFY_PAIRED = (
    "example must verify the denied-observation bundle with --profile-version v1"
)
EXAMPLE_MATRIX_FORWARDS_ARGS = (
    "example matrix() must forward extra args to verify-privileged-mcp-action"
)

LINUX_JOURNEY_MATRIX_ROWS = (
    {"os": "ubuntu-24.04", "label": "Linux x86_64", "target": "x86_64-unknown-linux-gnu"},
    {"os": "ubuntu-24.04-arm", "label": "Linux arm64", "target": "aarch64-unknown-linux-gnu"},
)
LINUX_JOURNEY_ARTIFACT_NAME = (
    "name: published-release-golden-path-${{ matrix.target }}-${{ inputs.release_tag }}-${{ github.sha }}"
)
HOST_MAP_X86 = 'x86_64|amd64) printf \'%s\\n\' "x86_64-unknown-linux-gnu" ;;'
HOST_MAP_ARM = 'aarch64|arm64) printf \'%s\\n\' "aarch64-unknown-linux-gnu" ;;'
HOST_MAP_UNKNOWN = (
    '*) fail "unsupported host architecture for published Linux journey: $(uname -m)" ;;'
)
HOST_MISMATCH_ELIF = 'elif [[ "$target" != "$host_target" ]]; then'
HOST_MISMATCH_FAIL = (
    'fail "requested target ${target} does not match host architecture (${host_target})"'
)


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


def linux_journey_include_rows(job_text: str) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    in_include = False
    for line in active_lines(job_text):
        if line == "include:":
            in_include = True
            continue
        if not in_include:
            continue
        if (
            line.startswith("- name:")
            or line.startswith("steps:")
            or line.startswith("uses:")
            or line.startswith("exclude:")
        ):
            break
        if line.startswith("- os:"):
            if current:
                rows.append(current)
            current = {"os": line.split(":", 1)[1].strip()}
            continue
        if current is None:
            continue
        if line.startswith("label:"):
            current["label"] = line.split(":", 1)[1].strip()
        elif line.startswith("target:"):
            current["target"] = line.split(":", 1)[1].strip()
    if current:
        rows.append(current)
    return rows


def validate_linux_journey_matrix(workflow_text: str, problems: list[str]) -> None:
    job = mapping_block(workflow_text, "published-linux-journey", 2, problems)
    if not job:
        problems.append("workflow must define the shared published-linux-journey matrix")
        return
    rows = linux_journey_include_rows(job)
    targets = [row.get("target") for row in rows]
    runners = [row.get("os") for row in rows]
    if "aarch64-unknown-linux-gnu" not in targets:
        problems.append("Linux journey matrix must include aarch64-unknown-linux-gnu")
    if "x86_64-unknown-linux-gnu" not in targets:
        problems.append("Linux journey matrix must include x86_64-unknown-linux-gnu")
    if "ubuntu-24.04-arm" not in runners:
        problems.append("Linux arm64 journey must use ubuntu-24.04-arm")
    if rows != [dict(row) for row in LINUX_JOURNEY_MATRIX_ROWS]:
        if "Linux journey matrix must include aarch64-unknown-linux-gnu" not in problems and (
            "Linux arm64 journey must use ubuntu-24.04-arm" not in problems
        ):
            problems.append("Linux journey matrix rows drifted")
    if LINUX_JOURNEY_ARTIFACT_NAME not in active_lines(job):
        problems.append("Linux journey artifact names must include matrix.target")
    if active_lines(job).count("runs-on: ${{ matrix.os }}") != 1:
        problems.append("Linux journey job must set runs-on: ${{ matrix.os }}")
    if "bash scripts/ci/published-release-golden-path.sh" not in job:
        problems.append("Linux journey matrix must execute the reviewed golden-path driver")
    if any(
        line.strip().startswith("if:")
        for line in job.splitlines()[1:]
        if line.startswith("    ")
        and not line.startswith("     ")
        and line.strip()
        and not line.lstrip().startswith("#")
    ):
        problems.append("Linux journey job must not be conditional")
    exercise_problems: list[str] = []
    exercise = named_step_lines(job, "Exercise the attested published release", exercise_problems)
    problems.extend(exercise_problems)
    if not exercise_problems and any(line.startswith("if:") for line in exercise):
        problems.append("Linux journey exercise step must not be conditional")


DARWIN_JOURNEY_MATRIX_ROWS = (
    {"os": "macos-26", "label": "macOS arm64", "target": "aarch64-apple-darwin"},
    {"os": "macos-26-intel", "label": "macOS x86_64", "target": "x86_64-apple-darwin"},
)
DARWIN_JOURNEY_DRIVER = "bash scripts/ci/published-release-golden-path.sh"
DARWIN_OPENING_DRIVER = "bash scripts/ci/published-release-platform-opening.sh"
SERVER_INSTALL_ARGV = (
    'cargo install assay-mcp-server --version "$version" --locked --root "$install_root"'
)
DOCUMENTED_INIT_ARGV = "assay init --preset dev --hello-trace"
DOCUMENTED_DEFAULT_PROFILE_ARGV = (
    'assay evidence verify-privileged-mcp-action "$v0_bundle" --format json'
)
DOCUMENTED_SARIF_ARGV = "assay-mcp-server enforcement-sarif --input - --output -"
V0_PROFILE_INPUT = (
    "conformance/privileged-mcp-action-v0/vectors/ok-001-deny-bound-observation.bundle.tar.gz"
)


def server_install_argv_problem(argv: list[str], version: str) -> str | None:
    """Refuse every server install that is not the pinned crates.io command.

    ``--root`` is the disposable prefix. ``--path`` and ``--git`` select a
    different source, and a missing ``--locked`` does not install the published lock.
    """
    if any(part == "--path" or part.startswith("--path=") or part == "--git" for part in argv):
        return "server install must not be a local --path build"
    root = ""
    if len(argv) >= 8 and argv[6] == "--root":
        root = argv[7]
    expected = [
        "cargo",
        "install",
        "assay-mcp-server",
        "--version",
        version,
        "--locked",
        "--root",
        root,
    ]
    if argv != expected or not root or root.startswith("-"):
        return (
            "server install must be cargo install assay-mcp-server "
            "--version <pin> --locked from crates.io"
        )
    return None


def validate_darwin_journey_matrix(workflow_text: str, problems: list[str]) -> None:
    job = mapping_block(workflow_text, "published-darwin-journey", 2, problems)
    if not job:
        problems.append("workflow must define the shared published-darwin-journey matrix")
        return
    rows = linux_journey_include_rows(job)
    if rows != [dict(row) for row in DARWIN_JOURNEY_MATRIX_ROWS]:
        problems.append("Darwin journey matrix must include macos-26 and macos-26-intel")
    if "exclude:" in active_lines(job):
        problems.append("Darwin journey matrix must not hide a row under exclude")
    if DARWIN_JOURNEY_DRIVER not in job:
        problems.append("Darwin journey must execute the reviewed golden-path driver")
    if DARWIN_OPENING_DRIVER in job:
        problems.append("Darwin journey must not point at the opening script")
    if "macos-latest" in job:
        problems.append("Darwin journey must not use macos-latest")
    exercise_problems: list[str] = []
    exercise = named_step_lines(
        job, "Exercise the attested published Darwin release", exercise_problems
    )
    problems.extend(exercise_problems)
    if not exercise_problems and any(line.startswith("if:") for line in exercise):
        problems.append("Darwin journey exercise step must not be conditional")
    if any(
        line.strip().startswith("if:")
        for line in job.splitlines()[1:]
        if line.startswith("    ")
        and not line.startswith("     ")
        and line.strip()
        and not line.lstrip().startswith("#")
    ):
        problems.append("Darwin journey job must not be conditional")
    artifact = (
        "name: published-release-golden-path-${{ matrix.target }}-"
        "${{ inputs.release_tag }}-${{ github.sha }}"
    )
    if artifact not in active_lines(job):
        problems.append("Darwin journey artifact names must include matrix.target")
    if "needs: published-checksum-consumer" not in active_lines(job):
        problems.append("Darwin journey must consume the checksum-verified CLI archive")
    if "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c" not in job:
        problems.append("Darwin journey must download the checksum-verified CLI archive")
    if '--verified-cli-dir "${RUNNER_TEMP}/verified-cli-incoming"' not in active_lines(job):
        problems.append("Darwin journey must rehash the checksum-verified CLI archive")
    live_workflow = "\n".join(active_lines(workflow_text))
    for archive in (
        "assay-${RELEASE_TAG}-aarch64-apple-darwin.tar.gz",
        "assay-${RELEASE_TAG}-x86_64-apple-darwin.tar.gz",
    ):
        if archive not in live_workflow:
            problems.append(f"checksum consumer must verify {archive}")


def validate_darwin_driver_portability(driver_text: str, problems: list[str]) -> None:
    """Darwin bash is 3.2 and has no sha256sum. The shared driver is that path."""
    driver_lines = active_lines(driver_text)
    if any("mapfile" in line for line in driver_lines):
        problems.append("driver must not use mapfile; Darwin bash is 3.2")
    if any("sha256sum" in line for line in driver_lines):
        problems.append("driver must hash with Python, not sha256sum")
    for line, message in (
        (
            'Darwin) resolve_darwin_target_from_host ;;',
            "driver lost the Darwin host resolver",
        ),
        (
            '*) fail "refusing Rosetta-translated process (sysctl.proc_translated=${host_proc_translated})" ;;',
            "driver lost translated-process refuse",
        ),
        (
            'elif [[ "$target" != "$host_target" ]]; then',
            "driver lost host/target mismatch refuse",
        ),
        (
            'aarch64-apple-darwin) platform_claim="macOS arm64" ;;',
            "driver lost the macOS arm64 target",
        ),
        (
            'x86_64-apple-darwin) platform_claim="macOS x86_64" ;;',
            "driver lost the macOS x86_64 target",
        ),
        (
            '"${SYSCTL_BIN:-/usr/sbin/sysctl}" -n sysctl.proc_translated',
            "driver lost the injected sysctl reader",
        ),
        (
            '|| fail "verified CLI archive sha256 does not match the checksum consumer"',
            "driver lost the checksum-consumer rehash",
        ),
    ):
        if driver_lines.count(line) != 1:
            problems.append(message)
    if driver_lines.count(SERVER_INSTALL_ARGV) != 1:
        problems.append(
            "server install must be cargo install assay-mcp-server "
            "--version <pin> --locked from crates.io"
        )
    if any("--path" in line and "cargo install" in line for line in driver_lines):
        problems.append("server install must not be a local --path build")
    if driver_lines.count(DOCUMENTED_INIT_ARGV) != 1:
        problems.append("driver must retain init without --format json")
    if driver_lines.count(DOCUMENTED_DEFAULT_PROFILE_ARGV) != 1:
        problems.append("driver must retain the documented default-profile verify")
    if V0_PROFILE_INPUT not in driver_text:
        problems.append("default-profile verify must use an input profile v0 accepts")
    if driver_lines.count(DOCUMENTED_SARIF_ARGV) != 1:
        problems.append("driver must retain enforcement-sarif on stdin and stdout")
    produced_v1 = (
        'assay evidence verify-privileged-mcp-action "$bundle" --format json --profile-version v1'
    )
    if driver_lines.count(produced_v1) != 1:
        problems.append("driver must keep explicit v1 on the produced bundle")
    bare_verifies = [
        line
        for line in driver_lines
        if "verify-privileged-mcp-action" in line and "--profile-version v1" not in line
    ]
    if bare_verifies != [DOCUMENTED_DEFAULT_PROFILE_ARGV]:
        problems.append("only the documented default-profile verify may omit --profile-version v1")
    for field in (
        '"name": "assay-mcp-server"',
        '"yanked":',
        '"index_checksum":',
        '"rustc_version":',
        '"binary_sha256":',
        '"version_stdout":',
        '"source_kind": "crates.io"',
    ):
        if field not in driver_text:
            problems.append(f"server install record lost {field}")


LINUX_OFFLINE_CONSTRUCTOR_ARM = "x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)"
DARWIN_OFFLINE_CONSTRUCTOR_ARM = "aarch64-apple-darwin|x86_64-apple-darwin)"
CLOSED_OFFLINE_CONSTRUCTOR_ARM = "*)"
LINUX_OFFLINE_CONSTRUCTOR_LINES = [
    "if ! unshare -rn true >/dev/null 2>&1; then",
    "if command -v sudo >/dev/null 2>&1; then",
    'if sudo PATH="/usr/sbin:/sbin:$PATH" sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 >/dev/null 2>&1; then',
    ":",
    "fi",
    "fi",
    "fi",
    'if ! unshare_err="$(unshare -rn true 2>&1)"; then',
    'fail "unshare -rn is not permitted in this environment: ${unshare_err:-unknown error}"',
    "fi",
]
DARWIN_OFFLINE_CONSTRUCTOR_LINES = [
    "if ! sandbox_err=\"$(/usr/bin/sandbox-exec -p '(version 1)(allow default)' true 2>&1)\"; then",
    'fail "sandbox-exec permissive profile was refused: ${sandbox_err:-unknown error}"',
    "fi",
]
DARWIN_CONSTRUCTOR_INVOKES_UNSHARE = "Darwin offline constructor must not invoke unshare"
# Command token. The substring also sits inside the existing "unshared" failure text.
UNSHARE_COMMAND = re.compile(r"(?<![A-Za-z0-9_])unshare(?![A-Za-z0-9_])")


def extract_shell_function(text: str, name: str) -> str:
    """Return one top-level shell function, including its closing brace."""
    lines = text.splitlines()
    marker = f"{name}() {{"
    start = next((index for index, line in enumerate(lines) if line.strip() == marker), None)
    if start is None:
        return ""
    body = [lines[start]]
    for line in lines[start + 1 :]:
        body.append(line)
        if line == "}":
            return "\n".join(body)
    return ""


def split_target_case_arms(function_body: str) -> dict[str, list[str]]:
    """Split the target case inside the constructor preflight. Patterns are exact."""
    lines = function_body.splitlines()
    start = next(
        (index for index, line in enumerate(lines) if line.strip() == 'case "$target" in'),
        None,
    )
    if start is None:
        return {}
    arms: dict[str, list[str]] = {}
    current: str | None = None
    buf: list[str] = []
    known = {
        LINUX_OFFLINE_CONSTRUCTOR_ARM,
        DARWIN_OFFLINE_CONSTRUCTOR_ARM,
        CLOSED_OFFLINE_CONSTRUCTOR_ARM,
    }
    for line in lines[start + 1 :]:
        stripped = line.strip()
        if stripped == "esac":
            if current is not None:
                arms[current] = buf
            break
        if stripped in known:
            if current is not None:
                arms[current] = buf
            current = stripped
            buf = []
            continue
        if current is not None and stripped != ";;":
            buf.append(line)
    return arms


def validate_offline_constructor_preflight(driver_text: str, problems: list[str]) -> None:
    """Linux keeps unshare. The Darwin arm must not invoke it. Other targets fail closed."""
    function = extract_shell_function(driver_text, "preflight_offline_constructor")
    arms = split_target_case_arms(function)
    linux_lines = active_lines("\n".join(arms.get(LINUX_OFFLINE_CONSTRUCTOR_ARM, [])))
    darwin_lines = active_lines("\n".join(arms.get(DARWIN_OFFLINE_CONSTRUCTOR_ARM, [])))
    closed_lines = active_lines("\n".join(arms.get(CLOSED_OFFLINE_CONSTRUCTOR_ARM, [])))
    driver_unshare = [line for line in active_lines(driver_text) if UNSHARE_COMMAND.search(line)]
    linux_unshare = [line for line in linux_lines if UNSHARE_COMMAND.search(line)]
    darwin_unshare = [line for line in darwin_lines if UNSHARE_COMMAND.search(line)]
    if darwin_unshare or Counter(driver_unshare) != Counter(linux_unshare):
        problems.append(DARWIN_CONSTRUCTOR_INVOKES_UNSHARE)
    if linux_lines != LINUX_OFFLINE_CONSTRUCTOR_LINES:
        problems.append("Linux offline constructor drifted")
    if darwin_lines != DARWIN_OFFLINE_CONSTRUCTOR_LINES:
        problems.append("Darwin offline constructor must run /usr/bin/sandbox-exec")
    if closed_lines != ['fail "no offline constructor for ${target}"']:
        problems.append("offline constructor must fail closed with no offline constructor for <target>")
    if active_lines(driver_text).count("preflight_offline_constructor") != 1:
        problems.append("driver must run the offline constructor preflight exactly once")


def validate_darwin_isolation(offline_text: str, problems: list[str]) -> None:
    if 'return ["unshare", "-rn", *command]' not in offline_text:
        problems.append("Linux isolation constructor drifted")
    if "SANDBOX_EXEC" not in offline_text or '"/usr/bin/sandbox-exec"' not in offline_text:
        problems.append("Darwin isolation must be /usr/bin/sandbox-exec")
    if 'return [SANDBOX_EXEC, "-p", DARWIN_RESTRICTIVE_PROFILE, *command]' not in offline_text:
        problems.append("Darwin isolation_argv must carry the restrictive profile")
    if "(deny network*)" not in offline_text or "(allow network*)" not in offline_text:
        problems.append("Darwin profiles must keep a network deny and its allow twin")
    if "DARWIN_DENIAL_ERRNO_NAMES" not in offline_text or '{"EPERM"}' not in offline_text:
        problems.append("Darwin denial allow-list must be the measured EPERM receipt")
    denial_set = offline_text.split("DENIAL_ERRNOS = {", 1)
    if len(denial_set) != 2 or "EPERM" in denial_set[1].split("}", 1)[0]:
        problems.append("EPERM must not join the Linux denial set")


def validate_linux_journey_driver_identity(driver_text: str, problems: list[str]) -> None:
    """Parse-only host-map and selected-archive pins. Do not execute driver_text."""
    driver_lines = active_lines(driver_text)
    exact_host_lines = {
        HOST_MAP_X86: "Linux x86_64 host mapping drifted",
        HOST_MAP_ARM: "Linux arm64 host mapping drifted",
        HOST_MAP_UNKNOWN: "driver lost unknown host refuse",
        HOST_MISMATCH_ELIF: "driver lost host/target mismatch refuse",
        HOST_MISMATCH_FAIL: "driver lost host/target mismatch refuse",
    }
    for line, message in exact_host_lines.items():
        if driver_lines.count(line) != 1:
            problems.append(message)
    selected_archive_lines = {
        "select_linux_journey_product_archives() {": "Linux product asset assignment drifted",
        'cli_asset="assay-${1}-${2}.tar.gz"': "Linux product asset assignment drifted",
        'mcp_asset="assay-mcp-server-${1}-${2}.tar.gz"': "Linux product asset assignment drifted",
        'select_linux_journey_product_archives "$release_tag" "$target"': (
            "Linux journey must select archives from the live resolved target"
        ),
        'printf \'%s\' "$cli_asset" >"$results/journey-cli-asset.txt"': (
            "Linux journey must record the selected CLI archive"
        ),
        'printf \'%s\' "$mcp_asset" >"$results/journey-mcp-asset.txt"': (
            "Linux journey must record the selected MCP archive"
        ),
    }
    for line, message in selected_archive_lines.items():
        if driver_lines.count(line) != 1:
            problems.append(message)
    if sum(1 for line in driver_lines if line.startswith("cli_asset=")) != 1:
        problems.append("driver must assign cli_asset exactly once")
    if sum(1 for line in driver_lines if line.startswith("mcp_asset=")) != 1:
        problems.append("driver must assign mcp_asset exactly once")


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
        "examples/privileged-action-gate/policies/allow.yaml",
        "examples/privileged-action-gate/baseline-approved.json",
        ".github/workflows/published-release-golden-path.yml",
        ".github/workflows/release.yml",
        "scripts/ci/published-release-golden-path.sh",
        "scripts/ci/lib/published-release-capture.sh",
        "scripts/ci/published_release_proxy_phase.py",
        "scripts/ci/published_release_offline_phase.py",
        "scripts/ci/published_release_offline_windows.py",
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
        "RELEASE_TARGET: ${{ matrix.target }}",
        "RUN_ROOT: ${{ runner.temp }}/assay-published-release-golden-path",
        "run: |",
        "set -euo pipefail",
        "bash scripts/ci/published-release-golden-path.sh \\",
        '--release-tag "$RELEASE_TAG" \\',
        '--target "$RELEASE_TARGET" \\',
        '--harness-sha "$GITHUB_SHA" \\',
        '--workflow-run-id "$GITHUB_RUN_ID" \\',
        '--workflow-run-attempt "$GITHUB_RUN_ATTEMPT" \\',
        '--run-root "$RUN_ROOT"',
    ]
    if named_step_lines(workflow_text, "Exercise the attested published release", problems) != expected_exercise_step:
        problems.append("workflow must execute only the exact reviewed driver invocation")

    validate_linux_journey_matrix(workflow_text, problems)
    validate_darwin_journey_matrix(workflow_text, problems)
    if "linux-x86_64:" in workflow_text:
        problems.append("legacy linux-x86_64 job must be replaced by the shared matrix")
    if workflow_text.count("bash scripts/ci/published-release-golden-path.sh") != 2:
        problems.append("workflow must invoke the golden-path driver for Linux and Darwin")

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
        "unset GH_TOKEN GITHUB_TOKEN PYTHONPATH",
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
        'call_request=\'{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}\'',
        "proxy_status=0",
        'printf \'%s\\n%s\\n\' "$init_request" "$call_request" \\',
        '| (cd "$results" && \\',
        '"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" \\',
        "--timeout-seconds 60) || proxy_status=$?",
    ]
    proxy_block = lines_between(
        driver_text,
        "call_request=",
        '[[ "$proxy_status"',
        problems,
    )
    if proxy_block != expected_proxy_block:
        problems.append("proxy execution and provenance block drifted")
    if 'record_command "proxy-enforce"' in driver_text:
        problems.append("driver must not record proxy provenance separately from execution")
    expected_mcp_binary_surface = [
        'mcp_asset="assay-mcp-server-${1}-${2}.tar.gz"',
        'local binary="$install_root/bin/assay-mcp-server"',
        'name = "assay-mcp-server"',
        '"name": "assay-mcp-server",',
        'raise SystemExit("published assay-mcp-server crate is yanked")',
        'cargo install assay-mcp-server --version "$version" --locked --root "$install_root"',
        'done < <(find "$mcp_extract" -type f -name assay-mcp-server -perm -u+x)',
        '[[ "${#mcp_candidates[@]}" -eq 1 ]] || fail "MCP archive must contain exactly one executable assay-mcp-server binary"',
        'cp "${mcp_candidates[0]}" "$install_root/bin/assay-mcp-server"',
        'chmod 0755 "$install_root/bin/assay" "$install_root/bin/assay-mcp-server"',
        '[[ "$(command -v assay-mcp-server)" == "$install_root/bin/assay-mcp-server" ]] || fail "assay-mcp-server did not resolve from the disposable install prefix"',
        'run_capture "mcp-version" 0 "$results/mcp-version.txt" "$results/mcp-version.stderr" assay-mcp-server --version',
        '[[ "$(tr -d \'\\r\\n\' <"$results/mcp-version.txt")" == "assay-mcp-server $version" ]] \\',
        '|| fail "assay-mcp-server version differs from pinned release"',
        'assay-mcp-server enforcement-sarif --input "$decisions" --output "$results/enforcement.sarif"',
        'assay-mcp-server enforcement-sarif --input - --output - <"$decisions" >"$results/sarif-stdio.stdout" 2>"$results/sarif-stdio.stderr" || stdio_status=$?',
        'assay-mcp-server enforcement-sarif --input - --output -',
    ]
    mcp_binary_surface = [line for line in driver_lines if "assay-mcp-server" in line]
    if mcp_binary_surface != expected_mcp_binary_surface:
        problems.append("driver MCP binary invocation surface drifted")
    required_driver_fragments = {
        "exact stable tag": "release tag must be an exact stable vX.Y.Z tag",
        "fresh run root": "run root already exists; refusing to reuse prior evidence",
        "stable published release": "release tag is still draft or prerelease",
        "external tag source": '"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}"',
        "reviewed attestation verifier": 'bash "$harness_root/scripts/ci/release_attestation_enforce.sh"',
        "single-owner proxy phase": 'published_release_proxy_phase.py',
        "compressed asset ceiling": "release asset exceeds compressed-size ceiling",
        "bounded archive extractor": "from safe_extract_release_archive import extract_archive",
        "retained release inputs": 'downloads="$results/release-assets"',
        "disposable HOME": 'export HOME="$run_root/home"',
        "restricted PATH": 'export PATH="$install_root/bin:/usr/bin:/bin"',
        "release credential boundary": "unset GH_TOKEN GITHUB_TOKEN PYTHONPATH",
        "installed CLI resolution": '"$(command -v assay)" == "$install_root/bin/assay"',
        "installed MCP resolution": '"$(command -v assay-mcp-server)" == "$install_root/bin/assay-mcp-server"',
        "harness head": '"head_sha": harness_sha',
        "workflow run binding": '"workflow_run_id": workflow_run_id',
        "separate release provenance": '"release": {',
        "separate harness provenance": '"harness": {',
        "produced bundle": 'bundle="$results/produced.bundle.tar.gz"',
        "inspect produced bundle": 'assay evidence show --format json -- "$bundle"',
        "produce denied observations": '--denied-observations "$observations"',
        "verify produced bundle": 'assay evidence verify-privileged-mcp-action "$bundle" --format json --profile-version v1',
        "verify tampered bundle": 'assay evidence verify-privileged-mcp-action "$tampered" --format json --profile-version v1',
        "produced bundle valid verdict": '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "pass" and .verdict == "valid"',
        "tamper failure code": 'reason_code == "E_EVIDENCE_INTEGRITY"',
        "artifact manifest": '"assay.published_release_golden_path.artifacts.v1"',
        "claim ceiling": "the harness is not a shipped release asset",
        "diagnostic claim boundary": "Doctor reports host capabilities, not kernel enforcement performed by this journey.",
    }
    for label, fragment in required_driver_fragments.items():
        require(driver_text, fragment, f"driver lost {label}", problems)
    session_lines = lines_between(
        driver_text, 'pushd "$session_root"', 'run_capture "policy-validate"', problems
    )
    if (
        session_lines != ['pushd "$session_root" >/dev/null', 'run_published_release_session_product']
        or driver_lines.count('run_published_release_session_product') != 1
    ):
        problems.append("driver must execute the tested pre-init session in the session cwd")
    if driver_lines.count('source "$harness_root/scripts/ci/lib/published-release-capture.sh"') != 1:
        problems.append("driver must source the manifest-verified capture library exactly once")
    if "unset GH_TOKEN GITHUB_TOKEN PYTHONPATH" not in driver_lines:
        problems.append("release binaries must not inherit GitHub credentials")
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
        'for pattern, expected in (("release-assets/*.tar.gz", archive_count), ("attestation-raw/*.json", archive_count)):': (
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
        'cli_asset="assay-${1}-${2}.tar.gz"',
        'mcp_asset="assay-mcp-server-${1}-${2}.tar.gz"',
        'select_linux_journey_product_archives "$release_tag" "$target"',
    ]
    for assignment in exact_assignments:
        if driver_lines.count(assignment) != 1:
            problems.append(f"Linux product asset assignment drifted: {assignment}")
    target_requirements = {
        "target flag parse": '--target)',
        "host architecture resolve": "resolve_linux_target_from_host",
        "closed linux targets": "unsupported published Linux journey target",
        "platform claim x86": 'x86_64-unknown-linux-gnu) platform_claim="Linux x86_64" ;;',
        "platform claim arm": 'aarch64-unknown-linux-gnu) platform_claim="Linux arm64" ;;',
        "run-pin target field": '"target": target,',
        "claim uses platform_claim": "bounded {platform_claim} journey",
        "persisted journey target": 'journey-target.txt',
        "persisted platform claim": 'journey-platform-claim.txt',
        "selected product archives": 'select_linux_journey_product_archives "$release_tag" "$target"',
        "recorded cli archive": "journey-cli-asset.txt",
        "recorded mcp archive": "journey-mcp-asset.txt",
    }
    for label, fragment in target_requirements.items():
        require(driver_text, fragment, f"driver lost {label}", problems)
    validate_linux_journey_driver_identity(driver_text, problems)
    validate_darwin_driver_portability(driver_text, problems)
    validate_offline_constructor_preflight(driver_text, problems)
    if 'bounded Linux x86_64 journey' in driver_text:
        problems.append("run-pin claim must not hardcode Linux x86_64 for every target")
    if driver_lines.count('cli_asset="assay-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"') != 0:
        problems.append("driver must not hardcode the x86_64 CLI archive without ${target}")
    if driver_lines.count('mcp_asset="assay-mcp-server-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"') != 0:
        problems.append("driver must not hardcode the x86_64 MCP archive without ${target}")
    inspect_command = 'assay evidence show --format json -- "$bundle"'
    if driver_lines.count(inspect_command) != 1:
        problems.append("driver must inspect the same bundle it produced exactly once")
    if sum("assay evidence show" in line for line in driver_lines) != 1:
        problems.append("driver contains an alternate evidence-inspection target")
    produced_verify = (
        'assay evidence verify-privileged-mcp-action "$bundle" --format json --profile-version v1'
    )
    tampered_verify = (
        'assay evidence verify-privileged-mcp-action "$tampered" --format json --profile-version v1'
    )
    if driver_lines.count(produced_verify) != 1:
        problems.append(
            "driver must verify the produced denial-observation bundle with --profile-version v1 exactly once"
        )
    if driver_lines.count(tampered_verify) != 1:
        problems.append(
            "driver must verify the tampered denial-observation bundle with --profile-version v1 exactly once"
        )
    if "unshare -rn curl" in driver_text:
        problems.append("driver must not treat a curl exit as network denial")
    expected_offline_block = [
        "offline_status=0",
        '(cd "$results" && \\',
        '"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_offline_phase.py" \\',
        "--timeout-seconds 30 \\",
        "-- \\",
        'assay evidence verify-privileged-mcp-action "$bundle" --profile-version v1 --format json) \\',
        "|| offline_status=$?",
        '[[ "$offline_status" -eq 0 ]] || fail "offline isolation phase exited $offline_status"',
    ]
    offline_block = lines_between(
        driver_text,
        "offline_status=0",
        'cmp -s "$results/verify.json" "$results/verify-offline.json"',
        problems,
    )
    if offline_block != expected_offline_block:
        problems.append("driver must run the offline phase through its reviewed helper")
    try:
        offline_helper = (
            source_root / "scripts/ci/published_release_offline_phase.py"
        ).read_text(encoding="utf-8")
    except OSError as error:
        problems.append(f"offline phase helper is unreadable: {error}")
        offline_helper = ""
    if offline_helper:
        if offline_helper.count('return ["unshare", "-rn", *command]') != 1:
            problems.append("offline isolation constructor drifted")
        validate_darwin_isolation(offline_helper, problems)
        if offline_helper.count("isolation_argv(command)") != 1 or offline_helper.count(
            "isolation_argv(verifier)"
        ) != 1:
            problems.append("offline probe and verifier must share one isolation constructor")
    if any(
        "verify-privileged-mcp-action" in line
        and "--profile-version v1" not in line
        and line != DOCUMENTED_DEFAULT_PROFILE_ARGV
        for line in driver_lines
    ):
        problems.append("driver verifies a produced or tampered bundle without --profile-version v1")
    require(
        driver_text,
        'cmp -s "$results/verify.json" "$results/verify-offline.json"',
        "driver must verify that offline verification output matches connected verification",
        problems,
    )
    required_artifacts = [
        "run-pin.json",
        "commands.ndjson",
        "doctor.json",
        "produced.bundle.tar.gz",
        "decisions.ndjson",
        "inspect.json",
        "verify.json",
        "tamper-verify.json",
        "enforcement.sarif",
        "release-api.json",
        "tag-ref.json",
        "attestation-summary.json",
        "allow/proxy.jsonl",
        "allow/decisions.ndjson",
        "allow/produced.bundle.tar.gz",
        "allow/verify.json",
        "unsupported/proxy.jsonl",
    ]
    for name in required_artifacts:
        if f'"{name}"' not in driver_text:
            problems.append(f"driver no longer requires retained artifact: {name}")

    if active_lines(driver_text).count("run_published_release_extra_request_cases") != 1:
        problems.append("driver must execute the allow and unsupported request cases exactly once")
    validate_manifest(manifest, source_root, workflow, release_workflow, driver, problems)
    return problems


def joined_active_commands(text: str) -> list[str]:
    commands: list[str] = []
    current: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        continued = stripped.endswith("\\")
        piece = stripped[:-1].rstrip() if continued else stripped
        current.append(piece)
        if not continued:
            commands.append(" ".join(current))
            current = []
    if current:
        commands.append(" ".join(current))
    return commands


def denied_observation_bundle_out(text: str) -> str:
    for command in joined_active_commands(text):
        if DENIED_OBSERVATIONS_FLAG not in command or "--bundle-out" not in command:
            continue
        parts = command.split()
        marker = parts.index("--bundle-out")
        if marker + 1 >= len(parts):
            break
        return parts[marker + 1]
    raise ValueError("example has no --denied-observations import with --bundle-out")


def shipping_denied_verify_source_line(text: str) -> str:
    """Return the shipping verify/matrix line that checks the denied-observation bundle."""
    bundle = denied_observation_bundle_out(text)
    matches = []
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if bundle not in stripped or "--bundle-out" in stripped or "import" in stripped:
            continue
        if stripped.startswith("matrix ") or VERIFY_PRIVILEGED in stripped:
            matches.append(stripped)
    if len(matches) != 1:
        raise ValueError(
            f"example must verify the denied-observation bundle exactly once, found {len(matches)}"
        )
    return matches[0]


def seed_denied_verify_bypass(text: str) -> str:
    """Remove --profile-version v1 from the shipping denied-bundle verify. Same mutant both guards."""
    line = shipping_denied_verify_source_line(text)
    return text.replace(line, line.replace(PROFILE_VERSION_V1, "").rstrip(), 1)


def matrix_forwards_verify_args(text: str) -> bool:
    verify_lines = [
        line
        for line in active_lines(text)
        if VERIFY_PRIVILEGED in line and '"$bundle"' in line
    ]
    return len(verify_lines) == 1 and '"$@"' in verify_lines[0]


def validate_example_pairing(example_run: Path) -> list[str]:
    try:
        text = example_run.read_text(encoding="utf-8")
    except OSError as error:
        return [f"example run.sh is unreadable: {error}"]
    try:
        line = shipping_denied_verify_source_line(text)
    except ValueError as error:
        return [str(error)]
    problems: list[str] = []
    if PROFILE_VERSION_V1 not in line:
        problems.append(EXAMPLE_DENIED_VERIFY_PAIRED)
    if line.startswith("matrix ") and not matrix_forwards_verify_args(text):
        problems.append(EXAMPLE_MATRIX_FORWARDS_ARGS)
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
    parser.add_argument(
        "--example-run",
        type=Path,
        default=ROOT / "examples/privileged-action-gate/run.sh",
    )
    args = parser.parse_args()
    problems = validate_contract(
        args.workflow, args.release_workflow, args.driver, args.manifest, args.source_root
    )
    problems.extend(validate_example_pairing(args.example_run))
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("ok: published-release golden-path contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
