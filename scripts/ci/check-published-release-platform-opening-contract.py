#!/usr/bin/env python3
"""Fail-closed contract for the published-archive Windows/macOS opening.

Linux full journeys share one matrixed driver invocation (x86_64 + arm64).
This checker imports the golden-path matrix pin (rows, ubuntu-24.04-arm, and
job-level runs-on: ${{ matrix.os }}) and does not execute the Linux driver.
This checker requires both Linux matrix rows to call that driver with --target
and keeps opening legs downloading their own published archive by tag.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


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


def _golden_path_contract():
    path = Path(__file__).with_name("check-published-release-golden-path-contract.py")
    spec = importlib.util.spec_from_file_location("published_release_golden_path_contract", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load published-release golden-path contract helper")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def validate_linux_journey_matrix(workflow_text: str, problems: list[str]) -> None:
    _golden_path_contract().validate_linux_journey_matrix(workflow_text, problems)
    if '--target "$RELEASE_TARGET"' not in workflow_text:
        problems.append("Linux journey matrix must pass --target from the matrix")
    if workflow_text.count("bash scripts/ci/published-release-golden-path.sh") != 2:
        problems.append("Linux and Darwin journeys must each invoke the golden-path driver")


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
    download_uses = [
        line for line in active_lines(workflow_text) if "actions/download-artifact" in line
    ]
    verified_cli = (
        "name: published-verified-darwin-cli-${{ inputs.release_tag }}-${{ github.sha }}"
    )
    allowed_download = (
        "uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1"
    )
    if download_uses and (
        download_uses != [allowed_download] or verified_cli not in active_lines(workflow_text)
    ):
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
        "shared doctor preflight": "run_published_release_doctor",
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
    if not any("run_published_release_doctor" in line for line in driver_lines):
        problems.append("opening driver does not execute the shared doctor preflight")
    if "jq" in driver_text:
        problems.append("opening driver must not gain a jq dependency")

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


def _load_session_phase():
    session_test = ROOT / "scripts/ci/test_published_release_session_phase.py"
    spec = importlib.util.spec_from_file_location("session_phase", session_test)
    if spec is None or spec.loader is None:
        return None, session_test
    session_phase = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(session_phase)
    return session_phase, session_test


def _explicit_config_argv(outcome: dict) -> bool:
    observed = outcome["observed"]
    if not observed:
        return False
    argv = observed[0].get("argv", [])
    return (
        argv[:4] == ["doctor", "--format", "json", "--config"]
        and bool(observed[0].get("explicit_config_exists"))
    )


def _run_opening_product(opening: Path, session_test: Path, report: dict) -> dict:
    """Exec the opening product function. The temp tree is gone on return."""
    with tempfile.TemporaryDirectory(prefix="opening probe ") as temporary:
        root = Path(temporary)
        results = root / "results"
        results.mkdir()
        bindir = root / "bin"
        bindir.mkdir()
        decoy = root / "cwd"
        decoy.mkdir()
        (decoy / "eval.yaml").write_text("decoy: true\n", encoding="utf-8")
        fake = bindir / "assay"
        fake.write_text(
            f"#!{sys.executable}\n"
            "import os, runpy\n"
            "os.environ['PUBLISHED_RELEASE_ASSAY_FAKE'] = '1'\n"
            f"raise SystemExit(runpy.run_path({str(session_test.resolve())!r}, run_name='__main__'))\n",
            encoding="utf-8",
        )
        fake.chmod(0o755)
        (root / "control.json").write_text(
            json.dumps({"output": json.dumps(report), "exit": 0}),
            encoding="utf-8",
        )
        script = f'''set -euo pipefail
source {shlex.quote(str(opening.resolve()))}
PYTHON_BIN={shlex.quote(sys.executable)}
assay_bin={shlex.quote(str(fake))}
results={shlex.quote(str(results))}
run_root={shlex.quote(str(root))}
expected_version=5.5.1
cd {shlex.quote(str(decoy))}
run_published_release_opening_product
'''
        env = {**os.environ, "TEST_ROOT": str(root), "PATH": f"{bindir}:/usr/bin:/bin"}
        result = subprocess.run(
            ["bash", "-c", script], env=env, capture_output=True, text=True, timeout=15,
        )
        observed = []
        observed_path = root / "observed.jsonl"
        if observed_path.is_file():
            observed = [
                json.loads(line)
                for line in observed_path.read_text(encoding="utf-8").splitlines()
                if line
            ]
        recorded = []
        recorded_path = results / "commands.ndjson"
        if recorded_path.is_file():
            recorded = [
                json.loads(line)
                for line in recorded_path.read_text(encoding="utf-8").splitlines()
                if line
            ]
        config_path = ""
        config_text = ""
        config_is_cwd_eval = False
        if observed and len(observed[0].get("argv", [])) > 4:
            path = Path(observed[0]["argv"][4])
            config_path = str(path)
            config_is_cwd_eval = path == decoy / "eval.yaml"
            if path.is_file():
                config_text = path.read_text(encoding="utf-8")
        doctor = None
        doctor_path = results / "doctor.json"
        if doctor_path.is_file():
            try:
                doctor = json.loads(doctor_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                doctor = None
        init_scratch = root / "init-scratch"
        return {
            "returncode": result.returncode,
            "stderr": result.stderr,
            "observed": observed,
            "recorded": recorded,
            "config_path": config_path,
            "config_text": config_text,
            "config_is_cwd_eval": config_is_cwd_eval,
            "config_in_init": bool(config_path) and Path(config_path).is_relative_to(init_scratch),
            "doctor": doctor,
            "init_files": (init_scratch / "eval.yaml").is_file()
            and (init_scratch / "traces" / "hello.jsonl").is_file(),
        }


def probe_opening_doctor_invocation(opening: Path) -> list[str]:
    """Run the opening product function against a fake binary.

    The same existing --config path carries a checked report, a skipped
    report, and a wrong-schema report. Matching the helper name does not
    count: the opening function has to exec that argv and refuse the two
    reports that are not checked.
    """
    session_phase, session_test = _load_session_phase()
    if session_phase is None:
        return ["opening doctor probe could not load the shared assay fake"]
    problems: list[str] = []
    skipped = session_phase.doctor_report()
    wrong = session_phase.checked_doctor_report()
    wrong["schema"] = "other"
    for label, report in (("skipped", skipped), ("wrong-schema", wrong)):
        outcome = _run_opening_product(opening, session_test, report)
        if not _explicit_config_argv(outcome):
            problems.append(
                f"opening {label} case did not exec the explicit-config argv: {outcome['observed']}"
            )
            continue
        if outcome["returncode"] == 0:
            problems.append(
                f"opening caller accepted a {label} report on the explicit-config argv"
            )
        if any(row.get("argv", [None])[0] == "init" for row in outcome["observed"]):
            problems.append(f"opening caller continued to init after a {label} report")
    outcome = _run_opening_product(opening, session_test, session_phase.checked_doctor_report())
    if outcome["returncode"] != 0:
        return problems + [f"opening doctor invocation failed: {outcome['stderr'].strip()}"]
    observed = outcome["observed"]
    recorded = outcome["recorded"]
    if not observed:
        return problems + ["opening doctor invocation recorded no process argv"]
    if not recorded:
        return problems + ["opening doctor invocation did not retain commands.ndjson"]
    if [row["argv"][0] for row in observed] != ["doctor", "init"]:
        problems.append(f"opening caller order drifted: {observed}")
    if not _explicit_config_argv(outcome):
        problems.append(f"opening doctor argv is not the explicit-config command: {observed}")
    else:
        config_path = Path(outcome["config_path"])
        if config_path.name != "published-release-doctor-config.yaml":
            problems.append(f"opening doctor config is not the harness fixture: {config_path}")
        if not observed[0].get("config_exists"):
            problems.append(
                "opening probe cwd had no eval.yaml decoy, so an implicit config could not be distinguished"
            )
        if outcome["config_is_cwd_eval"]:
            problems.append("opening doctor used the cwd eval.yaml instead of the harness fixture")
        if outcome["config_text"] != session_phase.DOCTOR_HARNESS_FIXTURE:
            problems.append("opening doctor config bytes drifted from the pinned harness fixture")
        if outcome["config_in_init"]:
            problems.append("opening doctor config is inside the init scratch")
        if recorded[0].get("argv", [])[-1:] != [outcome["config_path"]]:
            problems.append(f"retained opening argv does not name the config: {recorded}")
    body = outcome["doctor"]
    if (
        not isinstance(body, dict)
        or body.get("schema") != "assay.doctor_report.v0"
        or body.get("config_check", {}).get("status") != "checked"
    ):
        problems.append(f"opening doctor report is not schema-checked success: {body}")
    if observed[-1]["argv"] != ["init", "--preset", "dev", "--hello-trace"]:
        problems.append(f"opening init argv drifted: {observed[-1:]}")
    if not outcome["init_files"]:
        problems.append("opening init did not leave a fresh eval.yaml and hello trace")
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
    parser.add_argument("--probe-opening-doctor", action="store_true")
    args = parser.parse_args()
    if args.probe_opening_doctor:
        problems = probe_opening_doctor_invocation(args.driver)
        if problems:
            for problem in problems:
                print(f"FAIL: {problem}")
            return 1
        print("ok: opening caller refuses skipped and wrong-schema reports on the explicit-config argv")
        return 0
    problems = validate_contract(args.workflow, args.driver)
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("ok: published-release platform-opening contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
