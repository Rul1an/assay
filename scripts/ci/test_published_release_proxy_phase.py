#!/usr/bin/env python3
"""Behavioral tests for the published-release proxy phase."""

from __future__ import annotations

import json
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/published_release_proxy_phase.py"
WINDOWS_LAUNCHER = ROOT / "scripts/ci/published_release_offline_windows.py"


def load_helper():
    spec = importlib.util.spec_from_file_location("published_release_proxy_phase", HELPER)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(HELPER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_windows_launcher():
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", WINDOWS_LAUNCHER)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(WINDOWS_LAUNCHER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def local_archive(path, size=2):
    import io, tarfile
    with tarfile.open(path, 'w:gz', compresslevel=0) as stream:
        member=tarfile.TarInfo('payload');member.size=size
        stream.addfile(member, io.BytesIO(b'x'*size))


class DocumentedRouteTests(unittest.TestCase):
    def test_acquired_archive_is_admitted_before_opening(self):
        import io, tarfile
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reference, acquired = root/'reference.tar.gz', root/'acquired.tar.gz'
            with tarfile.open(reference, 'w:gz') as handle:
                member=tarfile.TarInfo('payload');member.size=2;handle.addfile(member,io.BytesIO(b'ok'))
            acquired.write_bytes(reference.read_bytes())
            subject.admit_documented_archive(acquired, reference, root/'bounded')
            self.assertEqual((root/'bounded/payload').read_bytes(), b'ok')
            acquired.write_bytes(b'wrong live response')
            with self.assertRaisesRegex(ValueError, 'differs'):
                subject.admit_documented_archive(acquired, reference, root/'wrong')
            self.assertFalse((root/'wrong').exists())
            with tarfile.open(reference, 'w:gz') as handle:
                for index in range(33):
                    member=tarfile.TarInfo(str(index));member.size=0;handle.addfile(member)
            acquired.write_bytes(reference.read_bytes())
            with self.assertRaises(ValueError):
                subject.admit_documented_archive(acquired, reference, root/'many')
            self.assertFalse((root/'many').exists())
            with acquired.open('wb') as stream:
                stream.truncate(134217729)
            with self.assertRaisesRegex(ValueError, 'compressed ceiling'):
                subject.admit_documented_archive(acquired, reference, root/'compressed')
            self.assertFalse((root/'compressed').exists())
            import gzip
            for label, member in [('decoded', tarfile.TarInfo('oversize')), ('unsafe', tarfile.TarInfo('../escape'))]:
                member.size=134217729 if label=='decoded' else 0
                reference.write_bytes(gzip.compress(member.tobuf()+b'\0'*1024))
                acquired.write_bytes(reference.read_bytes())
                with self.assertRaises(ValueError):
                    subject.admit_documented_archive(acquired, reference, root/label)
                self.assertFalse((root/label).exists())
            self.assertFalse((root/'escape').exists())


    def test_actual_guide_fences_run_offline_with_the_same_cli_and_default_import(self):
        import hashlib
        import tarfile
        import platform
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            binary_dir = root / 'bin'; binary_dir.mkdir()
            calls = root / 'cli-calls.ndjson'
            cli = binary_dir / 'assay'
            cli.write_text('#!' + sys.executable + '\n' + '''import json, pathlib, sys
args = sys.argv[1:]
with open(CALLS, 'a') as stream:
    stream.write(json.dumps(args) + '\\n')
if args == ['version']:
    print('6.9.0')
elif args[0] == 'init':
    pathlib.Path('eval.yaml').write_text('local fixture')
    pathlib.Path('policy.yaml').write_text('local fixture')
    pathlib.Path('traces').mkdir()
    pathlib.Path('traces/hello.jsonl').write_text('local fixture')
elif args[:2] == ['evidence', 'import']:
    assert '--run-id' not in args and '--import-time' not in args
    pathlib.Path(args[args.index('--bundle-out') + 1]).write_text('fake produced bundle')
elif args[:2] == ['evidence', 'verify-privileged-mcp-action']:
    assert args[args.index('--profile-version') + 1] == 'v1'
    print(json.dumps({'bundle_integrity': 'pass', 'verdict': 'valid'}))
else:
    print('{}')
'''.replace('CALLS', repr(str(calls))))
            cli.chmod(0o755)
            system = platform.system()
            machine = platform.machine()
            target = ('aarch64' if machine in ('arm64', 'aarch64') else 'x86_64') + (
                '-apple-darwin' if system == 'Darwin' else '-unknown-linux-gnu')
            name = 'assay-v6.9.0-' + target
            payload = root / name; payload.mkdir()
            server = payload / 'assay-mcp-server'
            server.write_text('#!' + sys.executable + '\n' + '''import json,pathlib,sys
args=sys.argv[1:]
if args == ['--version']:
    print('assay-mcp-server 6.9.0')
elif args[0] == 'enforcement-sarif':
    pathlib.Path(args[args.index('--output')+1]).write_text(json.dumps({'version':'2.1.0'}))
else:
    assert args[0] == 'proxy-enforce'
    for flag in ('--enforcement-decision-out','--denied-call-observation-out'):
        pathlib.Path(args[args.index(flag)+1]).write_text('real fake-child record\\n')
    for line in sys.stdin:
        if json.loads(line).get('id') == 9:
            print(json.dumps({'id':9,'error':{'code':-31999,'data':{'reason':'no_declared_allowance'}}}),flush=True)
''')
            server.chmod(0o755)
            example = payload / 'packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate'
            (example / 'policies').mkdir(parents=True)
            for file in ('mock_github_mcp.py', 'baseline-approved.json', 'policies/no-allowance.yaml'):
                (example / file).write_text('opaque local fixture')
            archive = root / (name + '.tar.gz')
            with tarfile.open(archive, 'w:gz') as stream:
                stream.add(payload, arcname=name)
            python = binary_dir / 'python3'
            python.write_text('#!' + sys.executable + '\n' + '''import hashlib,io,pathlib,runpy,sys,urllib.request,os
if sys.argv[1]=='-c':os.execv(sys.executable,[sys.executable,*sys.argv[1:]])
source=pathlib.Path(SOURCE)
def response(request, timeout):
    url=request.full_url
    assert url.startswith('https://github.com/Rul1an/assay/releases/download/v6.9.0/'),url
    data=(hashlib.sha256(source.read_bytes()).hexdigest()+'  '+source.name+'\\n').encode() if url.endswith('.sha256') else source.read_bytes()
    result=io.BytesIO(data);result.headers={'Content-Length':str(len(data))};return result
urllib.request.urlopen=response
sys.argv=sys.argv[1:]
runpy.run_path(sys.argv[0],run_name='__main__')
'''.replace('SOURCE', repr(str(archive))))
            python.chmod(0o755)
            results = root / 'results'; results.mkdir()
            guide = ROOT / 'docs/guides/installed-release-journey.md'
            with mock.patch.dict(os.environ, {'PATH': str(binary_dir) + os.pathsep + os.environ['PATH']}):
                subject.run_documented_route(guide, results, cli, archive)
            observed = [json.loads(line) for line in calls.read_text().splitlines()]
            self.assertEqual(observed, [
                ['version'], ['init', '--preset', 'dev', '--hello-trace'],
                ['doctor', '--config', 'eval.yaml', '--format', 'json'],
                ['policy', 'validate', '--input', 'policy.yaml', '--format', 'json'],
                ['run', '--config', 'eval.yaml', '--trace-file', 'traces/hello.jsonl', '--format', 'json'],
                ['evidence', 'import', 'privileged-mcp-action', '--decisions', 'decisions.ndjson',
                 '--denied-observations', 'denied-observations.ndjson', '--bundle-out', 'action.bundle.tar.gz'],
                ['evidence', 'show', '--format', 'json', '--', 'action.bundle.tar.gz'],
                ['evidence', 'verify-privileged-mcp-action', 'action.bundle.tar.gz', '--profile-version', 'v1', '--format', 'json'],
            ])
            receipt = json.loads((results / 'documented-route/receipt.json').read_text())
            self.assertEqual(receipt['status'], 'completed')
            self.assertEqual(receipt['cli_before_sha256'], receipt['cli_after_sha256'])
            verified_dir = root / 'verified'; verified_dir.mkdir()
            verified = verified_dir / archive.name; verified.write_bytes(archive.read_bytes())
            (example / 'baseline-approved.json').write_text('different live archive bytes')
            with tarfile.open(archive, 'w:gz') as stream:
                stream.add(payload, arcname=name)
            refused = root/'changed-live'; refused.mkdir()
            with mock.patch.dict(os.environ, {'PATH': str(binary_dir) + os.pathsep + os.environ['PATH']}):
                with self.assertRaisesRegex(ValueError, 'acquisition differs'):
                    subject.run_documented_route(guide, refused, cli, verified)
            self.assertFalse((refused/'documented-route/execution').exists(),
                             'native extraction/companion ran before verified archive identity')


    def test_whole_download_fence_enforces_archive_and_sidecar_arguments(self):
        import hashlib, io
        subject=load_helper()
        code=subject.guide_blocks((ROOT/'docs/guides/installed-release-journey.md').read_text())['download-python']
        asset='assay-v6.9.0-x86_64-unknown-linux-gnu.tar.gz'
        payload=b'tiny opaque transport fixture'
        checksum=(hashlib.sha256(payload).hexdigest()+'  '+asset+'\n').encode()
        for kind in ('positive','archive','sidecar'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                old=Path.cwd();os.chdir(directory)
                def respond(request, timeout):
                    sidecar=request.full_url.endswith('.sha256')
                    data=checksum if sidecar else payload
                    declared=(16385 if sidecar else 134217729) if kind==('sidecar' if sidecar else 'archive') else len(data)
                    response=io.BytesIO(data);response.headers={'Content-Length':str(declared)}
                    return response
                try:
                    with mock.patch('subprocess.check_output',return_value='6.9.0'), \
                         mock.patch('platform.system',return_value='Linux'), \
                         mock.patch('platform.machine',return_value='x86_64'), \
                         mock.patch('urllib.request.urlopen',side_effect=respond):
                        if kind=='positive':
                            exec(compile(code,'documented-acquire.py','exec'),{})
                            self.assertEqual(Path(asset).read_bytes(),payload)
                            self.assertEqual(json.loads(Path('acquisition.json').read_text()),{'archive':asset})
                        else:
                            with self.assertRaisesRegex(ValueError,'content length exceeds ceiling'):
                                exec(compile(code,'documented-acquire.py','exec'),{})
                            self.assertFalse(Path('acquisition.json').exists())
                            self.assertFalse(Path(asset if kind=='archive' else asset+'.sha256').exists())
                            self.assertFalse(list(Path('.').glob('*.downloading')))
                finally:
                    os.chdir(old)

    def test_standalone_downloader_is_canonical_and_refuses_before_overflow(self):
        import ast, io
        subject = load_helper()
        code = subject.guide_blocks((ROOT/'docs/guides/installed-release-journey.md').read_text())['download-python']
        canonical = ast.parse((ROOT/'scripts/ci/bounded_download.py').read_text())
        names = ('DownloadRejected', 'download')
        expected = [node for node in canonical.body if getattr(node,'name',None) in names]
        actual_tree = ast.parse(code)
        actual = [node for node in actual_tree.body if getattr(node,'name',None) in names]
        self.assertEqual([ast.dump(node) for node in actual], [ast.dump(node) for node in expected])
        # Execute the actual documented imports/exception/function, not a test reimplementation.
        prefix = []
        for node in actual_tree.body:
            if isinstance(node,(ast.Import,ast.ImportFrom,ast.ClassDef,ast.FunctionDef)):
                prefix.append(node)
            else:
                break
        namespace={};exec(compile(ast.Module(body=prefix,type_ignores=[]),'<documented downloader>','exec'),namespace)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            for declared in ('33', None):
                response=io.BytesIO(b'x'*33);response.headers={} if declared is None else {'Content-Length':declared}
                with mock.patch('urllib.request.urlopen',return_value=response):
                    with self.assertRaisesRegex(ValueError,'ceiling'):
                        namespace['download']('https://fixture.invalid/archive',root/'asset',max_bytes=32)
                self.assertFalse((root/'asset').exists())
                self.assertFalse((root/'.asset.downloading').exists())
            response=io.BytesIO(b'x'*32);response.headers={}
            with mock.patch('urllib.request.urlopen',return_value=response):
                namespace['download']('https://fixture.invalid/archive',root/'asset',max_bytes=32)
            self.assertEqual((root/'asset').read_bytes(),b'x'*32)

    def test_tagged_public_fences_are_the_only_route_source(self):
        subject = load_helper()
        blocks = subject.guide_blocks((ROOT / 'docs/guides/installed-release-journey.md').read_text())
        self.assertEqual(set(blocks), {'download-python', 'open-unix', 'open-windows', 'acquire-unix', 'acquire-windows', 'cli-init', 'cli-doctor', 'cli-policy', 'cli-run',
                                      'deny-python', 'deny-unix', 'deny-windows',
                                      'cli-import', 'cli-show', 'cli-verify', 'sarif-unix', 'sarif-windows'})
        self.assertIn('assay init --preset dev --hello-trace', blocks['cli-init'])
        self.assertNotIn('--run-id', blocks['cli-import'])
        self.assertNotIn('--import-time', blocks['cli-import'])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            subject.guide_blocks('<!-- assay-route: cli-start -->\n```sh\na\n```\n' * 2)

    def test_route_executes_fences_in_order_without_import_overrides(self):
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            cli, archive, guide = root / 'cli', root / 'asset.tar.gz', root / 'guide.md'
            cli.write_bytes(b'opaque CLI fixture, never executed')
            local_archive(archive)
            blocks = {name: ':\n' for name in ('download-python', 'open-unix', 'open-windows', 'acquire-unix', 'acquire-windows', 'cli-init', 'cli-doctor', 'cli-policy', 'cli-run',
                     'deny-python', 'deny-unix', 'deny-windows', 'cli-import', 'cli-show', 'cli-verify', 'sarif-unix', 'sarif-windows')}
            blocks['acquire-unix'] = "cp '" + str(archive) + "' asset.tar.gz\nprintf 'acquire\\n'\n"
            blocks['cli-init'] = "printf 'init-doctor-policy-run\\n'\n"
            marker = root / 'native-open-marker'
            blocks['open-unix'] = "touch '" + str(marker) + "'\n"
            blocks['deny-python'] = "from pathlib import Path\nPath('decisions.ndjson').write_text('decision')\nPath('denied-observations.ndjson').write_text('observation')\nprint('real-python-fence')\n"
            blocks['deny-unix'] = "'" + sys.executable + "' deny.py\n"
            blocks['cli-import'] = "printf bundle > action.bundle.tar.gz\nprintf 'literal-import-inspect-v1\\n'\n"
            blocks['sarif-unix'] = "printf sarif > enforcement.sarif\n"
            guide.write_text(''.join('<!-- assay-route: ' + name + ' -->\n```sh\n' + code + '```\n' for name, code in blocks.items()))
            # Extraction accepts language-independent executable fence bytes.
            results = root / 'results'
            results.mkdir()
            subject.run_documented_route(guide, results, cli, archive)
            receipt = json.loads((results / 'documented-route/receipt.json').read_text())
            self.assertEqual(receipt['status'], 'completed')
            output = (results / 'documented-route/acquisition-execution/stdout').read_text() + (results / 'documented-route/execution/stdout').read_text()
            self.assertEqual(output.splitlines(), ['acquire', 'init-doctor-policy-run',
                                                 'real-python-fence', 'literal-import-inspect-v1'])
            self.assertNotIn('--run-id', (results / 'documented-route/route.sh').read_text())
            self.assertTrue(marker.exists())
            marker.unlink()
            blocks['acquire-unix'] = "printf changed > asset.tar.gz\n"
            guide.write_text(''.join('<!-- assay-route: ' + name + ' -->\n```sh\n' + code + '```\n' for name, code in blocks.items()))
            refused = root/'wrong-acquisition';refused.mkdir()
            with self.assertRaisesRegex(ValueError,'acquisition differs'):
                subject.run_documented_route(guide, refused, cli, archive)
            self.assertFalse(marker.exists(), 'native opening ran before archive identity admission')
            self.assertFalse((refused/'documented-route/bounded-archive-preflight').exists())
            self.assertEqual(receipt['cli_before_sha256'], receipt['cli_after_sha256'])

    def test_rendered_fence_closers_cannot_carry_prose(self):
        subject = load_helper()
        source = (ROOT / 'docs/guides/installed-release-journey.md').read_text()
        # The actual guide must be renderable, not merely accepted by a loose regex.
        self.assertFalse(__import__('re').search(r'^``` .+', source, __import__('re').M))
        malformed = source.replace('\n```\n', '\n``` trailing prose\n', 1)
        with self.assertRaisesRegex(ValueError, 'fence'):
            subject.guide_blocks(malformed)

    def test_route_archive_above_proxy_limit_is_allowed_but_stdout_is_bounded(self):
        subject = load_helper()
        # Use the actual route adapter with opaque local bytes, never a release archive.
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            cli, archive, guide = root / 'cli', root / 'asset.tar.gz', root / 'guide.md'
            cli.write_bytes(b'opaque CLI')
            local_archive(archive, 17 * 1024 * 1024)
            blocks = {name: ':\n' for name in ('download-python', 'open-unix', 'open-windows', 'acquire-unix', 'acquire-windows', 'cli-init', 'cli-doctor', 'cli-policy', 'cli-run',
                     'deny-python', 'deny-unix', 'deny-windows', 'cli-import', 'cli-show', 'cli-verify', 'sarif-unix', 'sarif-windows')}
            quoted = "'" + sys.executable + "'"
            blocks['acquire-unix'] = "cp '" + str(archive) + "' asset.tar.gz\n"
            blocks['deny-python'] = "from pathlib import Path\nfor name in ('decisions.ndjson','denied-observations.ndjson','action.bundle.tar.gz','enforcement.sarif'):\n Path(name).write_text('opaque fixture')\n"
            blocks['deny-unix'] = quoted + ' deny.py\n'
            def write_guide():
                guide.write_text(''.join('<!-- assay-route: ' + name + ' -->\n```sh\n' + code + '```\n' for name, code in blocks.items()))
            write_guide()
            results = root / 'positive'; results.mkdir()
            try:
                subject.run_documented_route(guide, results, cli, archive)
            except ValueError as error:
                self.fail(str(error) + ': ' + (results / 'documented-route/execution/stderr').read_text())
            self.assertEqual(json.loads((results / 'documented-route/receipt.json').read_text())['status'], 'completed')
            blocks['cli-init'] = quoted + " -c 'print(chr(120)*10000)'\n"
            write_guide()
            results = root / 'overflow'; results.mkdir()
            with self.assertRaisesRegex(ValueError, 'output ceiling'):
                subject.run_documented_route(guide, results, cli, archive, output_limit=1024)
            self.assertLessEqual((results / 'documented-route/execution/stdout').stat().st_size, 1024)
            blocks['cli-init'] = "exit 7\nprintf 'must-not-run'\n"
            write_guide()
            results = root / 'failed-stage'; results.mkdir()
            with self.assertRaisesRegex(ValueError, 'documented route failed: 7'):
                subject.run_documented_route(guide, results, cli, archive)
            receipt = json.loads((results / 'documented-route/receipt.json').read_text())
            self.assertEqual(receipt['stage_records'], [{'name': 'acquire-unix', 'exit_code': 0},
                                                        {'name': 'open-unix', 'exit_code': 0},
                                                        {'name': 'cli-init', 'exit_code': 7}])
            self.assertNotIn('must-not-run', (results / 'documented-route/execution/stdout').read_text())
            blocks['cli-init'] = ':\n'
            blocks['cli-policy'] = "exit 7\n"
            blocks['cli-run'] = "printf 'must-not-run-after-middle'\n"
            write_guide()
            results = root / 'middle-stage'; results.mkdir()
            with self.assertRaisesRegex(ValueError, 'documented route failed: 7'):
                subject.run_documented_route(guide, results, cli, archive)
            receipt = json.loads((results / 'documented-route/receipt.json').read_text())
            self.assertEqual([row['name'] for row in receipt['stage_records']],
                             ['acquire-unix', 'open-unix', 'cli-init', 'cli-doctor', 'cli-policy'])
            self.assertEqual(receipt['stage_records'][-1]['exit_code'], 7)
            self.assertNotIn('cli-run', (results / 'documented-route/stages-begun.txt').read_text().split())
            self.assertNotIn('must-not-run-after-middle', (results / 'documented-route/execution/stdout').read_text())
            blocks['cli-policy'] = blocks['cli-run'] = ':\n'
            marker = root / 'escaped-descendant'
            blocks['cli-init'] = "(printf 'owned-descendant-ready\\n'; sleep 1; touch '" + str(marker) + "') & wait\n"
            write_guide()
            results = root / 'timeout'; results.mkdir()
            with self.assertRaises(TimeoutError):
                subject.run_documented_route(guide, results, cli, archive, timeout=0.3)
            self.assertIn('owned-descendant-ready', (results / 'documented-route/execution/stdout').read_text())
            time.sleep(1.1)
            self.assertFalse(marker.exists(), 'route descendant escaped owned cleanup')
            blocks['cli-init'] = ':\n'
            original_deny = blocks['deny-unix']
            for label, change, reason in (
                ('missing-producer', ':\n', 'documented route output missing'),
                ('changed-cli', original_deny + "printf changed > '" + str(cli) + "'\n", 'changed installed CLI'),
                ('changed-archive', original_deny + "printf changed > ../asset.tar.gz\n", 'acquisition differs')):
                cli.write_bytes(b'opaque CLI')
                blocks['deny-unix'] = change
                write_guide()
                results = root / label; results.mkdir()
                with self.assertRaisesRegex(ValueError, reason):
                    subject.run_documented_route(guide, results, cli, archive)
                self.assertEqual(json.loads((results / 'documented-route/receipt.json').read_text())['status'], 'failed')

    def test_actual_published_python_fence_exchanges_with_real_fake_child(self):
        subject = load_helper()
        blocks = subject.guide_blocks((ROOT / 'docs/guides/installed-release-journey.md').read_text())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            program, server = root / 'deny.py', root / 'fake-server'
            program.write_text(blocks['deny-python'])
            server.write_text('#!' + sys.executable + '\n' +
                             "import sys,json,pathlib\n" +
                             "pathlib.Path('argv.json').write_text(json.dumps(sys.argv[1:]))\n" +
                             "for flag in ('--enforcement-decision-out','--denied-call-observation-out'):\n" +
                             " pathlib.Path(sys.argv[sys.argv.index(flag)+1]).write_text('actual fake child record\\n')\n" +
                             "for line in sys.stdin:\n" +
                             " request=json.loads(line)\n" +
                             " if request.get('id')==9:\n" +
                             "  print(json.dumps({'id':9,'error':{'code':-31999,'data':{'reason':'no_declared_allowance'}}}),flush=True)\n")
            server.chmod(0o755)
            result = subprocess.run([sys.executable, str(program), str(server), str(root / 'example')],
                                    cwd=root, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / 'decisions.ndjson').read_text(), 'actual fake child record\n')
            argv = json.loads((root / 'argv.json').read_text())
            self.assertEqual(argv[0], 'proxy-enforce')
            self.assertEqual(argv[argv.index('--enforce-policy') + 1], str(root / 'example/policies/no-allowance.yaml'))

    def test_windows_launch_preserves_systemroot_without_user_configuration(self):
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for key in ('SystemRoot', 'SYSTEMROOT', 'systemroot'):
                with self.subTest(key=key):
                    environment = {key: r'C:\Windows', 'PATH': 'installed-prefix;host-tools',
                                   'GH_TOKEN': 'private', 'PYTHONPATH': 'poison',
                                   'PSModulePath': 'user-modules', 'USERPROFILE': 'user-config'}
                    def launch(argv, env, *args):
                        self.assertEqual({k.upper(): v for k, v in env.items()},
                                         {'SYSTEMROOT': r'C:\Windows', 'PATH': 'installed-prefix;host-tools'})
                        return {'stdout': b'', 'stderr': b'', 'create_process': True,
                                'job_closed': True, 'wait_result': 'exited', 'truncated': False,
                                'job_total_processes': 1, 'exit': 0}
                    launcher = mock.Mock()
                    launcher.launch_interactive_job.side_effect = launch
                    with mock.patch.dict(os.environ, environment, clear=True), \
                         mock.patch.object(subject.sys, 'platform', 'win32'), \
                         mock.patch.object(subject, 'load_windows_launcher', return_value=launcher):
                        status = subject.run_proxy_child(['pwsh', '-File', 'fixture.ps1'], b'',
                                  root/'stdout', root/'stderr', expected_lines=None, timeout=1)
                    self.assertEqual(status, 0)

    def test_native_probe_requires_inner_exit_diagnostics_and_no_later_marker(self):
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def launch(argv, request, stdout, stderr, **kwargs):
                script = Path(argv[-1]); name = script.stem
                statuses = {'positive': [0, 0, 0], 'negative': [7], 'middle': [0, 7], 'python-negative': [7]}[name]
                code = script.read_text()
                self.assertEqual(code.count('$assayNativeStatus = $LASTEXITCODE'), 3)
                if name in ('negative', 'middle'):
                    self.assertIn('-Command "exit 7"', code)
                if name == 'python-negative':
                    self.assertIn('sys.exit(7)', code)
                    self.assertNotIn('pwsh.exe', code)
                for index, status in enumerate(statuses):
                    with (script.parent / (name + '.native.ndjson')).open('a') as stream:
                        stream.write(json.dumps({'stage': str(index), 'exit_code': status, 'powershell_version': 'synthetic', 'ps_home': 'fixture-home'}) + '\n')
                    if status == 0:
                        (script.parent / (name + '.' + str(index) + '.marker')).write_text('continued')
                stdout.write_bytes(b''); stderr.write_bytes(b'')
                return 0 if name == 'positive' else 1
            with mock.patch.object(subject, 'run_proxy_child', side_effect=launch):
                report = subject.verify_native_fail_fast('fixture-pwsh', root / 'good')
            self.assertEqual([row['name'] for row in report], ['positive', 'negative', 'middle', 'python-negative'])
            self.assertEqual([row['native_exit_codes'] for row in report], [[0, 0, 0], [7], [0, 7], [7]])
            self.assertTrue(all('launcher' in row for row in report))
            for corruption in ('mask', 'missing', 'wrong-inner'):
                def corrupted(*args, **kwargs):
                    status = launch(*args, **kwargs)
                    script = Path(args[0][-1])
                    if script.stem == 'negative':
                        if corruption == 'mask':
                            (script.parent / 'negative.0.marker').write_text('continued')
                            return 0
                        receipt = script.parent / 'negative.native.ndjson'
                        if corruption == 'missing':
                            receipt.unlink()
                        else:
                            rows = [json.loads(line) for line in receipt.read_text().splitlines()]
                            rows[0]['exit_code'] = 0
                            receipt.write_text(json.dumps(rows[0]) + '\n')
                    return status
                with self.subTest(corruption=corruption), mock.patch.object(subject, 'run_proxy_child', side_effect=corrupted):
                    with self.assertRaisesRegex(ValueError, 'native'):
                        subject.verify_native_fail_fast('fixture-pwsh', root / corruption)

    def test_windows_guard_records_status_before_refusing(self):
        subject = load_helper()
        guard = subject.windows_native_guard(Path('receipt.ndjson'), 'cli-run')
        record, missing, failed = (guard.index('Add-Content'), guard.index('-isnot [int]'),
                                   guard.index('-ne 0'))
        self.assertLess(record, missing)
        self.assertLess(missing, failed)
        self.assertTrue(guard.startswith('$assayNativeStatus = $LASTEXITCODE'))
        wrapped = subject.windows_native_command('assay version', Path('receipt.ndjson'), 'cli-run')
        self.assertTrue(wrapped.startswith('$global:LASTEXITCODE = $null'))
        self.assertLess(wrapped.index('assay version'), wrapped.index('$assayNativeStatus = $LASTEXITCODE'))

    def test_native_exit_receipt_must_cover_every_stage_with_zero(self):
        subject = load_helper()
        stages = ['acquire-windows', 'open-windows', 'cli-init']
        def row(stage, code=0):
            return {'stage': stage, 'exit_code': code, 'powershell_version': '7.5.3', 'ps_home': 'x'}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'native-exits.ndjson'
            def write(rows):
                path.write_text(''.join(json.dumps(item) + '\n' for item in rows))
            write([row(name) for name in stages])
            self.assertEqual(len(subject.validate_native_exits(path, stages)), 3)
            for label, rows in (('missing', [row(name) for name in stages[:2]]),
                                ('reordered', [row(name) for name in reversed(stages)]),
                                ('nonzero', [row(stages[0]), row(stages[1], 7), row(stages[2])]),
                                ('null', [row(stages[0]), row(stages[1], None), row(stages[2])]),
                                ('boolean', [row(stages[0]), row(stages[1], False), row(stages[2])])):
                with self.subTest(label=label):
                    write(rows)
                    with self.assertRaisesRegex(ValueError, 'native stage exits'):
                        subject.validate_native_exits(path, stages)
            path.unlink()
            with self.assertRaisesRegex(ValueError, 'native stage exits'):
                subject.validate_native_exits(path, stages)

    def test_copy_identity_before_and_after_and_mutated_destination(self):
        subject = load_helper()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, copied, receipt = root / 'installed', root / 'copy', root / 'receipt.json'
            source.write_bytes(b'actual opaque executable bytes')
            copied.write_bytes(source.read_bytes())
            subject.record_binary_continuity(source, copied, receipt, 'before')
            subject.record_binary_continuity(source, copied, receipt, 'after')
            record = json.loads(receipt.read_text())
            self.assertEqual(record['before']['source_sha256'], record['after']['destination_sha256'])
            copied.write_bytes(b'changed destination')
            with self.assertRaisesRegex(ValueError, 'binary continuity'):
                subject.record_binary_continuity(source, copied, receipt, 'after')
            with self.assertRaisesRegex(ValueError, 'binary continuity'):
                subject.record_binary_continuity(source, copied, receipt, 'before')
            source.write_bytes(copied.read_bytes())
            with self.assertRaisesRegex(ValueError, 'source changed'):
                subject.record_binary_continuity(source, copied, receipt, 'after')


class PublishedReleaseProxyPhaseTests(unittest.TestCase):
    def test_request_case_appends_existing_driver_ledger_and_refuses_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            results = root / 'results'; results.mkdir()
            server = root / 'assay-mcp-server'
            server.write_text('#!' + sys.executable + '\n' + """import json,pathlib,sys
args=sys.argv[1:]
pathlib.Path('child-ran').write_text('yes')
for line in sys.stdin:
    if json.loads(line).get('id') == 9:
        decision={'schema':'assay.enforcement_decision.v0','decision':'deny','reason':'no_declared_allowance','tool':{'name':'github.add_deploy_key'},'action':{'target':{'provider':'github','owner':'acme','repo':'prod-app'}}}
        pathlib.Path(args[args.index('--enforcement-decision-out')+1]).write_text(json.dumps(decision)+'\\n')
        pathlib.Path(args[args.index('--denied-call-observation-out')+1]).write_text('{}\\n')
        print(json.dumps({'jsonrpc':'2.0','id':9,'error':{'code':-31999,'data':{'origin':'assay-proxy','reason':'no_declared_allowance'}}}),flush=True)
""")
            server.chmod(0o755)
            ledger = results / 'commands.ndjson'
            prior = b'{"name":"assay-version","exit_code":0,"argv":["assay","version"]}\n'
            ledger.write_bytes(prior)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'])
            argv = [sys.executable, '-I', str(HELPER), '--expect', 'deny']
            request = b'{"jsonrpc":"2.0","id":9}\n'
            first = subprocess.run(argv, input=request, capture_output=True, cwd=results, env=env, timeout=10)
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertTrue((results / 'child-ran').exists())
            contents = ledger.read_bytes()
            self.assertTrue(contents.startswith(prior))
            rows = [json.loads(line) for line in contents.splitlines()]
            self.assertEqual(len(rows), 2)
            self.assertEqual(rows[1]['name'], 'proxy-enforce')
            self.assertEqual(rows[1]['exit_code'], 0)
            (results / 'child-ran').unlink()
            second = subprocess.run(argv, input=request, capture_output=True, cwd=results, env=env, timeout=10)
            self.assertNotEqual(second.returncode, 0)
            self.assertIn(b'requires fresh output paths', second.stderr)
            self.assertFalse((results / 'child-ran').exists())
            self.assertEqual(ledger.read_bytes(), contents)

    def test_explicit_packaged_inputs_reach_child(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory).resolve()
            (fixture / 'policies').mkdir()
            for name in ('mock_github_mcp.py', 'baseline-approved.json', 'policies/no-allowance.yaml'):
                (fixture / name).write_text('trusted test input')
            completed, results = self.run_phase(0, fixture_dir=fixture)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            invocation = json.loads((results / 'fake-invocations.jsonl').read_text().splitlines()[0])
            self.assertEqual(invocation[invocation.index('--enforce-policy') + 1], str(fixture / 'policies/no-allowance.yaml'))
            self.assertIn(str(fixture / 'mock_github_mcp.py'), invocation)

    def test_missing_explicit_fixture_never_falls_back(self):
        completed, results = self.run_phase(0, fixture_dir=Path('/nonexistent-assay-packaged-fixture'))
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(b'explicit fixture directory', completed.stderr)
        self.assertFalse((results / 'fake-invocations.jsonl').exists())

    def run_phase(
        self,
        fake_exit: int,
        request: bytes = b'{"jsonrpc":"2.0","id":1}\n',
        fake_sleep: float = 0,
        fake_output_bytes: int = 0,
        spawn_grandchild: bool = False,
        timeout_seconds: int = 60,
        fixture_dir: Path | None = None,
    ) -> tuple[subprocess.CompletedProcess[bytes], Path]:
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="proxy phase ")))
        fake = temporary / "assay-mcp-server"
        fake.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json, os, pathlib, subprocess, sys, time

                def value(flag):
                    return pathlib.Path(sys.argv[sys.argv.index(flag) + 1])

                decisions = value("--enforcement-decision-out")
                observations = value("--denied-call-observation-out")
                invocation = decisions.parent / "fake-invocations.jsonl"
                with invocation.open("a", encoding="utf-8") as handle:
                    handle.write(json.dumps(sys.argv) + "\\n")
                (decisions.parent / "fake-environment.json").write_text(
                    json.dumps(dict(os.environ)), encoding="utf-8"
                )
                control = json.loads(
                    (decisions.parent / "fake-control.json").read_text(encoding="utf-8")
                )
                sys.stdin.buffer.read()
                if control["spawn_grandchild"]:
                    sentinel = decisions.parent / "grandchild-sentinel"
                    subprocess.Popen(
                        [
                            sys.executable,
                            "-c",
                            'import pathlib,sys,time; time.sleep(1.5); pathlib.Path(sys.argv[1]).write_text("survived"); pathlib.Path(sys.argv[2]).write_text("mutated")',
                            str(sentinel),
                            str(decisions),
                        ]
                    )
                time.sleep(control["sleep"])
                if control["output_bytes"]:
                    sys.stdout.write("x" * control["output_bytes"])
                    sys.stdout.flush()
                decisions.write_text('{"decision":"deny"}\\n', encoding="utf-8")
                observations.write_text('{"observed":true}\\n', encoding="utf-8")
                print("fake proxy stdout")
                print("fake proxy stderr", file=sys.stderr)
                raise SystemExit(control["exit"])
                """
            ),
            encoding="utf-8",
        )
        fake.chmod(0o755)
        results = temporary / "results"
        results.mkdir()
        poison = temporary / "pythonpath-poison"
        poison.mkdir()
        poison_sentinel = results / "pythonpath-imported"
        (poison / "json.py").write_text(
            f"open({str(poison_sentinel)!r}, 'w').write('loaded')\nraise RuntimeError('PYTHONPATH loaded')\n",
            encoding="utf-8",
        )
        (results / "fake-control.json").write_text(
            json.dumps(
                {
                    "exit": fake_exit,
                    "sleep": fake_sleep,
                    "output_bytes": fake_output_bytes,
                    "spawn_grandchild": spawn_grandchild,
                }
            ),
            encoding="utf-8",
        )
        command = [
            sys.executable,
            "-I",
            str(HELPER),
            "--timeout-seconds",
            str(timeout_seconds),
        ]
        if fixture_dir is not None:
            command.extend(["--fixture-dir", str(fixture_dir)])
        environment = os.environ.copy()
        environment["GH_TOKEN"] = "must-not-reach-release-code"
        environment["GITHUB_TOKEN"] = "must-not-reach-release-code"
        environment["PYTHONPATH"] = str(poison)
        environment["PATH"] = f"{temporary}{os.pathsep}{environment['PATH']}"
        completed = subprocess.run(
            command,
            input=request,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            env=environment,
            cwd=results,
        )
        self.assertFalse(poison_sentinel.exists(), "helper interpreter imported from PYTHONPATH")
        return completed, results

    def assert_observed_equals_recorded(self, results: Path, expected_status: int) -> None:
        observed = [
            json.loads(line)
            for line in (results / "fake-invocations.jsonl").read_text(encoding="utf-8").splitlines()
        ]
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(observed), 1)
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["name"], "proxy-enforce")
        self.assertEqual(records[0]["exit_code"], expected_status)
        self.assertEqual(records[0]["argv"], observed[0])
        self.assertEqual((results / "proxy.jsonl").read_text(encoding="utf-8"), "fake proxy stdout\n")
        self.assertEqual((results / "proxy.stderr").read_text(encoding="utf-8"), "fake proxy stderr\n")
        self.assertTrue((results / "decisions.ndjson").is_file())
        self.assertTrue((results / "denied-observations.ndjson").is_file())
        child_environment = json.loads(
            (results / "fake-environment.json").read_text(encoding="utf-8")
        )
        self.assertNotIn("GH_TOKEN", child_environment)
        self.assertNotIn("GITHUB_TOKEN", child_environment)
        self.assertNotIn("PYTHONPATH", child_environment)

    def test_success_records_the_executed_argv_once(self) -> None:
        completed, results = self.run_phase(0)
        self.assertEqual(completed.returncode, 0, completed.stderr.decode())
        self.assert_observed_equals_recorded(results, 0)

    def test_failure_preserves_the_real_status_and_argv(self) -> None:
        completed, results = self.run_phase(23)
        self.assertEqual(completed.returncode, 23, completed.stderr.decode())
        self.assert_observed_equals_recorded(results, 23)

    def test_request_ceiling_fails_before_execution(self) -> None:
        completed, results = self.run_phase(0, b"x" * (1_048_576 + 1))
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(b"proxy request exceeds 1 MiB ceiling", completed.stderr)
        self.assertFalse((results / "fake-invocations.jsonl").exists())
        self.assertFalse((results / "commands.ndjson").exists())

    def test_timeout_records_the_bounded_harness_status(self) -> None:
        completed, results = self.run_phase(0, fake_sleep=2, timeout_seconds=1)
        self.assertEqual(completed.returncode, 124, completed.stderr.decode() + (results / "proxy.stderr").read_text())
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["exit_code"], 124)
        observed = [
            json.loads(line)
            for line in (results / "fake-invocations.jsonl").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(records[0]["argv"], observed[0])

    def test_timeout_reaps_proxy_descendants_before_recording(self) -> None:
        completed, results = self.run_phase(
            0, fake_sleep=3, spawn_grandchild=True, timeout_seconds=1
        )
        self.assertEqual(completed.returncode, 124, completed.stderr.decode() + (results / "proxy.stderr").read_text())
        time.sleep(1)
        self.assertFalse((results / "grandchild-sentinel").exists())

    def test_success_reaps_proxy_descendants_before_recording(self) -> None:
        completed, results = self.run_phase(0, spawn_grandchild=True)
        self.assertEqual(
            completed.returncode,
            0,
            (results / "proxy.stderr").read_text(encoding="utf-8"),
        )
        time.sleep(2)
        self.assertFalse((results / "grandchild-sentinel").exists())
        self.assertEqual(
            (results / "decisions.ndjson").read_text(encoding="utf-8"),
            '{"decision":"deny"}\n',
        )

    def test_output_file_ceiling_stops_unbounded_child_output(self) -> None:
        completed, results = self.run_phase(0, fake_output_bytes=16_777_216 + 1)
        self.assertNotEqual(completed.returncode, 0)
        self.assertLessEqual((results / "proxy.jsonl").stat().st_size, 16_777_216)
        records = [
            json.loads(line)
            for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(len(records), 1)
        self.assertNotEqual(records[0]["exit_code"], 0)


class WindowsJobExecutionTests(unittest.TestCase):
    def test_shared_windows_launcher_forwards_the_interactive_contract(self) -> None:
        windows = load_windows_launcher()
        result = {"exit": 0}
        with (
            mock.patch.object(windows, "launch_environment", return_value={"PATH": "clean"}),
            mock.patch.object(windows, "launch_in_profile", return_value=result) as launch,
            mock.patch.object(windows, "read_process_token", return_value={}),
        ):
            observed = windows.launch_interactive_job(
                ["assay-mcp-server.exe"],
                {"PATH": "dirty", "GH_TOKEN": "secret"},
                b"request\n",
                19,
                2,
                4096,
            )
        self.assertIs(observed, result)
        self.assertEqual(launch.call_args.args[:5], (None, None, ["assay-mcp-server.exe"], {"PATH": "clean"}, 19))
        self.assertEqual(launch.call_args.kwargs["input_bytes"], b"request\n")
        self.assertEqual(launch.call_args.kwargs["expected_lines"], 2)
        self.assertEqual(launch.call_args.kwargs["output_limit"], 4096)

    def test_windows_proxy_uses_the_shared_suspended_job_session(self) -> None:
        helper = load_helper()
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="windows proxy ")))
        stdout_path = temporary / "proxy.jsonl"
        stderr_path = temporary / "proxy.stderr"
        calls = []

        class Launcher:
            @staticmethod
            def launch_interactive_job(argv, env, request, timeout, expected_lines, output_limit):
                calls.append((argv, env, request, timeout, expected_lines, output_limit))
                return {
                    "create_process": True,
                    "exit": 0,
                    "job_closed": True,
                    "job_total_processes": 2,
                    "stderr": b"bounded stderr",
                    "stdout": b'{"jsonrpc":"2.0","id":9,"error":{}}\n',
                    "truncated": False,
                    "wait_result": "exited",
                }

        with (
            mock.patch.object(helper.sys, "platform", "win32"),
            mock.patch.object(helper, "load_windows_launcher", return_value=Launcher),
        ):
            status = helper.run_proxy_child(
                [r"C:\\bin\\assay-mcp-server.exe", "proxy-enforce"],
                b'{"jsonrpc":"2.0","id":9}\n',
                stdout_path,
                stderr_path,
                expected_lines=1,
                timeout=17,
            )

        self.assertEqual(status, 0)
        self.assertEqual(stdout_path.read_bytes(), b'{"jsonrpc":"2.0","id":9,"error":{}}\n')
        self.assertEqual(stderr_path.read_bytes(), b"bounded stderr")
        self.assertEqual(len(calls), 1)
        argv, env, request, timeout, expected_lines, output_limit = calls[0]
        self.assertEqual(argv[0], r"C:\\bin\\assay-mcp-server.exe")
        self.assertNotIn("GH_TOKEN", env)
        self.assertEqual(request, b'{"jsonrpc":"2.0","id":9}\n')
        self.assertEqual((timeout, expected_lines, output_limit), (17, 1, helper.MAX_OUTPUT_BYTES))

    def test_windows_proxy_refuses_truncated_or_unreaped_job_results(self) -> None:
        helper = load_helper()
        temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="windows proxy ")))
        valid = {
            "create_process": True,
            "exit": 0,
            "job_closed": True,
            "job_total_processes": 2,
            "stderr": b"",
            "stdout": b"response\n",
            "truncated": False,
            "wait_result": "exited",
        }
        variants = {
            "job-not-closed": {"job_closed": False},
            "output-truncated": {"truncated": True},
            "process-still-running": {"wait_result": "still-running"},
        }
        for name, changed in variants.items():
            result = {**valid, **changed}

            class Launcher:
                @staticmethod
                def launch_interactive_job(*_args, **_kwargs):
                    return result

            with (
                self.subTest(name=name),
                mock.patch.object(helper.sys, "platform", "win32"),
                mock.patch.object(helper, "load_windows_launcher", return_value=Launcher),
            ):
                status = helper.run_proxy_child(
                    ["assay-mcp-server.exe"],
                    b"request",
                    temporary / "stdout",
                    temporary / "stderr",
                    expected_lines=None,
                    timeout=5,
                )
                self.assertNotEqual(status, 0)


if __name__ == "__main__":
    unittest.main()
