#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from urllib.parse import quote, unquote

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ci"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from closing_keywords import closing_problems  # noqa: E402
from assay_review_record_check import (  # noqa: E402
    GateError,
    Git,
    evaluate,
    MARKER as REVIEW_RECORD_MARKER,
    _loose_object,
    derive_carry,
    extract_record,
    resolve_supersedes,
    validate_record,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
COMMENT_URL_ID_RE = re.compile(r"#issuecomment-([1-9][0-9]*)$")
SHA_RE = re.compile(r"(?<![0-9a-f])[0-9a-f]{40}(?![0-9a-f])", re.IGNORECASE)
REPO_COMPONENT_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,99}")
MAX_JSON_BYTES = 8 * 1024 * 1024
COMMAND_TIMEOUT_SECONDS = 30
PR_VIEW_FIELDS = "number,title,url,state,author,isDraft,mergeable,mergeStateStatus,headRefName,headRefOid,baseRefName,baseRefOid,body,reviews,comments,commits"
PR_CHECK_FIELDS = "name,state,bucket,link,workflow,event"
REQUIRED_CONTEXT_QUERY = "query($owner:String!,$name:String!,$ref:String!){repository(owner:$owner,name:$name){ref(qualifiedName:$ref){branchProtectionRule{requiredStatusCheckContexts}}}}"


def parse_repo(repo):
    if not isinstance(repo, str) or repo.count("/") != 1:
        raise SystemExit("repository must be OWNER/REPO")
    owner, name = repo.split("/", 1)
    if REPO_COMPONENT_RE.fullmatch(owner) is None or REPO_COMPONENT_RE.fullmatch(name) is None:
        raise SystemExit("repository must be OWNER/REPO")
    return owner, name


def encoded_branch(branch):
    forbidden = ("..", "@{", "//")
    if (not isinstance(branch, str) or not branch or branch.startswith(("/", ".", "-"))
            or branch.endswith(("/", ".", ".lock"))
            or any(token in branch for token in forbidden)
            or any(ord(char) < 32 or char in " ~^:?*[\\" for char in branch)):
        raise SystemExit("branch name is not a supported Git ref")
    return quote(branch, safe="")


def validate_gh_command(args):
    if (not isinstance(args, list) or len(args) < 3
            or any(not isinstance(arg, str) or "\x00" in arg for arg in args)
            or args[0] != "gh"):
        raise SystemExit("unsupported GitHub command shape")
    shape = tuple(args[1:3])
    if shape == ("pr", "view"):
        if (len(args) != 8 or not args[3].isdigit() or args[4] != "--repo"
                or args[6:] != ["--json", PR_VIEW_FIELDS]):
            raise SystemExit("unsupported GitHub command shape")
        parse_repo(args[5])
        return
    if shape == ("pr", "checks"):
        if (len(args) not in {8, 9} or not args[3].isdigit() or args[4] != "--repo"):
            raise SystemExit("unsupported GitHub command shape")
        parse_repo(args[5])
        if args[6:] not in (["--json", PR_CHECK_FIELDS],
                            ["--required", "--json", PR_CHECK_FIELDS]):
            raise SystemExit("unsupported GitHub command shape")
        return
    if shape == ("api", "graphql"):
        if (len(args) != 11 or args[3::2] != ["-f", "-f", "-f", "-f"]
                or args[4] != f"query={REQUIRED_CONTEXT_QUERY}"
                or not args[6].startswith("owner=") or not args[8].startswith("name=")
                or not args[10].startswith("ref=refs/heads/")):
            raise SystemExit("unsupported GitHub command shape")
        parse_repo(f"{args[6][6:]}/{args[8][5:]}")
        encoded_branch(args[10][len("ref=refs/heads/"):])
        return
    if args[1] == "api":
        parts = args[2].split("/")
        if len(parts) == 5 and parts[0] == "repos" and parts[3] == "branches":
            encoded = parts[4]
            allowed_tail = []
        elif (len(parts) == 6 and parts[0] == "repos"
              and parts[3:5] == ["rules", "branches"]):
            encoded = parts[5]
            allowed_tail = ["--paginate", "--slurp"]
        elif (len(parts) == 6 and parts[0] == "repos" and parts[3] == "issues"
              and parts[4].isdigit() and int(parts[4]) > 0 and parts[5] == "comments"):
            # The comment list the required checker reads, in the checker's own shape: it
            # carries `user.type`, `created_at` and `updated_at`, which `gh pr view` does not.
            parse_repo(f"{parts[1]}/{parts[2]}")
            if args[3:] != ["--paginate", "--slurp"]:
                raise SystemExit("unsupported GitHub command shape")
            return
        elif (len(parts) == 6 and parts[0] == "repos"
              and parts[3:5] == ["issues", "comments"]
              and parts[5].isdigit() and int(parts[5]) > 0):
            parse_repo(f"{parts[1]}/{parts[2]}")
            if args[3:]:
                raise SystemExit("unsupported GitHub command shape")
            return
        else:
            raise SystemExit("unsupported GitHub command shape")
        parse_repo(f"{parts[1]}/{parts[2]}")
        if (encoded_branch(unquote(encoded)) != encoded
                or args[3:] not in ([], allowed_tail)):
            raise SystemExit("unsupported GitHub command shape")
        return
    raise SystemExit("unsupported GitHub command shape")


