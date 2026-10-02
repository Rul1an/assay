"""Portable source-stage observer: copy byte-identically to the owned root conftest.
No fixtures or tests. Observation failure invalidates the run, never a clean result.
"""
import hashlib
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import stat
import platform
import sysconfig
import sys
import types

import pytest

ROOT = Path(__file__).resolve().parent
SOURCE_ORIGIN = ROOT / 'observer-original.py'
INPUTS_SHA = '9f078064cd5188a3b5069f190663f8e5552a25d0b0ef49d965c25f3cfd43799e'
FIXTURE_SHA = 'ba7cc1307ce9ee81477a152ac6b5fb29d7c9214b9f094a87af38f3c68758914a'
ASSAY_PLUGIN_SHA = 'd7ee34b7167de41e1533ec3f439ee1bb6bbfab01a710baea491a8398417259dc'
DEFAULT_MODULES = []
PINNED_MODULE_FILES = {}
OBJECTS = {}
MEMBERS = {}
PYTEST_ENTRIES = {}
SITE = None
PLAT = None
SHARED_SOURCE_SHA = 'eb0db6bffc91604002e776683bdf113b4e17493ac6fb320794fb50d60bc94df7'
SHARED = None
STATE = {'configured': False, 'fixtures': [], 'errors': [], 'reports': []}


class ObservationError(RuntimeError):
    pass


def need(ok, code):
    if not ok:
        raise ObservationError(code)


def owned_file(path, cap):
    path = Path(path)
    need(path.is_relative_to(ROOT) or path == SOURCE_ORIGIN, 'origin_outside_owned_root')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and
             0 <= before.st_size <= cap, 'file_type_owner_size')
        identity = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
        digest = hashlib.sha256()
        remaining = before.st_size
        while remaining:
            part = os.read(fd, min(65536, remaining))
            need(bool(part), 'file_shrank')
            digest.update(part)
            remaining -= len(part)
        need(not os.read(fd, 1), 'file_grew')
        after = os.fstat(fd)
        named = os.stat(path, follow_symlinks=False)
        need((after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) == identity and
             (named.st_dev, named.st_ino, named.st_size, named.st_mtime_ns) == identity,
             'file_identity_changed')
        return {'path': str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path), 'bytes': before.st_size, 'sha256': digest.hexdigest(),
                'device': before.st_dev, 'inode': before.st_ino}
    finally:
        os.close(fd)


def load_shared_source():
    global SHARED
    path = ROOT / 'consumer-source.py'
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        need(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid() and
             0 < before.st_size <= 32768, 'shared_source_cap')
        raw = os.read(fd, before.st_size + 1)
        after, named = os.fstat(fd), path.lstat()
        identity = lambda q: (q.st_dev, q.st_ino, q.st_size, q.st_mtime_ns)
        need(len(raw) == before.st_size and identity(before) == identity(after) == identity(named)
             and hashlib.sha256(raw).hexdigest() == SHARED_SOURCE_SHA, 'shared_source_identity')
    finally:
        os.close(fd)
    ns = {'__name__': 'published_doc_shared_rule', '__file__': str(path)}
    exec(compile(raw, str(path), 'exec'), ns)
    SHARED = ns


def initialize_contract():
    global DEFAULT_MODULES, PINNED_MODULE_FILES, OBJECTS, MEMBERS, PYTEST_ENTRIES, SITE, PLAT
    path = ROOT / 'inputs.json'
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        a = os.fstat(fd)
        need(stat.S_ISREG(a.st_mode) and a.st_uid == os.getuid() and
             0 < a.st_size <= 32768, 'contract_type_cap')
        raw = os.read(fd, a.st_size + 1)
        b, c = os.fstat(fd), path.lstat()
        identity = lambda q: (q.st_dev, q.st_ino, q.st_size, q.st_mtime_ns)
        need(len(raw) == a.st_size and identity(a) == identity(b) == identity(c) and
             hashlib.sha256(raw).hexdigest() == INPUTS_SHA, 'contract_identity')
    finally:
        os.close(fd)
    contract = json.loads(raw)
    target = {('Linux', 'x86_64'): 'x86_64-unknown-linux-gnu',
              ('Darwin', 'arm64'): 'aarch64-apple-darwin',
              ('Darwin', 'x86_64'): 'x86_64-apple-darwin'}.get(
                  (platform.system(), platform.machine()))
    need(target in contract['platforms'] and sys.implementation.name == 'cpython' and
         sys.version_info[:2] in ((3, 12), (3, 13), (3, 14)), 'platform_outside_matrix')
    MEMBERS = {r['member']: r for r in contract['platforms'][target]['members']}
    DEFAULT_MODULES = contract['pytest']['default_modules']
    PINNED_MODULE_FILES = contract['pytest']['modules']
    OBJECTS = contract['pytest']['objects']
    PYTEST_ENTRIES = {r['member']: r for r in contract['pytest']['entry_members']}
    SITE, PLAT = Path(sysconfig.get_path('purelib')), Path(sysconfig.get_path('platlib'))
    need(SITE.is_relative_to(ROOT / 'venv') and PLAT.is_relative_to(ROOT / 'venv'),
         'sysconfig_outside_owned_venv')
    STATE['target'] = target
    # The observer explicitly loads the declared facade set for origin parity;
    # this is not a claim that every literal example calls every facade member.
    for member in MEMBERS:
        name = ('assay._native' if member.endswith('.so') else
                member[:-3].replace('/', '.').removesuffix('.__init__'))
        importlib.import_module(name)


