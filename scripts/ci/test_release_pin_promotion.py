"""Offline integration tests: real renderer/checker; synthetic publication metadata."""
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/ci/promote-release-pin.py'
PATHS = (
    '.github/assay-release-tag', '.github/assay-release-run-id', 'README.md', 'SECURITY.md',
    'docs/AIcontext/user-flows.md', 'docs/getting-started/ci-integration.md',
    'docs/getting-started/index.md', 'docs/getting-started/installation.md',
    'docs/getting-started/quickstart.md', 'docs/guides/editor-mcp-recipe.md',
    'docs/index.md', 'docs/reference/cli/index.md', 'docs/reference/release.md',
    'docs/use-cases/air-gapped.md', 'docs/use-cases/ci-gate.md',
    'examples/mcp-quickstart/README.md', 'docs/generated/agent-golden-path.json',
    'docs/guides/agent-golden-path.md',
    'packaging/agent-plugin/skills/assay-golden-path/references/agent-golden-path.json',
    'packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json',
)


def command(args, root):
    env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
    env['PYTHONDONTWRITEBYTECODE'] = '1'
    env.pop('GITHUB_OUTPUT', None)
    return subprocess.run(args, cwd=root, env=env, text=True, capture_output=True)


class PromotionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = (Path(self.tmp.name) / 'repo').resolve()
        self.root.mkdir()
        names = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
        names += ['scripts/ci/promote-release-pin.py', 'scripts/ci/lib/published_release.py']
        for name in filter(None, names):
            source = ROOT / name
            dest = self.root / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, dest)
        names = sorted(set(filter(None, names)))
        initialized = command(['git', 'init', '-q'], self.root)
        self.assertEqual(initialized.returncode, 0, initialized.stderr)
        # Reconstruct the known tracked set, including tracked-but-ignored files.
        staged = command(['git', 'update-index', '--add', '--', *names], self.root)
        self.assertEqual(staged.returncode, 0, staged.stderr)
        tracked = command(['git', 'ls-files', '-z'], self.root)
        self.assertEqual(tracked.returncode, 0, tracked.stderr)
        self.assertEqual(set(filter(None, tracked.stdout.split('\0'))), set(names))
        for name in PATHS:
            if name not in ('docs/guides/agent-golden-path.md',) and not name.endswith('.json'):
                path = self.root / name
                path.write_text(path.read_text().replace('6.9.0', '6.8.0'))
        (self.root / '.github/assay-release-run-id').write_text('36200000000\n')
        install = self.root / 'docs/getting-started/installation.md'
        install.write_text(install.read_text().replace('36297507646', '36200000000'))
        generated = command([sys.executable, 'scripts/docs/generate-agent-golden-path.py'], self.root)
        self.assertEqual(generated.returncode, 0, generated.stderr)
        self.metadata = Path(self.tmp.name) / 'promotion.json'
        self.identity = self.fixture('v6.9.0', 36297507646)
        self.metadata.write_text(json.dumps(self.identity))

    def fixture(self, tag, run):
        sha = 'a' * 40
        archive = f'assay-{tag}-x86_64-unknown-linux-gnu.tar.gz'
        return {
            'release': {'tag_name': tag, 'draft': False, 'prerelease': False,
                        'assets': [{'name': archive}, {'name': archive + '.sha256'}]},
            'run': {'id': run, 'head_branch': tag, 'head_sha': sha, 'path': '.github/workflows/release.yml',
                    'repository': {'full_name': 'Rul1an/assay'}, 'status': 'completed',
                    'conclusion': 'failure', 'event': 'push', 'run_attempt': 1},
            'jobs': {'total_count': 4, 'jobs': [
                {'name': name, 'run_id': run, 'head_sha': sha, 'run_attempt': 1,
                 'status': 'completed', 'conclusion': 'success'}
                for name in ('Create Release', 'Publish to crates.io', 'Verify published image (ubuntu-latest)',
                             'Verify published image (ubuntu-24.04-arm)')]},
            'image_binding': {'tag': tag, 'run_id': run, 'head_sha': sha,
                              'digest': 'sha256:' + 'b' * 64},
        }

    def run_generator(self, *args):
        return command([sys.executable, str(SCRIPT), '--root', str(self.root),
                        '--metadata', str(self.metadata), *args], ROOT)

    def test_current_checkout_noop_preserves_tracked_ignored_lock(self):
        lock = 'third_party/serde_jcs-0.2.0/Cargo.lock'
        # Both the real source and reconstructed fixture must retain this input.
        for root in (ROOT, self.root):
            tracked = command(['git', 'ls-files', '--error-unmatch', '--', lock], root)
            self.assertEqual(tracked.returncode, 0, tracked.stderr)
            ignored = command(['git', 'check-ignore', '--no-index', '--', lock], root)
            self.assertEqual(ignored.returncode, 0, ignored.stderr)
        tag = (ROOT / '.github/assay-release-tag').read_text().strip()
        run_id = int((ROOT / '.github/assay-release-run-id').read_text())
        metadata = self.fixture(tag, run_id)
        install = (ROOT / 'docs/getting-started/installation.md').read_text()
        digests = set(re.findall(r'ghcr.io/rul1an/assay-mcp-server@(sha256:[0-9a-f]{64})', install))
        self.assertEqual(len(digests), 1)
        metadata['image_binding']['digest'] = digests.pop()
        self.metadata.write_text(json.dumps(metadata))
        before = (ROOT / lock).read_bytes()
        result = command([sys.executable, str(SCRIPT), '--root', str(ROOT),
                          '--metadata', str(self.metadata), '--check'], ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, '')
        self.assertEqual((ROOT / lock).read_bytes(), before)

    def test_dry_run_is_deterministic_and_does_not_edit(self):
        before = {n: (self.root / n).read_bytes() for n in PATHS}
        result = self.run_generator()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, self.run_generator().stdout)
        self.assertEqual(before, {n: (self.root / n).read_bytes() for n in PATHS})


    def test_previous_to_current_all_surfaces_and_idempotence(self):
        before = {n: (self.root / n).read_bytes() for n in PATHS}
        result = self.run_generator('--apply')
        self.assertEqual(result.returncode, 0, result.stderr)
        changed = {n for n, data in before.items() if (self.root / n).read_bytes() != data}
        self.assertEqual(changed, set(PATHS))
        self.assertEqual(self.run_generator('--check').returncode, 0)
        self.assertEqual(self.run_generator('--apply').stdout, '')
        for args in ([sys.executable, 'scripts/docs/generate-agent-golden-path.py', '--check'],
                     ['bash', 'scripts/ci/check-release-surface.sh']):
            check = command(args, self.root)
            self.assertEqual(check.returncode, 0, check.stdout + check.stderr)
        generated = json.loads((self.root / 'docs/generated/agent-golden-path.json').read_text())
        self.assertEqual(generated['source_version'], '6.9.0')
        self.assertEqual(generated['release_version'], '6.9.0')
        # The generator's baseline guard and the existing checker must both catch
        # stale install commands on each of their overlapping installation surfaces.
        for name in ('README.md', 'docs/getting-started/index.md',
                     'docs/getting-started/installation.md', 'docs/getting-started/quickstart.md',
                     'docs/getting-started/ci-integration.md', 'docs/reference/cli/index.md',
                     'docs/AIcontext/user-flows.md', 'docs/use-cases/ci-gate.md',
                     'docs/guides/editor-mcp-recipe.md'):
            with self.subTest(path=name):
                path = self.root / name
                correct = path.read_bytes()
                path.write_bytes(correct.replace(b'cargo install assay-cli --version 6.9.0',
                                                 b'cargo install assay-cli --version 6.8.0'))
                checker = command(['bash', 'scripts/ci/check-release-surface.sh'], self.root)
                self.assertNotEqual(checker.returncode, 0, name)
                self.assertNotEqual(self.run_generator('--check').returncode, 0, name)
                path.write_bytes(correct)

    def test_future_fixture_preserves_history_and_source_identity(self):
        # Future source release preparation is a synthetic scratch fixture only.
        for path in self.root.rglob('Cargo.toml'):
            path.write_text(path.read_text().replace('6.9.0', '7.1.2'))
        for path in self.root.rglob('Cargo.lock'):
            path.write_text(path.read_text().replace('6.9.0', '7.1.2'))
        generated = command([sys.executable, 'scripts/docs/generate-agent-golden-path.py'], self.root)
        self.assertEqual(generated.returncode, 0, generated.stderr)
        history = self.root / 'CHANGELOG.md'
        historical_bytes = history.read_bytes()
        before = {name: (self.root / name).read_bytes() for name in PATHS}
        self.identity = self.fixture('v7.1.2', 40000000000)
        self.metadata.write_text(json.dumps(self.identity))
        result = self.run_generator('--apply')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual({name for name, data in before.items()
                          if (self.root / name).read_bytes() != data}, set(PATHS))
        self.assertEqual(history.read_bytes(), historical_bytes)
        self.assertEqual(self.run_generator('--check').returncode, 0)
        self.assertIn('`7.1`', (self.root / 'docs/getting-started/installation.md').read_text())

    def test_bad_metadata_never_edits(self):
        import copy
        before = {n: (self.root / n).read_bytes() for n in PATHS}
        mutations = [
            ('release', 'draft', True), ('release', 'prerelease', True),
            ('release', 'assets', []), ('run', 'head_branch', 'v6.8.0'),
            ('run', 'path', '.github/workflows/ci.yml'), ('run', 'id', True),
            ('run', 'status', 'in_progress'), ('image_binding', 'run_id', 1),
            ('image_binding', 'digest', 'sha256:bad'), ('image_binding', 'head_sha', 'b' * 40),
            ('jobs', 'total_count', 5), ('jobs', 'jobs', []),
        ]
        for section, key, value in mutations:
            with self.subTest(section=section, key=key):
                bad = copy.deepcopy(self.identity)
                bad[section][key] = value
                self.metadata.write_text(json.dumps(bad))
                self.assertEqual(self.run_generator('--apply').returncode, 2)
                self.assertEqual(before, {n: (self.root / n).read_bytes() for n in PATHS})
        bad = copy.deepcopy(self.identity)
        bad['jobs']['jobs'][1]['conclusion'] = 'skipped'
        self.metadata.write_text(json.dumps(bad))
        self.assertEqual(self.run_generator('--apply').returncode, 2)

    def test_missing_surface_and_drift_refuse_without_partial_edits(self):
        for name in ('README.md', 'docs/use-cases/air-gapped.md'):
            path = self.root / name
            original = path.read_bytes()
            path.write_bytes(original.replace(b'6.8.0', b'6.7.0'))
            before = {n: (self.root / n).read_bytes() for n in PATHS}
            self.assertEqual(self.run_generator('--apply').returncode, 2)
            self.assertEqual(before, {n: (self.root / n).read_bytes() for n in PATHS})
            path.write_bytes(original)
        path = self.root / 'SECURITY.md'
        path.unlink()
        self.assertEqual(self.run_generator('--apply').returncode, 2)
        self.assertEqual((self.root / '.github/assay-release-tag').read_text(), 'v6.8.0\n')

    def test_duplicate_metadata_and_unexpected_generated_output_refuse(self):
        original = self.metadata.read_text()
        self.metadata.write_text(original.replace('"draft": false', '"draft": true, "draft": false'))
        self.assertEqual(self.run_generator('--apply').returncode, 2)
        self.metadata.write_text(original)
        renderer = self.root / 'scripts/docs/generate-agent-golden-path.py'
        text = renderer.read_text()
        text = text.replace('def write_outputs(outputs: list[tuple[Path, bytes]]) -> None:',
                            'def write_outputs(outputs: list[tuple[Path, bytes]]) -> None:\n    (ROOT / "unexpected-output.txt").write_text("drift")')
        renderer.write_text(text)
        before = {n: (self.root / n).read_bytes() for n in PATHS}
        result = self.run_generator('--apply')
        self.assertEqual(result.returncode, 2, result.stdout)
        self.assertIn('outside promotion scope', result.stderr)
        self.assertEqual(before, {n: (self.root / n).read_bytes() for n in PATHS})
        self.assertFalse((self.root / 'unexpected-output.txt').exists())

    def test_apply_rolls_back_reported_write_error(self):
        import importlib.util
        from unittest.mock import patch
        spec = importlib.util.spec_from_file_location('promotion', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        original = {'README.md': b'first', 'SECURITY.md': b'second'}
        for name, data in original.items():
            (self.root / name).write_bytes(data)
        install = module.install_bytes
        calls = 0
        def fail_second(path, data):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('synthetic disk write failure')
            install(path, data)
        with patch.object(module, 'install_bytes', side_effect=fail_second):
            with self.assertRaisesRegex(OSError, 'synthetic disk'):
                module.apply(self.root, original, {name: b'new' for name in original})
        self.assertEqual(original, {n: (self.root / n).read_bytes() for n in original})


class PublicationGateTests(unittest.TestCase):
    def setUp(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location('promotion', SCRIPT)
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.packet = PromotionTests.fixture(None, 'v6.9.0', 36297507646)
        self.assertEqual(len(self.packet['jobs']['jobs']), self.packet['jobs']['total_count'])
        self.assertEqual(sum(job['name'] == 'Publish to crates.io' for job in self.packet['jobs']['jobs']), 1)

    def test_crates_publication_is_required_and_bound(self):
        import copy
        cases = [('failure', 'conclusion', 'failure'), ('skipped', 'conclusion', 'skipped'),
                 ('cancelled', 'conclusion', 'cancelled'), ('missing', None, None),
                 ('duplicate', None, None), ('foreign-run', 'run_id', 36297507647),
                 ('foreign-head', 'head_sha', 'b' * 40), ('foreign-attempt', 'run_attempt', 2),
                 ('boolean-attempt', 'run_attempt', True), ('in-progress', 'status', 'in_progress')]
        for name, key, value in cases:
            with self.subTest(case=name):
                packet = copy.deepcopy(self.packet)
                jobs = packet['jobs']['jobs']
                job = next(row for row in jobs if row['name'] == 'Publish to crates.io')
                if name == 'missing':
                    jobs.remove(job)
                elif name == 'duplicate':
                    jobs.append(copy.deepcopy(job))
                else:
                    job[key] = value
                packet['jobs']['total_count'] = len(jobs)
                with self.assertRaisesRegex(ValueError, 'Publish to crates.io'):
                    self.module.identity(packet)

    def test_retained_run_projection_allows_unrelated_failure(self):
        # Projection of retained API metadata, not an authenticated fixture or artifact.
        # https://github.com/Rul1an/assay/actions/runs/36297507646 (attempt 1)
        # Crates publication succeeded; the Windows post-publication journey failed.
        head = '61f1adf57302fb8c49ebdbbb29e1ae2adac9694e'
        self.packet['run']['head_sha'] = head
        self.packet['image_binding'].update(head_sha=head,
            digest='sha256:be2abc91d27be6d4203eca031ccdbe07316359cc7bd614313699d18cef7789dc')
        for job in self.packet['jobs']['jobs']:
            job['head_sha'] = head
        self.packet['jobs']['jobs'].append(dict(self.packet['jobs']['jobs'][0],
            name='Verify the published release journey / Windows x86_64 post-publication journey',
            conclusion='failure'))
        self.packet['jobs']['total_count'] = len(self.packet['jobs']['jobs'])
        self.assertEqual(self.packet['run']['conclusion'], 'failure')
        self.assertEqual(self.module.identity(self.packet), ('v6.9.0', '36297507646',
            'sha256:be2abc91d27be6d4203eca031ccdbe07316359cc7bd614313699d18cef7789dc'))


if __name__ == '__main__':
    unittest.main()
