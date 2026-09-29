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
# Byte pin of the harness fixture. A published v6.6.2 macOS-arm64 doctor
# accepted this document (config_check.status checked, exit 0) and rejected
# the same document with expected.type not_a_real_metric (failed, exit 2).
# This comparison does not execute that binary. Overall status Unsupported
# on that run is not an enforcement result.
DOCTOR_HARNESS_FIXTURE = """\
# harness-fixture-provenance: scripts/ci/lib/published-release-capture.sh
# Written by the published-release harness before doctor.
# This file is not created by assay init.
configVersion: 1
suite: "published_release_doctor_preflight"
model: "trace"
tests:
  - id: "published_release_doctor_regex"
    input:
      prompt: "hello_prompt"
    expected:
      type: regex_match
      pattern: "Hello\\\\s+Assay"
      flags: ["i"]
"""


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
    def test_literal_identity_refuses_missing_duplicate_wrong_argv_and_changed_bytes(self):
        import importlib.util
        def load(name):
            spec=importlib.util.spec_from_file_location(name,ROOT/'scripts/ci'/ (name+'.py'))
            module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
        subject, offline = load('published_release_proxy_phase'), load('published_release_offline_phase')
        with tempfile.TemporaryDirectory() as directory:
            results=Path(directory);root=results/'documented-route';root.mkdir()
            bundle=root/'acquisition/project/action.bundle.tar.gz';bundle.parent.mkdir(parents=True)
            cli=results/'cli';cli.write_bytes(b'owned fake executable')
            args=['evidence','verify-privileged-mcp-action',str(bundle),'--profile-version','v1','--format','json']
            connected={'name':'verify-documented-connected','exit_code':0,'argv':['assay',*args]}
            expected={'name':'verify-produced-bundle-offline','exit_code':0,'classification':'verified',
                      'argv':offline.isolation_argv(['assay',*args])}
            for kind in ('positive','missing','duplicate','executable','profile','bundle','connected','bytes'):
                with self.subTest(kind=kind):
                    (root/'input-identity.json').unlink(missing_ok=True)
                    bundle.write_bytes(b'first immutable input')
                    subject.record_literal_verification(results,cli,offline,'before')
                    operations=[json.loads(json.dumps(expected))]
                    connection=json.loads(json.dumps(connected))
                    if kind=='missing':operations=[]
                    if kind=='duplicate':operations*=2
                    if kind=='executable':operations[0]['argv'][-8]='other-executable'
                    if kind=='profile':operations[0]['argv'][-3]='v0'
                    if kind=='bundle':operations[0]['argv'][-5]=str(results/'produced.bundle.tar.gz')
                    if kind=='connected':connection['argv'][3]=str(results/'produced.bundle.tar.gz')
                    if kind=='bytes':bundle.write_bytes(b'changed input')
                    (results/'commands.ndjson').write_text(json.dumps(connection)+'\n')
                    (root/'offline-operations.ndjson').write_text(''.join(json.dumps(row)+'\n' for row in operations))
                    if kind=='positive':
                        subject.record_literal_verification(results,cli,offline,'after')
                        self.assertEqual(json.loads((root/'input-identity.json').read_text())['status'],'verified')
                    else:
                        with self.assertRaisesRegex(ValueError,'literal .*input'):
                            subject.record_literal_verification(results,cli,offline,'after')

    def test_literal_argv_compares_windows_paths_by_path_not_by_spelling(self):
        # Hosted run 36542544734: the Windows ledger recorded D:/a/... (bash) while the helper expected
        # D:\\a\\... (Path.cwd()); the same file failed a byte comparison.
        import importlib.util
        spec = importlib.util.spec_from_file_location('proxy_phase', ROOT/'scripts/ci/published_release_proxy_phase.py')
        subject = importlib.util.module_from_spec(spec); spec.loader.exec_module(subject)
        expected = ['assay', 'evidence', 'verify-privileged-mcp-action',
                    r'D:\a\_temp\run\results\documented-route\acquisition\project\action.bundle.tar.gz',
                    '--profile-version', 'v1', '--format', 'json']
        ledger = list(expected); ledger[3] = 'D:/a/_temp/run/results/documented-route/acquisition/project/action.bundle.tar.gz'
        self.assertTrue(subject.same_argv(ledger, expected, windows=True))
        for label, change in (('other file', (3, 'D:/a/_temp/run/results/produced.bundle.tar.gz')),
                              ('flag case', (4, '--Profile-Version')), ('profile', (5, 'v0')),
                              ('separator on one side only', (5, 'v1\\')),
                              ('executable', (0, 'assay2'))):
            with self.subTest(label=label):
                altered = list(ledger); altered[change[0]] = change[1]
                self.assertFalse(subject.same_argv(altered, expected, windows=True))
        self.assertFalse(subject.same_argv(ledger[:-1], expected, windows=True))
        self.assertFalse(subject.same_argv('not-a-list', expected, windows=True))
        # POSIX keeps exact spelling for everything, including paths.
        self.assertFalse(subject.same_argv(ledger, expected, windows=False))
        self.assertTrue(subject.same_argv(list(expected), expected, windows=False))

    def test_windows_literal_verification_accepts_other_separator_spelling_only(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('proxy_phase_w', ROOT/'scripts/ci/published_release_proxy_phase.py')
        subject = importlib.util.module_from_spec(spec); spec.loader.exec_module(subject)
        class Offline:
            @staticmethod
            def harness_binary():
                return r'C:\off\assay.exe'
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory); root = results/'documented-route'
            bundle = root/'acquisition/project/action.bundle.tar.gz'; bundle.parent.mkdir(parents=True)
            bundle.write_bytes(b'literal'); cli = results/'cli'; cli.write_bytes(b'cli')
            other = str(bundle).replace('/', '\\')  # same file, Windows separators
            args = ['evidence', 'verify-privileged-mcp-action', other, '--profile-version', 'v1', '--format', 'json']
            for label, connected_bundle in (('same file', other), ('different file', str(results/'produced.bundle.tar.gz'))):
                with self.subTest(label=label):
                    (root/'input-identity.json').unlink(missing_ok=True)
                    subject.record_literal_verification(results, cli, Offline, 'before', windows=True)
                    connected = ['assay', *args]; connected[3] = connected_bundle
                    (results/'commands.ndjson').write_text(json.dumps(
                        {'name': 'verify-documented-connected', 'exit_code': 0, 'argv': connected}) + '\n')
                    (root/'offline-operations.ndjson').write_text(json.dumps(
                        {'name': 'verify-produced-bundle-offline', 'exit_code': 0, 'classification': 'verified',
                         'argv': ['C:/off/assay.exe', *args]}) + '\n')
                    if label == 'same file':
                        try:
                            subject.record_literal_verification(results, cli, Offline, 'after', windows=True)
                        except ValueError as error:
                            self.fail('same file in Windows spelling was refused: ' + str(error))
                        self.assertEqual(json.loads((root/'input-identity.json').read_text())['status'], 'verified')
                    else:
                        with self.assertRaisesRegex(ValueError, 'literal connected verifier input identity differs'):
                            subject.record_literal_verification(results, cli, Offline, 'after', windows=True)

    def run_offline_consumer(self, driver=None, *, mismatch=False, target="x86_64-unknown-linux-gnu"):
        root = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        results = root / 'results'; results.mkdir()
        literal = results / 'documented-route/acquisition/project/action.bundle.tar.gz'
        literal.parent.mkdir(parents=True); literal.write_bytes(b'literal guide production')
        supplemental = results / 'produced.bundle.tar.gz'; supplemental.write_bytes(b'supplemental production')
        tools = root / 'bin'; tools.mkdir()
        cli = tools / 'assay'
        cli.write_text('#!' + sys.executable + '\n' + '''import hashlib,json,pathlib,sys
bundle=pathlib.Path(sys.argv[3])
print(json.dumps({'schema':'assay.privileged_mcp_action.verify.report.v0','profile':'privileged-mcp-action/v1',
'profile_selection':'explicit','input_profile':None,'input_profile_status':'undeclared_legacy',
'bundle_integrity':'pass','verdict':'valid','claims':{
'policy_decision_recorded':{'status':'confirmed','source_class':'producer_reported'},
'caller_visible_denial':{'status':'confirmed','source_class':'producer_reported'},
'upstream_delivery':{'status':'incomplete'},'external_side_effect':{'status':'incomplete'}},
'findings':[],'non_claims':['allow does not prove upstream delivery','deny does not establish maliciousness',
'caller-visible denial does not prove external side-effect absence','bundle integrity does not upgrade source class']}))
'''); cli.chmod(0o755)
        (results / 'verify.json').write_bytes(subprocess.check_output([str(cli), 'evidence', 'verify-privileged-mcp-action', str(supplemental)]))
        python = tools / 'python-fixture'
        python.write_text('#!' + sys.executable + '\n' + '''import json,os,pathlib,subprocess,sys
args=sys.argv[1:]
import importlib.util
from unittest import mock
def load(name):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(os.environ['CI_SOURCE'])/(name+'.py'))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
offline=load('published_release_offline_phase')
if args[-1] in ('--literal-input-before','--literal-input-after'):
    subject=load('published_release_proxy_phase')
    with mock.patch.object(offline,'harness_binary',return_value=os.environ['COPY']):
        subject.record_literal_verification(pathlib.Path(os.environ['results']),pathlib.Path(os.environ['assay_path']),offline,
            'before' if args[-1].endswith('before') else 'after',windows=os.environ['target'].endswith('windows-msvc'))
    sys.exit(0)
if args[0]=='-':sys.exit(subprocess.run([sys.executable,*args]).returncode)
if '--print-harness-binary' in args:
    print(os.environ['COPY']);sys.exit(0)
if args[-1] in ('--windows-copy-before','--windows-copy-after'):
    with open(os.environ['OBSERVED'], 'a') as stream:stream.write(json.dumps({'witness':args[-1]})+'\\n')
    sys.exit(0)
assert args[0]=='-I' and args[1].endswith('/published_release_offline_phase.py'),args
verifier=args[args.index('--')+1:]
with open(os.environ['OBSERVED'], 'a') as stream:stream.write(json.dumps({'cwd':str(pathlib.Path.cwd()),'argv':verifier})+'\\n')
value=subprocess.check_output(verifier)
if os.environ.get('MISMATCH')=='1' and pathlib.Path.cwd().name=='documented-route':value=b'wrong report\\n'
pathlib.Path('verify-offline.json').write_bytes(value)
pathlib.Path('offline-cleanup.json').write_text(json.dumps({'status':'clean'}))
pathlib.Path('offline-operations.ndjson').write_text(json.dumps({'name':'verify-produced-bundle-offline','classification':'verified','exit_code':0,'argv':verifier if os.environ['target'].endswith('windows-msvc') else offline.isolation_argv(verifier),'claim_ceiling':'synthetic'})+'\\n')
'''); python.chmod(0o755)
        text = DRIVER.read_text() if driver is None else driver
        start = text.index('\npreflight_offline_constructor\n')
        end = text.index('\nrun_published_release_extra_request_cases\n', start)
        setup = '''set -euo pipefail
fail() { echo "$*" >&2; exit 1; }
preflight_offline_constructor() { :; }
source "$CAPTURE_LIBRARY"
'''
        observed = root / 'observed.ndjson'
        env = {**os.environ, 'PATH':str(tools)+os.pathsep+os.environ['PATH'], 'results':str(results),
               'bundle':str(supplemental), 'target':target, 'assay_path':str(cli), 'COPY':str(root/'copy/assay'),
               'PYTHON_BIN':str(python), 'JQ_BIN':'jq', 'harness_root':str(root/'harness'),
               'OBSERVED':str(observed), 'MISMATCH':str(int(mismatch)), 'CI_SOURCE':str(ROOT/'scripts/ci'),
               'CAPTURE_LIBRARY':str(LIBRARY), 'commands_file':str(results/'commands.ndjson'), 'version':'6.9.0'}
        result = subprocess.run(['bash','-c',setup+text[start:end]],env=env,capture_output=True,text=True,timeout=20)
        rows = [json.loads(line) for line in observed.read_text().splitlines()] if observed.exists() else []
        return result, rows, literal, supplemental

    def test_windows_copy_witnesses_enclose_both_actual_offline_invocations(self):
        result, rows, literal, supplemental = self.run_offline_consumer(target='x86_64-pc-windows-msvc')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(rows[0], {'witness':'--windows-copy-before'})
        self.assertEqual(rows[-1], {'witness':'--windows-copy-after'})
        self.assertEqual([row['argv'][3] for row in rows[1:-1]], [str(supplemental),str(literal)])
        self.assertEqual(rows[1]['argv'][0],rows[2]['argv'][0])
        self.assertTrue(rows[1]['argv'][0].endswith('/copy/assay'))

    def test_literal_bundle_reaches_actual_offline_consumer_and_matches_connected(self):
        result, rows, literal, supplemental = self.run_offline_consumer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row['argv'][3] for row in rows], [str(supplemental), str(literal)])
        self.assertEqual(rows[1]['cwd'], str(literal.parents[2]))
        root = literal.parents[2]
        self.assertEqual((root/'verify.json').read_bytes(), (root/'verify-offline.json').read_bytes())
        altered = DRIVER.read_text().replace('"$literal_bundle" --profile-version v1 --format json)',
                                            '"$bundle" --profile-version v1 --format json)')
        self.assertNotEqual(altered, DRIVER.read_text())
        mutation, rows, literal, supplemental = self.run_offline_consumer(altered)
        self.assertNotEqual(mutation.returncode, 0)
        self.assertEqual(rows[-1]['argv'][3], str(supplemental))
        self.assertIn('literal offline verifier input identity differs', mutation.stderr)
        mismatch, _, _, _ = self.run_offline_consumer(mismatch=True)
        self.assertNotEqual(mismatch.returncode, 0)
        self.assertIn('documented offline verification output differs', mismatch.stderr)

    def run_phase(self, *, report=None, output=None, doctor_exit=0, library=None,
                  skip_linux_capabilities=False):
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
''' + ("published_release_skip_linux_capabilities=1\n" if skip_linux_capabilities else "") + '''
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
        self.assertEqual(config_path.read_text(encoding="utf-8"), DOCTOR_HARNESS_FIXTURE)
        self.assertEqual(recorded[0]["argv"][-1], str(config_path))
        self.assertEqual(recorded[0]["exit_code"], 0)
        report = json.loads((results / "doctor.json").read_text(encoding="utf-8"))
        self.assertEqual(report["schema"], "assay.doctor_report.v0")
        self.assertEqual(report["config_check"]["status"], "checked")
        self.assertEqual(report, checked_doctor_report())
        self.assertTrue((session / "eval.yaml").is_file())

    def test_doctor_executes_before_init_and_preserves_observations(self):
        self.assert_session_contract(self.run_phase())

    def test_darwin_retains_checked_doctor_without_linux_capability_conjuncts(self):
        report = checked_doctor_report()
        for key in ("landlock", "bpf_lsm", "helper"):
            report.pop(key)
        result, observed, _, results, _ = self.run_phase(
            report=report, skip_linux_capabilities=True
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([row["argv"][0] for row in observed], ["doctor", "init"])
        retained = json.loads((results / "doctor.json").read_text(encoding="utf-8"))
        self.assertEqual(retained["config_check"]["status"], "checked")
        self.assertNotIn("landlock", retained)

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

    def run_recording(self, results, *, driver=None, omit=None, target="x86_64-unknown-linux-gnu", continuity=None):
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
        for name in ("installer/receipt.json", "installer/default/executed-install.sh",
                     "installer/signed/executed-install.sh", "installer/contrasts.json"):
            path = results / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('{"status":"completed"}' if name.endswith('receipt.json') else 'fixture')
        for name in ("documented-route/receipt.json", "documented-route/acquisition/project/action.bundle.tar.gz",
                     "documented-route/verify.json", "documented-route/verify-offline.json",
                     "documented-route/offline-operations.ndjson", "documented-route/input-identity.json"):
            path = results / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('{"status":"completed"}' if name.endswith('receipt.json') else 'fixture')
        if target != "x86_64-pc-windows-msvc":
            (results / 'server-install.json').write_text('{"source_kind":"synthetic fixture"}')
        if target == "x86_64-pc-windows-msvc":
            for name in ("verify-offline.json", "offline-operations.ndjson", "offline-cleanup.json", "offline-claim-ceiling.txt"):
                (results / name).write_text('synthetic Windows retention fixture')
            if continuity is None:
                continuity = {phase: {key: 'a' * 64 for key in ('source_sha256', 'destination_sha256')}
                              for phase in ('before', 'after')}
            (results / 'offline-binary-continuity.json').write_text(json.dumps(continuity))
            (results / 'documented-route/offline-cleanup.json').write_text('{"status":"clean"}')
        if omit:
            (results / omit).unlink()
        for directory, suffix in (("release-assets", ".tar.gz"), ("attestation-raw", ".json")):
            folder = results / directory
            folder.mkdir(exist_ok=True)
            for name in ("cli",):
                (folder / (name + suffix)).write_text("fixture")
        (results / "journey-target.txt").write_text(target)
        (results / "journey-platform-claim.txt").write_text("Linux x86_64")
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

    def test_installer_artifacts_cannot_be_omitted(self):
        _, _, _, results, _ = self.run_phase()
        for name in ("installer/receipt.json", "installer/default/executed-install.sh",
                     "installer/signed/executed-install.sh", "installer/contrasts.json"):
            with self.subTest(name=name):
                complete = self.run_recording(results)
                self.assertEqual(complete.returncode, 0, complete.stderr)
                missing = self.run_recording(results, omit=name)
                self.assertNotEqual(missing.returncode, 0)
                self.assertIn("required retained artifact is missing or empty: " + name, missing.stderr)

    def test_windows_continuity_receipt_cannot_be_omitted_or_disagree(self):
        _, _, _, results, _ = self.run_phase()
        target = 'x86_64-pc-windows-msvc'
        positive = self.run_recording(results, target=target)
        self.assertEqual(positive.returncode, 0, positive.stderr)
        missing_cleanup = self.run_recording(results, target=target, omit='documented-route/offline-cleanup.json')
        self.assertNotEqual(missing_cleanup.returncode, 0)
        self.assertIn('documented-route/offline-cleanup.json', missing_cleanup.stderr)
        missing = self.run_recording(results, target=target, omit='offline-binary-continuity.json')
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn('offline-binary-continuity.json', missing.stderr)
        for phase in ('before', 'after'):
            for key in ('source_sha256', 'destination_sha256'):
                witness = {p: {k: 'a' * 64 for k in ('source_sha256', 'destination_sha256')}
                           for p in ('before', 'after')}
                witness[phase][key] = 'b' * 64
                mismatch = self.run_recording(results, target=target, continuity=witness)
                self.assertNotEqual(mismatch.returncode, 0)
                self.assertIn('Windows offline binary continuity is incomplete', mismatch.stderr)

    def test_documented_route_artifacts_are_mandatory(self):
        _, _, _, results, _ = self.run_phase()
        for name in ("documented-route/receipt.json", "documented-route/acquisition/project/action.bundle.tar.gz",
                     "documented-route/verify.json", "documented-route/verify-offline.json",
                     "documented-route/offline-operations.ndjson", "documented-route/input-identity.json"):
            with self.subTest(name=name):
                complete = self.run_recording(results)
                self.assertEqual(complete.returncode, 0, complete.stderr)
                missing = self.run_recording(results, omit=name)
                self.assertNotEqual(missing.returncode, 0)
                self.assertIn(name, missing.stderr)

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
