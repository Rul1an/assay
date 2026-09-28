#!/usr/bin/env python3
"""Behavioral controls for the retained, credential-free streaming install."""
import unittest
import os
import shutil
import tempfile
import json
import sys
from unittest import mock
import published_release_installer as subject
from cosign_release_pin import read_pin
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class InstallerContract(unittest.TestCase):
    def test_installer_produced_cli_drives_journey(self):
        driver = (ROOT / 'scripts/ci/published-release-golden-path.sh').read_text()
        self.assertIn('published_release_installer.py', driver,
                      'Unix journey still copies the archive CLI instead of running the served installer')
        self.assertNotIn('cp "${cli_candidates[0]}" "$install_root/bin/assay"', driver)
        self.assertLess(driver.index('published_release_installer.py'), driver.index('run_capture "assay-version"'))

    def test_entry_rejects_arguments_before_filesystem_reads(self):
        with mock.patch.object(sys, 'argv', ['installer', '../foreign']), \
             mock.patch.object(subject.Path, 'read_text', side_effect=AssertionError('unexpected read')):
            with self.assertRaisesRegex(SystemExit, 'usage:'):
                subject.main()

    def test_missing_metadata_retains_failure_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = root / 'harness/scripts/ci/published_release_installer.py'
            helper.parent.mkdir(parents=True)
            (root / 'results').mkdir()
            with mock.patch.object(subject, '__file__', str(helper)), \
                 mock.patch.object(sys, 'argv', ['installer']):
                with self.assertRaises(FileNotFoundError):
                    subject.main()
            receipt = json.loads((root / 'results/installer/receipt.json').read_text())
            self.assertEqual(receipt['status'], 'failed')
            self.assertIn('release-api.json', receipt['failure'])

    def test_required_ci_callsite_and_removal_control(self):
        def check(text):
            call = 'bash "$ROOT/scripts/ci/test-published-release-golden-path-contract.sh"'
            self.assertEqual(text.count(call), 1, 'required hardening callsite missing or duplicate')
            self.assertTrue(text.rstrip().endswith(call + '\n\necho "ci-hardening-b1 contract: PASS"'),
                            'required contract must complete before PASS')
        text = (ROOT / 'scripts/ci/test-ci-hardening-b1.sh').read_text()
        check(text)
        with self.assertRaisesRegex(AssertionError, 'required hardening callsite'):
            check(text.replace('bash "$ROOT/scripts/ci/test-published-release-golden-path-contract.sh"', ':'))


