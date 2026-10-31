"""Bounded synthetic CLI regression; no credentials or model invocation."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from collections import Counter


def frame(rpc_id=2, meta=None, name='echo', arguments=None):
    return json.dumps({'jsonrpc':'2.0','id':rpc_id,'method':'tools/call','params':{'name':name,'arguments':arguments or {},'_meta':meta if meta is not None else {'claudecode/toolUseId':'toolu_test'}}})+'\n'


def validate(receipts, decision_bytes):
    """Test assertion only; no production semantic admission/authenticity."""
    events=[json.loads(b) for b in decision_bytes.splitlines(keepends=True)]
    assert Counter(r['decision_event_id'] for r in receipts)==Counter(e['id'] for e in events)
    assert len({e['id'] for e in events})==len(events)
    assert len({r['scope'] for r in receipts})==1
    assert [r['sequence'] for r in receipts]==list(range(len(receipts)))
    by_id={e['id']:b for e,b in zip(events,decision_bytes.splitlines(keepends=True))}
    for r in receipts:
        b=r['request_line'].encode()
        assert len(b)==r['request_bytes']
        assert hashlib.sha256(b).hexdigest()==r['request_sha256']
        assert hashlib.sha256(by_id[r['decision_event_id']]).hexdigest()==r['decision_sha256']
        req=json.loads(b);event=json.loads(by_id[r['decision_event_id']])
        assert req['method']=='tools/call'
        assert req['params']['name']==event['data']['tool']
        assert ('id' in req)==r['rpc_id_present'] and req.get('id')==r['rpc_id']
        assert req.get('id')==event['data'].get('request_id')
        assert type(req.get('id')) is type(r['rpc_id'])
        assert type(req.get('id')) is type(event['data'].get('request_id'))
        meta=req['params'].get('_meta',{})
        declared=meta.get('io.modelcontextprotocol/protocolVersion') if isinstance(meta,dict) else None
        assert r['declared_protocol_version']==declared
        assert r['declared_protocol_version_state']==('present' if declared else 'absent' if isinstance(meta,dict) else 'malformed')
        assert r['assertion_basis']=='caller_asserted' and not {'forwarded','delivered','executed'}.intersection(r)


class RequestReceiptTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
        (self.root/'policy.yaml').write_text('version: "2.0"\nname: receipt-test\ntools:\n  allow: [echo]\n')
        (self.root/'server.py').write_text("import sys,json,pathlib\nfor line in sys.stdin.buffer:\n pathlib.Path('forwarded').open('ab').write(line)\n obj=json.loads(line)\n print(json.dumps({'jsonrpc':'2.0','id':obj.get('id'),'result':{'content':[]}}),flush=True)\n")

    def run_wrap(self, lines, enabled=True, extra=()):
        args=[os.environ['ASSAY_BIN'],'mcp','wrap','--policy',str(self.root/'policy.yaml'),'--decision-log',str(self.root/'decisions'),'--event-source','assay://test/receipt']
        if enabled:args+=['--request-receipts-out',str(self.root/'receipts')]
        args+=list(extra)+['--',sys.executable,'-I',str(self.root/'server.py')]
        # Binary stdin avoids platform newline translation of the bytes under test.
        result=subprocess.run(args,input=lines.encode(),capture_output=True,cwd=self.root,timeout=20)
        result.stdout=result.stdout.decode();result.stderr=result.stderr.decode()
        self.assertEqual(result.returncode,0,result.stderr)
        decisions=(self.root/'decisions').read_bytes()
        receipts=[json.loads(l) for l in (self.root/'receipts').read_bytes().splitlines()] if enabled else []
        return result,decisions,receipts

    def test_original_request_binds_actual_decision_without_rewriting_id(self):
        line=frame();_,decisions,receipts=self.run_wrap(line)
        self.assertEqual((self.root/'forwarded').read_bytes(),line.encode())
        self.assertEqual(json.loads(decisions)['data']['tool_call_id'],'req_2')
        validate(receipts,decisions)
        self.assertEqual(receipts[0]['request_line'],line)
        if os.name=='posix':self.assertEqual((self.root/'receipts').stat().st_mode&0o777,0o600)

    def test_default_off(self):
        line=frame();_,decisions,receipts=self.run_wrap(line,False)
        self.assertFalse((self.root/'receipts').exists());self.assertEqual(receipts,[])
        self.assertEqual((self.root/'forwarded').read_bytes(),line.encode())
        self.assertEqual(json.loads(decisions)['data']['tool_call_id'],'req_2')

    def test_typed_ids_and_duplicate_native_ids_preserve_occurrences(self):
        lines=frame(2)+frame('2')+frame(2)+frame(3,arguments={'_meta':{'tool_call_id':'explicit'}})
        _,decisions,receipts=self.run_wrap(lines);validate(receipts,decisions)
        self.assertEqual([r['rpc_id'] for r in receipts],[2,'2',2,3])
        self.assertEqual([json.loads(l)['data']['tool_call_id'] for l in decisions.splitlines()],['req_2','req_2','req_2','explicit'])
        self.assertEqual((self.root/'forwarded').read_bytes(),lines.encode())

    def test_missing_null_and_wrong_metadata_are_not_rewritten(self):
        missing=json.loads(frame());del missing['id']
        lines=json.dumps(missing)+'\n'+frame(None)+frame(4,meta={'claudecode/toolUseId':False})
        _,decisions,receipts=self.run_wrap(lines);validate(receipts,decisions)
        self.assertEqual([r['rpc_id_present'] for r in receipts],[False,True,True])
        events=[json.loads(l) for l in decisions.splitlines()]
        self.assertTrue(events[0]['data']['tool_call_id'].startswith('gen_'))
        self.assertTrue(events[1]['data']['tool_call_id'].startswith('gen_'))
        self.assertEqual(events[2]['data']['tool_call_id'],'req_4')
        self.assertEqual((self.root/'forwarded').read_bytes(),lines.encode())

    def test_missing_empty_and_wrong_typed_metadata_are_retained(self):
        missing=json.loads(frame(1));del missing['params']['_meta']
        lines=json.dumps(missing)+'\n'+frame(2,meta={})+frame(3,meta='wrong')
        _,decisions,receipts=self.run_wrap(lines);validate(receipts,decisions)
        self.assertEqual(''.join(r['request_line'] for r in receipts),lines)
        self.assertEqual([json.loads(l)['data']['tool_call_id'] for l in decisions.splitlines()],['req_1','req_2','req_3'])

    def test_deny_is_recorded_but_not_forwarded(self):
        _,decisions,receipts=self.run_wrap(frame(name='blocked'));validate(receipts,decisions)
        self.assertEqual(json.loads(decisions)['data']['decision'],'deny')
        self.assertFalse((self.root/'forwarded').exists())

    def test_dry_run_preserves_allow_and_forwarding(self):
        line=frame(name='blocked');_,decisions,receipts=self.run_wrap(line,extra=['--dry-run']);validate(receipts,decisions)
        self.assertEqual(json.loads(decisions)['data']['decision'],'allow')
        self.assertEqual((self.root/'forwarded').read_bytes(),line.encode())

    def test_invalid_duplicate_and_non_tool_frames_are_not_captured(self):
        control='{"jsonrpc":"2.0","id":9,"method":"ping"}\n'
        invalid='{"jsonrpc":"2.0","id":1,"id":2,"method":"tools/call"}\n'
        _,decisions,receipts=self.run_wrap(invalid+control)
        self.assertEqual(decisions,b'');self.assertEqual(receipts,[])
        self.assertEqual((self.root/'forwarded').read_bytes(),control.encode())

    def test_frame_limit_stops_capture_without_changing_forwarding(self):
        # Existing parser trims trailing whitespace; capture counts every original byte.
        line=frame().rstrip('\n')+' '*(1024*1024)+'\n'
        result,decisions,receipts=self.run_wrap(line+frame(3))
        self.assertIn('request receipt capture incomplete',result.stderr)
        self.assertEqual(receipts,[]);self.assertEqual(len(decisions.splitlines()),2)
        self.assertEqual((self.root/'forwarded').read_bytes(),(line+frame(3)).encode())

    def test_count_limit_keeps_prefix_and_policy(self):
        lines=''.join(frame(i) for i in range(49));result,decisions,receipts=self.run_wrap(lines)
        self.assertEqual(len(receipts),48);self.assertEqual(len(decisions.splitlines()),49)
        self.assertIn('request receipt capture incomplete',result.stderr)
        self.assertEqual((self.root/'forwarded').read_bytes(),lines.encode())
        with self.assertRaises(AssertionError):validate(receipts,decisions)

    def test_consumer_assertions_discriminate_corruption_and_reference_swaps(self):
        _,decisions,receipts=self.run_wrap(frame(2)+frame(3));validate(receipts,decisions)
        mutations=[]
        r=json.loads(json.dumps(receipts));r[0]['request_line']=r[0]['request_line'].replace('echo','xxxx');mutations.append(r)
        r=json.loads(json.dumps(receipts));r[0]['decision_event_id'],r[1]['decision_event_id']=r[1]['decision_event_id'],r[0]['decision_event_id'];mutations.append(r)
        mutations += [receipts[:1],receipts+[receipts[0]],list(reversed(receipts))]
        r=json.loads(json.dumps(receipts));r[1]['scope']='foreign';mutations.append(r)
        for r in mutations:
            with self.assertRaises(AssertionError):validate(r,decisions)

    def test_unicode_escapes_and_request_boundary(self):
        # ID reaches the decision serializer; metadata remains only in original bytes.
        line=frame('雪\\"',meta={'text':'雪\\"'})
        _,decisions,receipts=self.run_wrap(line)
        validate(receipts,decisions)
        self.assertEqual((self.root/'forwarded').read_bytes(),line.encode())

    def test_exact_request_byte_boundary(self):
        blank=frame(2,meta={'padding':''})
        line=frame(2,meta={'padding':'x'*(1024*1024-len(blank.encode()))})
        self.assertEqual(len(line.encode()),1024*1024)
        result,decisions,receipts=self.run_wrap(line+line[:-1]+' \n'+frame(3))
        self.assertEqual(len(receipts),1)
        self.assertEqual(receipts[0]['request_bytes'],1024*1024)
        self.assertEqual(len(decisions.splitlines()),3)
        self.assertIn('capture incomplete',result.stderr)

    def test_default_and_enabled_decisions_have_same_content(self):
        line=frame(2)+frame(3,name='blocked')
        off,decisions,_=self.run_wrap(line,False)
        (self.root/'decisions').unlink()
        (self.root/'forwarded').unlink()
        on,enabled,receipts=self.run_wrap(line)
        validate(receipts,enabled)
        def stable(raw):
            events=[json.loads(l) for l in raw.splitlines()]
            for event in events:
                del event['id'];del event['time']
            return events
        self.assertEqual(stable(decisions),stable(enabled))
        self.assertEqual(off.stdout,on.stdout)
        self.assertEqual(off.returncode,on.returncode)

    def test_context_does_not_leak_across_non_tool_or_deny(self):
        ping='{"jsonrpc":"2.0","id":8,"method":"ping"}\n'
        inputs=[frame(2),frame(3,name='blocked'),frame(4)]
        _,decisions,receipts=self.run_wrap(inputs[0]+ping+inputs[1]+inputs[2])
        validate(receipts,decisions)
        self.assertEqual([r['request_line'] for r in receipts],inputs)
        self.assertEqual((self.root/'forwarded').read_bytes(),(inputs[0]+ping+inputs[2]).encode())

    def test_total_byte_limit_is_not_a_policy_limit(self):
        lines=''.join(frame(i,meta={'padding':'x'*800000}) for i in range(6))
        result,decisions,receipts=self.run_wrap(lines)
        self.assertEqual(len(receipts),5)
        self.assertEqual(len(decisions.splitlines()),6)
        self.assertLessEqual((self.root/'receipts').stat().st_size,4*1024*1024)
        self.assertIn('capture incomplete',result.stderr)
        self.assertEqual((self.root/'forwarded').read_bytes(),lines.encode())
        with self.assertRaises(AssertionError):validate(receipts,decisions)

    def test_output_alias_is_rejected_before_spawn(self):
        args=[os.environ['ASSAY_BIN'],'mcp','wrap','--policy',str(self.root/'policy.yaml'),
              '--decision-log',str(self.root/'receipts'),'--event-source','assay://test/receipt',
              '--request-receipts-out',str(self.root/'receipts'),'--',sys.executable,str(self.root/'server.py')]
        result=subprocess.run(args,input=frame(),text=True,capture_output=True,cwd=self.root,timeout=20)
        self.assertNotEqual(result.returncode,0)
        self.assertIn("cannot reserve --request-receipts-out output",result.stderr)
        self.assertFalse((self.root/'forwarded').exists())
        self.assertFalse((self.root/'receipts').exists())

    @unittest.skipUnless(sys.platform=='linux','Linux RLIMIT_FSIZE partial-write control')
    def test_partial_write_is_retained_and_not_reported_complete(self):
        import resource, signal
        # Keep downstream output small; only the receipt crosses this file-size limit.
        (self.root/'server.py').write_text("import sys,json\nfor line in sys.stdin: print(json.dumps({'jsonrpc':'2.0','id':json.loads(line)['id'],'result':{}}),flush=True)\n")
        def limit_file():
            resource.setrlimit(resource.RLIMIT_FSIZE,(4096,4096))
            signal.signal(signal.SIGXFSZ,signal.SIG_IGN)
        args=[os.environ['ASSAY_BIN'],'mcp','wrap','--policy',str(self.root/'policy.yaml'),
              '--decision-log',str(self.root/'decisions'),'--event-source','assay://test/receipt',
              '--request-receipts-out',str(self.root/'receipts'),'--',sys.executable,str(self.root/'server.py')]
        result=subprocess.run(args,input=frame(meta={'padding':'x'*5000})+frame(3),text=True,
                              capture_output=True,cwd=self.root,timeout=20,preexec_fn=limit_file)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(len(result.stdout.splitlines()),2)
        self.assertEqual(len((self.root/'decisions').read_bytes().splitlines()),2)
        raw=(self.root/'receipts').read_bytes()
        self.assertEqual(len(raw),4096)
        self.assertIn('capture incomplete',result.stderr)
        with self.assertRaises(json.JSONDecodeError):json.loads(raw)

    def test_existing_destination_is_not_overwritten(self):
        (self.root/'receipts').write_bytes(b'preserve')
        with self.assertRaisesRegex(AssertionError,"cannot reserve --request-receipts-out output"):self.run_wrap(frame())
        self.assertEqual((self.root/'receipts').read_bytes(),b'preserve')
        self.assertFalse((self.root/'forwarded').exists())


if __name__=='__main__':unittest.main()
