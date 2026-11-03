#!/usr/bin/env python3
"""Exercise the actual release-driver diagnostic callsite without downloading binaries."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / 'scripts/ci/published-release-golden-path.sh'
LIBRARY = ROOT / 'scripts/ci/lib/published-release-capture.sh'

# The process double recognizes the inputs independently of the harness validators.
# This tests the harness, not the shipped binaries; published verification is separate.
FAKE = '''import json,os,pathlib,sys
a=sys.argv[1:]; root=pathlib.Path(os.environ['TEST_ROOT']); case=''; status=0; channel=sys.stdout
contract=json.loads(pathlib.Path(os.environ['JOURNEY_CONTRACT']).read_text())
def outcome(step,name):
    return next(o for s in contract['steps'] if s['id']==step for o in s['outcomes'] if o['name']==name)
if a[0]=='doctor':
    assert a[:4]==['doctor','--format','json','--config'] and len(a)==5,a
    assert not pathlib.Path(a[4]).exists()
    case='doctor-missing-config'; status=2
    report={'schema':'assay.doctor_report.v0','config_check':{'status':'failed'},'reason_code':outcome('preflight','missing-config')['reason_code'],'next_step':outcome('preflight','missing-config')['next_step']}
elif a[:2]==['policy','validate']:
    assert a[2]=='--input' and a[4:]==['--format','json'],a
    path=pathlib.Path(a[3]); assert path.is_file()
    if path.read_text()=='caller: [\\n':
        case='policy-malformed'; status=2
        report={'schema':'assay.run_summary.v1','reason_code':outcome('policy-validation','malformed')['reason_code'],'next_step':outcome('policy-validation','malformed')['next_step'].replace('<policy>',str(path))}
    else: report={'schema':'assay.run_summary.v1'}
elif a[0]=='proxy-enforce':
    assert pathlib.Path(a[a.index('--enforce-policy')+1]).read_text()==pathlib.Path(os.environ['VALID_POLICY']).read_text()
    assert not pathlib.Path(a[a.index('--declared-mcp-manifest')+1]).exists()
    assert sys.stdin.read()==''
    case='proxy-missing-manifest'; status=1; channel=sys.stderr
    report={'event':'startup_failure','reason_code':'proxy_declared_manifest_invalid','next_step':'Check --declared-mcp-manifest and retry proxy-enforce.'}
elif a[0]=='run': report={'schema':'assay.run_report.v1'}
else: raise AssertionError(a)
with (root/'observed.jsonl').open('a') as f:f.write(json.dumps({'case':case,'argv':a,'binary':pathlib.Path(sys.argv[0]).name})+'\\n')
fault=os.environ.get('FAULT','') if case==os.environ.get('FAULT_CASE') else ''
if fault=='exit':status=0
if fault=='reason':report['reason_code']='WRONG_REASON'
if fault=='remediation':report['next_step']=''
if fault=='identity':report['event' if case.startswith('proxy') else 'schema']='wrong'
if fault=='channel':channel=sys.stdout if channel is sys.stderr else sys.stderr
if fault=='config-status':report['config_check']={'status':'checked'}
print(json.dumps(report),file=channel)
if case=='proxy-missing-manifest':print('assay-mcp-server: read declared manifest failed',file=sys.stderr)
if fault=='duplicate':print(json.dumps(report),file=channel)
if fault=='stdout' and case.startswith('proxy'):print('unexpected protocol response')
sys.exit(status)
'''


class DiagnosticJourneyTests(unittest.TestCase):
    def run_journey(self, *, fault='', case='', driver=None):
        root = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        results = root / 'results'; results.mkdir()
        session = root / 'session'; session.mkdir()
        (session / 'policy.yaml').write_text('valid control')
        tools = root / 'bin'; tools.mkdir()
        for binary in ('assay', 'assay-mcp-server'):
            path = tools / binary
            path.write_text('#!' + sys.executable + '\n' + FAKE)
            path.chmod(0o755)
        text = DRIVER.read_text() if driver is None else driver
        start = text.index('pushd "$session_root" >/dev/null')
        end = text.index('popd >/dev/null', start) + len('popd >/dev/null')
        script = ('set -euo pipefail\nfail() { echo "$*" >&2; exit 1; }\nsource '
                  + shlex.quote(str(LIBRARY)) + '\n'
                  # Existing doctor/init success phase is outside this test's scope.
                  + 'run_published_release_session_product() { :; }\n' + text[start:end])
        env = {**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH'],
               'JOURNEY_CONTRACT': str(ROOT / 'docs/generated/agent-golden-path.json'),
               'TEST_ROOT': str(root), 'results': str(results), 'session_root': str(session),
               'harness_root': str(ROOT), 'PYTHON_BIN': sys.executable, 'JQ_BIN': 'jq',
               'commands_file': str(results / 'commands.ndjson'),
               'VALID_POLICY': str(ROOT / 'examples/privileged-action-gate/policies/no-allowance.yaml'),
               'FAULT': fault, 'FAULT_CASE': case}
        result = subprocess.run(['bash', '-c', script], env=env, text=True,
                                capture_output=True, timeout=15)
        observed = [json.loads(line) for line in (root / 'observed.jsonl').read_text().splitlines()]
        return result, results, observed

    def assert_cases(self, result, results, observed):
        self.assertEqual(result.returncode, 0, result.stderr)
        cases = [row for row in observed if row['case']]
        self.assertEqual([row['case'] for row in cases],
                         ['doctor-missing-config', 'policy-malformed', 'proxy-missing-manifest'],
                         'release driver did not execute the three diagnostic cases')
        self.assertEqual([row['binary'] for row in cases], ['assay', 'assay', 'assay-mcp-server'])
        records = [json.loads(line) for line in (results / 'commands.ndjson').read_text().splitlines()]
        for row, expected_exit in zip(cases, (2, 2, 1)):
            matches = [item for item in records if item['name'] == row['case']]
            self.assertEqual(len(matches), 1)
            self.assertEqual(matches[0]['argv'][1:], row['argv'])
            self.assertEqual(matches[0]['exit_code'], expected_exit)
            for suffix in ('stdout', 'stderr'):
                self.assertTrue((results / 'diagnostics' / (row['case'] + '.' + suffix)).is_file())

    def test_driver_executes_and_retains_three_diagnostic_cases(self):
        self.assert_cases(*self.run_journey())

    def test_omitted_or_commented_callsite_cannot_satisfy_the_journey(self):
        text = DRIVER.read_text()
        call = 'run_published_release_diagnostic_cases\n'
        self.assert_cases(*self.run_journey(driver=text + '\n# no-op control\n'))
        for replacement in ('', '# ' + call):
            with self.subTest(replacement=replacement):
                phase = self.run_journey(driver=text.replace(call, replacement))
                self.assertEqual(phase[0].returncode, 0, phase[0].stderr)
                with self.assertRaisesRegex(AssertionError, 'three diagnostic cases'):
                    self.assert_cases(*phase)

    def test_wrong_diagnostics_fail_the_release_journey(self):
        for case in ('doctor-missing-config', 'policy-malformed', 'proxy-missing-manifest'):
            for fault in ('exit', 'reason', 'remediation', 'identity', 'channel', 'duplicate'):
                with self.subTest(case=case, fault=fault):
                    result, _, observed = self.run_journey(case=case, fault=fault)
                    self.assertIn(case, [row['case'] for row in observed])
                    self.assertNotEqual(result.returncode, 0, 'release journey accepted ' + fault)
        for case, fault in (('doctor-missing-config', 'config-status'), ('proxy-missing-manifest', 'stdout')):
            with self.subTest(case=case, fault=fault):
                self.assertNotEqual(self.run_journey(case=case, fault=fault)[0].returncode, 0)


if __name__ == '__main__':
    unittest.main()