def run_json(args, allowed_returncodes=(0,)):
    validate_gh_command(args)
    try:
        with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
            # argv is list-based and its command family is closed above; no shell is involved.
            # codeql[py/command-line-injection]
            result = subprocess.run(
                args, check=False, stdout=out, stderr=err,
                timeout=COMMAND_TIMEOUT_SECONDS,
            )
            if out.tell() > MAX_JSON_BYTES:
                raise SystemExit("command output exceeds byte limit")
            err.seek(0)
            error_text = err.read(262144).decode("utf-8", errors="replace").strip()
            out.seek(0)
            output = out.read().decode("utf-8", errors="strict")
    except subprocess.TimeoutExpired as error:
        raise SystemExit("GitHub command timed out") from error
    except UnicodeDecodeError as error:
        raise SystemExit("command returned non-UTF-8 output") from error
    if result.returncode not in allowed_returncodes:
        raise SystemExit(error_text or output.strip())
    if not output.strip():
        raise SystemExit(error_text or "command returned no JSON")
    try:
        return json.loads(output)
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
    blocked = re.search(r"(?m)^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:VERDICT\s*:\s*)?BLOCKED(?:\*\*)?\s*[.!]?\s*$", upper)
    ready = re.search(r"(?m)^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:VERDICT\s*:\s*)?READY(?:\*\*)?\s*[.!]?\s*$", upper)
    if blocked:
        return "BLOCKED"
    if any(marker in upper for marker in unavailable):
        return None
    if ready:
        return "READY"
    return None


def machine_review_candidate(body, author, head=None, branch_ref=""):
    if REVIEW_RECORD_MARKER not in body:
        return None
    try:
        record = extract_record(body)
    except GateError as error:
        loose = _loose_object(body)
        bound = (loose or {}).get("head_sha")
        if head and (bound == head or head.lower() in body.lower()):
            return {
                "verdict": "BLOCKED", "bound_sha": head,
                "reviewer_identity": None, "validation_error": error.reason,
            }
        return None
    if record is None:
        return None
    bound = record.get("head_sha")
    if head and (not isinstance(bound, str) or bound.lower() != head.lower()):
        return None
    live_sha = head or bound
    try:
        validate_record(record, live_sha=live_sha, branch_ref=branch_ref, require_ready=False)
        reviewer = record["reviewer"]
        if reviewer.get("github_login") != author:
            raise GateError("login_mismatch", str(author))
    except (GateError, KeyError, TypeError) as error:
        if head and isinstance(bound, str) and bound.lower() == head.lower():
            reason = error.reason if isinstance(error, GateError) else "malformed_record"
            return {
                "verdict": "BLOCKED", "bound_sha": head,
                "reviewer_identity": None, "validation_error": reason,
            }
        return None
    return {
        "verdict": record["verdict"],
        "bound_sha": bound,
        "reviewer_identity": f"{reviewer['agent']}/{reviewer['instance']}",
        "validation_error": None,
    }


def head_record(body, head):
    if not head or REVIEW_RECORD_MARKER not in body:
        return None
    try:
        record = extract_record(body)
    except GateError:
        return None
    bound = (record or {}).get("head_sha")
    return record if isinstance(bound, str) and bound.lower() == head.lower() else None