def sdk_member_origins():
    result = {}
    for member, expected in MEMBERS.items():
        name = ('assay._native' if member.endswith('.so') else
                member[:-3].replace('/', '.').removesuffix('.__init__'))
        module = sys.modules.get(name)
        need(module is not None, 'required_module_not_loaded')
        path = getattr(module, '__file__', None)
        spec_path = getattr(getattr(module, '__spec__', None), 'origin', None)
        directory = PLAT if member.endswith('.so') else SITE
        need(type(path) is str and path == spec_path and len(path) <= 1024 and
             Path(path) == directory / member, 'module_file_spec_origin')
        evidence = owned_file(path, 16 * 1024**2)
        need(evidence['bytes'] == expected['bytes'] and
             evidence['sha256'] == expected['sha256'], 'released_member_mismatch')
        result[member] = evidence
    return result


def module_origins():
    result = {}
    for member, evidence in sdk_member_origins().items():
        name = ('assay._native' if member.endswith('.so') else
                member[:-3].replace('/', '.').removesuffix('.__init__'))
        result[name] = evidence
    for runtime_name, spec_name, member in (
            ('pytest', 'pytest', 'pytest/__init__.py'),
            ('__main__', 'pytest.__main__', 'pytest/__main__.py')):
        module = sys.modules.get(runtime_name)
        spec = getattr(module, '__spec__', None)
        need(module is not None and getattr(module, '__name__', None) == runtime_name and
             getattr(spec, 'name', None) == spec_name and
             getattr(module, '__file__', None) == getattr(spec, 'origin', None) and
             Path(module.__file__) == SITE / member, 'pytest_runtime_origin')
        evidence = owned_file(module.__file__, 1024 * 1024)
        expected = PYTEST_ENTRIES[member]
        need(evidence['bytes'] == expected['bytes'] and
             evidence['sha256'] == expected['sha256'], 'pytest_entry_mismatch')
        result[spec_name] = evidence
    return result


def compact_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode('utf8')).hexdigest()


