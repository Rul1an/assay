#!/usr/bin/env python3
"""Validate a linked review declaration; does not authenticate agent identity."""
import argparse
import json
import re
import subprocess
import tempfile
from urllib.parse import urlsplit

from pr_landing_readiness import (
    REVIEW_RECORD_FENCE,
    REVIEW_RECORD_MARKER,
    machine_review_candidate,
    parse_repo,
)


def verify(repo, pr, head, record_author, identity, evidence_url, pr_author):
    parse_repo(repo)
    if not re.fullmatch(r"[1-9][0-9]*", str(pr)):
        raise ValueError('PR must be a positive decimal integer')
    url = urlsplit(evidence_url)
    expected_path = f'/{repo}/pull/{pr}'
    if (url.scheme != 'https' or url.netloc != 'github.com'
            or url.path != expected_path or url.query
            or not re.fullmatch(r'issuecomment-[1-9][0-9]*', url.fragment)):
        raise ValueError('evidence must be a GitHub comment on this repository and PR')
    if not identity.strip() or (record_author == pr_author and identity == record_author):
        raise ValueError('reviewing identity must be explicit and distinct from record author')
    comment_id = url.fragment.split('-', 1)[1]
    # Construct a fixed API endpoint, never fetch the caller-supplied URL.
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        result = subprocess.run(
            ['gh', 'api', f'repos/{repo}/issues/comments/{comment_id}'],
            stdout=out, stderr=err, timeout=30, check=False,
        )
        if result.returncode != 0:
            raise ValueError('review evidence unavailable')
        if out.tell() > 262144:
            raise ValueError('review evidence exceeds byte limit')
        out.seek(0)
        comment = json.load(out)
    if (comment.get('html_url') != evidence_url
            or comment.get('issue_url') != f'https://api.github.com/repos/{repo}/issues/{pr}'
            or comment.get('user', {}).get('login') != record_author):
        raise ValueError('review evidence publisher or PR does not match')
    body = comment.get('body', '').strip()
    machine = machine_review_candidate(body, record_author)
    if machine is None or machine['verdict'] != 'READY' or machine['bound_sha'] != head:
        raise ValueError('review identity, head, verdict or independence declaration mismatch')
    match = REVIEW_RECORD_FENCE.fullmatch(body[len(REVIEW_RECORD_MARKER):].strip())
    if match is None:
        raise ValueError('review record envelope is malformed')
    record = json.loads(match.group(1))
    reviewer = record.get('reviewer', {})
    direct_human = (
        identity == record_author and record_author != pr_author
        and reviewer.get('agent') == 'human'
        and reviewer.get('instance') == record_author
    )
    if not (machine['reviewer_identity'] == identity or direct_human):
        raise ValueError('review identity, head, verdict or independence declaration mismatch')
    builder = record.get('builder', {})
    if (builder.get('agent'), builder.get('instance')) == (reviewer.get('agent'), reviewer.get('instance')):
        raise ValueError('reviewer is also the declared builder')
    print(f'Record author: {record_author}')
    print(f'Reviewing identity: {identity}')
    print(f'Review evidence: {evidence_url}')
    print('The linked record declares a completed non-building review; agent identity is not authenticated by this check.')


def main():
    parser = argparse.ArgumentParser()
    for name in ('repo', 'pr', 'head', 'record-author', 'reviewer-identity', 'review-evidence-url', 'pr-author'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    try:
        verify(args.repo, args.pr, args.head, args.record_author,
               args.reviewer_identity, args.review_evidence_url, args.pr_author)
    except (ValueError, TypeError, AttributeError, OSError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f'BLOCKED: {error}\n')


if __name__ == '__main__':
    main()
