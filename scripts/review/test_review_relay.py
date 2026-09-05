"""Synthetic protocol tests only: gh is replaced; no real merge is possible."""
import json
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent
HEAD = 'b' * 40
URL = 'https://github.com/example/repo/pull/30#issuecomment-123'

GH = '''#!/usr/bin/env python3
import json, os, sys
args=sys.argv[1:]; case=os.environ['CASE']; head='b'*40
record={'schema':'assay.review-record.v0','head_sha':head,'review_completed':True,
 'builder':{'agent':'codex','instance':'writer'},
 'verdict':'READY','reviewer':{'agent':'claude','instance':'independent-123','github_login':'owner'},
 'independence':{'did_not_build':True,'did_not_author_governing_spec':True},
 'findings':[], 'no_findings':True}
if case=='wrong-head': record['head_sha']='a'*40
if case=='blocked': record['verdict']='BLOCKED'
if case=='incomplete': record['review_completed']=False
if case=='no-independence': record.pop('independence')
if case=='wrong-identity': record['reviewer']['instance']='other'
if case=='same-builder': record['reviewer']={'agent':'codex','instance':'writer','github_login':'owner'}
if case=='missing-builder': record.pop('builder')
body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(record)+'\\n```'
fetched_body=body
if case in ('fetched-blocked','fetched-stale','fetched-identity-mismatch','fetched-login-mismatch'):
 fetched=dict(record)
 if case=='fetched-blocked': fetched['verdict']='BLOCKED'
 if case=='fetched-stale': fetched['head_sha']='a'*40
 if case=='fetched-identity-mismatch': fetched['reviewer']=dict(record['reviewer'],instance='other')
 if case=='fetched-login-mismatch': fetched['reviewer']=dict(record['reviewer'],github_login='other')
 fetched_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(fetched)+'\\n```'
if case=='carrier-only': body='READY\\n'+head+'\\nRelay: '+os.environ['URL']
if args[:2]==['pr','view']:
 candidate_body=body
 if case=='candidate-blocked':
  candidate_record=dict(record); candidate_record['verdict']='BLOCKED'
  candidate_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(candidate_record)+'\\n```'
 if case=='candidate-stale':
  candidate_record=dict(record); candidate_record['head_sha']='a'*40
  candidate_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(candidate_record)+'\\n```'
 if case=='candidate-identity-mismatch':
  candidate_record=dict(record); candidate_record['reviewer']=dict(record['reviewer'],instance='other')
  candidate_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(candidate_record)+'\\n```'
 candidate_author='owner'
 if case=='candidate-author-mismatch':
  candidate_record=dict(record); candidate_record['reviewer']=dict(record['reviewer'],github_login='other')
  candidate_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(candidate_record)+'\\n```'
  candidate_author='other'
 comments=[{'author':{'login':candidate_author},'body':candidate_body}]
 if case=='candidate-stale':
  helper=dict(record); helper['reviewer']=dict(record['reviewer'],instance='helper',github_login='helper')
  helper_body='<!-- assay-review-record -->\\n```json\\n'+json.dumps(helper)+'\\n```'
  comments.append({'author':{'login':'helper'},'body':helper_body})
 print(json.dumps(dict(number=30,author={'login':'owner'},state='OPEN',isDraft=False,
 mergeable='MERGEABLE',headRefOid=head,headRefName='codex/review-fix',baseRefOid='a'*40,baseRefName='main',body=head,
 comments=comments)))
elif args[:2]==['pr','checks']:
 print(json.dumps([dict(name='CI',state='SUCCESS',bucket='pass')]))
elif args[:2]==['pr','merge']:
 pathlib_missing=None
 open(os.environ['MERGE_LOG'],'w').write(json.dumps(args))
elif args[:2]==['api','graphql']:
 print(json.dumps({'data':{'repository':{'ref':{'branchProtectionRule':{'requiredStatusCheckContexts':['CI']}}}}}))
elif '--slurp' in args: print('[[]]')
elif args[:2]==['api','repos/example/repo/issues/comments/123']:
 if case=='unavailable': sys.exit(1)
 print(json.dumps({'id':123,'html_url':os.environ['URL'],
 'issue_url':'https://api.github.com/repos/example/repo/issues/30',
 'user':{'login':'other' if case=='wrong-author' else 'owner'},'body':fetched_body}))
else: sys.exit('unexpected gh argv: '+repr(args))
'''

class RelayProtocol(unittest.TestCase):
    def test_relay_acceptance_and_fail_closed_matrix(self):
        for case in ('valid', 'wrong-head', 'blocked', 'incomplete',
                     'no-independence', 'wrong-identity', 'carrier-only',
                     'unavailable', 'wrong-author', 'missing-url', 'ambiguous',
                     'candidate-author-mismatch', 'candidate-blocked',
                     'candidate-stale', 'candidate-identity-mismatch',
                     'same-builder', 'missing-builder', 'wrong-repo-url',
                     'url-query', 'no-confirm', 'fetched-blocked',
                     'fetched-stale', 'fetched-identity-mismatch',
                     'fetched-login-mismatch'):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                gh = root / 'gh'
                gh.write_text(GH)
                gh.chmod(0o755)
                log = root / 'merge.json'
                url = URL
                if case == 'wrong-repo-url': url = 'https://github.com/other/repo/pull/30#issuecomment-123'
                if case == 'url-query': url = URL.replace('#', '?x=1#')
                env = dict(os.environ, PATH=str(root)+os.pathsep+os.environ['PATH'],
                           CASE=case, MERGE_LOG=str(log), URL=url)
                cmd = ['bash', str(ROOT/'safe_merge.sh'), '30', '--repo', 'example/repo',
                       '--record-author', 'owner', '--reviewer-identity',
                       'owner' if case=='ambiguous' else 'claude/independent-123',
                       '--merge']
                if case != 'no-confirm': cmd.insert(-1, '--confirm-findings-disposed')
                if case != 'missing-url': cmd += ['--review-evidence-url', url]
                result = subprocess.run(cmd, env=env, text=True, capture_output=True, timeout=15)
                self.assertEqual(result.returncode == 0, case == 'valid', result.stderr)
                self.assertEqual(log.exists(), case == 'valid', result.stdout)
                if case == 'valid':
                    self.assertIn('Record author: owner', result.stdout)
                    self.assertIn('Reviewing identity: claude/independent-123', result.stdout)
                    self.assertIn('declares', result.stdout)
                    self.assertNotIn('attests reviewer owner is independent', result.stdout)
                    self.assertEqual(json.loads(log.read_text())[-2:], ['--match-head-commit', HEAD])

if __name__ == '__main__': unittest.main()
