import json, os, pathlib, shutil, subprocess, tempfile
source = pathlib.Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='safe-merge-protocol-') as d:
    root = pathlib.Path(d)
    review = root / 'scripts' / 'review'
    ci = root / 'scripts' / 'ci'
    review.mkdir(parents=True)
    ci.mkdir(parents=True)
    for name in ('safe_merge.sh', 'pr_landing_readiness.py', 'verify_review_identity.py'):
        shutil.copy2(source / name, review / name)
    shutil.copy2(source.parent / 'ci' / 'assay_review_record_check.py',
                 ci / 'assay_review_record_check.py')
    gh = root / 'gh'
    gh.write_text('''#!/usr/bin/env python3
import json,os,sys
args=sys.argv[1:]
head='b'*40
case=os.environ['CASE']
if args[:2]==['pr','view']:
 record=dict(schema='assay.review-record.v0',head_sha='a'*40 if case=='stale' else head,review_completed=True,verdict='BLOCKED' if case=='blocked' else 'READY',builder=dict(agent='codex',instance='writer'),reviewer=dict(agent='claude',instance='other' if case=='identity-mismatch' else 'reviewer',github_login='reviewer'),independence=dict(did_not_build=True,did_not_author_governing_spec=True),findings=[],no_findings=True)
 body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(record)+'\\n```'
 print(json.dumps(dict(number=30,author=dict(login='owner'),state='OPEN',isDraft=False,mergeable='MERGEABLE',headRefOid=head,headRefName='codex/review-fix',baseRefOid='a'*40,baseRefName='main',body=head,comments=[] if case=='no-review' else [dict(author=dict(login='reviewer'),body=body)])))
elif args[:2]==['pr','checks']:
 print(json.dumps([dict(name='reproduce',state='SKIPPED' if case=='skipped' else 'SUCCESS',bucket='pass')]))
elif args[:2]==['pr','merge']:
 open(os.environ['MERGE_LOG'],'w').write(json.dumps(args))
elif args[:2]==['api','repos/example/repo/issues/comments/123']:
 record=dict(schema='assay.review-record.v0',head_sha=head,review_completed=True,verdict='READY',builder=dict(agent='codex',instance='writer'),reviewer=dict(agent='claude',instance='reviewer',github_login='reviewer'),independence=dict(did_not_build=True,did_not_author_governing_spec=True),findings=[],no_findings=True)
 print(json.dumps(dict(html_url='https://github.com/example/repo/pull/30#issuecomment-123',issue_url='https://api.github.com/repos/example/repo/issues/30',user=dict(login='reviewer'),body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(record)+'\\n```')))
elif args[:2]==['api','graphql']:
 print(json.dumps(dict(data=dict(repository=dict(ref=dict(branchProtectionRule=None if case=='ruleset' else dict(requiredStatusCheckContexts=['reproduce'])))))))
elif '--slurp' in args:
 print(json.dumps([[dict(type='required_status_checks',parameters=dict(required_status_checks=[dict(context='reproduce')]))]] if case=='ruleset' else [[]]))
elif args[-1].endswith('/protection/required_status_checks'):
 print(json.dumps(dict(contexts=['reproduce'])))
elif '/rules/branches/' in args[-1]: print('[]')
elif args[-1].endswith('/branches/main'): print('{"protected":false}')
else: raise SystemExit('unexpected gh arguments: '+repr(args))
''')
    gh.chmod(0o755)
    for case, explicit in [('protected',False),('ruleset',False),('unprotected',True),('no-review',True),('skipped',True),('blocked',True),('stale',True),('identity-mismatch',True)]:
        log=root/'merge.json'
        log.unlink(missing_ok=True)
        env=dict(os.environ, PATH=str(root)+':'+os.environ['PATH'], CASE=case, MERGE_LOG=str(log))
        args=['/bin/bash', str(review/'safe_merge.sh'), '30','--repo','example/repo','--record-author','reviewer','--reviewer-identity','claude/reviewer','--review-evidence-url','https://github.com/example/repo/pull/30#issuecomment-123','--confirm-findings-disposed','--merge']
        if explicit: args += ['--unprotected-require-check','reproduce']
        p=subprocess.run(args,env=env,text=True,capture_output=True)
        expected=case in ('protected','ruleset','unprotected')
        assert (p.returncode==0)==expected,(case,p.stderr)
        assert log.exists()==expected,case
        if expected:
            command=json.loads(log.read_text())
            assert command[-2:]==['--match-head-commit','b'*40],command
            assert '--admin' not in command
        print(case, p.returncode, 'merge-called='+str(log.exists()))
