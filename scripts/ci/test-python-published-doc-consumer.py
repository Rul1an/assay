#!/usr/bin/env python3
"""Behavioral models for the shared published-doc result acceptance funnel.
These are result-shape controls, not installed SDK or hosted observations.
Breaks caught: missing named phase, wrong executed fixture origin, contrast
setup failure credited as threshold failure, omitted or duplicated cell.
"""
import copy
import builtins
import json
import types
import os
from pathlib import Path
import stat
import unittest
from unittest import mock
import tempfile
import io

SOURCE = Path(__file__).with_name('python-published-doc-consumer.py')


def subject():
    # Read once, then execute that same bounded source buffer in the later model
    # run. Final source hashes are bound by the review/run artifact separately.
    fd = os.open(SOURCE, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size > 32768:
            raise RuntimeError('model source type or size invalid')
        raw = os.read(fd, before.st_size + 1)
        after = os.fstat(fd)
        named = SOURCE.lstat()
        identity = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
        if len(raw) != before.st_size or any(
                (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns) != identity
                for s in (after, named)):
            raise RuntimeError('model source changed during read')
    finally:
        os.close(fd)
    ns = {'__name__': 'published_doc_result_model', '__file__': str(SOURCE)}
    exec(compile(raw, str(SOURCE), 'exec'), ns)
    return ns


def complete_cell(line='3.12'):
    # Hand-declared complete observation shape. All values are synthetic; no
    # native wheel, registry consumer or hosted venv is implied by this model.
    nodes = ['test_validate.py::test_compliance', 'test_coverage.py::test_coverage',
             'test_agent.py::test_agent_run']
    positive = [{'nodeid': node, 'phase': phase, 'outcome': 'passed', 'wasxfail': False}
                for node in nodes for phase in ('setup', 'call', 'teardown')]
    contrast = [{'nodeid': node, 'phase': phase,
                 'outcome': 'failed' if phase == 'call' else 'passed', 'wasxfail': False,
                 'assertion': {'message': ('AssertionError: Coverage is below threshold 80.0.'
                                            if node == nodes[0] else 'assert 68.75 >= 90.0'),
                               'lines': ['assert report["meets_threshold"]' if node == nodes[0]
                                         else 'assert report["overall_coverage_pct"] >= 90.0']}}
                for node in nodes[:2] for phase in ('setup', 'call', 'teardown')]
    origins = [{'module': name, 'origin': 'venv/site-packages/' + name,
                'sha256': '1' * 64}
               for name in ['assay/__init__.py', 'assay/claim_support.py', 'assay/client.py',
                            'assay/coverage.py', 'assay/explain.py', 'assay/pytest_plugin.py',
                            'assay/_native.model']]
    row = {'cell': {'target': 'aarch64-apple-darwin', 'python': line},
            'public_install': {'argv': ['pip', 'install', 'assay-it'], 'first_install': True,
                               'exit': 0, 'version': '6.9.0', 'pip_origin': 'venv/bin/pip',
                               'archive_digest_layer': 'provider_report_only'},
            'positive': {'exit': 0, 'collected': 3, 'failed': 0, 'phases': positive,
                         'fixture': {'nodeid': nodes[2], 'function': 'assay_client',
                                     'origin': 'positive/conftest.py', 'bytes': 175,
                                     'sha256': 'ba7cc1307ce9ee81477a152ac6b5fb29d7c9214b9f094a87af38f3c68758914a'}},
            'contrast': {'exit': 1, 'collected': 2, 'failed': 2, 'phases': contrast},
            'config': {'rootdir': '.', 'inifile': 'pytest.ini', 'confcutdir': '.',
                       'importmode': 'importlib', 'addopts': '',
                       'positive_basetemp': 'positive-tmp', 'contrast_basetemp': 'contrast-tmp'},
            'modules_before': origins, 'modules_after': copy.deepcopy(origins),
            'plugin_base_before': ['observer', 'literal', 'assay'],
            'plugin_base_after': ['observer', 'literal', 'assay'],
            'plugin_declared_addition': 'funcmanage',
            'reports': {'positive': {'overall': 100.0, 'thresholds': [80.0, 90.0],
                                     'meets_threshold': [True, True]},
                        'contrast': {'overall': 68.75, 'thresholds': [80.0, 90.0],
                                     'meets_threshold': [False, False], 'tools_seen': 3,
                                     'total_tools': 6, 'tool_pct': 50.0},
                        'same_input_zero': {'overall': 68.75, 'threshold': 0.0,
                                            'meets_threshold': True}},
            'trace_bytes': b'{"args":{"q":"foo"},"tool":"search"}\n',
            'inputs_postflight_match': True, 'capture_complete': True,
            'direct_children_reaped': True, 'observer_errors': []}

    # Full driver evidence model: trusted expected data, synthetic observations.
    contract_raw = SOURCE.with_name('fixtures').joinpath('python-published-doc-inputs.v1.json').read_bytes()
    if len(contract_raw) > 32768:
        raise RuntimeError('model input cap')
    contract = json.loads(contract_raw)
    platform = contract['platforms']['aarch64-apple-darwin']
    members = {r['member']: {'path': 'venv/lib/python' + line + '/site-packages/' + r['member'],
                            'bytes': r['bytes'], 'sha256': r['sha256'], 'device': 1, 'inode': n + 1}
               for n, r in enumerate(platform['members'])}
    row.update(status='COMPLETE', members_before=members,
               members_after=copy.deepcopy(members),
               nonpytest_origins={name: {'before': copy.deepcopy(members), 'after': copy.deepcopy(members)}
                                  for name in ('positive-recorder', 'contrast-recorder', 'zero')})
    row['public_install'].update(pip_version='modeled-bundled',
                                 provider_digest=platform['sha256'],
                                 selected_wheel=platform['wheel'], archive_digest_layer='provider_report_only')
    row['pip_report'] = {'pip_version': 'modeled-bundled', 'install': [
        {'metadata': {'name': 'assay-it', 'version': '6.9.0'},
         'download_info': {'url': 'https://files.pythonhosted.org/' + platform['wheel'],
                           'archive_info': {'hashes': {'sha256': platform['sha256']}}}}]}
    def full_report(value, threshold):
        return {'overall_coverage_pct': value, 'threshold': threshold,
                'meets_threshold': value >= threshold,
                'tool_coverage': {'coverage_pct': 50.0, 'tools_seen_in_traces': 3,
                                  'total_tools_in_policy': 6}}
    row['reports'] = {case: {'validate_default': full_report(value, 80.0),
                             'coverage_min_90': full_report(value, 90.0)}
                      for case, value in [('positive', 100.0), ('contrast', 68.75)]}
    row['reports']['same_input_zero'] = full_report(68.75, 0.0)
    return row


class SharedResultModel(unittest.TestCase):
    def setUp(self):
        self.api = subject()

    def accepted(self, row):
        return self.api['validate_cell_result'](row)

    def refusal(self, row, expected):
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.accepted(row)
        self.assertEqual(str(caught.exception), expected)

    def test_complete_cell_control_is_accepted(self):
        self.assertEqual(self.accepted(complete_cell()), ('aarch64-apple-darwin', '3.12'))

    def test_missing_positive_call_phase_is_refused(self):
        # Delete the phase completeness guard: summaries remain green while a
        # method's actual call observation is absent. The real shared funnel must
        # reject rather than laundering summaries into an example pass.
        row = complete_cell()
        self.assertEqual(self.accepted(copy.deepcopy(row)), ('aarch64-apple-darwin', '3.12'))
        row['positive']['phases'] = [r for r in row['positive']['phases']
                                    if not (r['nodeid'] == 'test_agent.py::test_agent_run'
                                            and r['phase'] == 'call')]
        self.refusal(row, 'PHASE_INVENTORY')

    def test_wrong_executed_fixture_origin_is_refused(self):
        row = complete_cell()
        row['positive']['fixture']['origin'] = 'venv/site-packages/assay/pytest_plugin.py'
        self.refusal(row, 'FIXTURE_ORIGIN')

    def test_contrast_setup_failure_is_not_threshold_evidence(self):
        row = complete_cell()
        for phase in row['contrast']['phases']:
            if phase['nodeid'] == 'test_validate.py::test_compliance':
                phase['outcome'] = 'failed' if phase['phase'] == 'setup' else 'passed'
        self.refusal(row, 'PHASE_OUTCOME')

    def test_missing_cell_is_refused_by_actual_job_funnel(self):
        expected = [('aarch64-apple-darwin', '3.12'), ('aarch64-apple-darwin', '3.13'),
                    ('aarch64-apple-darwin', '3.14')]
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.api['validate_job_results']([complete_cell('3.12'), complete_cell('3.13')], expected)
        self.assertEqual(str(caught.exception), 'CELL_INVENTORY')

    def test_duplicate_cell_is_refused_by_actual_job_funnel(self):
        expected = [('aarch64-apple-darwin', '3.12'), ('aarch64-apple-darwin', '3.13'),
                    ('aarch64-apple-darwin', '3.14')]
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.api['validate_job_results']([complete_cell('3.12'), complete_cell('3.12'),
                                             complete_cell('3.14')], expected)
        self.assertEqual(str(caught.exception), 'CELL_INVENTORY')




class StructuredAssertionModel(unittest.TestCase):
    def setUp(self):
        self.api = subject()

    def test_valid_structured_comparator(self):
        self.assertEqual(self.api['validate_cell_result'](complete_cell()),
                         ('aarch64-apple-darwin', '3.12'))

    def detector(self, lines):
        row = complete_cell()
        self.assertEqual(self.api['validate_cell_result'](copy.deepcopy(row)),
                         ('aarch64-apple-darwin', '3.12'))
        for phase in row['contrast']['phases']:
            if phase['nodeid'] == 'test_validate.py::test_compliance' and phase['phase'] == 'call':
                phase['assertion']['lines'] = lines
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.api['validate_cell_result'](row)
        self.assertEqual(str(caught.exception), 'THRESHOLD_ASSERTION')

    def test_missing_literal_assertion_line_is_refused(self):
        self.detector([])

    def test_wrong_literal_assertion_line_is_refused(self):
        self.detector(['assert unrelated_condition'])


class InstallationOrderModel(unittest.TestCase):
    def setUp(self):
        self.api = subject()
        self.calls = []
        self.pip = '/owned/venv/bin/pip'
        self.tooling = ['install', '--require-hashes', '-r', '/owned/tooling.txt']

    def dispatch(self, argv):
        self.calls.append(list(argv))
        return 0

    def test_both_actual_installation_calls_are_observed(self):
        self.api['run_install_chain'](self.pip, self.tooling, self.dispatch)
        self.assertEqual(len(self.calls), 2)
        self.assertEqual({tuple(call) for call in self.calls},
                         {(self.pip, 'install', 'assay-it'),
                          tuple([self.pip] + self.tooling)})

    def test_literal_public_install_precedes_tooling(self):
        self.api['run_install_chain'](self.pip, self.tooling, self.dispatch)
        # Establish a complete observation before the ordering detector. A
        # missing provider call or scaffold exception is not the intended RED.
        self.assertEqual(len(self.calls), 2)
        self.assertIn([self.pip, 'install', 'assay-it'], self.calls)
        self.assertIn([self.pip] + self.tooling, self.calls)
        self.assertEqual(self.calls[0], [self.pip, 'install', 'assay-it'])

    def test_public_install_failure_is_preserved_without_later_commands(self):
        def failed_public(argv):
            self.calls.append(list(argv))
            return 7 if argv == [self.pip, 'install', 'assay-it'] else 0
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.api['run_install_chain'](self.pip, self.tooling, failed_public)
        self.assertEqual(str(caught.exception), 'PUBLIC_INSTALL_EXIT:7')
        public_index = self.calls.index([self.pip, 'install', 'assay-it'])
        self.assertEqual(self.calls[public_index + 1:], [])


def observer_subject():
    path = SOURCE.with_name('python-published-doc-observer.py')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        a = os.fstat(fd)
        if not stat.S_ISREG(a.st_mode) or a.st_size > 32768:
            raise RuntimeError('observer source type/cap invalid')
        raw = os.read(fd, a.st_size + 1)
        b, c = os.fstat(fd), path.lstat()
        identity = lambda q: (q.st_dev, q.st_ino, q.st_size, q.st_mtime_ns)
        if len(raw) != a.st_size or identity(a) != identity(b) or identity(a) != identity(c):
            raise RuntimeError('observer source changed')
    finally:
        os.close(fd)
    def hookimpl(function=None, **kwargs):
        return function if function is not None else (lambda function: function)
    fakepytest = types.SimpleNamespace(hookimpl=hookimpl,
                                      ExitCode=types.SimpleNamespace(INTERNAL_ERROR=3))
    def model_import(name, globals=None, locals=None, fromlist=(), level=0):
        if name == 'pytest':
            return fakepytest
        if name == 'assay' or name.startswith('assay.'):
            raise RuntimeError('unexpected real SDK import')
        return builtins.__import__(name, globals, locals, fromlist, level)
    bootstrap = dict(vars(builtins)); bootstrap['__import__'] = model_import
    ns = {'__name__': 'portable_observer_model', '__file__': '/owned/conftest.py',
          '__builtins__': bootstrap}
    exec(compile(raw, str(path), 'exec'), ns)
    return ns


class ObserverLowerProviderModel(unittest.TestCase):
    """Actual observer funnels; file/module/plugin/FixtureDef providers modeled.

    No SDK/native/pytest is imported. These do not measure installed bytes or
    validate the unexercised native descriptor/sysconfig/configure bootstrap.
    """
    def setUp(self):
        self.api = observer_subject()
        self.shared = subject()
        raw = SOURCE.with_name('fixtures').joinpath('python-published-doc-inputs.v1.json').read_bytes()
        if len(raw) > 32768:
            raise RuntimeError('model table cap')
        self.table = json.loads(raw)
        self.root = Path('/owned')
        self.site = self.root / 'venv/lib/python3.12/site-packages'
        self.modules, self.files = {}, {}
        for name, (member, size, digest) in self.table['pytest']['modules'].items():
            self.module(name, self.site / member, size, digest)
        self.members = {r['member']: r for r in self.table['platforms']['aarch64-apple-darwin']['members']}
        for member, record in self.members.items():
            name = 'assay._native' if member.endswith('.so') else member[:-3].replace('/', '.').removesuffix('.__init__')
            self.module(name, self.site / member, record['bytes'], record['sha256'])
        self.entries = {r['member']: r for r in self.table['pytest']['entry_members']}
        for runtime_name, member in [('pytest', 'pytest/__init__.py'),
                                     ('__main__', 'pytest/__main__.py')]:
            row = self.entries[member]
            self.module(runtime_name, self.site / member, row['bytes'], row['sha256'])
        self.modules['__main__'].__spec__.name = 'pytest.__main__'
        self.module('literal_model', self.root / 'positive/conftest.py', 175,
                    'ba7cc1307ce9ee81477a152ac6b5fb29d7c9214b9f094a87af38f3c68758914a')
        self.module('observer_model', self.root / 'conftest.py', 17322, '4' * 64)
        self.files[self.root / 'observer-original.py'] = dict(self.files[self.root / 'conftest.py'])
        self.api.update(ROOT=self.root, SOURCE_ORIGIN=self.root / 'observer-original.py',
                        SITE=self.site, PLAT=self.site, MEMBERS=self.members,
                        PYTEST_ENTRIES=self.entries,
                        DEFAULT_MODULES=self.table['pytest']['default_modules'],
                        PINNED_MODULE_FILES=self.table['pytest']['modules'],
                        OBJECTS=self.table['pytest']['objects'],
                        sys=types.SimpleNamespace(modules=self.modules),
                        owned_file=self.file_provider, SHARED=self.shared)
        self.api['STATE'].update(configured=True, case='positive',
                                target='aarch64-apple-darwin',
                                versions={'assay-it': '6.9.0', 'pytest': '9.1.1', 'pip': 'modeled-bundled'},
                                config={'cwd': 'positive', 'root': '.', 'inipath': 'pytest.ini',
                                        'importmode': 'importlib', 'confcutdir': '.',
                                        'basetemp': 'positive-tmp', 'addopts': ''})
        cls = type('PytestPluginManager', (), {'__module__': '_pytest.config'})
        self.manager = cls()
        self.pairs = [(str(id(self.manager)), self.manager)]
        for name in self.table['pytest']['default_modules']:
            self.pairs.append((name, self.modules['_pytest.' + name]))
        for registration, (module, classname) in self.table['pytest']['objects'].items():
            obj = type(classname, (), {'__module__': module})()
            self.pairs.append((registration, obj))
        self.pairs += [('assay', self.modules['assay.pytest_plugin']),
                       (str(self.root / 'conftest.py'), self.modules['observer_model']),
                       (str(self.root / 'positive/conftest.py'), self.modules['literal_model'])]
        self.manager.list_name_plugin = lambda: list(self.pairs)
        self.config = types.SimpleNamespace(pluginmanager=self.manager)

    def module(self, name, path, size, digest):
        module = types.ModuleType(name); module.__file__ = str(path)
        module.__spec__ = types.SimpleNamespace(origin=str(path), name=name)
        self.modules[name] = module
        self.files[path] = {'path': str(path.relative_to(self.root)), 'bytes': size,
                            'sha256': digest, 'device': 1, 'inode': len(self.files) + 1}

    def file_provider(self, path, cap):
        row = dict(self.files[Path(path)])
        if row['bytes'] > cap:
            raise self.api['ObservationError']('model_file_cap')
        return row

    def test_valid_module_and_closed_plugin_comparator(self):
        self.assertEqual(len(self.api['module_origins']()), 9)
        before = self.api['plugin_records'](self.config)
        self.assertEqual(len(before), len(self.pairs))
        obj = type('FixtureManager', (), {'__module__': '_pytest.fixtures'})()
        self.pairs.append(('funcmanage', obj))
        final = self.api['plugin_records'](self.config, final=True)
        self.assertEqual([r for r in final if r['identity'] != 'funcmanage'], before)

    def test_wrong_pytest_init_hash_is_refused(self):
        self.assertEqual(len(self.api['module_origins']()), 9)
        self.files[self.site / 'pytest/__init__.py']['sha256'] = '0' * 64
        with self.assertRaises(self.api['ObservationError']) as caught:
            self.api['module_origins']()
        self.assertEqual(str(caught.exception), 'pytest_entry_mismatch')

    def test_wrong_executed_pytest_main_bytes_are_refused(self):
        self.assertEqual(len(self.api['module_origins']()), 9)
        self.assertEqual(self.modules['__main__'].__spec__.name, 'pytest.__main__')
        self.files[self.site / 'pytest/__main__.py']['bytes'] += 1
        with self.assertRaises(self.api['ObservationError']) as caught:
            self.api['module_origins']()
        self.assertEqual(str(caught.exception), 'pytest_entry_mismatch')

    def test_wrong_loaded_origin_is_refused(self):
        self.api['module_origins']()
        self.modules['assay.client'].__file__ = '/checkout/assay/client.py'
        self.modules['assay.client'].__spec__.origin = '/checkout/assay/client.py'
        with self.assertRaises(self.api['ObservationError']):
            self.api['module_origins']()

    def test_changed_member_hash_is_refused(self):
        self.api['module_origins']()
        self.files[self.site / 'assay/client.py']['sha256'] = '0' * 64
        with self.assertRaises(self.api['ObservationError']):
            self.api['module_origins']()

    def test_file_spec_disagreement_is_refused(self):
        self.api['module_origins']()
        self.modules['assay.client'].__spec__.origin = '/other/client.py'
        with self.assertRaises(self.api['ObservationError']):
            self.api['module_origins']()

    def test_unexpected_plugin_is_refused(self):
        self.api['plugin_records'](self.config)
        self.pairs.append(('uninvited', self.modules['assay.client']))
        with self.assertRaises(self.api['ObservationError']):
            self.api['plugin_records'](self.config)

    def fixture(self, origin):
        func = types.SimpleNamespace(__code__=types.SimpleNamespace(co_filename=str(origin)),
                                     __qualname__='assay_client')
        definition = types.SimpleNamespace(argname='assay_client', func=func)
        request = types.SimpleNamespace(node=types.SimpleNamespace(nodeid='positive/test_agent.py::test_agent_run'))
        wrapper = self.api['pytest_fixture_setup'](definition, request)
        next(wrapper)
        try:
            wrapper.send('modeled fixture value')
        except StopIteration as end:
            return end.value

    def test_actual_fixture_wrapper_comparator_and_wrong_origin(self):
        self.assertEqual(self.fixture(self.root / 'positive/conftest.py'), 'modeled fixture value')
        self.assertEqual(len(self.api['STATE']['fixtures']), 1)
        self.api['STATE']['fixtures'].clear()
        with self.assertRaises(self.api['ObservationError']):
            self.fixture(self.site / 'assay/pytest_plugin.py')

    def healthy_state(self):
        self.fixture(self.root / 'positive/conftest.py')
        self.api['STATE']['modules_before'] = self.api['module_origins']()
        self.api['STATE']['plugins_before'] = self.api['plugin_records'](self.config)
        self.actual_positive_reports()
        obj = type('FixtureManager', (), {'__module__': '_pytest.fixtures'})()
        self.pairs.append(('funcmanage', obj))
        return types.SimpleNamespace(config=self.config, testscollected=3, testsfailed=0, exitstatus=0)

    def test_missing_actual_fixture_is_refused_by_sessionfinish(self):
        session = self.healthy_state()
        self.api['STATE']['fixtures'].clear()
        with self.assertRaises(self.api['ObservationError']):
            self.api['pytest_sessionfinish'](session, 0)
        self.assertEqual(session.exitstatus, 3)

    def actual_positive_reports(self):
        self.api['STATE']['reports'].clear()
        for row in complete_cell()['positive']['phases']:
            report = types.SimpleNamespace(nodeid='positive/' + row['nodeid'],
                                           when=row['phase'], outcome=row['outcome'])
            self.api['pytest_runtest_logreport'](report)

    def test_missing_phase_is_refused_by_actual_bridge(self):
        self.actual_positive_reports()
        self.api['validate_phase_receipts'](0, 3, 0)
        self.api['STATE']['reports'].pop()
        with self.assertRaises(self.shared['ResultRefused']):
            self.api['validate_phase_receipts'](0, 3, 0)

    def test_healthy_actual_session_receipt_fits_cap(self):
        session = self.healthy_state()
        captured = bytearray()
        class Sink:
            O_WRONLY, O_CREAT, O_EXCL, O_NOFOLLOW = 1, 2, 4, 8
            def open(self, path, flags, mode):
                if Path(path) != Path('/owned/observer-positive.json') or mode != 0o600:
                    raise RuntimeError('wrong output sink')
                return 1
            def write(self, fd, raw):
                captured.extend(raw)
                return len(raw)
            def close(self, fd):
                pass
        self.api['os'] = Sink()
        self.api['pytest_sessionfinish'](session, 0)
        self.assertGreater(len(captured), 0)
        self.assertLessEqual(len(captured), 16384)
        record = json.loads(captured)
        self.assertEqual(record['tests_collected'], 3)
        self.assertEqual(len(record['reports']), 9)
        self.assertEqual(len(record['fixtures']), 1)
        self.assertEqual(record['errors'], [])


class FixtureCompositionModel(unittest.TestCase):
    def emitted_cell(self):
        model = ObserverLowerProviderModel('test_actual_fixture_wrapper_comparator_and_wrong_origin')
        model.setUp()
        api = model.shared
        self.assertEqual(api['validate_cell_result'](complete_cell()),
                         ('aarch64-apple-darwin', '3.12'))
        self.assertEqual(model.fixture(model.root / 'positive/conftest.py'), 'modeled fixture value')
        self.assertEqual(len(model.api['STATE']['fixtures']), 1)
        emitted = model.api['STATE']['fixtures'][0]
        self.assertEqual(emitted['nodeid'], 'positive/test_agent.py::test_agent_run')
        self.assertEqual(emitted['argname'], 'assay_client')
        self.assertEqual(emitted['source']['path'], 'positive/conftest.py')
        row = complete_cell()
        row['positive']['fixture'] = emitted  # SAME actual emitted object, no proxy mapper.
        return api, row

    def test_actual_emitted_nested_fixture_is_accepted(self):
        api, row = self.emitted_cell()
        self.assertEqual(api['validate_cell_result'](row), ('aarch64-apple-darwin', '3.12'))

    def malformed(self, alter):
        api, row = self.emitted_cell()
        self.assertEqual(api['validate_cell_result'](copy.deepcopy(row)),
                         ('aarch64-apple-darwin', '3.12'))
        alter(row['positive']['fixture'])
        with self.assertRaises(api['ResultRefused']) as caught:
            api['validate_cell_result'](row)
        self.assertEqual(str(caught.exception), 'FIXTURE_ORIGIN')

    def test_malformed_nested_source_has_no_flat_fallback(self):
        def alter(fixture):
            fixture.update(source=None, origin='positive/conftest.py', bytes=175,
                           sha256='ba7cc1307ce9ee81477a152ac6b5fb29d7c9214b9f094a87af38f3c68758914a')
        self.malformed(alter)

    def test_missing_nested_path_is_refused(self):
        self.malformed(lambda fixture: fixture['source'].pop('path'))

    def test_wrong_nested_hash_is_refused(self):
        self.malformed(lambda fixture: fixture['source'].update(sha256='0' * 64))

    def test_wrong_nested_argname_is_refused(self):
        self.malformed(lambda fixture: fixture.update(argname='other_fixture'))

    def test_wrong_case_prefix_is_refused(self):
        self.malformed(lambda fixture: fixture.update(nodeid='contrast/test_agent.py::test_agent_run'))


class NativeCellEvidenceModel(unittest.TestCase):
    def setUp(self):
        self.api = subject()

    def invalid(self, change, code):
        row = complete_cell()
        self.assertEqual(self.api['validate_cell_result'](copy.deepcopy(row)),
                         ('aarch64-apple-darwin', '3.12'))
        change(row)
        with self.assertRaises(self.api['ResultRefused']) as caught:
            self.api['validate_cell_result'](row)
        self.assertEqual(str(caught.exception), code)

    def test_missing_pip_report_is_incomplete(self):
        self.invalid(lambda row: row.pop('pip_report'), 'PIP_REPORT')

    def test_actual_other_release_is_not_credited(self):
        self.invalid(lambda row: row['pip_report']['install'][0]['metadata'].update(version='7.0.0'),
                     'PIP_REPORT_SELECTION')

    def test_loaded_checkout_member_is_refused(self):
        self.invalid(lambda row: row['members_before']['assay/client.py'].update(path='checkout/assay/client.py'),
                     'INSTALLED_MEMBER')

    def test_empty_recorder_is_refused(self):
        self.invalid(lambda row: row['reports'].update(positive={}), 'FULL_REPORT')

    def test_min_zero_reversal_must_be_true(self):
        self.invalid(lambda row: row['reports']['same_input_zero'].update(meets_threshold=False),
                     'MIN_ZERO')

    def test_fresh_trace_must_be_exact(self):
        self.invalid(lambda row: row.update(trace_bytes=b'{}\n'), 'FRESH_TRACE')

    def test_recorder_origin_cannot_borrow_pytest_identity(self):
        self.invalid(lambda row: row['nonpytest_origins'].pop('zero'), 'NONPYTEST_ORIGINS')

    def test_incomplete_status_never_accepts(self):
        self.invalid(lambda row: row.update(status='INCOMPLETE'), 'CELL_STATUS')

    def test_capture_cap_refuses_before_sink_write(self):
        sink = types.SimpleNamespace(write=lambda raw: self.fail('oversize wrote sink'))
        with self.assertRaises(self.api['ResultRefused']):
            self.api['capped_write'](sink, b'XX', 1, 0)

    def test_deadline_prevents_lower_spawn(self):
        provider = self.api['NativeProvider'](Path('/owned'), Path('/owned/artifacts'),
                                            clock=lambda: 600.0, start=0.0)
        provider.popen = lambda *a, **k: self.fail('expired lower spawn')
        with self.assertRaises(self.api['ResultRefused']):
            provider.run('expired', ['owned-python'], Path('/owned'), {}, 30)


class NativeBoundaryRegressionModel(unittest.TestCase):
    def final_wait(self, advance):
        api = subject()
        clock = [0.0]
        events = []
        class Pipe(io.BytesIO):
            def fileno(self): return 123
        class Child:
            def __init__(self):
                self.stdout, self.stderr = Pipe(), Pipe()
                self.returncode = None
            def wait(self, timeout):
                events.append(('wait', timeout))
                if self.returncode is None:
                    clock[0] = advance
                    self.returncode = 0
                return self.returncode
            def poll(self): return self.returncode
        class Selector:
            def __init__(self): self.rows = {}
            def register(self, stream, event, channel): self.rows[stream] = channel
            def unregister(self, stream): self.rows.pop(stream)
            def get_map(self): return self.rows
            def select(self, timeout):
                return [(types.SimpleNamespace(fileobj=s, data=c), 1)
                        for s, c in self.rows.items()]
            def close(self): pass
        with tempfile.TemporaryDirectory() as directory:
            child = Child()
            provider = api['NativeProvider'](directory, directory, clock=lambda:clock[0], start=0.0)
            provider.popen = lambda *a, **k: child
            provider.growth = lambda *a: None  # only the lower host metrics seam
            with mock.patch.object(api['selectors'], 'DefaultSelector', Selector), \
                 mock.patch.object(api['os'], 'set_blocking', lambda *a:None), \
                 mock.patch.object(api['os'], 'read', lambda *a:b''):
                refusal = None
                try: provider.run('modeled', ['owned'], directory, {}, 30)
                except api['ResultRefused'] as error: refusal = str(error)
            self.assertTrue(events, 'actual final wait must have occurred')
            self.assertTrue(child.stdout.closed and child.stderr.closed)
            self.assertTrue(provider.records[0]['direct_child_reaped'])
            return refusal, provider.records[0]

    def test_fixed_phase_deadline_survives_final_wait(self):
        refusal, record = self.final_wait(29.0)
        self.assertIsNone(refusal)
        self.assertTrue(record['complete'], 'healthy actual funnel comparator')
        refusal, record = self.final_wait(31.0)
        self.assertEqual(refusal, 'COMMAND_DEADLINE', 'late actual final wait must refuse')
        self.assertFalse(record['complete'])

    def failed_product(self, version):
        api = subject()
        facts = complete_cell()
        contract = api['load_contract']()
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory); work=parent/'work'; artifacts=parent/'artifacts'
            work.mkdir();artifacts.mkdir()
            interpreter=parent/'python';interpreter.write_bytes(b'modeled executable bytes')
            calls=[]; captured=[]
            class Provider:
                def __init__(self, root, evidence, start):
                    self.root=Path(root);self.records=[];self.live=None;captured.append(self)
                def admit(self): pass
                def run(self,name,argv,cwd,env,seconds,expected=0):
                    calls.append(name)
                    self.records.append({'complete':True,'direct_child_reaped':True})
                    if name=='interpreter':
                        return json.dumps({'line':'3.12','implementation':'cpython','system':'Darwin',
                                           'machine':'arm64','prefix':str(parent),'base_prefix':str(parent)}).encode()
                    if name=='venv':
                        (self.root/'venv/bin').mkdir(parents=True)
                        (self.root/'venv/bin/python').write_bytes(interpreter.read_bytes())
                        (self.root/'venv/pyvenv.cfg').write_bytes(b'include-system-site-packages = false\n')
                        return b''
                    if name=='bundled-pip':
                        return json.dumps({'version':'modeled-bundled','origin':str(self.root/'venv/pip.py'),
                                           'prefix':str(self.root/'venv'),'base_prefix':str(parent)}).encode()
                    if name=='public-install':
                        report=copy.deepcopy(facts['pip_report'])
                        report['install'][0]['metadata']['version']=version
                        raw=(json.dumps(report,indent=2)+'\n').encode()
                        (self.root/'public-install-report.json').write_bytes(raw)
                        self.original=raw
                        return b''
                    if name=='tooling-install': raise api['ResultRefused']('MODEL_TOOLING_STOP')
                    raise AssertionError('unexpected lower call:'+name)
            api['NativeProvider']=Provider
            row=api['native_cell'](str(interpreter),'3.12','aarch64-apple-darwin',work,artifacts,contract)
            self.assertEqual(row['status'],'INCOMPLETE')
            self.assertEqual(calls[:4],['interpreter','venv','bundled-pip','public-install'])
            self.assertEqual(len(captured),1)
            self.assertTrue((artifacts/'cell-3.12/RESULT.json').is_file())
            expected='MODEL_TOOLING_STOP' if version=='6.9.0' else 'PIP_REPORT_SELECTION'
            self.assertIn(expected,row['error'],'must reach actual report validation/failure funnel')
            retained=artifacts/'cell-3.12/public-install-report.json'
            self.assertTrue(retained.is_file(),'actual original failed-cell JSON must be uploaded')
            self.assertEqual(retained.read_bytes(),captured[0].original)

    def test_bounded_valid_report_survives_later_cell_failure(self):
        self.failed_product('6.9.0')

    def test_other_release_original_survives_early_refusal(self):
        self.failed_product('7.0.0')