def supersede_resolution(comments, head, branch_ref):
    """Map comment positions through the checker's own supersede rule."""
    entries, where = [], {}
    for n, comment in enumerate(comments):
        record = head_record(comment.get("body") or "", head)
        if record is None:
            continue
        match = COMMENT_URL_ID_RE.search(str(comment.get("url") or ""))
        where[n] = len(entries)
        entries.append((int(match.group(1)) if match else None,
                        comment.get("author", {}).get("login"), comment.get("createdAt"), record))
    retired, refused = resolve_supersedes(entries, live_sha=head, branch_ref=branch_ref)
    return ({n for n, i in where.items() if i in retired},
            {n: refused[i] for n, i in where.items() if i in refused})


def record_head_sha(body):
    """The head a well-formed record names, or None. No judgement, just the binding."""
    if REVIEW_RECORD_MARKER not in body:
        return None
    try:
        record = extract_record(body)
    except GateError:
        return None
    bound = (record or {}).get("head_sha")
    return bound.lower() if isinstance(bound, str) else None


def _objects_root(git_root, repo, cache):
    """A checkout to derive in: this one, or a temporary clone that can fetch the commits.

    The landing recipe allows running these helpers from a `git archive` extract, which has no
    `.git` at all, and `derive_carry` needs the commits. Rather than falling back to the literal
    head comparison there - a second answer to the question this gate just delegated - a bare
    temporary repository is created with `origin` set to the PR's repository, and the checker's
    own bounded fetch brings in the two commits. The caller says so in its output.
    """
    if cache.get("root") is None:
        probe = Git(str(git_root))
        if probe.run("rev-parse", "--is-inside-work-tree")[1] == "true":
            cache["root"], cache["note"] = str(git_root), ""
        elif repo:
            tmp = tempfile.TemporaryDirectory(prefix="landing-carry-")
            cache["tmp"] = tmp  # kept alive for the process; removed when it exits
            scratch = Git(tmp.name)
            scratch.run("init", "-q")
            scratch.run("remote", "add", "origin", f"https://github.com/{repo}.git")
            cache["root"], cache["note"] = tmp.name, " (objects fetched into a temporary clone)"
        else:
            cache["root"], cache["note"] = str(git_root), ""
    return cache["root"], cache["note"]


def carried_to_head(bound, head, git_root, cache, repo=None):
    """Does the CI gate's own derivation bind `bound` to `head`? One rule, one function.

    The required `review-record-check` carries a record across an upstream-advance merge by
    re-deriving both AGENTS.md conditions from the commits (#2955). This gate asks that same
    function rather than restating the conditions, so the two cannot answer differently about
    one head. A refusal is kept for the human read: it says which condition failed.
    """
    if not bound or bound == head:
        return False, None
    if bound not in cache:
        root, note = _objects_root(git_root, repo, cache.setdefault("_root", {}))
        try:
            cache[bound] = (True, derive_carry(Git(root), bound, head) + note)
        except GateError as exc:
            cache[bound] = (False, f"{exc.reason}: {exc.detail}" if exc.detail else exc.reason)
    return cache[bound]


def gate_answer(repo, number, head, branch_ref, git_root, cache):
    """What the required `review-record-check` says about this head, from its own function.

    The landing decision is that gate's decision. `evaluate` judges the whole comment set, not
    one record: a bot carrier, an edited record, two current records or a refused supersede all
    refuse there, and a record on an earlier head passes only through `derive_carry`. Asking it
    here is what keeps the two gates from drifting apart again (#2958).
    """
    if "answer" not in cache:
        try:
            slurped = run_json([
                "gh", "api", f"repos/{repo}/issues/{number}/comments",
                "--paginate", "--slurp",
            ])
            comments = [c for page in (slurped or []) for c in (page or [])]
            root, note = _objects_root(git_root, repo, cache.setdefault("_root", {}))
            evaluate(head, branch_ref, comments, git_root=root)
            cache["answer"] = (True, f"review-record-check would pass{note}")
        except GateError as exc:
            cache["answer"] = (False, f"{exc.reason}: {exc.detail}" if exc.detail else exc.reason)
        except SystemExit:
            raise
    return cache["answer"]


