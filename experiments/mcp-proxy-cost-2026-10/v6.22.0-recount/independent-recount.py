import pathlib,json,statistics,random,hashlib,tarfile
P=pathlib.Path(__file__).parent; R=P/'independent-recount'
def load(p):return json.loads(p.read_text())
def lines(p):return [json.loads(x) for x in p.read_text().splitlines() if x.strip()]
assert hashlib.sha256((R/'source.tar').read_bytes()).hexdigest()=='c9addcd5e93cedf809074373072cffcb198ce669de44a16773e52bf54a138518'
reports=[]
for attempt in range(1,4):
 a=R/f'attempt-{attempt}';res=a/'result';s=load(res/'summary.json');bs=lines(res/'blocks.jsonl');before=load(a/'before.json');after=load(a/'after.json')
 assert before['source_hashes']==after['source_hashes'] and before['binary_sha256']==after['binary_sha256']==s['provenance']['mcp_binary_sha256']
 with tarfile.open(R/'source.tar') as t:
  for name,h in before['source_hashes'].items():assert hashlib.sha256(t.extractfile(name).read()).hexdigest()==h
 assert load(a/'execution.json')['returncode']==0 and s['execution_valid'] and len(bs)==20
 for i,b in enumerate(bs):
  assert b['block_index']==i and b['order']==('AB' if i%2==0 else 'BA')
  wires=[]
  for arm in ['direct_stdio','proxy_enforce_allow']:
   v=b[arm];d=res/'sessions'/f"block_{i:02d}_{b['order']}"/f"{v['order_in_block']}_{arm}";req=lines(d/'caller_requests.ndjson');up=lines(d/'upstream_raw.jsonl');resp=lines(d/'caller_responses.ndjson')
   assert sum(x.get('method')=='tools/call' for x in req)==sum(x.get('method')=='tools/call' for x in up)==20
   assert len(resp)==22 and all('error' not in x for x in resp)
   assert len(v['in_session_call_rtts_ms'])==20 and abs(statistics.median(v['in_session_call_rtts_ms'])-v['in_session_call_median_ms'])<=0.000001
   assert abs(sum(v['in_session_call_rtts_ms'])-v['in_session_call_total_ms'])<=0.00002
   for name,key in [('caller_requests.ndjson','caller_request_bytes'),('caller_responses.ndjson','caller_response_bytes'),('child_stderr.log','child_stderr_bytes'),('upstream_raw.jsonl','upstream_fixture_normalized_bytes')]:assert (d/name).stat().st_size==v[key]
   dec=d/'decisions.ndjson';assert (dec.stat().st_size if dec.exists() else 0)==v['evidence_bytes'];assert (len(lines(dec)) if dec.exists() else 0)==v['decision_evidence_records']==(20 if arm=='proxy_enforce_allow' else 0)
   assert v['exit_code']==0;wires.append((d/'caller_requests.ndjson').read_bytes())
  assert wires[0]==wires[1]
 for metric,m in s['paired_block_metrics'].items():
  key='evidence_bytes' if metric=='evidence_bytes_per_session' else metric
  deltas=[round(b['proxy_enforce_allow'][key]-b['direct_stdio'][key],6) for b in bs];assert deltas==m['block_deltas']
  rng=random.Random(m['bootstrap_seed']);boot=sorted(statistics.median(rng.choices(deltas,k=20)) for _ in range(2000))
  assert round(statistics.median(deltas),6)==m['median_paired_delta'];assert [round(boot[50],6),round(boot[1949],6)]==m['bootstrap_95_ci']
 deny=res/'sessions/deny_check';assert sum(x.get('method')=='tools/call' for x in lines(deny/'upstream_raw.jsonl'))==0
 assert any(x.get('method')=='initialize' for x in lines(deny/'upstream_raw.jsonl'));assert len(lines(deny/'decisions.ndjson'))==1
 assert load(res/'deny_check.json')['status']=='passed'
 reports.append({'attempt':attempt,'metrics':{k:{'median':v['median_paired_delta'],'ci95':v['bootstrap_95_ci']} for k,v in s['paired_block_metrics'].items()}})
(P/'independent-recount.json').write_text(json.dumps({'runs':reports,'session_count':120,'tool_calls':2400,'deny_controls':3},indent=2)+'\n');print(json.dumps(reports,indent=2))