class NativeGrowthRaceModel(unittest.TestCase):
    def setup_provider(self, parent):
        api=subject();root=parent/'work';artifacts=parent/'artifacts'
        root.mkdir();artifacts.mkdir()
        return api,api['NativeProvider'](root,artifacts,clock=lambda:1.0,start=0.0)

    def race(self, entry_race):
        with tempfile.TemporaryDirectory() as directory:
            api,p=self.setup_provider(Path(directory));child=p.root/'tmp';child.mkdir()
            leaf=child/'transient';leaf.write_bytes(b'one byte')
            p.growth(True)
            original=api['os'].scandir;reached=[]
            class VanishingEntry:
                def __init__(self,entry):self.entry=entry;self.path=entry.path
                def stat(self,follow_symlinks):
                    reached.append('entry-stat');Path(self.path).unlink()
                    return self.entry.stat(follow_symlinks=follow_symlinks)
            class Scan:
                def __init__(self,inner):self.inner=inner
                def __enter__(self):return self
                def __exit__(self,*args):self.inner.close()
                def __iter__(self):return (VanishingEntry(e) for e in self.inner)
            def lower(path):
                if Path(path)==child:
                    if entry_race:return Scan(original(path))
                    reached.append('descendant-scandir');leaf.unlink();child.rmdir()
                return original(path)
            failure=None
            with mock.patch.object(api['os'],'scandir',lower):
                try:p.growth(True)
                except FileNotFoundError as error:failure=error
            self.assertEqual(reached,['entry-stat'if entry_race else'descendant-scandir'])
            self.assertTrue(p.root.is_dir()and p.artifacts.is_dir())
            self.assertIsNone(failure,'vanished descendant must not become sampler failure')
            self.assertEqual(p.last_scan,1.0)

    def test_vanished_descendant_directory_does_not_fail_sampler(self):self.race(False)
    def test_vanished_descendant_entry_does_not_fail_sampler(self):self.race(True)

    def test_missing_owned_root_is_not_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            api,p=self.setup_provider(Path(directory));p.growth(True);p.root.rmdir()
            with self.assertRaises(FileNotFoundError):p.growth(True)

    def test_permission_refusal_is_not_skipped(self):
        with tempfile.TemporaryDirectory() as directory:
            api,p=self.setup_provider(Path(directory));p.growth(True)
            lower=api['os'].scandir
            def denied(path):
                if Path(path)==p.artifacts:raise PermissionError('modeled actual scandir permission')
                return lower(path)
            with mock.patch.object(api['os'],'scandir',denied):
                with self.assertRaises(PermissionError):p.growth(True)

    def test_work_byte_cap_remains_real(self):
        with tempfile.TemporaryDirectory() as directory:
            api,p=self.setup_provider(Path(directory));p.growth(True)
            with (p.root/'sparse').open('wb')as f:f.truncate(2*1024**3+1)
            with self.assertRaises(api['ResultRefused'])as caught:p.growth(True)
            self.assertEqual(str(caught.exception),'WORK_BYTES')

    def test_retained_byte_cap_remains_real(self):
        with tempfile.TemporaryDirectory() as directory:
            api,p=self.setup_provider(Path(directory));p.growth(True)
            with (p.artifacts/'sparse').open('wb')as f:f.truncate(512*1024**2+1)
            with self.assertRaises(api['ResultRefused'])as caught:p.growth(True)
            self.assertEqual(str(caught.exception),'ARTIFACT_BYTES')


if __name__ == '__main__':
    unittest.main(verbosity=2)