def review_candidates(pr, head, git_root=None, repo=None, gate=None):
    rows = []
    git_root = REPO_ROOT if git_root is None else git_root
    carries = {}
    for review in pr.get("reviews", []):
        state = review.get("state")
        if state == "DISMISSED":
            continue
        body = review.get("body") or ""
        bound = (review.get("commit") or {}).get("oid") or next(iter(SHA_RE.findall(body)), None)
        result = "BLOCKED" if state == "CHANGES_REQUESTED" else verdict(body)
        if result:
            rows.append({
                "record_author": review.get("author", {}).get("login"),
                "reviewer_identity": None,
                "verdict": result,
                "bound_sha": bound,
                "current_head": bound == head,
                "source": "review",
            })
    comments = pr.get("comments", [])
    retired, refused = supersede_resolution(comments, head, pr.get("headRefName") or "")
    for n, comment in enumerate(comments):
        if n in retired:
            continue
        body = comment.get("body") or ""
        author = comment.get("author", {}).get("login")
        if n in refused:
            rows.append({
                "record_author": author,
                "reviewer_identity": None,
                "verdict": "BLOCKED",
                "bound_sha": head,
                "current_head": True,
                "source": "invalid-machine-comment",
            })
            continue
        machine = machine_review_candidate(body, author, head, pr.get("headRefName") or "")
        if machine:
            rows.append({
                "record_author": author,
                "reviewer_identity": machine["reviewer_identity"],
                "verdict": machine["verdict"],
                "bound_sha": machine["bound_sha"],
                "current_head": machine["bound_sha"] == head,
                "source": "machine-comment" if machine["validation_error"] is None else "invalid-machine-comment",
                "carry": None,
            })
            continue
        # A record bound to an earlier head still binds this one when the CI gate's derivation
        # says so. It is validated against its own head, exactly as that gate validates it.
        earlier = record_head_sha(body)
        if earlier and earlier != head:
            carried, carry_note = carried_to_head(earlier, head, git_root, carries, repo)
            if carried and gate is not None and not gate[0]:
                # The derivation carries this record, but the gate judges the set: a bot
                # carrier, an edit, an ambiguity or a refused supersede refuses there.
                carried, carry_note = False, f"{carry_note}; gate refuses the set: {gate[1]}"

            reviewed = machine_review_candidate(body, author, earlier, pr.get("headRefName") or "")
            if reviewed and reviewed["validation_error"] is None:
                rows.append({
                    "record_author": author,
                    "reviewer_identity": reviewed["reviewer_identity"],
                    "verdict": reviewed["verdict"],
                    "bound_sha": earlier,
                    "current_head": carried,
                    "source": "machine-comment-carry" if carried else "machine-comment",
                    "carry": carry_note,
                })
                continue
        if body.strip().startswith(REVIEW_RECORD_MARKER):
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
    owner, name = parse_repo(repo)
    branch_path = encoded_branch(branch)
    response = run_json([
        "gh", "api", "graphql", "-f",
        f"query={REQUIRED_CONTEXT_QUERY}",
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
        pages = run_json(["gh", "api", f"repos/{repo}/rules/branches/{branch_path}",
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
    parse_repo(args.repo)

    pr = run_json([
        "gh", "pr", "view", str(args.pr), "--repo", args.repo, "--json",
        PR_VIEW_FIELDS,
    ])
    # gh exits 8 for pending checks and 1 for failed checks while still emitting
    # the JSON needed to report their actual state.
    explicit_checks = args.unprotected_require_check
    if explicit_checks:
        if any(not name.strip() for name in explicit_checks):
            raise SystemExit("check names must not be empty")
        branch_path = encoded_branch(pr["baseRefName"])
        branch = run_json(["gh", "api", f"repos/{args.repo}/branches/{branch_path}"])
        rules = run_json(["gh", "api", f"repos/{args.repo}/rules/branches/{branch_path}"])
        if not isinstance(branch, dict) or branch.get("protected") is not False or rules != []:
            raise SystemExit("explicit policy requires an unprotected branch and an empty active-rule list")
    required = run_json([
        "gh", "pr", "checks", str(args.pr), "--repo", args.repo,
        *([] if explicit_checks else ["--required"]), "--json",
        PR_CHECK_FIELDS,
    ], allowed_returncodes=(0, 1, 8))
    if explicit_checks:
        expected_required = sorted(set(explicit_checks))
    else:
        expected_required = required_contexts(args.repo, pr["baseRefName"])
    missing_required = missing_required_contexts(required, expected_required)
    head = pr["headRefOid"]
    body_shas = SHA_RE.findall(pr.get("body") or "")
    body_mentions_head = head in body_shas
    gate = gate_answer(args.repo, args.pr, head, pr.get("headRefName") or "",
                       REPO_ROOT, {}) if any(
        REVIEW_RECORD_MARKER in (c.get("body") or "") for c in pr.get("comments", [])) else None
    candidates = review_candidates(pr, head, repo=args.repo, gate=gate)
    current_ready = [row for row in candidates if row["current_head"] and row["verdict"] == "READY"]
    current_blocked = [row for row in candidates if row["current_head"] and row["verdict"] == "BLOCKED"]
    failing = [check for check in required if check.get("bucket") == "fail"]
    pending = [check for check in required if check.get("bucket") in {"pending", "cancel"}]
    required_green = (
        bool(expected_required) and not missing_required and not failing and not pending
        and all(any(
            check.get("name") == name and check.get("state") == "SUCCESS"
            and check.get("bucket") == "pass" for check in required
        ) for name in expected_required)
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
    # Closing keywords in text that lands on the default branch (#2880). GitHub has no notion
    # of negation and reads frozen commit messages, so both have closed issues their authors
    # meant to keep open. See scripts/review/closing_keywords.py for the rule.
    commit_messages = [
        (f"commit {str(c.get('oid') or '')[:9]}",
         f"{c.get('messageHeadline') or ''}\n\n{c.get('messageBody') or ''}")
        for c in (pr.get("commits") or [])
    ]
    keyword_problems = closing_problems(
        args.repo, pr.get("title") or "", pr.get("body") or "", commit_messages)
    blockers.extend(keyword_problems)

    payload = {
        "pr": {key: pr.get(key) for key in ("number", "title", "url", "state", "author", "isDraft", "mergeable", "mergeStateStatus", "headRefName", "headRefOid", "baseRefName", "baseRefOid")},
        "required_checks": required,
        "expected_required_contexts": expected_required,
        "missing_required_contexts": missing_required,
        "required_green": required_green,
        "check_policy": "explicit-unprotected" if explicit_checks else "classic-and-active-rulesets",
        "review_candidates": candidates,
        "body_mentions_head": body_mentions_head,
        "closing_keyword_problems": keyword_problems,
        "blockers": blockers,
        "landing_candidate": not blockers,
        "non_claim": "Reviewer independence and actionable-finding disposition require human verification.",
    }
    if args.format == "json":
        json.dump(payload, sys.stdout, indent=2)
        print()
        return

    print(f"# PR #{markdown_atom(pr['number'])} landing readiness")
    print(f"- head: {markdown_atom(head)}")
    print(f"- base: {markdown_atom(pr['baseRefOid'])}")
    print(f"- draft: {markdown_atom(pr['isDraft'])}; mergeable: {markdown_atom(pr['mergeable'])}")
    print(f"- required green: {markdown_atom(required_green)}")
    for check in required:
        print(f"  - {markdown_atom(check.get('name'))}: {markdown_atom(check.get('state'))} ({markdown_atom(check.get('bucket'))})")
    if missing_required:
        print(f"  - not reported: {markdown_atom(', '.join(missing_required))}")
    print(f"- PR body names current head: {markdown_atom(body_mentions_head)}")
    print("- review candidates:")
    if not candidates:
        print("  - none")
    for row in candidates:
        identity = row["reviewer_identity"] or "not declared"
        print(f"  - {markdown_atom(row['verdict'])} record by {markdown_atom(row['record_author'])}; reviewer {markdown_atom(identity)} on {markdown_atom(row['bound_sha'])}; current={markdown_atom(row['current_head'])} ({markdown_atom(row['source'])}){markdown_atom('; ' + row['carry']) if row.get('carry') else ''}")
    print("- blockers:")
    if not blockers:
        print("  - none from machine-verifiable state")
    for blocker in blockers:
        print(f"  - {markdown_atom(blocker)}")
    print("- non-claim: reviewer independence and finding disposition still require human verification")


def markdown_atom(value):
    """Render untrusted scalar data as one JSON-escaped Markdown-safe line."""
    rendered = json.dumps(value, ensure_ascii=True, separators=(",", ":"))
    for delimiter in "`*_[]<>":
        rendered = rendered.replace(delimiter, f"\\u{ord(delimiter):04x}")
    return rendered


if __name__ == "__main__":
    main()
