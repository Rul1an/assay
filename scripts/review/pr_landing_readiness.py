#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys
from urllib.parse import quote


SHA_RE = re.compile(r"(?<![0-9a-f])[0-9a-f]{40}(?![0-9a-f])", re.IGNORECASE)
REVIEW_RECORD_MARKER = "<!-- assay-review-record -->"
REVIEW_RECORD_FENCE = re.compile(r"^```(?:json)?\n(.*)\n```$", re.S)


def run_json(args, allowed_returncodes=(0,)):
    result = subprocess.run(args, check=False, capture_output=True, text=True)
    if result.returncode not in allowed_returncodes:
        raise SystemExit(result.stderr.strip() or result.stdout.strip())
    if not result.stdout.strip():
        raise SystemExit(result.stderr.strip() or "command returned no JSON")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"command returned invalid JSON: {error}") from error


def verdict(text):
    upper = text.upper()
    unavailable = (
        "UNABLE TO REVIEW",
        "COULD NOT REVIEW",
        "REVIEW RATE LIMITED",
        "RATE LIMIT",
        "QUOTA LIMIT",
        "QUOTA EXCEEDED",
    )
    if any(marker in upper for marker in unavailable):
        return None
    blocked = re.search(r"(?m)^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:VERDICT\s*:\s*)?BLOCKED(?:\*\*)?\s*[.!]?\s*$", upper)
    ready = re.search(r"(?m)^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:VERDICT\s*:\s*)?READY(?:\*\*)?\s*[.!]?\s*$", upper)
    if blocked:
        return "BLOCKED"
    if "NOT READY" not in upper and ready:
        return "READY"
    return None


def machine_review_candidate(body, author):
    stripped = body.strip()
    if not stripped.startswith(REVIEW_RECORD_MARKER):
        return None
    fenced = stripped[len(REVIEW_RECORD_MARKER):].strip()
    match = REVIEW_RECORD_FENCE.fullmatch(fenced)
    if match is None or fenced.count("```") != 2:
        return None
    try:
        record = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    if not isinstance(record, dict) or record.get("schema") != "assay.review-record.v0":
        return None
    bound = record.get("head_sha")
    result = record.get("verdict")
    reviewer = record.get("reviewer")
    independence = record.get("independence")
    if (
        not isinstance(bound, str)
        or SHA_RE.fullmatch(bound) is None
        or result not in {"READY", "BLOCKED"}
        or record.get("review_completed") is not True
        or not isinstance(reviewer, dict)
        or reviewer.get("github_login") != author
        or not isinstance(independence, dict)
        or independence.get("did_not_build") is not True
        or independence.get("did_not_author_governing_spec") is not True
    ):
        return None
    return {
        "verdict": result,
        "bound_sha": bound,
        "reviewer_identity": f"{reviewer.get('agent')}/{reviewer.get('instance')}",
    }


def review_candidates(pr, head):
    rows = []
    for review in pr.get("reviews", []):
        body = review.get("body") or ""
        bound = (review.get("commit") or {}).get("oid") or next(iter(SHA_RE.findall(body)), None)
        result = verdict(body)
        if result:
            rows.append({
                "record_author": review.get("author", {}).get("login"),
                "reviewer_identity": None,
                "verdict": result,
                "bound_sha": bound,
                "current_head": bound == head,
                "source": "review",
            })
    for comment in pr.get("comments", []):
        body = comment.get("body") or ""
        author = comment.get("author", {}).get("login")
        machine = machine_review_candidate(body, author)
        if machine:
            rows.append({
                "record_author": author,
                "reviewer_identity": machine["reviewer_identity"],
                "verdict": machine["verdict"],
                "bound_sha": machine["bound_sha"],
                "current_head": machine["bound_sha"] == head,
                "source": "machine-comment",
            })
            continue
        result = verdict(body)
        shas = SHA_RE.findall(body)
        if result and shas:
            rows.append({
                "record_author": author,
                "reviewer_identity": None,
                "verdict": result,
                "bound_sha": shas[0],
                "current_head": shas[0] == head,
                "source": "comment",
            })
    return rows


def missing_required_contexts(reported, expected):
    reported_names = {check.get("name") for check in reported}
    return sorted(set(expected) - reported_names)


def required_contexts(repo, branch):
    owner, name = repo.split("/", 1)
    response = run_json([
        "gh", "api", "graphql", "-f",
        "query=query($owner:String!,$name:String!,$ref:String!){repository(owner:$owner,name:$name){ref(qualifiedName:$ref){branchProtectionRule{requiredStatusCheckContexts}}}}",
        "-f", f"owner={owner}", "-f", f"name={name}", "-f", f"ref=refs/heads/{branch}",
    ])
    try:
        if response.get("errors"):
            raise ValueError("GraphQL errors")
        classic = response["data"]["repository"]["ref"]["branchProtectionRule"]
        contexts = [] if classic is None else classic["requiredStatusCheckContexts"]
        if not isinstance(contexts, list):
            raise ValueError("invalid classic contexts")
        contexts = list(contexts)
        pages = run_json(["gh", "api", f"repos/{repo}/rules/branches/{quote(branch, safe='')}",
                          "--paginate", "--slurp"])
        if not isinstance(pages, list) or not pages:
            raise ValueError("invalid rule pages")
        for page in pages:
            if not isinstance(page, list):
                raise ValueError("invalid rule page")
            for rule in page:
                if not isinstance(rule, dict) or not isinstance(rule.get("type"), str):
                    raise ValueError("invalid rule")
                if rule["type"] == "required_status_checks":
                    checks = rule["parameters"]["required_status_checks"]
                    if not isinstance(checks, list):
                        raise ValueError("invalid required checks")
                    contexts.extend(check["context"] for check in checks)
        if not contexts or any(not isinstance(c, str) or not c.strip() for c in contexts):
            raise ValueError("no valid enforced check policy")
        return sorted(set(contexts))
    except (KeyError, TypeError, ValueError, AttributeError) as error:
        raise SystemExit(f"cannot establish required check policy: {error}") from error


