"""Offline boundary tests. Synthetic metadata does not prove hosted App behavior."""
import base64
import copy
from contextlib import ExitStack
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('automation', ROOT / 'scripts/ci/release-pin-promotion.py')
auto = importlib.util.module_from_spec(spec)
spec.loader.exec_module(auto)


def archive(data, name='release-pin-binding.json', mode=stat.S_IFREG | 0o600):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w') as z:
        entry = zipfile.ZipInfo(name)
        entry.external_attr = mode << 16
        z.writestr(entry, json.dumps(data))
    return stream.getvalue()


class FixtureAPI:
    def __init__(self):
        self.run = {'id': 42, 'head_branch': 'v6.9.0', 'head_sha': 'a' * 40,
                    'path': auto.WORKFLOW, 'repository': {'full_name': auto.REPO},
                    'status': 'completed', 'conclusion': 'failure', 'event': 'push',
                    'run_attempt': 1, 'workflow_id': 77}
        self.release = {'id': 12, 'tag_name': 'v6.9.0', 'draft': False, 'prerelease': False,
                        'published_at': '2026-09-27T05:48:29Z', 'updated_at': '2026-09-27T05:48:29Z',
                        'assets': [{'id': i, 'name': 'assay-v6.9.0-x86_64-unknown-linux-gnu.tar.gz' + suffix}
                                   for i, suffix in enumerate(('', '.sha256'), 1)]}
        self.jobs = [dict(name=name, run_id=42, run_attempt=1, head_sha='a' * 40,
                         status='completed', conclusion='success') for name in (
            'Publish GHCR image', 'Create Release', 'Publish to crates.io', 'Verify published image (ubuntu-latest)',
            'Verify published image (ubuntu-24.04-arm)')]
        self.data = dict(schema='assay.release-pin-binding.v1', repository=auto.REPO,
                         workflow_path=auto.WORKFLOW, run_id=42, run_attempt=1, head_sha='a' * 40,
                         tag='v6.9.0', image=auto.IMAGE, digest='sha256:' + 'b' * 64)
        self.source = (ROOT / auto.WORKFLOW).read_text()
        self.tag_head = 'a' * 40
        self.base = 'c' * 40
        self.pack()
        self.prs = []
        self.remote = None
        self.writes = []

    def pack(self):
        self.zip = archive(self.data)
        self.artifacts = [dict(id=51, name='assay-release-pin-binding-42-1', expired=False,
                               size_in_bytes=len(self.zip), workflow_run={'id': 42, 'head_sha': 'a' * 40},
                               digest='sha256:' + hashlib.sha256(self.zip).hexdigest())]

    def pages(self, suffix, key=None):
        if key == 'jobs':
            assert suffix == 'actions/runs/42/attempts/1/jobs'
            return copy.deepcopy(self.jobs)
        if key == 'artifacts':
            return copy.deepcopy(self.artifacts)
        assert suffix.startswith('pulls?state=all&head=Rul1an:codex/release-pin-')
        return copy.deepcopy(self.prs)

    def get(self, suffix, **kwargs):
        if kwargs.get('method'):
            self.writes.append((suffix, kwargs))
            return {}
        values = {'actions/runs/42': self.run,
                  'actions/workflows/release.yml': {'id': 77, 'path': auto.WORKFLOW},
                  'releases/latest': self.release,
                  'git/ref/tags/v6.9.0': {'object': {'type': 'commit', 'sha': self.tag_head}},
                  'contents/' + auto.WORKFLOW + '?ref=' + 'a' * 40:
                      {'encoding': 'base64', 'content': base64.b64encode(self.source.encode()).decode()},
                  'actions/artifacts/51/zip': self.zip,
                  'git/ref/heads/main': {'object': {'sha': self.base}},
                  'git/ref/heads/codex/release-pin-v6.9.0': self.remote}
        return copy.deepcopy(values[suffix])


