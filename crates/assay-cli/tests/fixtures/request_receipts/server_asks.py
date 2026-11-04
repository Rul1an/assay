
import sys, json
MODE = sys.argv[1]
# Optional test-only witness in the harness-owned working directory.
# Accept a mode switch, never an operator-selected output path.
if sys.argv[2:] not in ([], ["--record-seen"]):
    raise SystemExit("expected optional --record-seen")
seen = open("seen", "w", encoding="utf-8", newline="") if sys.argv[2:] else None
def out(o):
    print(json.dumps(o, separators=(',', ':')), flush=True)
pending = None
calls = []
for line in sys.stdin:
    if seen is not None:
        seen.write(line)
        seen.flush()
    r = json.loads(line)
    if 'method' not in r:
        if MODE == 'legacy_ask' and r.get('id') == 'ask-1':
            out({'jsonrpc':'2.0','id':pending,'result':{'content':[{'type':'text','text':r['result']['action']}]}})
        continue
    k = r['method']
    i = r.get('id')
    if k == 'initialize':
        ver = '2026-07-28' if MODE.startswith('modern') else '2025-11-25'
        out({'jsonrpc':'2.0','id':i,'result':{'protocolVersion':ver,'capabilities':{'tools':{}},'serverInfo':{'name':'s','version':'1'}}})
    elif MODE == 'tasks' and k in ('tasks/get', 'tasks/result'):
        out({'jsonrpc':'2.0','id':i,'result':{'taskId':'task-1','status':'completed'}})
    elif k == 'tools/call':
        if MODE == 'legacy_ask':
            pending = i
            out({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'Delete repository prod?','requestedSchema':{'type':'object','properties':{}}}})
        elif MODE == 'modern_ask':
            if r.get('params', {}).get('inputResponses'):
                out({'jsonrpc':'2.0','id':i,'result':{'resultType':'complete','content':[{'type':'text','text':'done'}]}})
            else:
                out({'jsonrpc':'2.0','id':i,'result':{'resultType':'input_required','inputRequests':{'c':{'method':'elicitation/create','params':{'message':'Sure?','requestedSchema':{'type':'object','properties':{}}}}},'requestState':'st1'}})
        elif MODE == 'malformed_legacy':
            print('{"jsonrpc":"2.0","id":"ask-1","method":"elicitation/create","params":[]}', flush=True)
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
        elif MODE == 'malformed_modern':
            out({'jsonrpc':'2.0','id':i,'result':{'resultType':'input_required','inputRequests':['not-an-object'],'requestState':'st1'}})
        elif MODE == 'two_pending':
            calls.append(i)
            if len(calls) == 2:
                out({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'Two pending?','requestedSchema':{'type':'object','properties':{}}}})
        elif MODE == 'sampling_after':
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
            out({'jsonrpc':'2.0','id':'samp-1','method':'sampling/createMessage','params':{'messages':[{'role':'user','content':{'type':'text','text':'unlinked sampling'}}]}})
        elif MODE == 'oversize_ask':
            print(json.dumps({'jsonrpc':'2.0','id':'ask-1','method':'elicitation/create','params':{'message':'x'*1048576}}, separators=(',', ':')), flush=True)
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
        else:
            out({'jsonrpc':'2.0','id':i,'result':{'content':[]}})
    elif i is not None:
        out({'jsonrpc':'2.0','id':i,'result':{}})