class InstallerMainFunnel(unittest.TestCase):
    """Transport/process boundaries are offline fakes; main's identity decisions are real."""
    def run_case(self, mismatch=None):
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            helper = root / 'harness/scripts/ci/published_release_installer.py'
            helper.parent.mkdir(parents=True)
            pin = root / 'harness/.github/workflows/release.yml'
            pin.parent.mkdir(parents=True)
            pin.write_text('          cosign-release: v3.1.3\n')
            results = root / 'results'
            (results / 'release-assets').mkdir(parents=True)
            archive = results / 'release-assets/assay-v6.9.0-x86_64-unknown-linux-gnu.tar.gz'
            archive.write_bytes(b'opaque verified archive fixture')
            reference = root / 'cli-extract/assay'
            reference.parent.mkdir()
            reference.write_bytes(b'opaque matching executable bytes')
            reference.chmod(0o755)
            cosign = root / 'cosign'
            cosign.write_bytes(b'opaque tool fixture')
            cosign.chmod(0o755)
            api = {'id': 71, 'tag_name': 'v6.9.0', 'draft': False,
                   'prerelease': False, 'assets': []}
            tag = {'object': {'type': 'commit', 'sha': 'a' * 40}}
            (results / 'release-api.json').write_text(json.dumps(api))
            (results / 'tag-ref.json').write_text(json.dumps(tag))
            (results / 'journey-source-sha.txt').write_text('a' * 40)
            source = b'# released source fixture; never executed\n'
            stages = []

            def transport(request, timeout):
                import io
                url = request.full_url
                expected = "application/vnd.github+json" if url.startswith("https://api.github.com/") else "application/octet-stream"
                self.assertEqual(request.get_header("Accept"), expected, "actual metadata request representation")
                self.assertNotIn('/git/ref/', url, 'tag identity must be re-read over git, not the REST API')
                data = json.dumps(api).encode() if '/releases/tags/' in url else source
                response = io.BytesIO(data)
                response.headers = {"Content-Length": str(len(data))}
                return response

            def pipeline(curl, tee, shell, url, capture, env, output):
                mode = output.parent.name
                stages.append(mode)
                capture.write_bytes(source + (b'# distinct response\n' if mismatch == 'capture' else b''))
                binary = Path(env['ASSAY_INSTALL_DIR']) / 'assay'
                binary.parent.mkdir(parents=True)
                binary.write_bytes(reference.read_bytes() + (b'different' if mismatch == 'binary' else b''))
                binary.chmod(0o755)
                output.mkdir()
                observation = ('signed_manifest_skipped reason=cosign_not_installed' if mode == 'default' else
                               'signed_manifest_verified asset=' + archive.name +
                               ' identity=https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v6.9.0')
                (output / 'stdout').write_text('verification=checksum_verified asset=' + archive.name +
                                              ' sha256=' + subject.digest(archive) +
                                              '\nverification=provenance_not_requested\nverification=' + observation)
                (output / 'sh-stderr').write_text('')
                return {'argv': ['offline-transport'], 'exit_code': 0, 'pipeline_status': [0, 0, 0]}

            def process(argv, env, output):
                output.mkdir()
                value = 'GitVersion: v3.1.3' if str(argv[0]) == str(cosign) else (
                    'assay 6.9.0' if argv[-1] == '--version' else '6.9.0')
                (output / 'stdout').write_text(value + '\n')
                return {'exit_code': 0}

            def which(name, path=None):
                return '/usr/bin/curl' if name == 'curl' else (str(cosign) if path is None else None)

            with ExitStack() as stack:
                stack.enter_context(mock.patch('bounded_download.urllib.request.urlopen', side_effect=transport))
                for name, value in [('__file__', str(helper)),
                                    ('run_pipeline', pipeline), ('supervise', process),
                                    ('contrasts', lambda *args: None),
                                    ('remote_tag_identity', lambda release_tag, scratch: {
                                        'argv': ['git', 'ls-remote'], 'object_sha': ('b' if mismatch == 'tag' else 'a') * 40,
                                        'commit_sha': ('c' if mismatch == 'peeled' else 'a') * 40})]:
                    stack.enter_context(mock.patch.object(subject, name, value))
                stack.enter_context(mock.patch.object(subject.shutil, 'which', which))
                stack.enter_context(mock.patch.object(subject.subprocess, 'check_output', return_value='curl 8.4.0'))
                stack.enter_context(mock.patch.object(sys, 'argv', ['installer']))
                stack.enter_context(mock.patch.dict(os.environ, {'PUBLISHED_COSIGN': str(cosign),
                                                                'PUBLISHED_COSIGN_RELEASE': 'v3.1.3'}))
                failure = None
                try:
                    subject.main()
                except ValueError as error:
                    failure = str(error)
            receipt = json.loads((results / 'installer/receipt.json').read_text())
            return failure, receipt, stages

    def test_matching_main_funnel_completes_both_modes(self):
        failure, receipt, stages = self.run_case()
        self.assertIsNone(failure)
        self.assertEqual(receipt['status'], 'completed')
        self.assertEqual(stages, ['default', 'signed'])
        self.assertEqual([row['mode'] for row in receipt['installations']], stages)

    def test_main_refuses_distinct_executed_source(self):
        failure, receipt, stages = self.run_case('capture')
        self.assertEqual(failure, 'executed streaming response differs from released installer')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default'])

    def test_main_refuses_a_tag_moved_during_installation(self):
        failure, receipt, stages = self.run_case('tag')
        self.assertEqual(failure, 'tag metadata changed during installation')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default', 'signed'])

    def test_main_refuses_a_tag_that_now_peels_to_another_commit(self):
        failure, receipt, stages = self.run_case('peeled')
        self.assertEqual(failure, 'tag metadata changed during installation')
        self.assertEqual(receipt['status'], 'failed')

    def test_tag_listing_accepts_annotated_and_lightweight_tags_only(self):
        a, c = 'a' * 40, 'c' * 40
        self.assertEqual(subject.parse_tag_listing(f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.0^{{}}\n', 'v6.9.0'),
                         {'object_sha': a, 'commit_sha': c})
        self.assertEqual(subject.parse_tag_listing(f'{a}\trefs/tags/v6.9.0\n', 'v6.9.0'),
                         {'object_sha': a, 'commit_sha': a})
        for text in ('', f'{c}\trefs/tags/v6.9.0^{{}}\n', f'{a}\trefs/tags/v6.9.1\n',
                     f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.0^{{}}\n{c}\trefs/heads/main\n',
                     f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.00\n',
                     f'{a}\trefs/tags/v6.9.0\n{a}\trefs/tags/v6.9.0\n', 'not-a-sha\trefs/tags/v6.9.0\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                subject.parse_tag_listing(text, 'v6.9.0')

    def test_tag_reread_uses_credential_free_git(self):
        seen = {}
        def run(argv, env, **kwargs):
            seen.update(argv=argv, env=env, kwargs=kwargs)
            return mock.Mock(returncode=0, stdout=('a' * 40 + '\trefs/tags/v6.9.0\n').encode())
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(subject.shutil, 'which', return_value='/usr/bin/git'), \
                mock.patch.object(subject.subprocess, 'run', side_effect=run), \
                mock.patch.dict(os.environ, {'GH_TOKEN': 'must-not-leak', 'GITHUB_TOKEN': 'must-not-leak'}):
            identity = subject.remote_tag_identity('v6.9.0', Path(directory) / 'git')
        self.assertEqual(identity['commit_sha'], 'a' * 40)
        self.assertIn('credential.helper=', seen['argv'])
        self.assertIn('https://github.com/Rul1an/assay.git', seen['argv'])
        self.assertEqual(seen['env']['GIT_TERMINAL_PROMPT'], '0')
        self.assertFalse({'GH_TOKEN', 'GITHUB_TOKEN'} & set(seen['env']))
        self.assertEqual(seen['kwargs']['timeout'], 60)

    def test_main_refuses_distinct_installed_binary(self):
        failure, receipt, stages = self.run_case('binary')
        self.assertEqual(failure, 'installed binary differs from verified archive member')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default'])


class PipelineBehavior(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.env = {'PATH': '/usr/bin:/bin', 'HOME': str(self.root)}

    def executable(self, name, body):
        path = self.root / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)
        return str(path)

    def pipeline(self, curl_body, tee='/usr/bin/tee'):
        curl = self.executable('curl', curl_body)
        return subject.run_pipeline(curl, tee, '/bin/sh', 'https://example.invalid/install',
                                    self.root / 'captured', self.env, self.root / 'output', timeout=3)

    def test_real_stdin_and_exact_capture(self):
        result = self.pipeline("printf '%s\\n' 'printf stdin-ok'")
        self.assertEqual(result['pipeline_status'], [0, 0, 0])
        self.assertEqual((self.root / 'captured').read_bytes(), b'printf stdin-ok\n')
        self.assertEqual((self.root / 'output/stdout').read_bytes(), b'stdin-ok')

    def test_stage_stderr_is_separate_and_bounded(self):
        tee = self.executable('observed-tee', 'printf tee-note >&2; exec /usr/bin/tee "$@"')
        self.pipeline("printf curl-note >&2; printf '%s\\n' 'printf shell-note >&2'", tee)
        for stage, text in [('curl', b'curl-note'), ('tee', b'tee-note'), ('sh', b'shell-note')]:
            self.assertEqual((self.root / 'output' / (stage + '-stderr')).read_bytes(), text)
        for stage in ('curl', 'tee', 'sh'):
            producer = self.executable('producer-' + stage, "yes x >&2" if stage == 'curl' else
                                       "printf '%s\\n' 'yes x >&2'" if stage == 'sh' else
                                       "printf '%s\\n' 'exit 0'")
            selected_tee = self.executable('noisy-tee', 'yes x >&2') if stage == 'tee' else '/usr/bin/tee'
            output = self.root / ('overflow-' + stage)
            with self.assertRaisesRegex(ValueError, stage + '-stderr output ceiling'):
                subject.run_pipeline(producer, selected_tee, '/bin/sh', 'https://example.invalid/',
                                     self.root / ('capture-' + stage), self.env, output,
                                     output_limit=1024)
            self.assertLessEqual((output / (stage + '-stderr')).stat().st_size, 1024)
            record = json.loads((output / 'command.json').read_text())
            self.assertIn(stage + '-stderr output ceiling', record['failure'])

    def test_curl_partial_successful_shell_is_red(self):
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 0'; exit 23")
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [23, 0, 0])

    def test_tee_failure_is_red(self):
        tee = self.executable('bad-tee', 'cat >/dev/null; exit 19')
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 0'", tee)
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [0, 19, 0])

    def test_shell_failure_is_red(self):
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 17'")
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [0, 0, 17])

    def test_output_bound_is_applied_before_retention(self):
        with self.assertRaisesRegex(ValueError, 'output ceiling'):
            subject.supervise([sys.executable, '-c', 'print("x"*10000)'], self.env,
                              self.root / 'cap', output_limit=1024)
        self.assertLessEqual((self.root / 'cap/stdout').stat().st_size, 1024)
        self.assertEqual(json.loads((self.root / 'cap/command.json').read_text())['status'], 'failed')

    def test_descendant_does_not_survive_timeout(self):
        import time
        def exercise(name):
            marker = self.root / (name + '-survived')
            program = self.executable(name, f'(echo descendant-ready; sleep 1; touch "{marker}") & wait')
            with self.assertRaises(TimeoutError):
                subject.supervise([program], self.env, self.root / (name + '-output'), timeout=0.3)
            self.assertIn(b'descendant-ready', (self.root / (name + '-output') / 'stdout').read_bytes(),
                          'mandatory descendant witness absent')
            time.sleep(1.1)
            self.assertFalse(marker.exists(), 'descendant survived process-group cleanup')
        exercise('owned-child')
        with mock.patch.object(subject.os, 'killpg', return_value=None):
            with self.assertRaisesRegex(AssertionError, 'descendant survived'):
                exercise('cleanup-removed')

    def test_environment_is_total_allowlist(self):
        with mock.patch.dict(os.environ, {'GH_TOKEN': 'sentinel', 'ASSAY_REQUIRE_PROVENANCE': '1'}):
            env = subject.child_environment(self.root, self.root, self.root)
        self.assertNotIn('GH_TOKEN', env)
        self.assertNotIn('ASSAY_REQUIRE_PROVENANCE', env)
        self.assertNotIn('ASSAY_COSIGN', env)
        self.assertNotIn('GITHUB_TOKEN', env)

    def test_actual_curl_bounds_unknown_length_stream(self):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        import threading
        import subprocess
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200)
                self.end_headers()  # Deliberately no Content-Length.
                try:
                    self.wfile.write(b'x' * 131072)
                except (BrokenPipeError, ConnectionResetError):
                    pass
            def log_message(self, *args):
                pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            path = self.root / 'bounded-transfer'
            result = subprocess.run(['/usr/bin/curl', '-fsSL', '--max-filesize', '65536',
                                     '--max-time', '5', f'http://127.0.0.1:{server.server_port}/',
                                     '-o', str(path)], capture_output=True, timeout=6)
            self.assertEqual(result.returncode, 63, result.stderr)
            self.assertLessEqual(path.stat().st_size, 65536)
        finally:
            server.shutdown(); server.server_close(); thread.join()

    def test_pin_unique_and_same_owner(self):
        path = self.root / 'release.yml'
        path.write_text('          cosign-release: v3.1.3\n')
        self.assertEqual(read_pin(path), 'v3.1.3')
        path.write_text(path.read_text() * 2)
        with self.assertRaises(ValueError):
            read_pin(path)
        path.write_text('cosign-release: latest\n')
        with self.assertRaises(ValueError):
            read_pin(path)


