#!/usr/bin/env python3
"""Collect release-produced data and open one repository-scoped install-pin PR."""
from __future__ import annotations

import argparse
import base64
from datetime import datetime
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('promotion', ROOT / 'scripts/ci/promote-release-pin.py')
promotion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(promotion)
REPO = 'Rul1an/assay'
WORKFLOW = '.github/workflows/release.yml'
IMAGE = 'ghcr.io/rul1an/assay-mcp-server'
LIMIT = 1048576
MARKER = '<!-- assay-release-pin:v1 '


def require(ok, message):
    if not ok:
        raise ValueError(message)


def integer(value):
    require(type(value) is int and value > 0, 'expected a positive integer')
    return value


def sha(value):
    require(isinstance(value, str) and re.fullmatch('[0-9a-f]{40}', value), 'invalid commit SHA')
    return value


def decode(raw):
    require(len(raw) <= LIMIT, 'metadata exceeds 1 MiB')
    # Share duplicate-key and byte-limit behavior with the published-pin checker.
    with tempfile.NamedTemporaryFile() as stream:
        stream.write(raw)
        stream.flush()
        return promotion.load_metadata(stream.name)


class Redirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        require(newurl.startswith('https://'), 'non-HTTPS artifact redirect')
        redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
        redirected.remove_header('Authorization')
        return redirected


