#!/usr/bin/env python3
"""Drive the release session with a CLI-observed invocation oracle, without downloads."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
LIBRARY = ROOT / "scripts/ci/lib/published-release-capture.sh"
DRIVER = ROOT / "scripts/ci/published-release-golden-path.sh"


DOCTOR_CONFIG_NAME = "published-release-doctor-config.yaml"
PROVENANCE = "harness-fixture-provenance: scripts/ci/lib/published-release-capture.sh"


def doctor_report() -> dict:
    return {
        "schema": "assay.doctor_report.v0",
        "assay_version": "5.5.1",
        "platform": "linux x86_64",
        "status": "Degraded",
        "backend": {"selected": "Landlock", "mode": "Enforcement", "reason": "probe"},
        "config_check": {"status": "skipped", "reason": "fresh project"},
        "landlock": {"available": True, "fs_enforce": True, "net_enforce": True,
                     "abi_probe_status": "ok", "net_connect_ruleset_probe": "usable"},
        "bpf_lsm": {"available": True},
        "helper": {"exists": False, "socket_exists": False},
        "sandbox_features": {"env_scrubbing": True, "scoped_tmp": True,
                             "fork_safe_preexec": True, "deny_conflict_detection": True},
    }


def checked_doctor_report() -> dict:
    report = doctor_report()
    report["config_check"] = {"status": "checked"}
    return report


def fake_assay_main() -> int:
    """Stand-in for the published binary at the process boundary.

    No `--config` is the documented exit-0 skipped outcome. An explicit path
    that exists yields the control document. A missing explicit path is exit 2
    with `config_check.status == failed`.
    """
    root = Path(os.environ["TEST_ROOT"])
    argv = sys.argv[1:]
    config_arg = None
    if "--config" in argv:
        index = argv.index("--config")
        if index + 1 < len(argv):
            config_arg = argv[index + 1]
    explicit_exists = bool(config_arg) and Path(config_arg).is_file()
    with (root / "observed.jsonl").open("a", encoding="utf-8") as handle:
        handle.write(json.dumps({
            "argv": argv,
            "cwd": str(Path.cwd()),
            "config_exists": Path("eval.yaml").exists(),
            "explicit_config": config_arg,
            "explicit_config_exists": explicit_exists,
        }) + "\n")
    if argv == ["doctor", "--format", "json"]:
        print(json.dumps(doctor_report()), end="")
        return 0
    if argv[:4] == ["doctor", "--format", "json", "--config"] and len(argv) == 5:
        if not explicit_exists:
            failed = doctor_report()
            failed["config_check"] = {"status": "failed"}
            print(json.dumps(failed), end="")
            return 2
        control = json.loads((root / "control.json").read_text(encoding="utf-8"))
        print(control["output"], end="")
        return int(control["exit"])
    if argv == ["init", "--preset", "dev", "--hello-trace", "--format", "json"]:
        Path("eval.yaml").write_text("created", encoding="utf-8")
        print('{"schema":"assay.init_report.v0"}')
        return 0
    if argv == ["init", "--preset", "dev", "--hello-trace"]:
        Path("eval.yaml").write_text("created", encoding="utf-8")
        Path("traces").mkdir(exist_ok=True)
        Path("traces/hello.jsonl").write_text("{}\n", encoding="utf-8")
        print("Next: assay validate --config=eval.yaml --trace-file=traces/hello.jsonl --format json")
        return 0
    return 92


class PublishedReleaseSessionTests(unittest.TestCase):
    def run_phase(self, *, report=None, output=None, doctor_exit=0, library=None):
        root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="session phase ")))
        results = root / "results"
        results.mkdir()
        session = root / "session"
        session.mkdir()
        bindir = root / "bin"
        bindir.mkdir()
        (root / "control.json").write_text(json.dumps({
            "output": output if output is not None else json.dumps(
                checked_doctor_report() if report is None else report),
            "exit": doctor_exit,
        }))
        fake = bindir / "assay"
        module = str(Path(__file__).resolve())
        fake.write_text(
            f"#!{sys.executable}\n"
            "import os, runpy, sys\n"
            "os.environ['PUBLISHED_RELEASE_ASSAY_FAKE'] = '1'\n"
            f"raise SystemExit(runpy.run_path({module!r}, run_name='__main__'))\n"
        )
        fake.chmod(0o755)
        source = root / "capture.sh"
        source.write_text(LIBRARY.read_text() if library is None else library)
        script = '''set -euo pipefail
fail() { echo "FAIL: $*" >&2; exit 1; }
PYTHON_BIN=''' + shlex.quote(sys.executable) + '''
JQ_BIN=/usr/bin/jq
version=5.5.1
results="$TEST_ROOT/results"
session_root="$TEST_ROOT/session"
commands_file="$results/commands.ndjson"
: > "$commands_file"
source ''' + shlex.quote(str(source)) + '''
cd "$session_root"
run_published_release_session_product
'''
        env = {**os.environ, "TEST_ROOT": str(root), "PATH": f"{bindir}:/usr/bin:/bin"}
        result = subprocess.run(["bash", "-c", script], env=env, capture_output=True,
                                text=True, timeout=15)
        observed = [json.loads(line) for line in (root / "observed.jsonl").read_text().splitlines()]
        recorded = [json.loads(line) for line in (results / "commands.ndjson").read_text().splitlines()]
        return result, observed, recorded, results, session

    def assert_session_contract(self, phase):
        result, observed, recorded, results, session = phase
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row["argv"][0] for row in observed], ["doctor", "init"])
        self.assertEqual([row["name"] for row in recorded], ["doctor", "init"])
        self.assertEqual([row["argv"][1:] for row in recorded], [row["argv"] for row in observed])
        self.assertTrue(all(row["cwd"] == str(session.resolve()) for row in observed))
        self.assertTrue(all(not row["config_exists"] for row in observed))
        doctor_argv = observed[0]["argv"]
        self.assertEqual(doctor_argv[:4], ["doctor", "--format", "json", "--config"])
        config_path = Path(doctor_argv[4])
        self.assertEqual(config_path.name, DOCTOR_CONFIG_NAME)
        self.assertNotEqual(config_path, session / "eval.yaml")
        self.assertTrue(observed[0]["explicit_config_exists"], config_path)
        self.assertIn(PROVENANCE, config_path.read_text(encoding="utf-8"))
        self.assertIn("not created by assay init", config_path.read_text(encoding="utf-8"))
        self.assertEqual(recorded[0]["argv"][-1], str(config_path))
        self.assertEqual(recorded[0]["exit_code"], 0)
        report = json.loads((results / "doctor.json").read_text(encoding="utf-8"))
        self.assertEqual(report["schema"], "assay.doctor_report.v0")
        self.assertEqual(report["config_check"]["status"], "checked")
        self.assertEqual(report, checked_doctor_report())
        self.assertTrue((session / "eval.yaml").is_file())

    def test_doctor_executes_before_init_and_preserves_observations(self):
        self.assert_session_contract(self.run_phase())

    def test_missing_reordered_and_comment_only_preflight_are_detected(self):
        library = LIBRARY.read_text()
        start = library.index('  run_published_release_doctor ')
        end = library.index('  run_capture "init"', start)
        doctor = library[start:end]
        missing = library[:start] + library[end:]
        head, tail = missing.rsplit("}\n", 1)
        variants = {
            "missing": missing,
            "after-init": head + doctor + "}\n" + tail,
            "comment-only": library[:start] + '  # assay doctor --format json --config "$config_path"\n' + library[end:],
        }
        self.assert_session_contract(self.run_phase(library=library + "\n# unchanged control\n"))
        for name, changed in variants.items():
            with self.subTest(name=name):
                phase = self.run_phase(library=changed)
                self.assertEqual(phase[0].returncode, 0, phase[0].stderr)
                with self.assertRaises(AssertionError):
                    self.assert_session_contract(phase)

    def test_invalid_or_empty_doctor_output_stops_before_init(self):
        for output in ("", "{", "{}", "null", "[]", json.dumps(doctor_report()) * 2):
            with self.subTest(output=output):
                result, observed, _, _, _ = self.run_phase(output=output)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual([row["argv"][0] for row in observed], ["doctor"])

    def run_recording(self, results, *, driver=None, omit=None):
        # Execute the production encoder/retention block with synthetic preceding artifacts.
        # These placeholders are not release or attestation proof.
        for name in ("produced.bundle.tar.gz", "decisions.ndjson", "inspect.json",
                     "verify.json", "tamper-verify.json", "enforcement.sarif",
                     "release-api.json", "tag-ref.json"):
            (results / name).write_text("fixture")
        (results / "attestation-summary.json").write_text('{"assets":[]}')
        (results / "harness-files.json").write_text('{"files":[]}')
        for name in ("allow/proxy.jsonl", "allow/decisions.ndjson", "allow/produced.bundle.tar.gz",
                     "allow/verify.json", "unsupported/proxy.jsonl"):
            path = results / name
            path.parent.mkdir(exist_ok=True)
            path.write_text("fixture")
        if omit:
            (results / omit).unlink()
        for directory, suffix in (("release-assets", ".tar.gz"), ("attestation-raw", ".json")):
            folder = results / directory
            folder.mkdir(exist_ok=True)
            for name in ("cli", "mcp"):
                (folder / (name + suffix)).write_text("fixture")
        driver = DRIVER.read_text() if driver is None else driver
        start = driver.index('"$PYTHON_BIN" - "$release_tag" "$source_digest"')
        end = driver.index('echo "PASS: published release', start)
        env = {**os.environ, "PYTHON_BIN": sys.executable, "results": str(results),
               "commands_file": str(results / "commands.ndjson"), "release_tag": "v5.5.1",
               "source_digest": "a" * 40, "harness_sha": "b" * 40, "workflow_run_id": "123",
               "workflow_run_attempt": "1", "driver_digest": "c" * 64,
               "harness_manifest_digest": "d" * 64}
        return subprocess.run(["bash", "-euc", driver[start:end]], env=env,
                              capture_output=True, text=True, timeout=15)

    def test_request_case_artifacts_are_required_and_content_hashed(self):
        _, _, _, results, _ = self.run_phase()
        for name in ("allow/proxy.jsonl", "allow/decisions.ndjson", "allow/produced.bundle.tar.gz",
                     "allow/verify.json", "unsupported/proxy.jsonl"):
            with self.subTest(name=name):
                complete = self.run_recording(results)
                self.assertEqual(complete.returncode, 0, complete.stderr)
                rows = json.loads((results / "retained-artifacts.json").read_text())["files"]
                row = next(row for row in rows if row["path"] == name)
                self.assertEqual(row["sha256"], hashlib.sha256((results / name).read_bytes()).hexdigest())
                missing = self.run_recording(results, omit=name)
                self.assertNotEqual(missing.returncode, 0)
                self.assertIn("required retained artifact is missing or empty: " + name, missing.stderr)

    def test_doctor_is_required_retained_and_content_hashed(self):
        result, _, _, results, _ = self.run_phase()
        self.assertEqual(result.returncode, 0, result.stderr)
        recorded = self.run_recording(results)
        self.assertEqual(recorded.returncode, 0, recorded.stderr)
        rows = json.loads((results / "retained-artifacts.json").read_text())["files"]
        row = next(row for row in rows if row["path"] == "doctor.json")
        self.assertEqual(row["sha256"], hashlib.sha256((results / "doctor.json").read_bytes()).hexdigest())
        for remove in (False, True):
            with self.subTest(remove=remove):
                if remove:
                    (results / "doctor.json").unlink()
                else:
                    (results / "doctor.json").write_text("")
                failed = self.run_recording(results)
                self.assertNotEqual(failed.returncode, 0)
                self.assertIn("required retained artifact is missing or empty: doctor.json", failed.stderr)

    def test_capability_true_does_not_become_an_executed_enforcement_claim(self):
        result, _, _, results, _ = self.run_phase()
        self.assertEqual(result.returncode, 0, result.stderr)
        recorded = self.run_recording(results)
        self.assertEqual(recorded.returncode, 0, recorded.stderr)
        pin = json.loads((results / "run-pin.json").read_text())
        self.assertIn("Doctor reports host capabilities, not kernel enforcement performed by this journey.",
                      pin["claim_ceiling"])
        self.assertEqual([row["name"] for row in pin["commands"]], ["doctor", "init"])

    def test_skipped_or_failed_explicit_report_stops_before_init(self):
        for status in ("skipped", "failed"):
            with self.subTest(status=status):
                report = checked_doctor_report()
                if status == "skipped":
                    report["config_check"] = {"status": "skipped", "reason": "fresh project"}
                else:
                    report["config_check"] = {"status": "failed"}
                result, observed, _, _, _ = self.run_phase(report=report)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertEqual([row["argv"][0] for row in observed], ["doctor"])

    def test_missing_explicit_config_stops_before_init(self):
        root = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="session phase ")))
        results = root / "results"
        results.mkdir()
        bindir = root / "bin"
        bindir.mkdir()
        module = str(Path(__file__).resolve())
        fake = bindir / "assay"
        fake.write_text(
            f"#!{sys.executable}\n"
            "import os, runpy\n"
            "os.environ['PUBLISHED_RELEASE_ASSAY_FAKE'] = '1'\n"
            f"raise SystemExit(runpy.run_path({module!r}, run_name='__main__'))\n"
        )
        fake.chmod(0o755)
        missing = root / "absent-doctor-config.yaml"
        script = '''set -euo pipefail
fail() { echo "FAIL: $*" >&2; exit 1; }
PYTHON_BIN=''' + shlex.quote(sys.executable) + '''
results="$TEST_ROOT/results"
commands_file="$results/commands.ndjson"
: > "$commands_file"
source ''' + shlex.quote(str(LIBRARY)) + '''
run_published_release_doctor assay ''' + shlex.quote(str(missing)) + '''
'''
        env = {**os.environ, "TEST_ROOT": str(root), "PATH": f"{bindir}:/usr/bin:/bin"}
        result = subprocess.run(["bash", "-c", script], env=env, capture_output=True,
                                text=True, timeout=15)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("doctor config fixture is missing", result.stderr)
        observed = (root / "observed.jsonl").read_text(encoding="utf-8") if (root / "observed.jsonl").exists() else ""
        self.assertEqual(observed, "")

    def test_no_config_argv_is_not_accepted_as_success(self):
        library = LIBRARY.read_text(encoding="utf-8")
        old = '"$assay_cmd" doctor --format json --config "$config_path"'
        replacement = '"$assay_cmd" doctor --format json'
        self.assertEqual(library.count(old), 1)
        result, observed, _, results, _ = self.run_phase(library=library.replace(old, replacement, 1))
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row["argv"][0] for row in observed], ["doctor"])
        self.assertEqual(observed[0]["argv"], ["doctor", "--format", "json"])
        report = json.loads((results / "doctor.json").read_text(encoding="utf-8"))
        self.assertEqual(report["schema"], "assay.doctor_report.v0")
        self.assertEqual(report["config_check"]["status"], "skipped")

    def test_nonzero_doctor_with_valid_json_stops_before_init(self):
        result, observed, recorded, _, _ = self.run_phase(doctor_exit=2)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual([row["argv"][0] for row in observed], ["doctor"])
        self.assertEqual(recorded[0]["exit_code"], 2)

    def test_wrong_identity_or_mistyped_capability_is_not_a_preflight(self):
        changes = (("schema", "other"), ("assay_version", "0.0.0"),
                   ("status", 1), ("landlock", {}), ("bpf_lsm", {"available": "false"}),
                   ("backend", {"selected": "Landlock", "mode": False}),
                   ("config_check", {"status": "valid"}))
        for key, value in changes:
            with self.subTest(key=key):
                report = doctor_report()
                report[key] = value
                result, observed, _, _, _ = self.run_phase(report=report)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual([row["argv"][0] for row in observed], ["doctor"])


if os.environ.get("PUBLISHED_RELEASE_ASSAY_FAKE") == "1":
    raise SystemExit(fake_assay_main())


if __name__ == "__main__":
    unittest.main()