class DocumentedRecipe(unittest.TestCase):
    def test_actual_python_fence_uses_released_assets_and_matches_proxy_argv(self):
        import io
        import types
        import published_release_proxy_phase as proxy
        source = (ROOT / 'docs/guides/installed-release-journey.md').read_text()
        program = proxy.guide_blocks(source)['deny-python']
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            fixture = root / 'packaged'
            (fixture / 'policies').mkdir(parents=True)
            for name in ('mock_github_mcp.py', 'baseline-approved.json', 'policies/no-allowance.yaml'):
                (fixture / name).write_text('trusted fixture')
            output = io.StringIO(json.dumps({'id': 9, 'error': {'code': -31999, 'data': {'reason': 'no_declared_allowance'}}}) + '\n')
            child = mock.Mock(stdin=io.StringIO(), stdout=output)
            child.wait.return_value = 0
            child.poll.return_value = 0
            old = Path.cwd()
            try:
                os.chdir(root)
                with mock.patch('subprocess.Popen', return_value=child) as spawn, mock.patch.object(sys, 'argv', ['deny.py', '/released/server', str(fixture)]):
                    exec(compile(program, 'documented-deny.py', 'exec'), {})
                documented = spawn.call_args.args[0]
                with mock.patch.object(sys, 'argv', ['proxy', '--fixture-dir', str(fixture)]), \
                     mock.patch.object(sys, 'stdin', types.SimpleNamespace(buffer=io.BytesIO(b'{}\n'))), \
                     mock.patch.object(proxy.shutil, 'which', return_value='/released/server'), \
                     mock.patch.object(proxy, 'run_proxy_child', return_value=0) as run:
                    self.assertEqual(proxy.main(), 0)
                executed = run.call_args.args[0]
                # The public recipe names cwd outputs; the harness makes them absolute.
                for flag in ('--enforcement-decision-out', '--denied-call-observation-out'):
                    index = documented.index(flag) + 1
                    documented[index] = str(root / documented[index])
                self.assertEqual(documented, executed)
            finally:
                os.chdir(old)

    def test_explicit_profile_and_windows_route_are_published(self):
        guide = (ROOT / 'docs/guides/installed-release-journey.md').read_text()
        install = (ROOT / 'docs/getting-started/installation.md').read_text()
        self.assertIn('```powershell', guide)
        self.assertIn('assay evidence verify-privileged-mcp-action action.bundle.tar.gz --profile-version v1 --format json', guide)
        self.assertIn('assay evidence verify-privileged-mcp-action <bundle> --profile-version v1 --format json', install)
        self.assertIn('default remains profile v0', install)
        self.assertNotIn('git clone', guide)
        self.assertNotIn('conformance/', guide)