def plugin_records(config, final=False):
    records, identities = [], set()
    required = set(DEFAULT_MODULES) | set(OBJECTS) | {'pluginmanager', 'assay', 'observer', 'literal'}
    if final:
        required.add('funcmanage')
    pairs = config.pluginmanager.list_name_plugin()
    need(len(pairs) <= 64, 'plugin_count')
    site = SITE
    for name, plugin in pairs:
        need(plugin is not None and type(name) is str and len(name) <= 256, 'blocked_or_invalid_plugin')
        kind = 'module' if isinstance(plugin, types.ModuleType) else 'object'
        if plugin is config.pluginmanager:
            identity, module_name, qualname = 'pluginmanager', '_pytest.config', 'PytestPluginManager'
            need(name == str(id(plugin)) and type(plugin).__module__ == module_name and
                 type(plugin).__qualname__ == qualname, 'pluginmanager_instance_registration')
            path = Path(sys.modules[module_name].__file__)
        elif kind == 'module':
            module_name = plugin.__name__
            qualname = None
            path = Path(plugin.__file__)
            if name in DEFAULT_MODULES:
                identity = name
                need(module_name == '_pytest.' + name, 'builtin_module_identity')
            elif name == 'assay':
                identity = 'assay'
                need(module_name == 'assay.pytest_plugin' and path == site / 'assay/pytest_plugin.py',
                     'assay_plugin_identity')
            elif path == ROOT / 'conftest.py' and name == str(path):
                identity = 'observer'
            elif path == ROOT / STATE['case'] / 'conftest.py' and name == str(path):
                identity = 'literal'
            else:
                raise ObservationError('unexpected_module_plugin')
        else:
            identity = name
            expected = OBJECTS.get(name)
            if final and name == 'funcmanage':
                expected = ['_pytest.fixtures', 'FixtureManager']
            need(expected is not None, 'unexpected_object_plugin')
            cls = plugin if isinstance(plugin, type) else type(plugin)
            module_name, qualname = cls.__module__, cls.__qualname__
            need([module_name, qualname] == expected, 'builtin_object_identity')
            path = Path(sys.modules[module_name].__file__)
        need(identity not in identities, 'duplicate_plugin_identity')
        identities.add(identity)
        evidence = owned_file(path, 1024 * 1024)
        if module_name in PINNED_MODULE_FILES:
            member, size, digest = PINNED_MODULE_FILES[module_name]
            need(path == site / member and evidence['bytes'] == size and
                 evidence['sha256'] == digest, 'builtin_plugin_source')
        elif identity == 'literal':
            need(evidence['bytes'] == 175 and evidence['sha256'] == FIXTURE_SHA, 'literal_plugin_source')
        elif identity == 'observer':
            original = owned_file(SOURCE_ORIGIN, 65536)
            need(evidence['bytes'] == original['bytes'] and evidence['sha256'] == original['sha256'],
                 'observer_copy_source')
        elif identity == 'assay':
            need(evidence['sha256'] == ASSAY_PLUGIN_SHA, 'assay_plugin_source')
        else:
            raise ObservationError('unbound_plugin_source')
        records.append({'identity': identity, 'registration': name if identity == 'pluginmanager' else None,
                        'module': module_name, 'class': qualname, 'path': evidence['path'],
                        'sha256': evidence['sha256']})
    need(identities == required, 'plugin_membership_missing_or_added')
    return sorted(records, key=lambda row: row['identity'])


@pytest.hookimpl(trylast=True)
def pytest_configure(config):
    try:
        load_shared_source()
        initialize_contract()
        cwd = Path(config.invocation_params.dir)
        need(cwd in (ROOT / 'positive', ROOT / 'contrast'), 'wrong_case_cwd')
        STATE['case'] = cwd.name
        need(Path(config.rootpath) == ROOT and Path(config.inipath) == ROOT / 'pytest.ini',
             'pytest_root_config')
        need(config.getoption('importmode') == 'importlib' and
             Path(config.getoption('confcutdir')) == ROOT and
             Path(config.getoption('basetemp')) == ROOT / (cwd.name + '-tmp'),
             'pytest_import_cut_temp')
        versions = {name: importlib.metadata.version(name) for name in ('assay-it', 'pytest', 'pip')}
        need(versions['assay-it'] == '6.9.0' and versions['pytest'] == '9.1.1',
             'installed_versions')
        # Bundled pip version is observed, not hardcoded or silently upgraded.
        STATE.update({'configured': True, 'versions': versions,
                      'modules_before': module_origins(), 'plugins_before': plugin_records(config),
                      'config': {'cwd': cwd.name, 'root': '.',
                                 'inipath': 'pytest.ini', 'importmode': config.getoption('importmode'),
                                 'confcutdir': '.',
                                 'basetemp': cwd.name + '-tmp',
                                 'addopts': config.getini('addopts')}})
    except Exception as error:
        STATE['errors'].append(type(error).__name__)
        raise ObservationError('configure_observation_invalid') from error


@pytest.hookimpl(wrapper=True)
def pytest_fixture_setup(fixturedef, request):
    value = yield
    if fixturedef.argname == 'assay_client':
        try:
            need(STATE['configured'] and STATE['case'] == 'positive' and
                 request.node.nodeid.endswith('test_agent.py::test_agent_run'), 'unexpected_fixture_node')
            need(not STATE['fixtures'], 'duplicate_assay_fixture')
            code = getattr(fixturedef.func, '__code__', None)
            need(code is not None and Path(code.co_filename) == ROOT / 'positive' / 'conftest.py',
                 'fixture_not_literal_local_function')
            source = owned_file(code.co_filename, 4096)
            need(source['bytes'] == 175 and source['sha256'] == FIXTURE_SHA, 'literal_fixture_hash')
            STATE['fixtures'].append({'nodeid': request.node.nodeid, 'argname': fixturedef.argname,
                                      'function': fixturedef.func.__qualname__, 'source': source})
        except Exception as error:
            STATE['errors'].append(type(error).__name__)
            raise ObservationError('fixture_observation_invalid') from error
    return value


