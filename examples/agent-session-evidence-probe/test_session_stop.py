"""Receiver tests use only retained companions, never the fixture oracle."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import session_stop


class StopReceiverTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.raw = b'{"type":"assay.tool.decision","id":"decision-1"}\n'
        self.context = {'specversion':'1.0','id':'evt_context_one','type':'assay.mcp.session.context.v1','source':'assay://test','data':{'server_id':'s','enforcement':'enforce','server_binding_basis':'spawned_command','server_binding_digest':'sha256:'+'a'*64,'server_binding_digest_alg':'sha256','server_binding_canonicalization':'jcs:argv.v1'}}
        self.stop = {'specversion':'1.0','type':'assay.mcp.session.stop.v1','id':'evt_stop_one','source':'assay://test','data':{
            'server_id':'s','session_context_id':'evt_context_one','session_end':'client_closed',
            'termination':'exited','exit_code':0,'signal':None,'core_dumped':False,
            'server_pid':1,'assay_kill_requested':False,'decision_log':'complete',
            'decision_events_written':1,'decision_events_dropped':0,
            'decision_prefix_sha256':hashlib.sha256(self.raw).hexdigest(),
            'assertion_basis':'producer_observation'}}
        self.save()

    def save(self):
        (self.root/'session-context.json').write_text(json.dumps(self.context))
        (self.root/'session-stop.json').write_text(json.dumps(self.stop))
        (self.root/'decisions.ndjson').write_bytes(self.raw)

    def test_bound_record_is_closed_without_claiming_success(self):
        self.assertEqual(session_stop.verify(self.root)['status'], 'closed')
        self.stop['data']['exit_code'] = 7; self.save()
        self.assertEqual(session_stop.verify(self.root)['status'], 'closed')
        self.assertEqual(session_stop.verify(self.root)['exit_code'], 7)

    def test_missing_corrupt_or_stale_record_is_unknown(self):
        for mutation in ['missing','json','context','bytes','count','bool','signal','extra']:
            with self.subTest(mutation=mutation):
                self.setUp()
                if mutation == 'context': self.stop['data']['session_context_id']='evt_context_old'
                if mutation == 'count': self.stop['data']['decision_events_written']=2
                if mutation == 'bool': self.stop['data']['decision_events_written']=True
                if mutation == 'signal': self.stop['data']['signal']=15
                self.save()
                if mutation == 'missing': (self.root/'session-stop.json').unlink()
                if mutation == 'json': (self.root/'session-stop.json').write_text('{')
                if mutation == 'bytes': (self.root/'decisions.ndjson').write_bytes(self.raw.replace(b'decision-1',b'decision-2'))
                if mutation == 'extra': (self.root/'decisions.ndjson').write_bytes(self.raw+self.raw)
                self.assertEqual(session_stop.verify(self.root)['status'],'unknown')

    def test_malformed_stable_context_fields_are_unknown(self):
        import copy
        original = copy.deepcopy(self.context)
        for field, value in [('enforcement', {}), ('server_binding_basis', 'guessed'),
                             ('server_binding_digest', 'sha256:bad'),
                             ('server_binding_digest', 'sha256:'+'A'*64),
                             ('server_binding_digest_alg', 'md5'),
                             ('server_binding_canonicalization', 'unknown')]:
            with self.subTest(field=field, value=value):
                self.context = copy.deepcopy(original)
                self.context['data'][field] = value; self.save()
                self.assertEqual(session_stop.verify(self.root)['status'], 'unknown')
        self.context = copy.deepcopy(original); del self.context['specversion']; self.save()
        self.assertEqual(session_stop.verify(self.root)['status'], 'unknown')
        for field in original['data']:
            self.context = copy.deepcopy(original); del self.context['data'][field]; self.save()
            self.assertEqual(session_stop.verify(self.root)['status'], 'unknown')

    def test_known_loss_is_incomplete_even_with_valid_prefix(self):
        self.stop['data'].update(decision_log='incomplete', decision_events_dropped=1)
        self.save(); (self.root/'decisions.ndjson').write_bytes(self.raw+b'{"partial":')
        self.assertEqual(session_stop.verify(self.root)['status'],'incomplete')

    def test_hostile_json_and_symlink_are_unknown(self):
        for raw in [b'{"type":1,"type":2}', b'['*100+b']'*100, b'{"x":NaN}', b'x'*65537]:
            (self.root/'session-stop.json').write_bytes(raw)
            self.assertEqual(session_stop.verify(self.root)['status'],'unknown')
        (self.root/'session-stop.json').unlink()
        (self.root/'session-stop.json').symlink_to('session-context.json')
        self.assertEqual(session_stop.verify(self.root)['status'],'unknown')

if __name__ == '__main__': unittest.main()
