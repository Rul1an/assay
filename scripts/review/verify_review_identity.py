#!/usr/bin/env python3
"""Validate a linked review declaration; does not authenticate agent identity."""
import argparse
import re
import subprocess
from urllib.parse import urlsplit

from pr_landing_readiness import (
    machine_review_candidate,
    parse_repo,
    run_json,
)


def verify(repo, pr, head, branch_ref, record_author, identity, evidence_url, pr_author):
    parse_repo(repo)
    if not re.fullmatch(r"[1-9][0-9]*", str(pr)):
        raise ValueError('PR must be a positive decimal integer')
    url = urlsplit(evidence_url)
    expected_path = f'/{repo}/pull/{pr}'
    if (url.scheme != 'https' or url.netloc != 'github.com'
            or url.path != expected_path or url.query
            or not re.fullmatch(r'issuecomment-[1-9][0-9]*', url.fragment)):
        raise ValueError('evidence must be a GitHub comment on this repository and PR')
    if not identity.strip():
        raise ValueError('reviewing identity must be explicit and distinct from record author')
    comment_id = url.fragment.split('-', 1)[1]
    # Construct a fixed API endpoint, never fetch the caller-supplied URL.
    comment = run_json(['gh', 'api', f'repos/{repo}/issues/comments/{comment_id}'])
    if (comment.get('html_url') != evidence_url
            or comment.get('issue_url') != f'https://api.github.com/repos/{repo}/issues/{pr}'
            or comment.get('user', {}).get('login') != record_author):
        raise ValueError('review evidence publisher or PR does not match')
    body = comment.get('body', '').strip()
    machine = machine_review_candidate(body, record_author, head, branch_ref)
    if machine is None or machine['verdict'] != 'READY' or machine['bound_sha'] != head:
        raise ValueError('review identity, head, verdict or independence declaration mismatch')
    direct_human = (
        identity == record_author and record_author != pr_author
        and machine['reviewer_identity'] == f'human/{record_author}'
    )
    if not (machine['reviewer_identity'] == identity or direct_human):
        raise ValueError('review identity, head, verdict or independence declaration mismatch')
    print(f'Record author: {record_author}')
    print(f'Reviewing identity: {identity}')
    print(f'Review evidence: {evidence_url}')
    print('The linked record declares a completed non-building review; agent identity is not authenticated by this check.')


def main():
    parser = argparse.ArgumentParser()
    for name in ('repo', 'pr', 'head', 'branch-ref', 'record-author', 'reviewer-identity', 'review-evidence-url', 'pr-author'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    try:
        verify(args.repo, args.pr, args.head, args.branch_ref, args.record_author,
               args.reviewer_identity, args.review_evidence_url, args.pr_author)
    except (ValueError, TypeError, AttributeError, OSError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f'BLOCKED: {error}\n')


if __name__ == '__main__':
    main()