class API:
    def request(self, path, data=None, method='GET', binary=False, absent=False):
        require(path.startswith('/repos/' + REPO + '/'), 'foreign API repository')
        token = os.environ.get('GH_READ_TOKEN' if method == 'GET' else 'GH_TOKEN', '')
        require(bool(token), 'required workflow token is absent')
        request = urllib.request.Request('https://api.github.com' + path,
            data=None if data is None else json.dumps(data).encode(), method=method,
            headers={'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json',
                     'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'assay-release-pin'})
        try:
            with urllib.request.build_opener(Redirect()).open(request, timeout=30) as response:
                raw = response.read(LIMIT + 1)
        except urllib.error.HTTPError as error:
            if absent and error.code == 404:
                return None
            raise ValueError(f'GitHub API refused with HTTP {error.code}') from None
        require(len(raw) <= LIMIT, 'API response exceeds 1 MiB')
        return raw if binary else decode(raw)

    def get(self, suffix, **kwargs):
        return self.request('/repos/' + REPO + '/' + suffix, **kwargs)

    def pages(self, suffix, key=None):
        result = []
        for page in range(1, 6):
            separator = '&' if '?' in suffix else '?'
            value = self.get(f'{suffix}{separator}per_page=100&page={page}')
            items = value[key] if key else value
            require(isinstance(items, list) and len(items) <= 100, 'invalid page')
            result.extend(items)
            if len(items) < 100:
                if key:
                    require(value.get('total_count') == len(result), 'incomplete paginated response')
                return result
        raise ValueError('pagination exceeds 500 entries')


def producer_block(text):
    blocks = re.findall(r'(?ms)^  publish-image:\n.*?(?=^  [A-Za-z][\w-]*:|\Z)', text)
    require(len(blocks) == 1, 'unique image producer required')
    return blocks[0]


def binding(archive):
    require(len(archive) <= LIMIT, 'artifact archive exceeds 1 MiB')
    with zipfile.ZipFile(io.BytesIO(archive)) as z:
        entries = z.infolist()
        require(len(entries) == 1, 'artifact must contain exactly one file')
        entry = entries[0]
        mode = entry.external_attr >> 16
        require(entry.orig_filename == entry.filename == 'release-pin-binding.json' and not entry.is_dir()
                and stat.S_IFMT(mode) in (0, stat.S_IFREG) and entry.file_size <= LIMIT
                and entry.compress_size <= LIMIT and not entry.flag_bits & 1,
                'invalid artifact member')
        with z.open(entry) as stream:
            return decode(stream.read(LIMIT + 1))


def tag_commit(api, tag):
    obj = api.get('git/ref/tags/' + tag)['object']
    for _ in range(5):
        if obj.get('type') == 'commit':
            return sha(obj['sha'])
        require(obj.get('type') == 'tag', 'unsupported tag target')
        obj = api.get('git/tags/' + sha(obj['sha']))['object']
    raise ValueError('annotated tag chain too deep')


def collect(api, run_id, root=ROOT):
    integer(run_id)
    run = api.get(f'actions/runs/{run_id}')
    require(run.get('id') == run_id and run.get('repository', {}).get('full_name') == REPO
            and run.get('event') == 'push' and run.get('status') == 'completed'
            and run.get('path') == WORKFLOW, 'unsupported release run')
    workflow = api.get('actions/workflows/release.yml')
    require(run.get('workflow_id') == workflow.get('id') and workflow.get('path') == WORKFLOW,
            'release workflow identity differs')
    attempt = integer(run.get('run_attempt'))
    head = sha(run.get('head_sha'))
    release = api.get('releases/latest')
    tag = promotion.published_tag(release)
    require(run.get('head_branch') == tag and tag_commit(api, tag) == head,
            'producer tag is not the latest release at its source SHA')
    jobs = api.pages(f'actions/runs/{run_id}/attempts/{attempt}/jobs', 'jobs')
    promotion._require_single_successful_job(jobs, ('Publish GHCR image',), {
        'run_id': run_id, 'run_attempt': attempt, 'head_sha': head,
        'status': 'completed', 'conclusion': 'success'}, 'Publish GHCR image')
    source = api.get(f'contents/{WORKFLOW}?ref={head}')
    require(source.get('encoding') == 'base64', 'unsupported workflow encoding')
    # Compare the entire trusted workflow: another job must not replace the artifact.
    require(base64.b64decode(source['content']).decode() == (root / WORKFLOW).read_text(),
            'producer source contract differs')
    artifacts = api.pages(f'actions/runs/{run_id}/artifacts', 'artifacts')
    name = f'assay-release-pin-binding-{run_id}-{attempt}'
    found = [a for a in artifacts if a.get('name') == name]
    require(len(found) == 1, 'required producer artifact missing or ambiguous')
    artifact = found[0]
    require(artifact.get('expired') is False and type(artifact.get('size_in_bytes')) is int
            and 0 < artifact['size_in_bytes'] <= LIMIT
            and artifact.get('workflow_run', {}).get('id') == run_id
            and artifact['workflow_run'].get('head_sha') == head, 'invalid artifact association or size')
    archive = api.get(f"actions/artifacts/{integer(artifact.get('id'))}/zip", binary=True)
    require(artifact.get('digest') == 'sha256:' + hashlib.sha256(archive).hexdigest(),
            'artifact digest differs')
    data = binding(archive)
    expected = {'schema': 'assay.release-pin-binding.v1', 'repository': REPO,
                'workflow_path': WORKFLOW, 'run_id': run_id, 'run_attempt': attempt,
                'head_sha': head, 'tag': tag, 'image': IMAGE}
    require(set(data) == set(expected) | {'digest'} and all(type(data.get(k)) is type(v)
            and data.get(k) == v for k, v in expected.items()), 'producer binding differs')
    metadata = {'release': release, 'run': run, 'jobs': {'total_count': len(jobs), 'jobs': jobs},
                'image_binding': {k: data[k] for k in ('run_id', 'head_sha', 'tag', 'digest')}}
    promotion.identity(metadata)  # one shared tag/run/verifier/image consistency rule
    published = release.get('published_at')
    require(isinstance(published, str) and re.fullmatch(r'\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z', published),
            'invalid publication timestamp')
    datetime.strptime(published, '%Y-%m-%dT%H:%M:%SZ')
    base = sha(api.get('git/ref/heads/main')['object']['sha'])
    asset_fields = ('id', 'name', 'size', 'digest', 'updated_at')
    fresh = {'base': base, 'tag': tag, 'head': head, 'run': run_id, 'attempt': attempt,
             'release': integer(release.get('id')), 'updated_at': release.get('updated_at'),
             'published_at': published,
             'assets': sorted([{k: a.get(k) for k in asset_fields} for a in release['assets']], key=lambda a: a['name']),
             'artifact': artifact['id'], 'artifact_digest': artifact['digest'], 'binding': data}
    return metadata, fresh


def prepare(api, run_id, root=ROOT):
    metadata, fresh = collect(api, run_id, root)
    require(promotion.run(['git', 'rev-parse', 'HEAD'], root).strip() == fresh['base'],
            'trusted checkout is not current main')
    require(not promotion.run(['git', 'status', '--porcelain'], root).strip(), 'checkout is dirty')
    _, changes = promotion.plan(root, metadata)
    return {'fresh': fresh, 'changes': {name: data.decode() for name, data in sorted(changes.items())}}


def validate_marker(value, tag, current):
    fields = {'schema', 'base', 'tag', 'run', 'attempt', 'release', 'head', 'artifact',
              'artifact_digest', 'image_digest', 'commit'}
    require(isinstance(value, dict) and set(value) == fields
            and value['schema'] == 'assay.release-pin-pr.v1', 'PR marker schema differs')
    for field in ('base', 'head', 'commit'):
        sha(value[field])
    for field in ('run', 'attempt', 'release', 'artifact'):
        integer(value[field])
    for field in ('artifact_digest', 'image_digest'):
        require(isinstance(value[field], str) and re.fullmatch(r'sha256:[0-9a-f]{64}', value[field]),
                'invalid PR marker digest')
    require(value['commit'] == current and value['tag'] == tag,
            'PR head or tag changed outside recorded promotion')
    return value


def marker(plan, commit):
    value = {**{k: plan['fresh'][k] for k in (
        'base', 'tag', 'run', 'attempt', 'release', 'head', 'artifact', 'artifact_digest')},
        'schema': 'assay.release-pin-pr.v1', 'image_digest': plan['fresh']['binding']['digest'],
        'commit': commit}
    return validate_marker(value, plan['fresh']['tag'], commit)


def owned_pr(pr, branch, slug, current):
    require(pr.get('user', {}).get('login') == slug + '[bot]' and pr['user'].get('type') == 'Bot'
            and pr.get('head', {}).get('repo', {}).get('full_name') == REPO
            and pr['head'].get('ref') == branch and pr['head'].get('sha') == current
            and pr.get('base', {}).get('repo', {}).get('full_name') == REPO
            and pr['base'].get('ref') == 'main',
            'foreign PR ownership')
    lines = [line for line in (pr.get('body') or '').splitlines() if line.startswith(MARKER)]
    require(len(lines) == 1 and lines[0].endswith(' -->'), 'PR ownership marker missing')
    value = decode(lines[0][len(MARKER):-4].encode())
    validate_marker(value, branch.removeprefix('codex/release-pin-'), current)
    require(pr.get('state') == 'open', 'closed PR requires coordinator recovery')


def git(args, root, extra=None):
    env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
    env.update(extra or {})
    result = subprocess.run(['git', *args], cwd=root, env=env, capture_output=True, text=True, timeout=120)
    require(result.returncode == 0, 'git operation failed: ' + args[0])
    return result.stdout.strip()


def publish(api, run_id, saved, slug, expected_slug, root=ROOT):
    require(isinstance(slug, str) and re.fullmatch('[a-z0-9-]+', slug) and slug == expected_slug,
            'App identity differs')
    current = prepare(api, run_id, root)
    require(current == saved, 'release or main changed after preparation')
    if not saved['changes']:
        return 'no-op'
    require(set(saved['changes']) <= promotion.SURFACES, 'unexpected promotion path')
    tag = saved['fresh']['tag']
    branch = 'codex/release-pin-' + tag
    prs = api.pages('pulls?state=all&head=Rul1an:' + branch)
    require(len(prs) <= 1, 'ambiguous promotion PRs')
    remote = api.get('git/ref/heads/' + branch, absent=True)
    previous = '' if remote is None else sha(remote['object']['sha'])
    if prs:
        require(bool(previous), 'PR branch missing')
        owned_pr(prs[0], branch, slug, previous)
    original = promotion.snapshot(root)
    promotion.apply(root, original, {n: b.encode() for n, b in saved['changes'].items()})
    git(['add', '--', *saved['changes']], root)
    # Stable metadata permits recovery after branch creation but before PR creation.
    published = saved['fresh']['published_at']
    env = {'GIT_AUTHOR_DATE': published, 'GIT_COMMITTER_DATE': published}
    git(['-c', 'user.name=assay-release-pin', '-c', 'user.email=release-pin@users.noreply.github.com',
         'commit', '-m', f'docs: promote install pin to {tag}'], root, env)
    commit = sha(git(['rev-parse', 'HEAD'], root))
    if previous and not prs:
        require(previous == commit, 'unowned branch collision')
    require(collect(api, run_id, root)[1] == saved['fresh'], 'release or main changed before branch write')
    if previous != commit:
        git(['-c', 'credential.helper=!gh auth git-credential', 'push',
             f'--force-with-lease=refs/heads/{branch}:{previous}',
             'https://github.com/' + REPO + '.git', f'{commit}:refs/heads/{branch}'], root)
    require(collect(api, run_id, root)[1] == saved['fresh'], 'release or main changed before PR write')
    require(api.get('git/ref/heads/' + branch)['object']['sha'] == commit,
            'branch changed before PR write')
    latest_prs = api.pages('pulls?state=all&head=Rul1an:' + branch)
    require(len(latest_prs) == len(prs), 'PR set changed before write')
    if prs:
        latest = latest_prs[0]
        require(latest.get('number') == prs[0].get('number') and latest.get('body') == prs[0].get('body')
                and latest.get('state') == 'open' and latest.get('head', {}).get('sha') == commit,
                'PR changed before write')
    body = (MARKER + json.dumps(marker(saved, commit), sort_keys=True) + ' -->\n\n'
            f'Promote the documented install pin to {tag} from release run {run_id}.\n\n'
            'Generated from the release producer artifact; review and required checks remain necessary. '
            'The Homebrew tap is managed separately. No automatic merge is enabled.\n')
    payload = {'title': f'docs: promote install pin to {tag}', 'body': body}
    if prs:
        if prs[0].get('body') != body:
            api.get('pulls/' + str(integer(prs[0]['number'])), data=payload, method='PATCH')
    else:
        api.get('pulls', data={**payload, 'head': branch, 'base': 'main'}, method='POST')
    return commit


def event_run(env):
    require(env.get('GITHUB_REPOSITORY') == REPO and env.get('GITHUB_REF') == 'refs/heads/main',
            'promotion must execute on repository main')
    event = promotion.load_metadata(env['GITHUB_EVENT_PATH'])
    require(event.get('repository', {}).get('full_name') == REPO, 'foreign event')
    if env.get('GITHUB_EVENT_NAME') == 'workflow_run':
        require(event.get('action') == 'completed', 'upstream run is not completed')
        return integer(event['workflow_run']['id'])
    require(env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch', 'unsupported event')
    value = event.get('inputs', {}).get('run_id', '')
    require(isinstance(value, str) and re.fullmatch('[0-9]{1,20}', value), 'invalid recovery run id')
    return integer(int(value))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['prepare', 'publish'])
    parser.add_argument('--plan', type=Path, required=True)
    args = parser.parse_args()
    try:
        api, run_id = API(), event_run(os.environ)
        if args.mode == 'prepare':
            plan = prepare(api, run_id)
            args.plan.write_text(json.dumps(plan, sort_keys=True) + '\n')
            with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
                stream.write('changed=' + str(bool(plan['changes'])).lower() + '\n')
        else:
            result = publish(api, run_id, promotion.load_metadata(args.plan), os.environ.get('APP_SLUG'),
                             os.environ.get('EXPECTED_APP_SLUG'))
            print('release-pin promotion: ' + result)
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print('release-pin automation refused: ' + str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