@pytest.hookimpl
def pytest_runtest_logreport(report):
    try:
        need(STATE['configured'], 'report_before_configure')
        nodes = {'test_validate.py::test_compliance', 'test_coverage.py::test_coverage'}
        if STATE['case'] == 'positive':
            nodes.add('test_agent.py::test_agent_run')
        node = report.nodeid
        need(type(node) is str and len(node) <= 1024 and
             node.split('/')[-1] in nodes, 'unexpected_report_node')
        phase, outcome = report.when, report.outcome
        need(phase in ('setup', 'call', 'teardown') and
             outcome in ('passed', 'failed', 'skipped'), 'unknown_report_phase_outcome')
        need(len(STATE['reports']) < 9, 'report_count')
        entry = {'nodeid': node, 'phase': phase, 'outcome': outcome,
                 'wasxfail': hasattr(report, 'wasxfail')}
        if outcome == 'failed':
            crash = getattr(report.longrepr, 'reprcrash', None)
            entries = getattr(getattr(report.longrepr, 'reprtraceback', None), 'reprentries', None)
            need(crash is not None and type(entries) is list and 0 < len(entries) <= 32,
                 'missing_bounded_assertion_representation')
            message = crash.message
            lines = entries[-1].lines
            need(type(message) is str and len(message.encode('utf8')) <= 2048 and
                 type(lines) is list and len(lines) <= 32 and
                 all(type(x) is str and len(x.encode('utf8')) <= 2048 for x in lines),
                 'assertion_representation_cap')
            entry['assertion'] = {'message': message, 'lines': lines,
                                  'line': int(crash.lineno)}
        STATE['reports'].append(entry)
    except Exception as error:
        STATE['errors'].append(type(error).__name__)
        raise ObservationError('test_phase_observation_invalid') from error


def validate_phase_receipts(exitstatus, collected, failed):
    need(type(SHARED) is dict, 'shared_rule_not_loaded')
    SHARED['validate_observation'](STATE['case'], int(exitstatus), collected, failed,
                                   STATE['reports'])


@pytest.hookimpl(trylast=True)
def pytest_sessionfinish(session, exitstatus):
    try:
        need(STATE['configured'], 'no_configuration_receipt')
        expected = 1 if STATE['case'] == 'positive' else 0
        need(len(STATE['fixtures']) == expected and not STATE['errors'], 'fixture_or_observer_incomplete')
        after_modules = module_origins()
        need(after_modules == STATE['modules_before'], 'module_source_changed')
        STATE['modules_after_hash'] = compact_hash(after_modules)
        STATE['modules_before_hash'] = compact_hash(STATE['modules_before'])
        final_plugins = plugin_records(session.config, final=True)
        addition = [row for row in final_plugins if row['identity'] == 'funcmanage']
        comparable = [row for row in final_plugins if row['identity'] != 'funcmanage']
        need(comparable == STATE['plugins_before'] and len(addition) == 1, 'plugin_lifecycle_parity')
        STATE['plugin_declared_addition'] = addition[0]
        STATE['plugins_before_hash'] = compact_hash(STATE['plugins_before'])
        STATE['plugins_final_base_hash'] = compact_hash(comparable)
        validate_phase_receipts(exitstatus, session.testscollected, session.testsfailed)
        STATE['pytest_exitstatus'] = int(exitstatus)
        STATE['tests_collected'] = session.testscollected
        STATE['tests_failed'] = session.testsfailed
        raw = (json.dumps(STATE, sort_keys=True, separators=(',', ':')) + '\n').encode('utf8')
        need(len(raw) <= 16384, 'observer_record_cap')
        path = ROOT / ('observer-' + STATE['case'] + '.json')
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        try:
            at = 0
            while at < len(raw):
                n = os.write(fd, raw[at:])
                need(type(n) is int and n > 0, 'observer_short_write')
                at += n
        finally:
            os.close(fd)
    except Exception as error:
        STATE['errors'].append(type(error).__name__)
        session.exitstatus = pytest.ExitCode.INTERNAL_ERROR
        raise ObservationError('session_observation_invalid') from error
