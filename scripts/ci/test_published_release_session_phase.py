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
                doctor_report() if report is None else report),
            "exit": doctor_exit,
        }))
        fake = bindir / "assay"
        fake.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, sys
root = pathlib.Path(os.environ["TEST_ROOT"])
with (root / "observed.jsonl").open("a") as handle:
    handle.write(json.dumps({"argv": sys.argv[1:], "cwd": str(pathlib.Path.cwd()),
                            "config_exists": pathlib.Path("eval.yaml").exists()}) + "\\n")
if sys.argv[1:] == ["doctor", "--format", "json"]:
    control = json.loads((root / "control.json").read_text())
    print(control["output"], end="")
    raise SystemExit(control["exit"])
if sys.argv[1:] == ["init", "--preset", "dev", "--hello-trace", "--format", "json"]:
    pathlib.Path("eval.yaml").write_text("created")
    print('{"schema":"assay.init_report.v0"}')
else:
    raise SystemExit(92)
''')
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
        self.assertEqual(json.loads((results / "doctor.json").read_text()), doctor_report())

    def test_doctor_executes_before_init_and_preserves_observations(self):
        self.assert_session_contract(self.run_phase())

    def test_missing_reordered_and_comment_only_preflight_are_detected(self):
        library = LIBRARY.read_text()
        start = library.index('  run_capture "doctor"')
        end = library.index('  run_capture "init"', start)
        doctor = library[start:end]
        missing = library[:start] + library[end:]
        head, tail = missing.rsplit("}\n", 1)
        variants = {
            "missing": missing,
            "after-init": head + doctor + "}\n" + tail,
            "comment-only": library[:start] + '  # assay doctor --format json\n' + library[end:],
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

    def run_recording(self, results, *, driver=None):
        # Execute the production encoder/retention block with synthetic preceding artifacts.
        # These placeholders are not release or attestation proof.
        for name in ("produced.bundle.tar.gz", "decisions.ndjson", "inspect.json",
                     "verify.json", "tamper-verify.json", "enforcement.sarif",
                     "release-api.json", "tag-ref.json"):
            (results / name).write_text("fixture")
        (results / "attestation-summary.json").write_text('{"assets":[]}')
        (results / "harness-files.json").write_text('{"files":[]}')
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


if __name__ == "__main__":
    unittest.main()
