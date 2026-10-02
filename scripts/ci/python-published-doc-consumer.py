#!/usr/bin/env python3
"""Published SDK doc consumer. Native execution needs reviewed source and admission.
Shared result rules, actual bounded direct-child provider and native cell driver.
Source preparation is distinct from observed installation and hosted acceptance.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import selectors
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

POSITIVE_NODES = ('test_validate.py::test_compliance', 'test_coverage.py::test_coverage',
                  'test_agent.py::test_agent_run')
PHASES = ('setup', 'call', 'teardown')
FIXTURE_SHA = 'ba7cc1307ce9ee81477a152ac6b5fb29d7c9214b9f094a87af38f3c68758914a'

class ResultRefused(ValueError):
    pass

def require(ok, reason):
    if not ok:
        raise ResultRefused(reason)

def validate_phases(rows, nodes, contrast):
    require(type(rows) is list, 'PHASE_SHAPE')
    seen = set()
    for row in rows:
        require(type(row) is dict, 'PHASE_SHAPE')
        node, phase = row.get('nodeid'), row.get('phase')
        require(node in nodes and phase in PHASES, 'PHASE_SHAPE')
        key = (node, phase)
        require(key not in seen, 'PHASE_INVENTORY')
        seen.add(key)
        desired = 'failed' if contrast and phase == 'call' else 'passed'
        require(row.get('outcome') == desired and row.get('wasxfail') is False,
                'PHASE_OUTCOME')
        if contrast and phase == 'call':
            evidence = row.get('assertion')
            require(type(evidence) is dict and type(evidence.get('message')) is str and
                    type(evidence.get('lines')) is list and
                    len(evidence['lines']) <= 32 and all(type(line) is str for line in evidence['lines']),
                    'THRESHOLD_ASSERTION')
            message = evidence['message']
            desired = ('Coverage is below threshold 80' if node == POSITIVE_NODES[0]
                       else 'assert')
            require(desired in message, 'THRESHOLD_ASSERTION')
            literal = ('assert report["meets_threshold"]' if node == POSITIVE_NODES[0]
                       else 'assert report["overall_coverage_pct"] >= 90.0')
            require(any(literal in line for line in evidence['lines']),
                    'THRESHOLD_ASSERTION')
    require(seen == {(node, phase) for node in nodes for phase in PHASES},
            'PHASE_INVENTORY')

def normalize_node(node, case, nodes, reason='PHASE_SHAPE'):
    require(type(node) is str, reason)
    if node.startswith(case + '/'):
        node = node[len(case) + 1:]
    require(node in nodes, reason)
    return node

def validate_fixture(fixture):
    """Flat/nested fixture receiver."""
    require(type(fixture) is dict, 'FIXTURE_ORIGIN')
    nested = 'source' in fixture
    source = fixture.get('source') if nested else fixture
    require(type(source) is dict, 'FIXTURE_ORIGIN')
    node = normalize_node(fixture.get('nodeid'), 'positive', POSITIVE_NODES, 'FIXTURE_ORIGIN')
    require(node == POSITIVE_NODES[2] and fixture.get('function') == 'assay_client' and
            (not nested or fixture.get('argname') == 'assay_client') and
            source.get('path' if nested else 'origin') == 'positive/conftest.py' and
            source.get('bytes') == 175 and source.get('sha256') == FIXTURE_SHA,
            'FIXTURE_ORIGIN')

def validate_observation(case, exitstatus, collected, failed, rows):
    """Shared structured phase/summary rule."""
    require(case in ('positive', 'contrast'), 'OBSERVATION_CASE')
    nodes = POSITIVE_NODES if case == 'positive' else POSITIVE_NODES[:2]
    require((exitstatus, collected, failed) ==
            ((0, 3, 0) if case == 'positive' else (1, 2, 2)), 'OBSERVATION_SUMMARY')
    require(type(rows) is list, 'PHASE_SHAPE')
    normalized = []
    for row in rows:
        require(type(row) is dict, 'PHASE_SHAPE')
        row = dict(row)
        row['nodeid'] = normalize_node(row.get('nodeid'), case, nodes)
        normalized.append(row)
    validate_phases(normalized, nodes, case == 'contrast')

def validate_cell_result(row):
    """Shared complete-cell funnel."""
    require(type(row) is dict, 'RESULT_SHAPE')
    cell, install = row.get('cell'), row.get('public_install')
    require(type(cell) is dict and type(install) is dict, 'RESULT_SHAPE')
    require(install.get('argv') == ['pip', 'install', 'assay-it'] and
            install.get('first_install') is True and install.get('exit') == 0 and
            install.get('version') == '6.9.0', 'PUBLIC_INSTALL_IDENTITY')
    positive, contrast = row.get('positive'), row.get('contrast')
    require(type(positive) is dict and type(contrast) is dict, 'RESULT_SHAPE')
    validate_fixture(positive.get('fixture'))
    for case, observation in (('positive', positive), ('contrast', contrast)):
        validate_observation(case, observation.get('exit'), observation.get('collected'),
                             observation.get('failed'), observation.get('phases'))
    require(row.get('observer_errors') == [] and row.get('capture_complete') is True and
            row.get('inputs_postflight_match') is True and
            row.get('direct_children_reaped') is True, 'RESULT_INCOMPLETE')
    validate_native_facts(row)
    target, python = cell.get('target'), cell.get('python')
    require(type(target) is str and type(python) is str, 'CELL_IDENTITY')
    return target, python

def validate_job_results(rows, expected_cells):
    """Shared cell rule, exact job inventory."""
    require(type(rows) is list and type(expected_cells) is list and expected_cells,
            'CELL_INVENTORY')
    actual = [validate_cell_result(row) for row in rows]
    require(len(set(expected_cells)) == len(expected_cells) and
            len(actual) == len(expected_cells) and len(set(actual)) == len(actual) and
            set(actual) == set(expected_cells), 'CELL_INVENTORY')
    return actual

def run_install_chain(pip, tooling_args, invoke):
    """Public acquisition first."""
    require(type(pip) is str and pip and type(tooling_args) is list and
            tooling_args and all(type(arg) is str for arg in tooling_args),
            'INSTALL_COMMAND_SHAPE')
    steps = [([pip, 'install', 'assay-it'], 'PUBLIC_INSTALL_EXIT'),
             ([pip] + tooling_args, 'TOOLING_INSTALL_EXIT')]
    for argv, reason in steps:
        code = invoke(argv)
        require(type(code) is int, 'INSTALL_PROVIDER_RESULT')
        require(code == 0, reason + ':' + str(code))

INPUTS_SHA = '9f078064cd5188a3b5069f190663f8e5552a25d0b0ef49d965c25f3cfd43799e'

def stable_read(path, cap):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        a = os.fstat(fd)
        require(stat.S_ISREG(a.st_mode) and 0 <= a.st_size <= cap, 'READ_CAP_TYPE')
        pieces, count = [], 0
        while count < a.st_size:
            part = os.read(fd, min(65536, a.st_size - count))
            require(bool(part), 'READ_SHRANK')
            pieces.append(part); count += len(part)
        require(not os.read(fd, 1), 'READ_GREW')
        b, c = os.fstat(fd), Path(path).lstat()
        identity = lambda q: (q.st_dev, q.st_ino, q.st_size, q.st_mtime_ns, q.st_ctime_ns)
        require(identity(a) == identity(b) == identity(c), 'READ_CHANGED')
        raw = b''.join(pieces)
        return raw, {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    finally:
        os.close(fd)

def load_contract():
    raw, record = stable_read(Path(__file__).with_name('fixtures') /
                              'python-published-doc-inputs.v1.json', 32768)
    require(record['sha256'] == INPUTS_SHA, 'INPUT_CONTRACT')
    return json.loads(raw)

def validate_members(rows, expected):
    require(type(rows) is dict and set(rows) == {r['member'] for r in expected},
            'INSTALLED_MEMBER')
    for record in expected:
        row = rows[record['member']]
        require(type(row) is dict and type(row.get('path')) is str and
                row['path'].startswith('venv/') and
                row['path'].endswith('/' + record['member']) and
                row.get('bytes') == record['bytes'] and row.get('sha256') == record['sha256'],
                'INSTALLED_MEMBER')

def validate_pip_report(report, install, wheel):
    require(type(report) is dict and type(report.get('install')) is list and
            len(report['install']) == 1 and type(report.get('pip_version')) is str and
            report['pip_version'] == install.get('pip_version'), 'PIP_REPORT')
    item = report['install'][0]
    require(type(item) is dict and type(item.get('metadata')) is dict and
            item['metadata'].get('name', '').lower().replace('_','-') == 'assay-it' and
            item['metadata'].get('version') == '6.9.0', 'PIP_REPORT_SELECTION')
    download = item.get('download_info', {})
    require(type(download) is dict and type(download.get('url')) is str and
            urlsplit(download['url']).path.split('/')[-1] == wheel['wheel'] and
            download.get('archive_info', {}).get('hashes', {}).get('sha256') == wheel['sha256'] and
            install.get('selected_wheel') == wheel['wheel'] and
            install.get('provider_digest') == wheel['sha256'] and
            install.get('archive_digest_layer') == 'provider_report_only', 'PIP_REPORT_SELECTION')

def validate_native_facts(row):
    require(row.get('status') == 'COMPLETE', 'CELL_STATUS')
    contract = load_contract()
    target = row['cell'].get('target')
    require(target in contract['platforms'] and row['cell'].get('python') in ('3.12','3.13','3.14'),
            'CELL_IDENTITY')
    wheel = contract['platforms'][target]
    validate_pip_report(row.get('pip_report'), row['public_install'], wheel)
    validate_members(row.get('members_before'), wheel['members'])
    validate_members(row.get('members_after'), wheel['members'])
    require(row['members_before'] == row['members_after'], 'MEMBER_PARITY')
    origins = row.get('nonpytest_origins')
    require(type(origins) is dict and set(origins) == {'positive-recorder','contrast-recorder','zero'},
            'NONPYTEST_ORIGINS')
    for fact in origins.values():
        require(type(fact) is dict and fact.get('before') == fact.get('after'), 'NONPYTEST_ORIGINS')
        validate_members(fact.get('before'), wheel['members'])
    reports = row.get('reports')
    require(type(reports) is dict, 'FULL_REPORT')
    for case, value in [('positive',100.0),('contrast',68.75)]:
        group = reports.get(case)
        require(type(group) is dict, 'FULL_REPORT')
        for name, threshold in [('validate_default',80.0),('coverage_min_90',90.0)]:
            fact = group.get(name)
            require(type(fact) is dict and fact.get('overall_coverage_pct') == value and
                    fact.get('threshold') == threshold and
                    fact.get('meets_threshold') is (case == 'positive'), 'FULL_REPORT')
    tools = reports['contrast']['coverage_min_90'].get('tool_coverage')
    require(type(tools) is dict and tools.get('coverage_pct') == 50.0 and
            tools.get('tools_seen_in_traces') == 3 and tools.get('total_tools_in_policy') == 6,
            'FULL_REPORT')
    zero = reports.get('same_input_zero')
    require(type(zero) is dict and zero.get('threshold') == 0.0 and
            zero.get('meets_threshold') is True and
            zero.get('overall_coverage_pct') == reports['contrast']['coverage_min_90']['overall_coverage_pct'],
            'MIN_ZERO')
    trace = row.get('trace_bytes')
    if type(trace) is str:
        require(len(trace) <= 4096, 'FRESH_TRACE'); trace = trace.encode('utf8')
    require(type(trace) is bytes and len(trace) <= 4096 and trace.endswith(b'\n') and
            trace.count(b'\n') == 1, 'FRESH_TRACE')
    try:
        parsed = json.loads(trace)
    except (ValueError, UnicodeError):
        raise ResultRefused('FRESH_TRACE')
    require(parsed == {'tool':'search','args':{'q':'foo'}}, 'FRESH_TRACE')

def capped_write(sink, raw, cap, used):
    require(type(raw) is bytes and len(raw) <= cap - used, 'RAW_CAP')
    count = sink.write(raw)
    require(count == len(raw), 'RAW_SHORT_WRITE')
    return used + count

class NativeProvider:
    """Cell deadline; owned children; sampled growth."""
    def __init__(self, root, artifacts, clock=time.monotonic, start=None):
        self.root, self.artifacts, self.clock = Path(root), Path(artifacts), clock
        self.start = clock() if start is None else start
        self.end, self.work_end = self.start + 600, self.start + 590
        self.popen, self.records, self.total_raw, self.last_scan = subprocess.Popen, [], 0, -1
        self.live = None

    def admit(self):
        require(self.clock() < self.work_end, 'CELL_DEADLINE')

    def growth(self, force=False):
        self.admit()
        now = self.clock()
        if not force and now - self.last_scan < 1:
            return
        require(shutil.disk_usage(self.root).free >= 6 * 1024**3, 'FREE_DISK')
        total, retained, count, stack = 0, 0, 0, [self.root,self.artifacts]
        while stack:
            self.admit()
            directory = stack.pop()
            try: entries = os.scandir(directory)
            except FileNotFoundError:
                if directory in (self.root,self.artifacts): raise
                continue
            with entries:
                for entry in entries:
                    self.admit()
                    count += 1
                    require(count <= 20000, 'WORK_ENTRIES')
                    try: meta = entry.stat(follow_symlinks=False)
                    except FileNotFoundError: continue
                    if stat.S_ISDIR(meta.st_mode): stack.append(Path(entry.path))
                    elif stat.S_ISREG(meta.st_mode):
                        total += meta.st_size
                        if Path(entry.path).is_relative_to(self.artifacts): retained += meta.st_size
                        require(retained <= 512*1024**2, 'ARTIFACT_BYTES')
                    require(total <= 2 * 1024**3, 'WORK_BYTES')
        for root in (self.root,self.artifacts):
            require(stat.S_ISDIR(root.stat().st_mode),'OWNED_ROOT')
        self.last_scan = self.clock()

    def cleanup(self, child):
        result = {'direct_child_reaped': False, 'pipes_closed': False}
        try:
            if child.poll() is None and self.clock() < self.end:
                child.terminate()
                try: child.wait(timeout=min(1, max(0, self.end-self.clock())))
                except subprocess.TimeoutExpired:
                    if self.clock() < self.end:
                        child.kill()
                        child.wait(timeout=max(0, self.end-self.clock()))
            elif child.poll() is not None:
                child.wait(timeout=0)
            result['direct_child_reaped'] = child.returncode is not None
        except (OSError, subprocess.TimeoutExpired):
            pass
        finally:
            for stream in (child.stdout, child.stderr):
                if stream is not None: stream.close()
            result['pipes_closed'] = True
        return result

    def run(self, name, argv, cwd, env, seconds, expected=0):
        self.admit()
        deadline = min(self.work_end, self.clock()+seconds)
        require(not self.live, 'CHILD_OVERLAP')
        child = self.popen([str(x) for x in argv], cwd=str(cwd), env=env,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.live = child
        record = {'stage':name, 'exit':None, 'complete':False,
                  'direct_child_reaped':False, 'raw':{}, 'deadline_seconds':seconds}
        self.records.append(record)
        sinks, captures, used = {}, {'stdout':bytearray(), 'stderr':bytearray()}, {}
        selector = selectors.DefaultSelector()
        try:
            for channel, stream in [('stdout',child.stdout),('stderr',child.stderr)]:
                path = self.artifacts / (name+'-'+channel+'.log')
                sinks[channel] = open(path, 'xb')
                used[channel] = 0
                os.set_blocking(stream.fileno(),False)
                selector.register(stream,selectors.EVENT_READ,channel)
            while selector.get_map():
                self.admit()
                require(self.clock() < deadline, 'COMMAND_DEADLINE')
                self.growth()
                for key, _ in selector.select(min(.2,max(0,deadline-self.clock()))):
                    self.admit()
                    raw = os.read(key.fileobj.fileno(),4096)
                    if not raw:
                        selector.unregister(key.fileobj); continue
                    channel = key.data
                    require(len(raw) <= 4*1024**2-self.total_raw, 'AGGREGATE_RAW_CAP')
                    used[channel] = capped_write(sinks[channel],raw,256*1024,used[channel])
                    self.total_raw += len(raw); captures[channel].extend(raw)
            self.admit()
            child.wait(timeout=max(0,min(deadline,self.work_end)-self.clock()))
            require(self.clock() < deadline, 'COMMAND_DEADLINE')
            record['exit'] = child.returncode
            require(child.returncode == expected, 'COMMAND_EXIT:'+name+':'+str(child.returncode))
            record['complete'] = True
            return bytes(captures['stdout'])
        finally:
            selector.close()
            record.update(self.cleanup(child))
            self.live = None
            for channel, sink in sinks.items():
                sink.close()
                raw = bytes(captures[channel])
                record['raw'][channel] = {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
            if record['complete'] and self.clock() >= deadline:
                record['complete'] = False
                raise ResultRefused('COMMAND_DEADLINE')

def write_exclusive(path, raw, cap):
    require(len(raw) <= cap, 'WRITE_CAP')
    fd = os.open(path, os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    try:
        at = 0
        while at < len(raw):
            n = os.write(fd,raw[at:]); require(n > 0,'WRITE_SHORT'); at += n
    finally:
        os.close(fd)

def member_rows(observer, contract, target, key):
    values = observer[key]
    return {row['member']: values['assay._native' if row['member'].endswith('.so') else
                                  row['member'][:-3].replace('/','.').removesuffix('.__init__')]
            for row in contract['platforms'][target]['members']}

def json_read(path, cap):
    raw, _ = stable_read(path,cap)
    return json.loads(raw)

def isolated_environment(root):
    return {'PATH':'/usr/bin:/bin','LANG':'C','LC_ALL':'C',
            'HOME':str(root/'home'),'TMPDIR':str(root/'tmp'),
            'PIP_CONFIG_FILE':os.devnull,'PIP_NO_INPUT':'1',
            'PIP_DISABLE_PIP_VERSION_CHECK':'1','PIP_RETRIES':'0','PIP_TIMEOUT':'15',
            'PIP_CACHE_DIR':str(root/'cache'),'PYTHONNOUSERSITE':'1'}

def native_cell(interpreter, line, target, work, artifacts, contract):
    started=time.monotonic()
    root = Path(tempfile.mkdtemp(prefix='cell-'+line+'-',dir=work)).resolve()
    evidence = artifacts/('cell-'+line); evidence.mkdir(mode=0o700)
    provider = NativeProvider(root,evidence,start=started)
    row = {'status':'INCOMPLETE','cell':{'target':target,'python':line},'commands':provider.records,
           'descendant_nonclaim':'Only directly retained Popen handles reaped; no process-tree closure.',
           'resource_nonclaim':'Root memory/swap admission external; disk/WORK growth sampled, not quota.'}
    try:
        provider.admit()
        for name in ['home','tmp','cache','positive','contrast']:(root/name).mkdir(mode=0o700)
        source = Path(__file__)
        consumer_raw, consumer_identity = stable_read(source,32768)
        observer_raw, observer_identity = stable_read(source.with_name('python-published-doc-observer.py'),32768)
        inputs_raw, inputs_identity = stable_read(source.with_name('fixtures')/'python-published-doc-inputs.v1.json',32768)
        require(inputs_identity['sha256']==INPUTS_SHA,'INPUT_CONTRACT')
        for name,raw in [('consumer-source.py',consumer_raw),('conftest.py',observer_raw),
                         ('observer-original.py',observer_raw),('inputs.json',inputs_raw),
                         ('pytest.ini',b'[pytest]\n')]:write_exclusive(root/name,raw,32768)
        for case in ['positive','contrast']:
            for name in ['test_validate.py','test_coverage.py','test_agent.py','conftest.py']:
                write_exclusive(root/case/name,contract['inputs'][name]['text'].encode(),4096)
            for suffix,dest in [('assay.yaml','assay.yaml'),('traces.jsonl','traces.jsonl')]:
                write_exclusive(root/case/dest,contract['inputs'][case+'-'+suffix]['text'].encode(),4096)
        write_exclusive(root/'n6_recorder.py',contract['inputs']['n6_recorder.py']['text'].encode(),4096)
        env = isolated_environment(root)
        _, executable = stable_read(Path(interpreter).resolve(strict=True),32*1024**2)
        row['source_identity']={'consumer':consumer_identity,'observer':observer_identity,
                                'inputs':inputs_identity,'interpreter':executable}
        identity_code = ('import sys,json,platform; print(json.dumps({"line":"%d.%d"%sys.version_info[:2],'
                         '"implementation":sys.implementation.name,"machine":platform.machine(),'
                         '"system":platform.system(),"prefix":sys.prefix,"base_prefix":sys.base_prefix}))')
        host = json.loads(provider.run('interpreter',[interpreter,'-I','-B','-c',identity_code],root,env,15))
        require(host['line']==line and host['implementation']=='cpython','INTERPRETER_LINE')
        actual_target={('Linux','x86_64'):'x86_64-unknown-linux-gnu',('Darwin','arm64'):'aarch64-apple-darwin',
                       ('Darwin','x86_64'):'x86_64-apple-darwin'}.get((host['system'],host['machine']))
        require(actual_target==target,'INTERPRETER_PLATFORM')
        provider.run('venv',[interpreter,'-I','-B','-m','venv',root/'venv'],root,env,60)
        v=root/'venv/bin/python'; pip=root/'venv/bin/pip'
        cfg,_=stable_read(root/'venv/pyvenv.cfg',4096)
        require(b'include-system-site-packages = false' in cfg,'VENV_ISOLATION')
        _, v_binary=stable_read(v.resolve(strict=True),32*1024**2)
        require(v_binary==executable,'VENV_BINARY_IDENTITY')
        pip_code=('import json,sys,pip,importlib.metadata; print(json.dumps({"version":importlib.metadata.version("pip"),'
                  '"origin":pip.__file__,"prefix":sys.prefix,"base_prefix":sys.base_prefix}))')
        bundled=json.loads(provider.run('bundled-pip',[v,'-I','-B','-c',pip_code],root,env,15))
        require(Path(bundled['origin']).is_relative_to(root/'venv') and
                Path(bundled['prefix']).resolve()==root/'venv' and
                Path(bundled['base_prefix']).resolve()==Path(host['base_prefix']).resolve(),'BUNDLED_PIP_ORIGIN')
        row['bundled_pip']=bundled
        requirements=''.join(contract['tooling_install']['requirements'])
        write_exclusive(root/'tooling.txt',requirements.encode(),4096)
        public_report=root/'public-install-report.json'
        def invoke(argv):
            public=argv==[str(pip),'install','assay-it']
            selected=dict(env)
            if public:selected['PIP_REPORT']=str(public_report)
            provider.run('public-install' if public else 'tooling-install',argv,root,selected,180 if public else 120)
            if public:
                report=json_read(public_report,65536)
                require(type(report) is dict and type(report.get('install')) is list and
                        len(report['install'])==1,'PIP_REPORT')
                item=report['install'][0]
                require(type(item) is dict and type(item.get('metadata')) is dict and
                        type(item.get('download_info')) is dict,'PIP_REPORT')
                download=item['download_info']
                install={'argv':['pip','install','assay-it'],'first_install':True,'exit':0,
                    'version':item['metadata'].get('version'),'pip_version':bundled['version'],
                    'selected_wheel':urlsplit(download.get('url','')).path.split('/')[-1],
                    'provider_digest':download.get('archive_info',{}).get('hashes',{}).get('sha256'),
                    'archive_digest_layer':'provider_report_only',
                    'measurement_environment':'PIP_REPORT owned-new-file; fresh sanitized config/cache'}
                validate_pip_report(report,install,contract['platforms'][target])
                row['pip_report'],row['public_install']=report,install
            return 0
        run_install_chain(str(pip),['install','--require-hashes','-r',str(root/'tooling.txt')],invoke)
        observers={}
        for case,nodes,exitcode in [('positive',POSITIVE_NODES,0),('contrast',POSITIVE_NODES[:2],1)]:
            provider.run(case+'-pytest',[v,'-I','-B','-m','pytest','-c',root/'pytest.ini','--rootdir',root,
                '--confcutdir',root,'--import-mode=importlib','--basetemp',root/(case+'-tmp'),'-v','--tb=short',*nodes],root/case,env,30,exitcode)
            observed=json_read(root/('observer-'+case+'.json'),16384); observers[case]=observed
            require(observed.get('configured')is True and observed.get('case')==case and
                    observed.get('versions',{}).get('pip')==bundled['version'],'OBSERVER_CONFIG_VERSION')
            row[case]={'exit':observed['pytest_exitstatus'],'collected':observed['tests_collected'],
                       'failed':observed['tests_failed'],'phases':observed['reports']}
            if case=='positive':
                require(len(observed['fixtures'])==1,'FIXTURE_ORIGIN');row[case]['fixture']=observed['fixtures'][0]
        row['members_before']=member_rows(observers['positive'],contract,target,'modules_before')
        row['members_after']=member_rows(observers['contrast'],contract,target,'modules_before')
        row['observer_errors']=observers['positive']['errors']+observers['contrast']['errors']
        origin_setup=("import runpy,json,sys; a=runpy.run_path("+repr(str(root/'observer-original.py'))+
                      ",run_name='doc_origin'); a['initialize_contract'](); before=a['sdk_member_origins'](); ")
        row['nonpytest_origins']={};row['reports']={}
        for case in ['positive','contrast']:
            reportpath=root/(case+'-report.json');originpath=root/(case+'-origins.json')
            code=origin_setup+"sys.argv=["+repr(str(root/'n6_recorder.py'))+","+repr(str(reportpath))+"]; runpy.run_path(sys.argv[0],run_name='__main__'); after=a['sdk_member_origins'](); "+"f=open("+repr(str(originpath))+",'x'); json.dump({'before':before,'after':after},f); f.close()"
            provider.run(case+'-recorder',[v,'-I','-B','-c',code],root/case,env,30)
            row['reports'][case]=json_read(reportpath,65536)
            row['nonpytest_origins'][case+'-recorder']=json_read(originpath,65536)
        zeropath=root/'zero.json';originpath=root/'zero-origins.json'
        code=origin_setup+"from assay import Coverage; t=[json.loads(x) for x in open('traces.jsonl')]; r=Coverage('assay.yaml').analyze(traces=[t],min_coverage=0.0); "+"f=open("+repr(str(zeropath))+",'x'); json.dump(r,f); f.close(); after=a['sdk_member_origins'](); f=open("+repr(str(originpath))+",'x'); json.dump({'before':before,'after':after},f); f.close()"
        provider.run('zero',[v,'-I','-B','-c',code],root/'contrast',env,30)
        row['reports']['same_input_zero']=json_read(zeropath,65536)
        row['nonpytest_origins']['zero']=json_read(originpath,65536)
        traces,stack,count=[],[root/'positive-tmp'],0
        while stack:
            provider.admit()
            with os.scandir(stack.pop()) as entries:
                for entry in entries:
                    provider.admit();count+=1;require(count<=20000,'TRACE_ENTRIES')
                    meta=entry.stat(follow_symlinks=False)
                    if stat.S_ISDIR(meta.st_mode):stack.append(Path(entry.path))
                    elif entry.name=='live_run.jsonl':
                        require(stat.S_ISREG(meta.st_mode),'FRESH_TRACE');traces.append(Path(entry.path))
        require(len(traces)==1,'FRESH_TRACE')
        row['trace_bytes'],_=stable_read(traces[0],4096)
        write_exclusive(evidence/'literal-trace.jsonl',row['trace_bytes'],4096)
        for case in ['positive','contrast']:
            for name in ['test_validate.py','test_coverage.py','test_agent.py','conftest.py']:
                raw,rec=stable_read(root/case/name,4096); require(rec['sha256']==contract['inputs'][name]['sha256'],'INPUT_POSTFLIGHT')
            for suffix in ['assay.yaml','traces.jsonl']:
                _,rec=stable_read(root/case/suffix,4096);require(rec['sha256']==contract['inputs'][case+'-'+suffix]['sha256'],'INPUT_POSTFLIGHT')
        for name,expected in [('consumer-source.py',consumer_identity),('conftest.py',observer_identity),
                              ('observer-original.py',observer_identity),('inputs.json',inputs_identity)]:
            _,rec=stable_read(root/name,32768);require(rec==expected,'SOURCE_POSTFLIGHT')
        _,rec=stable_read(root/'n6_recorder.py',4096);require(rec['sha256']==contract['inputs']['n6_recorder.py']['sha256'],'INPUT_POSTFLIGHT')
        row.update(status='COMPLETE',inputs_postflight_match=True,capture_complete=all(r['complete']for r in provider.records),
                   direct_children_reaped=all(r['direct_child_reaped']for r in provider.records))
        validate_cell_result(row)
        provider.growth(True)
    except Exception as error:
        row['status']='INCOMPLETE';row['error']=type(error).__name__+':'+str(error)[:256]
    finally:
        if provider.live is not None:
            row['abort_cleanup']=provider.cleanup(provider.live)
        row['retention_refusals'] = []
        for name in ['public-install-report.json','observer-positive.json','observer-contrast.json',
                     'positive-report.json','contrast-report.json','zero.json','positive-origins.json',
                     'contrast-origins.json','zero-origins.json']:
            if os.path.lexists(root/name):
                cap = 16384 if name.startswith('observer-') else 65536
                try:
                    raw,_=stable_read(root/name,cap);write_exclusive(evidence/name,raw,cap)
                except Exception as error:
                    row['retention_refusals'].append({'file':name,'reason':type(error).__name__})
                    row['status']='INCOMPLETE'
        serialized=dict(row)
        if type(serialized.get('trace_bytes'))is bytes:
            serialized['trace_bytes']=serialized['trace_bytes'].decode('utf8')
        raw=(json.dumps(serialized,sort_keys=True,indent=2)+'\n').encode()
        write_exclusive(evidence/'RESULT.json',raw,1024**2)
    return row

def main(argv=None):
    parser=argparse.ArgumentParser()
    parser.add_argument('--target',required=True)
    parser.add_argument('--interpreters',nargs=3,required=True)
    parser.add_argument('--python-lines',required=True)
    parser.add_argument('--work',required=True)
    parser.add_argument('--artifacts',required=True)
    args=parser.parse_args(argv)
    contract=load_contract()
    lines=json.loads(args.python_lines)
    matrix=json_read(Path(__file__).parents[2]/'assay-python-sdk/python-artifact-matrix.v0.json',65536)
    require(lines==matrix['smoke_pythons'] and args.target in {r['target']for r in matrix['wheels']},'CANONICAL_MATRIX')
    work,artifacts=Path(args.work).absolute(),Path(args.artifacts).absolute()
    work.mkdir(mode=0o700);artifacts.mkdir(mode=0o700)
    work,artifacts=work.resolve(),artifacts.resolve()
    rows=[]
    for interpreter,line in zip(args.interpreters,lines):
        try:
            rows.append(native_cell(interpreter,line,args.target,work,artifacts,contract))
        except Exception as error:
            row={'status':'INCOMPLETE','cell':{'target':args.target,'python':line},
                 'error':'outer:'+type(error).__name__,'capture_complete':False,
                 'direct_children_reaped':False}
            rows.append(row)
            write_exclusive(artifacts/('outer-failure-'+line+'.json'),
                            (json.dumps(row)+'\n').encode(),65536)
    validate_job_results(rows,[(args.target,line)for line in lines])
    return 0

if __name__=='__main__':
    try:raise SystemExit(main())
    except Exception as error:
        print('Consumer incomplete: '+type(error).__name__,file=sys.stderr)
        raise SystemExit(2)