class Collector(unittest.TestCase):
    def test_successful_required_jobs_allow_unrelated_run_failure(self):
        metadata, fresh = auto.collect(FixtureAPI(), 42)
        self.assertEqual(auto.promotion.identity(metadata), ('v6.9.0', '42', 'sha256:' + 'b' * 64))
        self.assertEqual(fresh['artifact'], 51)

    def test_x64_aliases_use_shared_identity(self):
        for name in ('Verify published image (ubuntu-24.04)',
                     'Verify published image (ubuntu-latest)'):
            with self.subTest(name=name):
                api = FixtureAPI()
                x64 = next(j for j in api.jobs if j['name'] == 'Verify published image (ubuntu-latest)')
                x64['name'] = name
                with patch.object(auto.promotion, 'identity', wraps=auto.promotion.identity) as shared:
                    auto.collect(api, 42)
                    shared.assert_called_once()
                api.jobs.append(dict(x64, name=(
                    'Verify published image (ubuntu-latest)' if name.endswith('(ubuntu-24.04)')
                    else 'Verify published image (ubuntu-24.04)')))
                with self.assertRaises(ValueError):
                    auto.collect(api, 42)

    def test_shared_identity_refusal_reaches_collector(self):
        with patch.object(auto.promotion, 'identity', side_effect=ValueError('shared identity sentinel')):
            with self.assertRaisesRegex(ValueError, 'shared identity sentinel'):
                auto.collect(FixtureAPI(), 42)

    def test_image_producer_uses_shared_job_rule(self):
        rule = auto.promotion._require_single_successful_job
        producer = ('Publish GHCR image',)
        expected = {'run_id': 42, 'run_attempt': 1, 'head_sha': 'a' * 40,
                    'status': 'completed', 'conclusion': 'success'}
        with self.subTest(route='successful producer delegates'):
            api = FixtureAPI()
            with patch.object(auto.promotion, '_require_single_successful_job', wraps=rule) as shared:
                auto.collect(api, 42)
            calls = [call for call in shared.call_args_list if call.args[1] == producer]
            self.assertEqual(len(calls), 1, 'image producer must use the shared job rule exactly once')
            self.assertEqual(calls[0].args[0], api.jobs)
            self.assertEqual(calls[0].args[2], expected)

        def refuse_producer(jobs, names, binding, label):
            if names == producer:
                raise ValueError('producer shared rule sentinel')
            return rule(jobs, names, binding, label)

        with self.subTest(route='producer refusal propagates'):
            with patch.object(auto.promotion, '_require_single_successful_job', side_effect=refuse_producer):
                with self.assertRaisesRegex(ValueError, 'producer shared rule sentinel'):
                    auto.collect(FixtureAPI(), 42)

    def test_source_outside_producer_cannot_replace_artifact(self):
        api = FixtureAPI()
        api.source += '\n  another-job:\n    steps: []\n'
        with self.assertRaisesRegex(ValueError, 'source contract'):
            auto.collect(api, 42)

    def test_missing_legacy_artifact_is_refused(self):
        api = FixtureAPI()
        api.artifacts = []
        with self.assertRaisesRegex(ValueError, 'artifact missing'):
            auto.collect(api, 42)

    def test_run_job_tag_and_artifact_mismatches(self):
        changes = [lambda a: a.run.update(event='workflow_dispatch'),
                   lambda a: a.run.update(path='.github/workflows/foreign.yml'),
                   lambda a: a.run.update(workflow_id=78),
                   lambda a: a.run.update(repository={'full_name': 'fork/assay'}),
                   lambda a: a.run.update(status='in_progress'),
                   lambda a: setattr(a, 'tag_head', 'd' * 40),
                   lambda a: a.release.update(draft=True),
                   lambda a: a.jobs[0].update(run_attempt=True),
                   lambda a: a.artifacts[0].update(expired=True),
                   lambda a: a.artifacts[0].update(size_in_bytes=auto.LIMIT + 1),
                   lambda a: a.artifacts[0].update(digest='sha256:' + '0' * 64),
                   lambda a: a.artifacts.append(copy.deepcopy(a.artifacts[0]))]
        for mutate in changes:
            with self.subTest(mutation=changes.index(mutate)):
                api = FixtureAPI()
                mutate(api)
                with self.assertRaises(ValueError):
                    auto.collect(api, 42)
        for index in range(5):
            for conclusion in ('failure', 'skipped', None):
                api = FixtureAPI()
                api.jobs[index]['conclusion'] = conclusion
                with self.subTest(job=index, conclusion=conclusion), self.assertRaises(ValueError):
                    auto.collect(api, 42)
        for key, value in [('run_id', 43), ('run_attempt', True), ('head_sha', 'd' * 40),
                           ('image', 'ghcr.io/foreign/image'), ('tag', 'v6.8.0'),
                           ('digest', 'latest'), ('extra', 'field')]:
            api = FixtureAPI()
            api.data[key] = value
            api.pack()
            with self.subTest(binding=key), self.assertRaises(ValueError):
                auto.collect(api, 42)

    def test_crates_publication_uses_shared_identity(self):
        for variant in ('failure', 'skipped', 'cancelled', 'missing', 'duplicate'):
            api = FixtureAPI()
            job = next(row for row in api.jobs if row['name'] == 'Publish to crates.io')
            if variant == 'missing':
                api.jobs.remove(job)
            elif variant == 'duplicate':
                api.jobs.append(copy.deepcopy(job))
            else:
                job['conclusion'] = variant
            with self.subTest(case=variant), self.assertRaisesRegex(ValueError, 'Publish to crates.io'):
                auto.collect(api, 42)

    def test_archive_closed_member_and_bounded_decode(self):
        for name, mode in [('../release-pin-binding.json', stat.S_IFREG),
                           ('release-pin-binding.json', stat.S_IFLNK),
                           ('release-pin-binding.json', stat.S_IFIFO)]:
            with self.subTest(name=name, mode=mode), self.assertRaises(ValueError):
                auto.binding(archive({}, name, mode))
        for raw in (b'{"a":1,"a":2}', b' ' * (auto.LIMIT + 1)):
            with self.assertRaises(ValueError):
                auto.decode(raw)

    def test_raw_nul_filename_is_refused(self):
        name = 'release-pin-binding.jsonXY'
        raw = archive({}, name)
        self.assertEqual(raw.count(name.encode()), 2)  # local and central directory names
        raw = raw.replace(name.encode(), b'release-pin-binding.json\x00Y')
        with zipfile.ZipFile(io.BytesIO(raw)) as z:
            self.assertEqual(z.infolist()[0].filename, 'release-pin-binding.json')
            self.assertEqual(z.infolist()[0].orig_filename, 'release-pin-binding.json\x00Y')
        with self.assertRaisesRegex(ValueError, 'artifact member'):
            auto.binding(raw)

    def test_pagination_is_complete_and_bounded(self):
        api = auto.API()
        with patch.object(api, 'get', side_effect=[{'total_count': 101, 'jobs': [{}] * 100},
                                                  {'total_count': 101, 'jobs': [{}]}]):
            self.assertEqual(len(api.pages('jobs', 'jobs')), 101)
        with patch.object(api, 'get', return_value={'total_count': 2, 'jobs': [{}]}):
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                api.pages('jobs', 'jobs')
        with patch.object(api, 'get', return_value=[{}] * 100):
            with self.assertRaisesRegex(ValueError, '500'):
                api.pages('pulls')


class MarkerContract(unittest.TestCase):
    def setUp(self):
        self.commit = 'd' * 40
        self.branch = 'codex/release-pin-v6.9.0'
        self.fresh = auto.collect(FixtureAPI(), 42)[1]
        self.expected = {'schema': 'assay.release-pin-pr.v1', 'base': self.fresh['base'],
            'tag': 'v6.9.0', 'run': 42, 'attempt': 1, 'release': 12, 'head': 'a' * 40,
            'artifact': 51, 'artifact_digest': self.fresh['artifact_digest'],
            'image_digest': 'sha256:' + 'b' * 64, 'commit': self.commit}

    def owned(self, value):
        pr = {'user': {'login': 'fixture-app[bot]', 'type': 'Bot'},
              'head': {'repo': {'full_name': auto.REPO}, 'ref': self.branch, 'sha': self.commit},
              'base': {'repo': {'full_name': auto.REPO}, 'ref': 'main'}, 'state': 'open',
              'body': auto.MARKER + json.dumps(value) + ' -->'}
        auto.owned_pr(pr, self.branch, 'fixture-app', self.commit)

    def test_emitted_marker_is_complete_versioned_provenance(self):
        self.assertEqual(auto.marker({'fresh': self.fresh}, self.commit), self.expected)
        self.owned(self.expected)

    def test_missing_extra_and_malformed_provenance_refuse(self):
        for key in self.expected:
            value = dict(self.expected)
            value.pop(key)
            with self.subTest(missing=key), self.assertRaises(ValueError):
                self.owned(value)
        with self.subTest(extra=True), self.assertRaises(ValueError):
            self.owned({**self.expected, 'extra': 'field'})
        for key, invalid in [('schema', 'assay.release-pin-pr.v2'), ('base', 'not-a-sha'),
                             ('head', 'not-a-sha'), ('run', '42'), ('attempt', True),
                             ('release', -1), ('artifact', None), ('artifact_digest', 'sha256:bad'),
                             ('image_digest', 'latest'), ('commit', 'e' * 40), ('tag', 'v6.8.0')]:
            with self.subTest(malformed=key), self.assertRaises(ValueError):
                self.owned({**self.expected, key: invalid})