class ModeAssertions(unittest.TestCase):
    def test_default_and_signed_observations_cannot_be_confused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive, sha, tag = 'asset.tar.gz', 'a' * 64, 'v6.9.0'
            common = f'verification=checksum_verified asset={archive} sha256={sha}\nverification=provenance_not_requested\n'
            skipped = 'verification=signed_manifest_skipped reason=cosign_not_installed\n'
            signed = f'verification=signed_manifest_verified asset={archive} identity=https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/{tag}\n'
            (root / 'sh-stderr').write_text('')
            for mode, extra in [('default', skipped), ('signed', signed)]:
                (root / 'stdout').write_text(common + extra)
                subject.assert_install(mode, root, archive, sha, tag)
                with self.assertRaises(ValueError):
                    subject.assert_install('signed' if mode == 'default' else 'default', root, archive, sha, tag)
                with self.assertRaises(ValueError):
                    subject.assert_install(mode, root, archive, 'b' * 64, tag)

    def test_release_identity_ignores_download_count_but_not_digest(self):
        data = {'id': 1, 'tag_name': 'v6.9.0', 'draft': False, 'prerelease': False,
                'assets': [{'id': 2, 'name': 'asset', 'size': 3, 'digest': 'sha256:a', 'download_count': 4}]}
        before = subject.release_identity(data)
        data['assets'][0]['download_count'] += 1
        self.assertEqual(before, subject.release_identity(data))
        data['assets'][0]['digest'] = 'sha256:b'
        self.assertNotEqual(before, subject.release_identity(data))


if __name__ == '__main__':
    unittest.main()