def main():
    parser = argparse.ArgumentParser(description="Read-only Assay PR final-head readiness report")
    parser.add_argument("pr", type=int)
    parser.add_argument("--repo", default="Rul1an/assay")
    parser.add_argument("--format", choices=("md", "json"), default="md")
    parser.add_argument("--unprotected-require-check", action="append", default=[], metavar="NAME",
                        help="Explicit check policy, only for a verified unprotected base with no active rules")
    args = parser.parse_args()

    pr = run_json([
        "gh", "pr", "view", str(args.pr), "--repo", args.repo, "--json",
        "number,title,url,state,author,isDraft,mergeable,mergeStateStatus,headRefName,headRefOid,baseRefName,baseRefOid,body,reviews,comments",
    ])
    # gh exits 8 for pending checks and 1 for failed checks while still emitting
    # the JSON needed to report their actual state.
    explicit_checks = args.unprotected_require_check
    if explicit_checks:
        if any(not name.strip() for name in explicit_checks):
            raise SystemExit("check names must not be empty")
        branch = run_json(["gh", "api", f"repos/{args.repo}/branches/{pr['baseRefName']}"])
        rules = run_json(["gh", "api", f"repos/{args.repo}/rules/branches/{pr['baseRefName']}"])
        if not isinstance(branch, dict) or branch.get("protected") is not False or rules != []:
            raise SystemExit("explicit policy requires an unprotected branch and an empty active-rule list")
    required = run_json([
        "gh", "pr", "checks", str(args.pr), "--repo", args.repo,
        *([] if explicit_checks else ["--required"]), "--json",
        "name,state,bucket,link,workflow,event",
    ], allowed_returncodes=(0, 1, 8))
    if explicit_checks:
        expected_required = sorted(set(explicit_checks))
    else:
        expected_required = required_contexts(args.repo, pr["baseRefName"])
    missing_required = missing_required_contexts(required, expected_required)
    head = pr["headRefOid"]
    body_shas = SHA_RE.findall(pr.get("body") or "")
    body_mentions_head = head in body_shas
    candidates = review_candidates(pr, head)
    current_ready = [row for row in candidates if row["current_head"] and row["verdict"] == "READY"]
    current_blocked = [row for row in candidates if row["current_head"] and row["verdict"] == "BLOCKED"]
    failing = [check for check in required if check.get("bucket") == "fail"]
    pending = [check for check in required if check.get("bucket") in {"pending", "cancel"}]
    required_green = bool(expected_required) and not missing_required and not failing and not pending
    if explicit_checks:
        required_green = required_green and all(
            check.get("state") == "SUCCESS" and check.get("bucket") == "pass"
            for check in required if check.get("name") in expected_required
        )
    blockers = []
    if pr.get("state") != "OPEN":
        blockers.append(f"PR state is {pr.get('state')}")
    if pr.get("isDraft"):
        blockers.append("PR is draft")
    if pr.get("mergeable") != "MERGEABLE":
        blockers.append(f"mergeable={pr.get('mergeable')}")
    if not required_green:
        blockers.append("required checks are not all green")
    if missing_required:
        blockers.append(f"required contexts not reported: {', '.join(missing_required)}")
    if not current_ready:
        blockers.append("no READY review candidate bound to current head")
    if current_blocked:
        blockers.append("current-head BLOCKED review exists")
    if not body_mentions_head:
        blockers.append("PR body does not mention current head SHA")

    payload = {
        "pr": {key: pr.get(key) for key in ("number", "title", "url", "state", "author", "isDraft", "mergeable", "mergeStateStatus", "headRefName", "headRefOid", "baseRefName", "baseRefOid")},
        "required_checks": required,
        "expected_required_contexts": expected_required,
        "missing_required_contexts": missing_required,
        "required_green": required_green,
        "check_policy": "explicit-unprotected" if explicit_checks else "classic-and-active-rulesets",
        "review_candidates": candidates,
        "body_mentions_head": body_mentions_head,
        "blockers": blockers,
        "landing_candidate": not blockers,
        "non_claim": "Reviewer independence and actionable-finding disposition require human verification.",
    }
    if args.format == "json":
        json.dump(payload, sys.stdout, indent=2)
        print()
        return

    print(f"# PR #{pr['number']} landing readiness")
    print(f"- head: `{head}`")
    print(f"- base: `{pr['baseRefOid']}`")
    print(f"- draft: `{pr['isDraft']}`; mergeable: `{pr['mergeable']}`")
    print(f"- required green: `{required_green}`")
    for check in required:
        print(f"  - `{check['name']}`: `{check['state']}` ({check['bucket']})")
    if missing_required:
        print(f"  - not reported: `{', '.join(missing_required)}`")
    print(f"- PR body names current head: `{body_mentions_head}`")
    print("- review candidates:")
    if not candidates:
        print("  - none")
    for row in candidates:
        identity = row["reviewer_identity"] or "not declared"
        print(f"  - `{row['verdict']}` record by `{row['record_author']}`; reviewer `{identity}` on `{row['bound_sha']}`; current=`{row['current_head']}` ({row['source']})")
    print("- blockers:")
    if not blockers:
        print("  - none from machine-verifiable state")
    for blocker in blockers:
        print(f"  - {blocker}")
    print("- non-claim: reviewer independence and finding disposition still require human verification")


if __name__ == "__main__":
    main()
