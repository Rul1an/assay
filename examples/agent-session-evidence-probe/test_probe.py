"""Bounded experiment-helper contracts; no model or credential access."""
import concurrent.futures
import importlib.util
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent


def module(test, name):
    path = ROOT / (name + '.py')
    test.assertTrue(path.exists(), 'Missing experiment helper: ' + name)
    spec = importlib.util.spec_from_file_location(name, path)
    obj = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(obj)
    return obj


class RecorderTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = pathlib.Path(self.tmp.name)
        self.rec = module(self, 'record_hook')

    def test_exact_original_bytes_and_duplicate_occurrences(self):
        raw = b' { "tool_use_id" : "same", "x": 1 }\r\n'
        a = self.rec.capture(io.BytesIO(raw), self.out, len(raw))
        b = self.rec.capture(io.BytesIO(raw), self.out, len(raw))
        self.assertNotEqual(a, b)
        self.assertEqual(a.read_bytes(), raw)
        self.assertEqual(b.read_bytes(), raw)
        self.assertTrue(a.with_suffix('.ok').exists())

    def test_concurrent_calls_never_overwrite(self):
        raws = [json.dumps({'n': n}).encode() for n in range(12)]
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            paths = list(pool.map(lambda raw: self.rec.capture(io.BytesIO(raw), self.out, 128), raws))
        self.assertEqual(len(set(paths)), 12)
        self.assertEqual(sorted(p.read_bytes() for p in paths), sorted(raws))

    def test_limit_plus_one_retained_but_not_complete(self):
        with self.assertRaises(ValueError):
            self.rec.capture(io.BytesIO(b'12345'), self.out, 4)
        self.assertEqual(list(self.out.glob('*.ok')), [])
        self.assertEqual([p.read_bytes() for p in self.out.glob('*.raw')], [b'12345'])

    def test_occurrence_ceiling_before_read(self):
        for _ in range(2):
            self.rec.capture(io.BytesIO(b'x'), self.out, 1, max_occurrences=2)
        stream = io.BytesIO(b'new')
        with self.assertRaises(ValueError):
            self.rec.capture(stream, self.out, 3, max_occurrences=2)
        self.assertEqual(stream.tell(), 0)
        self.assertEqual(len(list(self.out.glob('*.raw'))), 2)

    def test_failure_keeps_prior_bytes_and_no_complete_marker(self):
        old = self.rec.capture(io.BytesIO(b'old'), self.out, 8)
        class Broken:
            def read(self, n):
                raise OSError('simulated read failure')
        with self.assertRaises(OSError):
            self.rec.capture(Broken(), self.out, 8)
        self.assertEqual(old.read_bytes(), b'old')
        self.assertEqual(len(list(self.out.glob('*.ok'))), 1)

    def test_symlink_cannot_redirect_write(self):
        victim = self.out / 'victim'
        victim.write_bytes(b'unchanged')
        (self.out / '000.raw').symlink_to(victim)
        self.rec.capture(io.BytesIO(b'new'), self.out, 8)
        self.assertEqual(victim.read_bytes(), b'unchanged')

    def test_cli_failure_is_nonblocking_and_stdout_empty(self):
        invalid_dir = self.out / 'not-a-directory'
        invalid_dir.write_text('old')
        run = subprocess.run([sys.executable, '-I', str(ROOT / 'record_hook.py'), str(invalid_dir)],
                             input=b'{}', capture_output=True, timeout=5)
        self.assertEqual(run.returncode, 0)
        self.assertEqual(run.stdout, b'')
        self.assertIn(b'capture_failed', run.stderr)
        self.assertEqual(invalid_dir.read_text(), 'old')


class ReconciliationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = pathlib.Path(self.tmp.name)
        self.m = module(self, 'record_hook')
        self.key = ('PreToolUse', 'call-a')
        self.raw = b'{"hook_event_name":"PreToolUse","tool_use_id":"call-a","value":1}'
        self.path = self.m.capture(io.BytesIO(self.raw), self.out, 1024)

    def check(self, starts=None, responses=None, expected=None, final=True, wrappers=1):
        return self.m.reconcile([self.key] if starts is None else starts,
                                [self.key] if responses is None else responses,
                                self.out, expected=[self.key] if expected is None else expected,
                                final_seen=final, wrapper_starts=wrappers)

    def test_complete_bytes_and_lifecycle_match(self):
        self.assertTrue(self.check())

    def test_missing_killed_hook_missing_stream_and_restart_fail(self):
        for args in [dict(responses=[]), dict(final=False), dict(wrappers=2),
                     dict(expected=[self.key, ('PreToolUse','missing')]),
                     dict(starts=[('PostToolUse','call-a')]), dict(starts=[])]:
            self.assertFalse(self.check(**args))

    def test_tamper_with_marker_still_present_is_incomplete(self):
        self.path.write_bytes(self.raw.replace(b'"value":1', b'"value":2'))
        self.assertFalse(self.check())

    def test_missing_marker_or_extra_incomplete_slot_is_incomplete(self):
        self.path.with_suffix('.ok').unlink()
        self.assertFalse(self.check())
        self.m.capture(io.BytesIO(self.raw), self.out, 1024)
        self.assertFalse(self.check())

    def test_duplicate_occurrence_is_not_deduplicated(self):
        self.m.capture(io.BytesIO(self.raw), self.out, 1024)
        self.assertFalse(self.check())

    def test_fifo_capture_refuses_without_blocking(self):
        import os
        self.path.unlink()
        os.mkfifo(self.path)
        code = "import sys;sys.path.insert(0,sys.argv[1]);import record_hook as r;k=[('PreToolUse','call-a')];print(r.reconcile(k,k,sys.argv[2],expected=k,final_seen=True,wrapper_starts=1))"
        try:
            run=subprocess.run([sys.executable,'-B','-c',code,str(ROOT),str(self.out)],capture_output=True,timeout=1)
        except subprocess.TimeoutExpired:
            self.fail('reconciliation blocked before rejecting FIFO')
        self.assertEqual(run.stdout.strip(),b'False')

    def test_deleted_middle_slot_is_not_a_complete_capture(self):
        self.m.capture(io.BytesIO(self.raw), self.out, 1024)
        self.m.capture(io.BytesIO(self.raw), self.out, 1024)
        (self.out/'001.raw').unlink(); (self.out/'001.ok').unlink()
        keys=[self.key,self.key]
        self.assertFalse(self.check(starts=keys,responses=keys,expected=keys))

    def test_duplicate_json_keys_are_not_accepted_as_hook_identity(self):
        self.path.unlink(); self.path.with_suffix('.ok').unlink()
        self.m.capture(io.BytesIO(b'{"hook_event_name":"PreToolUse","tool_use_id":"x","tool_use_id":"call-a","value":1}'), self.out, 1024)
        self.assertFalse(self.check())