class Events(unittest.TestCase):
    def test_only_main_repository_events_with_strict_ids(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'event.json'
            event = {'repository': {'full_name': auto.REPO}, 'action': 'completed', 'workflow_run': {'id': 42}}
            env = {'GITHUB_REPOSITORY': auto.REPO, 'GITHUB_REF': 'refs/heads/main',
                   'GITHUB_EVENT_NAME': 'workflow_run', 'GITHUB_EVENT_PATH': str(path)}
            path.write_text(json.dumps(event))
            self.assertEqual(auto.event_run(env), 42)
            for key, value in [('GITHUB_REF', 'refs/heads/foreign'), ('GITHUB_REPOSITORY', 'fork/assay'),
                               ('GITHUB_EVENT_NAME', 'pull_request')]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    auto.event_run({**env, key: value})
            event['workflow_run']['id'] = True
            path.write_text(json.dumps(event))
            with self.assertRaises(ValueError):
                auto.event_run(env)
            env['GITHUB_EVENT_NAME'] = 'workflow_dispatch'
            for value in ('42', '0', '42;echo nope', '9' * 21, True):
                event['inputs'] = {'run_id': value}
                path.write_text(json.dumps(event))
                if value == '42':
                    self.assertEqual(auto.event_run(env), 42)
                else:
                    with self.subTest(value=value), self.assertRaises(ValueError):
                        auto.event_run(env)

    def test_redirect_never_forwards_authorization(self):
        request = auto.urllib.request.Request('https://api.github.com/repos/Rul1an/assay/actions/artifacts/1/zip',
                                              headers={'Authorization': 'Bearer synthetic'})
        redirected = auto.Redirect().redirect_request(request, None, 302, 'Found', {}, 'https://example.test/artifact')
        self.assertFalse(redirected.has_header('Authorization'))
        with self.assertRaisesRegex(ValueError, 'non-HTTPS'):
            auto.Redirect().redirect_request(request, None, 302, 'Found', {}, 'http://example.test/artifact')


class PlanHandoff(unittest.TestCase):
    """Exercise the real CLI/file boundary without API, token or Git activity."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.folder = Path(self.tmp.name).resolve()
        self.runner = self.folder / 'runner'
        self.runner.mkdir()
        self.checkout = self.folder / 'checkout'
        self.checkout.mkdir()
        (self.checkout / '.git').mkdir()
        self.plan_path = self.checkout / '.git' / 'assay-release-pin-plan.json'
        self.output = self.folder / 'github-output'
        self.event = self.folder / 'event.json'
        self.event.write_text('{"repository":{"full_name":"Rul1an/assay"},'
                              '"action":"completed","workflow_run":{"id":42}}')
        self.plan = {'fresh': {'run': 42}, 'changes': {'README.md': 'fixture pin\n'}}
        self.encoded = b'{"changes": {"README.md": "fixture pin\\n"}, "fresh": {"run": 42}}\n'
        self.env = {'RUNNER_TEMP': str(self.runner), 'GITHUB_OUTPUT': str(self.output),
                    'GITHUB_REPOSITORY': 'Rul1an/assay', 'GITHUB_REF': 'refs/heads/main',
                    'GITHUB_EVENT_NAME': 'workflow_run', 'GITHUB_EVENT_PATH': str(self.event),
                    'APP_SLUG': 'fixture-app', 'EXPECTED_APP_SLUG': 'fixture-app'}

    def invoke(self, mode, path=None, prepare=None, env=None):
        stderr, stdout = io.StringIO(), io.StringIO()
        real_api = auto.API()  # No constructor I/O; all requests below are sentinels.
        class NoTokenEnvironment(dict):
            def __getitem__(self, key):
                if key in ('GH_TOKEN', 'GH_READ_TOKEN', 'GITHUB_TOKEN'):
                    raise AssertionError('unexpected token lookup')
                return super().__getitem__(key)

            def get(self, key, default=None):
                try:
                    return self[key]
                except KeyError:
                    return default
        # Constructor observation verifies path rejection precedes API setup.
        # Request/Git sentinels are assertions, not caught production refusals.
        with ExitStack() as stack:
            stack.enter_context(patch.object(auto, 'ROOT', self.checkout))
            api = stack.enter_context(patch.object(auto, 'API', return_value=real_api))
            stack.enter_context(patch.object(real_api, 'request',
                side_effect=AssertionError('unexpected API/token boundary')))
            stack.enter_context(patch.object(auto, 'git',
                side_effect=AssertionError('unexpected Git boundary')))
            stack.enter_context(patch.object(auto.promotion, 'run',
                side_effect=AssertionError('unexpected generator process')))
            prepared = stack.enter_context(patch.object(auto, 'prepare',
                side_effect=prepare, return_value=self.plan))
            published = stack.enter_context(patch.object(auto, 'publish', return_value='fixture-result'))
            stack.enter_context(patch.object(auto.os, 'environ',
                NoTokenEnvironment(self.env if env is None else env)))
            stack.enter_context(patch.object(sys, 'argv', ['release-pin-promotion.py', mode,
                '--plan', str(self.plan_path if path is None else path)]))
            stack.enter_context(patch.object(sys, 'stderr', stderr))
            stack.enter_context(patch.object(sys, 'stdout', stdout))
            code = auto.main()
        return code, stderr.getvalue(), stdout.getvalue(), api, prepared, published

    def assert_refused_before_api(self, result):
        code, stderr, _, api, prepared, published = result
        self.assertEqual(code, 2, 'plan handoff must refuse through real main')
        self.assertIn('release-pin automation refused:', stderr)
        api.assert_not_called()
        prepared.assert_not_called()
        published.assert_not_called()
        self.assertFalse(self.output.exists(), 'refusal must not emit changed output')

    def test_prepare_fixed_leaf_writes_exact_plan_bytes(self):
        code, stderr, stdout, api, prepared, published = self.invoke('prepare')
        self.assertEqual((code, stderr), (0, ''))
        self.assertEqual(self.plan_path.read_bytes(), self.encoded)
        self.assertEqual(stdout, 'true\n')
        self.assertEqual(stat.S_IMODE(self.plan_path.stat().st_mode), 0o600)
        self.assertFalse(self.output.exists(), 'Python must not write workflow outputs')
        api.assert_called_once()
        prepared.assert_called_once_with(api.return_value, 42)
        published.assert_not_called()

    def test_publish_fixed_regular_leaf_consumes_saved_plan(self):
        self.plan_path.write_bytes(self.encoded)
        code, stderr, stdout, api, prepared, published = self.invoke('publish')
        self.assertEqual((code, stderr), (0, ''))
        self.assertEqual(stdout, 'release-pin promotion: fixture-result\n')
        self.assertEqual(self.plan_path.read_bytes(), self.encoded)
        api.assert_called_once()
        prepared.assert_not_called()
        published.assert_called_once_with(api.return_value, 42, self.plan, 'fixture-app', 'fixture-app')

    def test_off_location_plan_is_refused_in_both_modes(self):
        for mode in ('prepare', 'publish'):
            for location in ('wrong-leaf', 'outside-runner'):
                with self.subTest(mode=mode, location=location):
                    path = (self.runner / f'{mode}-other.json' if location == 'wrong-leaf'
                            else self.folder / mode / 'release-pin-plan.json')
                    path.parent.mkdir(exist_ok=True)
                    if mode == 'publish':
                        path.write_bytes(self.encoded)
                    self.assert_refused_before_api(self.invoke(mode, path))
                    if mode == 'prepare':
                        self.assertFalse(path.exists(), 'off-location prepare must create nothing')
                    else:
                        self.assertEqual(path.read_bytes(), self.encoded)

    def test_prepare_preexisting_regular_or_symlink_is_not_overwritten(self):
        for kind in ('regular', 'symlink', 'dangling-symlink'):
            with self.subTest(kind=kind):
                target = self.folder / ('preserved-' + kind)
                if kind != 'dangling-symlink':
                    target.write_bytes(b'preserve existing bytes\n')
                if kind == 'regular':
                    self.plan_path.write_bytes(b'preserve existing bytes\n')
                else:
                    self.plan_path.symlink_to(target)
                try:
                    self.assert_refused_before_api(self.invoke('prepare'))
                    if kind == 'dangling-symlink':
                        self.assertFalse(target.exists())
                    else:
                        self.assertEqual(target.read_bytes(), b'preserve existing bytes\n')
                        self.assertEqual(self.plan_path.read_bytes(), b'preserve existing bytes\n')
                finally:
                    self.plan_path.unlink(missing_ok=True)

    def test_prepare_creation_between_validation_and_write_is_exclusive(self):
        def collected(api, run_id):
            self.plan_path.write_bytes(b'new owner\n')
            return self.plan
        code, stderr, _, api, prepared, published = self.invoke('prepare', prepare=collected)
        self.assertEqual(code, 2, 'exclusive plan creation must refuse a late existing file')
        self.assertIn('release-pin automation refused:', stderr)
        prepared.assert_called_once()
        self.assertTrue(self.plan_path.exists(), 'prepare callback must create the late file')
        self.assertEqual(self.plan_path.read_bytes(), b'new owner\n')
        self.assertFalse(self.output.exists())
        api.assert_called_once()
        published.assert_not_called()

    def test_publish_symlink_or_directory_refuses_before_api(self):
        for kind in ('symlink', 'directory'):
            with self.subTest(kind=kind):
                target = self.folder / 'saved-target.json'
                target.write_bytes(self.encoded)
                if kind == 'symlink':
                    self.plan_path.symlink_to(target)
                else:
                    self.plan_path.mkdir()
                try:
                    self.assert_refused_before_api(self.invoke('publish'))
                    self.assertEqual(target.read_bytes(), self.encoded)
                finally:
                    if kind == 'symlink':
                        self.plan_path.unlink()
                    else:
                        self.plan_path.rmdir()

    def test_publish_retains_shared_duplicate_key_and_size_refusals(self):
        for name, raw in [('duplicate', b'{"changes":{},"changes":{}}'),
                          ('oversize', b' ' * 1048577)]:
            with self.subTest(name=name):
                self.plan_path.write_bytes(raw)
                code, stderr, _, _, _, published = self.invoke('publish')
                self.assertEqual(code, 2)
                self.assertRegex(stderr, 'duplicate metadata key|1048576-byte limit')
                published.assert_not_called()
                self.assertEqual(self.plan_path.read_bytes(), raw)

    def test_environment_cannot_select_plan_or_output_sink(self):
        # Supersedes RUNNER_TEMP-required: only script ROOT selects the location.
        for forged in (False, True):
            with self.subTest(forged=forged):
                env = {k: v for k, v in self.env.items() if k != 'RUNNER_TEMP'}
                self.output.write_bytes(b'preserve workflow target\n')
                if forged:
                    env['RUNNER_TEMP'] = str(self.runner)
                code, stderr, stdout, _, _, _ = self.invoke('prepare', env=env)
                self.assertEqual((code, stderr, stdout), (0, '', 'true\n'))
                self.assertEqual(self.plan_path.read_bytes(), self.encoded)
                self.assertEqual(self.output.read_bytes(), b'preserve workflow target\n')
                self.assertEqual(list(self.runner.iterdir()), [])
                self.assertEqual(self.invoke('publish', env=env)[0], 0)
                self.plan_path.unlink()

    def test_invalid_git_directory_refuses_before_api(self):
        gitdir = self.checkout / '.git'
        gitdir.rmdir()
        target = self.folder / 'foreign-gitdir'
        target.mkdir()
        for kind in ('absent', 'symlink', 'linked-worktree-file'):
            with self.subTest(kind=kind):
                if kind == 'symlink':
                    gitdir.symlink_to(target, target_is_directory=True)
                elif kind == 'linked-worktree-file':
                    gitdir.write_text('gitdir: ' + str(target) + '\n')
                try:
                    for mode in ('prepare', 'publish'):
                        self.assert_refused_before_api(self.invoke(mode))
                    self.assertEqual(list(target.iterdir()), [])
                finally:
                    if kind != 'absent':
                        gitdir.unlink()
                    # A failing no-follow mutant may have created this owned leaf.
                    # Restore the next case only after retaining the failed assertion.
                    (target / 'assay-release-pin-plan.json').unlink(missing_ok=True)

    def test_false_plan_emits_only_false(self):
        self.plan['changes'] = {}
        code, stderr, stdout, _, prepared, _ = self.invoke('prepare')
        self.assertEqual((code, stderr, stdout), (0, '', 'false\n'))
        prepared.assert_called_once()
        self.assertEqual(json.loads(self.plan_path.read_bytes()), self.plan)
        self.assertFalse(self.output.exists())

    def test_publish_fifo_refuses_without_waiting(self):
        os.mkfifo(self.plan_path, 0o600)
        self.assert_refused_before_api(self.invoke('publish'))
        self.assertTrue(stat.S_ISFIFO(self.plan_path.lstat().st_mode))


class WorkflowOutputAdapter(unittest.TestCase):
    def invoke(self, raw, status):
        doc = Wiring().mapping('.github/workflows/release-pin-promotion.yml')
        step = doc['jobs']['promote']['steps'][1]
        self.assertEqual(step['id'], 'prepare')
        self.assertEqual(step['shell'], 'bash')
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            output, witness = folder / 'output', folder / 'witness'
            validator = folder / 'validator-invocation'
            forbidden = folder / 'must-not-execute'
            wrapper = folder / 'python3'
            wrapper.write_text('#!' + sys.executable + '\n' +
                'import json, os, pathlib, sys\n' +
                "if sys.argv[1:] == ['scripts/ci/release-pin-promotion.py', 'validate-output']:\n" +
                '    pathlib.Path(' + repr(str(validator)) + ').write_text("actual-helper-exec\\n")\n' +
                '    os.execv(' + repr(sys.executable) + ', [' + repr(sys.executable) + ', ' +
                repr(str(ROOT / 'scripts/ci/release-pin-promotion.py')) + ', "validate-output"])\n' +
                'pathlib.Path(' + repr(str(witness)) + ').write_text(json.dumps(sys.argv[1:]))\n' +
                'sys.stdout.buffer.write(' + repr(raw) + ')\n' +
                'raise SystemExit(' + repr(status) + ')\n')
            wrapper.chmod(0o700)
            env = {'PATH': str(folder) + ':/usr/bin:/bin', 'HOME': str(folder),
                   'RUNNER_TEMP': str(folder), 'GITHUB_OUTPUT': str(output)}
            result = subprocess.run(['/bin/bash', '--noprofile', '--norc', '-e', '-o', 'pipefail',
                                     '-c', step['run']], cwd=folder, env=env,
                                    capture_output=True, timeout=5)
            self.assertTrue(witness.exists(), 'parsed workflow must invoke the synthetic Python command')
            args = json.loads(witness.read_text())
            self.assertEqual(args, ['scripts/ci/release-pin-promotion.py', 'prepare',
                                    '--plan', '.git/assay-release-pin-plan.json'])
            self.assertTrue(validator.exists(), 'parsed workflow must route validation to the actual helper')
            self.assertEqual(validator.read_text(), 'actual-helper-exec\n')
            self.assertFalse(forbidden.exists(), 'adapter must not execute emitted shell syntax')
            return result.returncode, output.read_bytes() if output.exists() else b''

    def test_exact_boolean_success_emits_one_record(self):
        for token in (b'true', b'false'):
            with self.subTest(token=token):
                self.assertEqual(self.invoke(token + b'\n', 0), (0, b'changed=' + token + b'\n'))

    def test_nul_output_is_refused(self):
        actual, output = self.invoke(b'true\x00\n', 0)
        self.assertNotEqual(actual, 0, 'NUL-containing stdout must not become a valid boolean')
        self.assertEqual(output, b'', 'NUL-containing stdout must emit no changed record')

    def test_nonzero_or_nonexact_output_is_refused(self):
        for raw, code in ((b'true\n', 7), (b'', 0), (b'true', 0),
                          (b'true\n\n', 0), (b'true false\n', 0),
                          (b'true\nchanged=false\n', 0),
                          (b'$(touch must-not-execute)\n', 0)):
            with self.subTest(raw=raw, code=code):
                actual, output = self.invoke(raw, code)
                self.assertNotEqual(actual, 0, 'adapter must refuse unsuccessful or nonexact output')
                self.assertEqual(output, b'', 'refused adapter must emit no changed record')


class CleanPlanHandoff(unittest.TestCase):
    def test_real_prepare_publish_keeps_clean_git_and_refuses_untracked_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp).resolve()
            checkout = folder / 'checkout'
            checkout.mkdir()
            home = folder / 'home'
            home.mkdir()
            template = folder / 'empty-template'
            template.mkdir()
            env = {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'XDG_CONFIG_HOME': str(home),
                   'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null',
                   'GIT_TEMPLATE_DIR': str(template), 'GIT_TERMINAL_PROMPT': '0'}
            def fixture_git(*args):
                result = subprocess.run(['/usr/bin/git', *args], cwd=checkout, env=env,
                                        capture_output=True, text=True, timeout=5)
                self.assertEqual(result.returncode, 0, result.stderr)
                return result.stdout.strip()
            fixture_git('init', '-q', '--template=' + str(template))
            source = checkout / auto.WORKFLOW
            source.parent.mkdir(parents=True)
            source.write_bytes((ROOT / auto.WORKFLOW).read_bytes())
            fixture_git('add', '--', auto.WORKFLOW)
            fixture_git('-c', 'user.name=fixture', '-c', 'user.email=fixture@example.test',
                        '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')
            api = FixtureAPI()
            api.base = fixture_git('rev-parse', 'HEAD')
            event = folder / 'event.json'
            event.write_text('{"repository":{"full_name":"Rul1an/assay"},'
                             '"action":"completed","workflow_run":{"id":42}}')
            env.update({'GITHUB_REPOSITORY': auto.REPO, 'GITHUB_REF': 'refs/heads/main',
                        'GITHUB_EVENT_NAME': 'workflow_run', 'GITHUB_EVENT_PATH': str(event),
                        'APP_SLUG': 'fixture-app', 'EXPECTED_APP_SLUG': 'fixture-app'})
            selected = checkout / '.git' / 'assay-release-pin-plan.json'
            real_prepare, real_publish = auto.prepare, auto.publish
            with ExitStack() as stack:
                stack.enter_context(patch.object(auto, 'ROOT', checkout))
                stack.enter_context(patch.dict(os.environ, env, clear=True))
                stack.enter_context(patch.object(auto, 'API', return_value=api))
                stack.enter_context(patch.object(auto.promotion, 'plan', return_value=({}, {})))
                collected = stack.enter_context(patch.object(auto, 'collect', wraps=auto.collect))
                stack.enter_context(patch.object(auto, 'prepare',
                    side_effect=lambda a, i, root=checkout: real_prepare(a, i, root)))
                stack.enter_context(patch.object(auto, 'publish',
                    side_effect=lambda a, i, p, s, e: real_publish(a, i, p, s, e, checkout)))
                for mode in ('prepare', 'publish'):
                    out, err = io.StringIO(), io.StringIO()
                    with patch.object(sys, 'argv', ['helper', mode, '--plan', str(selected)]), \
                         patch.object(sys, 'stdout', out), patch.object(sys, 'stderr', err):
                        code = auto.main()
                    self.assertEqual((code, err.getvalue()), (0, ''), 'real handoff must accept clean checkout')
                    self.assertTrue(selected.is_file())
                    self.assertEqual(fixture_git('status', '--porcelain'), '')
                self.assertEqual(collected.call_count, 2, 'publish must recompute the complete plan')
                self.assertEqual(api.writes, [])
                (checkout / 'unrelated-untracked').write_text('must refuse\n')
                with self.assertRaisesRegex(ValueError, 'checkout is dirty'):
                    real_prepare(api, 42, checkout)


class Wiring(unittest.TestCase):
    def mapping(self, name):
        spec = importlib.util.spec_from_file_location('attest_contract', ROOT / 'scripts/ci/check-actions-attest-lockstep.py')
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module.load_workflow_mapping(ROOT / name)

    def test_privileged_workflow_has_closed_execution_fields(self):
        doc = self.mapping('.github/workflows/release-pin-promotion.yml')
        self.assertEqual(set(doc), {'name', 'true', 'permissions', 'concurrency', 'jobs'})
        self.assertEqual(doc['true'], {
            'workflow_run': {'workflows': ['Release'], 'types': ['completed']},
            'workflow_dispatch': {'inputs': {'run_id': {
                'description': 'Completed tag-push Release run with its producer binding artifact',
                'type': 'string', 'required': True}}}})
        self.assertEqual(doc['permissions'], {})
        self.assertEqual(doc['concurrency'], {'group': 'release-pin-promotion', 'cancel-in-progress': False})
        self.assertEqual(set(doc['jobs']), {'promote'})
        job = doc['jobs']['promote']
        self.assertEqual(set(job), {'if', 'runs-on', 'timeout-minutes', 'environment', 'permissions', 'steps'})
        self.assertEqual(job['if'], "github.repository == 'Rul1an/assay' && github.ref == 'refs/heads/main'")
        self.assertEqual(job['runs-on'], 'ubuntu-24.04')
        self.assertEqual(job['environment'], {'name': 'dependabot-maintenance', 'deployment': False})
        self.assertEqual(job['permissions'], {'contents': 'read', 'actions': 'read'})
        checkout, prepare, mint, publish = job['steps']
        self.assertEqual(set(checkout), {'name', 'uses', 'with'})
        self.assertEqual(checkout['with'], {'ref': '${{ github.sha }}', 'persist-credentials': False})
        self.assertEqual(checkout['uses'], 'actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09')
        for step, mode in ((prepare, 'prepare'), (publish, 'publish')):
            fields = {'name', 'id', 'env', 'shell', 'run'} if mode == 'prepare' else {'name', 'if', 'env', 'shell', 'run'}
            self.assertEqual(set(step), fields)
            self.assertEqual(step['shell'], 'bash')
            self.assertIsInstance(step['run'], str)
            self.assertTrue(step['run'].strip())
        self.assertEqual(publish['run'],
            'python3 scripts/ci/release-pin-promotion.py publish --plan .git/assay-release-pin-plan.json')
        self.assertEqual(prepare['id'], 'prepare')
        self.assertEqual(prepare['env'], {'GH_READ_TOKEN': '${{ github.token }}'})
        self.assertEqual(set(mint), {'name', 'if', 'id', 'uses', 'with'})
        self.assertEqual(mint['uses'], 'actions/create-github-app-token@bcd2ba49218906704ab6c1aa796996da409d3eb1')
        self.assertEqual(mint['id'], 'app-token')
        self.assertEqual(mint['with'], {'client-id': '${{ vars.DEPENDABOT_APP_CLIENT_ID }}',
            'private-key': '${{ secrets.DEPENDABOT_APP_PRIVATE_KEY }}', 'owner': 'Rul1an', 'repositories': 'assay',
            'permission-contents': 'write', 'permission-pull-requests': 'write'})
        for step in (mint, publish):
            self.assertEqual(step['if'], "steps.prepare.outputs.changed == 'true'")
        self.assertEqual(publish['env'], {'GH_READ_TOKEN': '${{ github.token }}',
            'GH_TOKEN': '${{ steps.app-token.outputs.token }}', 'APP_SLUG': '${{ steps.app-token.outputs.app-slug }}',
            'EXPECTED_APP_SLUG': '${{ vars.DEPENDABOT_APP_SLUG }}'})
        driver = (ROOT / 'scripts/ci/test-check-assay-release-pin.sh').read_text()
        self.assertIn('python3 "${ROOT}/scripts/ci/test_release_pin_automation.py"', driver)
        ci = self.mapping('.github/workflows/ci.yml')
        self.assertEqual(ci['jobs']['ci']['if'], 'always()')
        steps = ci['jobs']['ci']['steps']
        contract = next(step for step in steps if step.get('name') == 'Verify CI hardening contracts')
        self.assertNotIn('if', contract)
        self.assertIn('bash scripts/ci/test-check-assay-release-pin.sh', contract['run'].splitlines())

    def test_required_driver_actually_invokes_both_python_suites(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            witness = folder / 'calls'
            wrapper = folder / 'python3'
            wrapper.write_text('#!' + sys.executable + '\n' +
                'import os, pathlib, sys\n' +
                "name = pathlib.Path(sys.argv[1]).name if len(sys.argv) > 1 else ''\n" +
                "if name in ('test_release_pin_promotion.py', 'test_release_pin_automation.py'):\n" +
                "    with open(os.environ['PIN_SUITE_WITNESS'], 'a') as out: out.write(name + '\\n')\n" +
                "    raise SystemExit(0)\n" +
                'os.execv(' + repr(sys.executable) + ', [' + repr(sys.executable) + '] + sys.argv[1:])\n')
            wrapper.chmod(0o700)
            env = {**os.environ, 'PATH': str(folder) + os.pathsep + os.environ['PATH'],
                   'PIN_SUITE_WITNESS': str(witness)}
            result = subprocess.run(['bash', str(ROOT / 'scripts/ci/test-check-assay-release-pin.sh')],
                                    env=env, capture_output=True, text=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(witness.read_text().splitlines(),
                             ['test_release_pin_promotion.py', 'test_release_pin_automation.py'])

    def test_producer_program_uses_real_step_outputs(self):
        doc = self.mapping(auto.WORKFLOW)
        steps = doc['jobs']['publish-image']['steps']
        write = next(step for step in steps if step.get('name') == 'Write release-pin binding')
        self.assertEqual(set(write), {'name', 'env', 'shell', 'run'})
        self.assertEqual(write['env'], {'DIGEST': '${{ steps.build-and-push.outputs.digest }}',
            'IMAGE': '${{ steps.tags.outputs.image }}', 'VERSION': '${{ needs.release-contract.outputs.version }}',
            'RUN_ID': '${{ github.run_id }}', 'RUN_ATTEMPT': '${{ github.run_attempt }}', 'SOURCE_SHA': '${{ github.sha }}'})
        upload = next(step for step in steps if step.get('name') == 'Upload release-pin binding')
        self.assertEqual(set(upload), {'name', 'uses', 'with'})
        self.assertEqual(upload['uses'], 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a')
        self.assertEqual(upload['with'], {'name': 'assay-release-pin-binding-${{ github.run_id }}-${{ github.run_attempt }}',
            'path': '${{ runner.temp }}/release-pin-binding.json', 'if-no-files-found': 'error', 'retention-days': 30})
        self.assertLess(steps.index(write), steps.index(upload))
        with tempfile.TemporaryDirectory() as tmp:
            env = {**os.environ, 'RUNNER_TEMP': tmp, 'GITHUB_REPOSITORY': auto.REPO,
                   'DIGEST': 'sha256:' + 'b' * 64, 'IMAGE': auto.IMAGE, 'VERSION': 'v6.9.0',
                   'RUN_ID': '42', 'RUN_ATTEMPT': '1', 'SOURCE_SHA': 'a' * 40}
            result = subprocess.run(['bash', '-c', write['run']], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads((Path(tmp) / 'release-pin-binding.json').read_text()), FixtureAPI().data)

    def test_release_emits_actual_producer_binding(self):
        text = (ROOT / auto.WORKFLOW).read_text()
        producer = auto.producer_block(text)
        self.assertIn('name: Upload release-pin binding', producer)
        self.assertIn('DIGEST: ${{ steps.build-and-push.outputs.digest }}', producer)
        self.assertIn('RUN_ATTEMPT: ${{ github.run_attempt }}', producer)
        self.assertIn('if-no-files-found: error', producer)


class Publisher(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = (Path(self.tmp.name) / 'checkout').resolve()
        self.root.mkdir()
        self.remote = Path(self.tmp.name) / 'remote.git'
        self.real_git = auto.git
        self.real_git(['init', '-q'], self.root)
        self.real_git(['init', '-q', '--bare', str(self.remote)], self.root)
        for name in auto.promotion.SURFACES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('old\n')
        self.real_git(['add', '--', *sorted(auto.promotion.SURFACES)], self.root)
        self.real_git(['-c', 'user.name=fixture', '-c', 'user.email=fixture@example.test',
                       'commit', '-qm', 'fixture'], self.root)
        self.base = self.real_git(['rev-parse', 'HEAD'], self.root)
        self.api = FixtureAPI()
        self.api.base = self.base
        self.plan = {'fresh': auto.collect(self.api, 42)[1], 'changes': {'README.md': 'new\n'}}
        self.pushes = []
        self.before_push = lambda: None
        self.after_push = lambda: None
        self.branch = 'codex/release-pin-v6.9.0'
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        self.prepare = self.stack.enter_context(patch.object(auto, 'prepare', return_value=self.plan))
        self.fresh = self.stack.enter_context(patch.object(auto, 'collect',
            return_value=({}, self.plan['fresh'])))
        self.stack.enter_context(patch.object(auto, 'git', side_effect=self.git))

    def git(self, args, root, extra=None):
        if 'push' in args:
            self.pushes.append(args)
            self.before_push()
            local_args = [str(self.remote) if arg == 'https://github.com/Rul1an/assay.git' else arg for arg in args]
            # No network or credential helper is executed: transport is a scratch bare repository.
            self.real_git(local_args, root, extra)
            head = self.real_git(['--git-dir', str(self.remote), 'rev-parse', 'refs/heads/' + self.branch], root)
            self.api.remote = {'object': {'sha': head}}
            for pr in self.api.prs:
                pr['head']['sha'] = head
            self.after_push()
            return ''
        return self.real_git(args, root, extra)

    def publish(self):
        return auto.publish(self.api, 42, self.plan, 'fixture-app', 'fixture-app', self.root)

    def test_new_branch_pr_and_partial_creation_recovery_are_deterministic(self):
        commit = self.publish()
        self.assertEqual((self.root / 'README.md').read_text(), 'new\n')
        self.assertIn('--force-with-lease=refs/heads/' + self.branch + ':', self.pushes[0])
        self.assertEqual(self.api.writes[0][1]['method'], 'POST')
        self.assertEqual(self.api.writes[0][1]['data']['base'], 'main')
        first = copy.deepcopy(self.api.writes)
        self.real_git(['reset', '--hard', self.base], self.root)
        self.api.writes.clear()
        self.pushes.clear()
        self.assertEqual(self.publish(), commit)
        self.assertEqual(self.pushes, [])
        self.assertEqual(self.api.writes, first)
        # Model the App-owned PR created by the preceding write, then retry unchanged.
        body = self.api.writes[0][1]['data']['body']
        self.api.prs = [self.pr(commit, body)]
        self.real_git(['reset', '--hard', self.base], self.root)
        self.api.writes.clear()
        self.assertEqual(self.publish(), commit)
        self.assertEqual(self.api.writes, [])

    def pr(self, head, body):
        return {'number': 7, 'user': {'login': 'fixture-app[bot]', 'type': 'Bot'},
                'head': {'repo': {'full_name': auto.REPO}, 'ref': self.branch, 'sha': head},
                'base': {'repo': {'full_name': auto.REPO}, 'ref': 'main'}, 'body': body, 'state': 'open'}

    def test_actual_git_creation_lease_rejects_concurrent_ref(self):
        def race():
            self.real_git(['push', str(self.remote), self.base + ':refs/heads/' + self.branch], self.root)
        self.before_push = race
        with self.assertRaisesRegex(ValueError, 'git operation failed'):
            self.publish()
        self.assertEqual(self.api.writes, [])
        remote = self.real_git(['--git-dir', str(self.remote), 'rev-parse', 'refs/heads/' + self.branch], self.root)
        self.assertEqual(remote, self.base)

    def test_ref_race_after_push_refuses_pr(self):
        self.after_push = lambda: setattr(self.api, 'remote', {'object': {'sha': 'd' * 40}})
        with self.assertRaisesRegex(ValueError, 'branch changed'):
            self.publish()
        self.assertEqual(self.api.writes, [])

    def test_pr_race_after_push_refuses_pr(self):
        self.after_push = lambda: self.api.prs.append(self.pr('d' * 40, 'foreign'))
        with self.assertRaisesRegex(ValueError, 'PR set changed'):
            self.publish()
        self.assertEqual(self.api.writes, [])

    def test_owned_pr_update_uses_exact_previous_lease(self):
        previous = self.publish()
        body = self.api.writes[-1][1]['data']['body']
        self.api.prs = [self.pr(previous, body)]
        self.real_git(['reset', '--hard', self.base], self.root)
        self.plan['changes']['README.md'] = 'newer\n'
        self.api.writes.clear()
        self.pushes.clear()
        head = self.publish()
        self.assertNotEqual(head, previous)
        self.assertIn('--force-with-lease=refs/heads/' + self.branch + ':' + previous, self.pushes[0])
        self.assertEqual(self.api.writes[0][1]['method'], 'PATCH')

    def test_interrupted_owned_pr_update_requires_coordinator_recovery(self):
        previous = self.publish()
        body = self.api.writes[-1][1]['data']['body']
        self.api.prs = [self.pr(previous, body)]
        self.real_git(['reset', '--hard', self.base], self.root)
        self.plan['changes']['README.md'] = 'newer\n'
        self.api.writes.clear()
        self.pushes.clear()
        real_get = self.api.get

        def fail_patch(suffix, **kwargs):
            result = real_get(suffix, **kwargs)
            if kwargs.get('method') == 'PATCH':
                self.assertEqual(suffix, 'pulls/7')
                raise ValueError('injected PR PATCH failure')
            return result

        with patch.object(self.api, 'get', side_effect=fail_patch):
            with self.assertRaisesRegex(ValueError, '^injected PR PATCH failure$'):
                self.publish()
        advanced = self.real_git(['--git-dir', str(self.remote), 'rev-parse',
                                  'refs/heads/' + self.branch], self.root)
        self.assertNotEqual(advanced, previous)
        self.assertEqual(len(self.pushes), 1)
        self.assertEqual(self.api.remote['object']['sha'], advanced)
        self.assertEqual(self.api.prs[0]['head']['sha'], advanced)
        self.assertEqual(self.api.prs[0]['body'], body)
        recorded = json.loads(body.splitlines()[0][len(auto.MARKER):-4])
        self.assertEqual(recorded['commit'], previous)
        self.assertEqual([(path, call['method']) for path, call in self.api.writes],
                         [('pulls/7', 'PATCH')])

        # Retry from a fresh trusted-main checkout, retaining the stranded remote state.
        retry = Path(self.tmp.name) / 'retry'
        self.real_git(['clone', '-q', str(self.root), str(retry)], self.root)
        self.real_git(['checkout', '-q', '--detach', self.base], retry)
        self.assertEqual(self.real_git(['rev-parse', 'HEAD'], retry), self.base)
        self.assertEqual(self.real_git(['status', '--porcelain'], retry), '')
        self.api.writes.clear()
        self.pushes.clear()
        with self.assertRaisesRegex(ValueError,
                '^PR head or tag changed outside recorded promotion$'):
            auto.publish(self.api, 42, self.plan, 'fixture-app', 'fixture-app', retry)
        self.assertEqual(self.pushes, [])
        self.assertEqual(self.api.writes, [])
        self.assertEqual(self.api.prs[0]['body'], body)
        self.assertEqual(self.real_git(['--git-dir', str(self.remote), 'rev-parse',
                                       'refs/heads/' + self.branch], retry), advanced)
        self.assertEqual(self.real_git(['rev-parse', 'HEAD'], retry), self.base)
        self.assertEqual(self.real_git(['status', '--porcelain'], retry), '')

    def test_edited_or_closed_pr_is_not_owned(self):
        previous = self.publish()
        body = self.api.writes[-1][1]['data']['body']
        for field, value in [('state', 'closed'), ('body', 'no marker'),
                             ('user', {'login': 'someone', 'type': 'User'}),
                             ('head', {'repo': {'full_name': auto.REPO}, 'ref': self.branch, 'sha': 'd' * 40})]:
            self.real_git(['reset', '--hard', self.base], self.root)
            pr = self.pr(previous, body)
            pr[field] = value
            self.api.prs = [pr]
            self.api.writes.clear()
            self.pushes.clear()
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.publish()
            self.assertEqual(self.pushes, [])
            self.assertEqual(self.api.writes, [])

    def test_stale_saved_plan_and_scope_expansion_prevent_edits(self):
        saved = copy.deepcopy(self.plan)
        saved['fresh']['artifact'] = 52
        with self.assertRaisesRegex(ValueError, 'after preparation'):
            auto.publish(self.api, 42, saved, 'fixture-app', 'fixture-app', self.root)
        self.plan['changes']['.github/workflows/foreign.yml'] = 'forbidden'
        with self.assertRaisesRegex(ValueError, 'unexpected promotion path'):
            self.publish()
        self.assertEqual((self.root / 'README.md').read_text(), 'old\n')

    def test_freshness_after_push_prevents_pr_write(self):
        self.fresh.side_effect = [({}, self.plan['fresh']), ({}, {**self.plan['fresh'], 'base': 'e' * 40})]
        with self.assertRaisesRegex(ValueError, 'before PR write'):
            self.publish()
        self.assertEqual(len(self.pushes), 1)
        self.assertEqual(self.api.writes, [])

    def test_freshness_guard_prevents_branch_or_pr_write(self):
        self.fresh.return_value = ({}, {**self.plan['fresh'], 'updated_at': 'changed'})
        with self.assertRaisesRegex(ValueError, 'before branch write'):
            self.publish()
        self.assertEqual(self.pushes, [])
        self.assertEqual(self.api.writes, [])

    def test_noop_has_no_writes(self):
        self.plan['changes'] = {}
        self.assertEqual(self.publish(), 'no-op')
        self.assertEqual(self.pushes, [])
        self.assertEqual(self.api.writes, [])

    def test_foreign_branch_and_app_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'App identity'):
            auto.publish(self.api, 42, self.plan, 'other-app', 'fixture-app', self.root)
        self.api.remote = {'object': {'sha': 'd' * 40}}
        with self.assertRaisesRegex(ValueError, 'unowned branch'):
            self.publish()
        self.assertEqual(self.pushes, [])
        self.assertEqual(self.api.writes, [])


if __name__ == '__main__':
    unittest.main()