class WrapperTests(unittest.TestCase):
    def test_restart_stops_without_overwriting_first_start(self):
        mod = module(self, 'wrap_entry')
        with tempfile.TemporaryDirectory() as d:
            root = pathlib.Path(d)
            mod.claim_start(root)
            original = (root/'wrap.started').read_bytes()
            with self.assertRaises(FileExistsError):
                mod.claim_start(root)
            self.assertEqual((root/'wrap.started').read_bytes(), original)
            self.assertTrue((root/'wrap.restart').exists())


class FixtureTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = pathlib.Path(self.tmp.name)
        self.server = module(self, 'fixture_server')

    def test_initialization_and_fixed_tools(self):
        r = self.server.respond({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-03-26'}}, self.out)
        self.assertEqual(r['result']['protocolVersion'], '2025-03-26')
        tools = self.server.respond({'jsonrpc':'2.0','id':2,'method':'tools/list'}, self.out)['result']['tools']
        self.assertEqual({t['name'] for t in tools}, {'observe_three','policy_denied','vendor_denied'})

    def test_fixed_observation_and_retry(self):
        def call(args):
            return self.server.respond({'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'observe_three','arguments':args}}, self.out)
        self.assertTrue(call({'attempt':1})['result']['isError'])
        self.assertFalse(call({'attempt':2})['result']['isError'])
        self.assertEqual((self.out/'task.txt').read_bytes(), b'controlled probe\n')
        self.assertIn('error', call({'command':'touch /tmp/unapproved'}))

    def test_concurrent_identical_arguments_and_denial_target(self):
        req={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'observe_three','arguments':{}}}
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            rows=list(pool.map(lambda _: self.server.respond(req,self.out),range(2)))
        self.assertTrue(all(not row['result']['isError'] for row in rows))
        req['params']['name']='policy_denied'
        self.assertEqual(self.server.respond(req,self.out)['result']['content'][0]['text'], 'denial target reached server')

    def test_changed_fixed_file_does_not_report_success(self):
        (self.out/'task.txt').write_bytes(b'wrong')
        req={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'observe_three','arguments':{}}}
        self.assertTrue(self.server.respond(req,self.out)['result']['isError'])

    def test_non_regular_fixed_file_is_refused_without_blocking(self):
        import os
        os.mkfifo(self.out/'task.txt')
        req=b'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"observe_three","arguments":{}}}\n'
        try:
            run=subprocess.run([sys.executable,'-I',str(ROOT/'fixture_server.py'),str(self.out)],input=req,capture_output=True,timeout=1)
        except subprocess.TimeoutExpired:
            self.fail('fixture blocked on a non-regular control file')
        self.assertNotEqual(run.returncode,0)

    def test_wire_cancellation_does_not_discard_following_request(self):
        payload=b'{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":99}}\n'
        payload+=b'{"jsonrpc":"2.0","id":1,"method":"ping"}\n'
        run=subprocess.run([sys.executable,'-I',str(ROOT/'fixture_server.py'),str(self.out)],input=payload,capture_output=True,timeout=5)
        self.assertEqual(run.returncode,0,run.stderr)
        self.assertEqual(json.loads(run.stdout)['id'],1)

    def test_wire_budget_is_separate_from_package_occurrence_budget(self):
        payload=b''.join(json.dumps({'jsonrpc':'2.0','id':i,'method':'ping'}).encode()+b'\n' for i in range(65))
        run=subprocess.run([sys.executable,'-I',str(ROOT/'fixture_server.py'),str(self.out)],input=payload,capture_output=True,timeout=5)
        self.assertEqual(run.returncode,0,run.stderr)
        self.assertEqual(len(run.stdout.splitlines()),65)

    def test_wire_preserves_original_and_rejects_malformed(self):
        raw=b' {"jsonrpc":"2.0","id":4,"method":"tools/list"} \n'
        for payload, success in [(raw,True),(b'{bad}\n',False),(b'{"id":1,"id":2}\n',False),(b'x'*1048577+b'\n',False)]:
            with tempfile.TemporaryDirectory() as d:
                run=subprocess.run([sys.executable,'-I',str(ROOT/'fixture_server.py'),d],input=payload,capture_output=True,timeout=5)
                if success:
                    self.assertEqual(run.returncode,0,run.stderr)
                    self.assertEqual(next(pathlib.Path(d).glob('wire-*.raw')).read_bytes(),raw)
                    self.assertEqual(json.loads(run.stdout)['id'],4)
                else:
                    self.assertNotEqual(run.returncode,0)
                    self.assertNotIn(b'"result"',run.stdout)


if __name__ == '__main__':
    unittest.main()
