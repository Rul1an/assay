#!/usr/bin/env python3
"""Exact-head review-record checker (#2561 slice A). Not a live CI gate."""
from __future__ import annotations

import json
import os
import re
import sys
import urllib.error
import urllib.request
from typing import Any

MARKER = "<!-- assay-review-record -->"
SCHEMA = "assay.review-record.v0"
CHECKER = "scripts/ci/assay_review_record_check.py"
HOOK_ID = "assay-review-record-self-test"
PREFIXES = frozenset({"codex", "claude", "cursor", "ruley"})
HEX40 = re.compile(r"^[0-9a-f]{40}$")
FENCE = re.compile(r"^```(?:json)?\n(.*)\n```$", re.S)
HTTP_TIMEOUT_S = 30
MAX_RESPONSE_BYTES = 8 * 1024 * 1024
COMMENT_PAGE_SIZE = 100
COMMENT_PAGE_MAX = 2


class GateError(Exception):
    def __init__(self, reason: str, detail: str = "") -> None:
        super().__init__(reason)
        self.reason, self.detail = reason, detail


def _root() -> str:
    return os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def _api_kind(path: str) -> str:
    return "comments_api_failure" if "comments" in path else "pr_api_failure"


def bounded_json(read: Any, kind: str, limit: int = MAX_RESPONSE_BYTES) -> Any:
    raw = read(limit + 1)
    if len(raw) > limit:
        raise GateError(kind, "response byte ceiling")
    try:
        return json.loads(raw.decode("utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as exc:
        raise GateError(kind, str(exc)) from exc


def head_fields(pr: Any) -> tuple[str, str]:
    head = pr.get("head") if isinstance(pr, dict) and isinstance(pr.get("head"), dict) else {}
    sha, ref = head.get("sha"), head.get("ref")
    if not isinstance(sha, str) or not HEX40.match(sha) or sha != sha.lower():
        raise GateError("pr_api_failure", "head.sha")
    if not isinstance(ref, str) or not ref.strip():
        raise GateError("pr_api_failure", "head.ref")
    return sha, ref


def inferred_builder_prefix(ref: str) -> str | None:
    head, sep, tail = (ref or "").partition("/")
    if not sep or not tail:
        return None
    first = head.lower()
    return first if first in PREFIXES else None


def _loose_object(text: str) -> dict[str, Any] | None:
    start, end = text.find("{"), text.rfind("}")
    if start < 0 or end <= start:
        return None
    try:
        value = json.loads(text[start : end + 1])
    except json.JSONDecodeError:
        return None
    return value if isinstance(value, dict) else None


def extract_record(body: str) -> dict[str, Any] | None:
    if MARKER not in body:
        return None
    stripped = body.strip()
    if not stripped.startswith(MARKER):
        raise GateError("extra_prose", "prose before marker")
    rest = stripped[len(MARKER) :].strip()
    if rest.count("```") != 2 or not FENCE.match(rest):
        if rest.count("```") > 2:
            raise GateError("multiple_fences", "more than one fence")
        raise GateError("extra_prose" if rest else "malformed_record", "carrier is not exactly one fence")
    try:
        value = json.loads(FENCE.match(rest).group(1))
    except json.JSONDecodeError as exc:
        raise GateError("malformed_record", str(exc)) from exc
    if not isinstance(value, dict):
        raise GateError("malformed_record", "JSON root is not an object")
    return value


def _need(obj: dict[str, Any], key: str, typ: type) -> Any:
    value = obj.get(key)
    if not isinstance(value, typ) or (typ is str and not value.strip()):
        raise GateError("missing_field", key)
    return value


def _pair(obj: Any, label: str) -> tuple[str, str]:
    if not isinstance(obj, dict):
        raise GateError("missing_field", label)
    return _need(obj, "agent", str), _need(obj, "instance", str)


def validate_record(record: dict[str, Any], *, live_sha: str, branch_ref: str) -> None:
    if record.get("schema") != SCHEMA:
        raise GateError("missing_field", "schema")
    sha = _need(record, "head_sha", str).lower()
    if not HEX40.match(sha):
        raise GateError("malformed_record", "head_sha")
    if sha != live_sha.lower():
        raise GateError("stale_sha", sha)
    if record.get("review_completed") is not True:
        raise GateError("did_not_review", "review_completed")
    verdict = _need(record, "verdict", str)
    if verdict not in {"READY", "BLOCKED"}:
        raise GateError("missing_field", "verdict")
    findings = record.get("findings")
    if not isinstance(findings, list):
        raise GateError("missing_field", "findings")
    no_findings = _need(record, "no_findings", bool)
    if bool(findings) == no_findings:
        raise GateError("missing_field", "no_findings")
    for item in findings:
        if not isinstance(item, dict):
            raise GateError("missing_disposition", "finding")
        for key in ("id", "summary", "disposition"):
            if not isinstance(item.get(key), str) or not str(item[key]).strip():
                raise GateError("missing_disposition", key)
    indep = record.get("independence")
    if not isinstance(indep, dict) or indep.get("did_not_build") is not True or indep.get(
        "did_not_author_governing_spec"
    ) is not True:
        raise GateError("missing_field", "independence")
    builder_agent, builder_instance = _pair(record.get("builder"), "builder")
    prefix = inferred_builder_prefix(branch_ref)
    if prefix and builder_agent.lower() != prefix:
        raise GateError("branch_prefix_mismatch", builder_agent)
    reviewer = record.get("reviewer")
    reviewer_agent, reviewer_instance = _pair(reviewer, "reviewer")
    _need(reviewer, "github_login", str)
    if (builder_agent, builder_instance) == (reviewer_agent, reviewer_instance):
        raise GateError("identical_writer_reviewer", builder_instance)
    if verdict != "READY":
        raise GateError("blocked", verdict)


def _current_sha(body: str, record: dict[str, Any] | None, live: str) -> bool:
    if record and str(record.get("head_sha") or "").lower() == live.lower():
        return True
    return live.lower() in body.lower()


def evaluate(live_sha: str, branch_ref: str, comments: list[dict[str, Any]]) -> None:
    current: list[dict[str, Any]] = []
    for comment in comments:
        if not isinstance(comment, dict):
            continue
        body = str(comment.get("body") or "")
        if MARKER not in body:
            continue
        try:
            record = extract_record(body)
        except GateError:
            loose = _loose_object(body)
            if _current_sha(body, loose, live_sha):
                raise
            continue
        if record is None or str(record.get("head_sha") or "").lower() != live_sha.lower():
            continue
        user = comment.get("user") if isinstance(comment.get("user"), dict) else {}
        typ = user.get("type")
        if not isinstance(typ, str) or not typ.strip():
            raise GateError("missing_field", "user.type")
        if typ != "User":
            raise GateError("bot_carrier", typ)
        created, updated = comment.get("created_at"), comment.get("updated_at")
        if not isinstance(created, str) or not created.strip():
            raise GateError("missing_field", "created_at")
        if not isinstance(updated, str) or not updated.strip():
            raise GateError("missing_field", "updated_at")
        if updated != created:
            raise GateError("edited_current", "updated_at != created_at")
        rev = record.get("reviewer")
        if not isinstance(rev, dict):
            raise GateError("missing_field", "reviewer")
        login, declared = str(user.get("login") or ""), str(rev.get("github_login") or "")
        if declared != login:
            raise GateError("login_mismatch", f"{declared} != {login}")
        current.append(record)
    if not current:
        raise GateError("no_current_record", live_sha)
    if len(current) != 1:
        raise GateError("ambiguous_current", str(len(current)))
    validate_record(current[0], live_sha=live_sha, branch_ref=branch_ref)


class GitHubApi:
    def __init__(self, repo: str, token: str) -> None:
        self.base = f"https://api.github.com/repos/{repo}"
        self.token = token

    def get(self, path: str) -> Any:
        kind = _api_kind(path)
        req = urllib.request.Request(
            f"{self.base}{path}",
            headers={"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
                     "User-Agent": "assay-review-record-check"},
        )
        try:
            with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT_S) as resp:
                return bounded_json(resp.read, kind)
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
            raise GateError(kind, str(exc)) from exc

    def comments(self, number: int) -> list[dict[str, Any]]:
        out, page = [], 1
        while page <= COMMENT_PAGE_MAX:
            batch = self.get(f"/issues/{number}/comments?per_page={COMMENT_PAGE_SIZE}&page={page}")
            if not isinstance(batch, list):
                raise GateError("comments_api_failure", "not a list")
            out.extend(x for x in batch if isinstance(x, dict))
            if len(batch) < COMMENT_PAGE_SIZE:
                return out
            page += 1
        raise GateError("comments_limit", "200-comment safety ceiling reached")


def live_check(number: int, api: GitHubApi | None = None) -> int:
    repo, token = os.environ.get("GITHUB_REPOSITORY", ""), os.environ.get("GITHUB_TOKEN", "")
    if api is None and (not repo or not token):
        print("GITHUB_REPOSITORY and GITHUB_TOKEN are required", file=sys.stderr)
        return 2
    client = api or GitHubApi(repo, token)
    first = client.get(f"/pulls/{number}")
    sha, ref = head_fields(first)
    comments = client.comments(number)
    again = client.get(f"/pulls/{number}")
    sha2, ref2 = head_fields(again)
    if (sha, ref) != (sha2, ref2):
        raise GateError("head_moved", f"{sha} {ref} -> {sha2} {ref2}")
    evaluate(sha2, ref2, comments)
    print(f"review-record-check=pass head={sha2}")
    return 0


def _rec(**over: Any) -> dict[str, Any]:
    row: dict[str, Any] = {
        "schema": SCHEMA, "head_sha": "a" * 40,
        "builder": {"agent": "ruley", "instance": "w1"},
        "reviewer": {"agent": "cursor", "instance": "r1", "github_login": "Rul1an"},
        "review_completed": True, "verdict": "READY", "findings": [], "no_findings": True,
        "independence": {"did_not_build": True, "did_not_author_governing_spec": True},
    }
    row.update(over)
    return row


def _cmt(record, *, extra="", second=False, bot=False, edited=False, login="Rul1an", body=None,
         user_type="User", created="t0", updated="keep"):
    if body is None:
        fence = "```json\n" + json.dumps(record) + "\n```"
        body = MARKER + "\nplease review\n" + fence + extra if extra else MARKER + "\n" + fence
        if second:
            body += "\n```json\n{}\n```"
    if bot:
        user_type = "Bot"
    user: dict[str, Any] = {"login": login}
    if user_type is not None:
        user["type"] = user_type
    if updated == "keep":
        updated = "t1" if edited else created
    row: dict[str, Any] = {"body": body, "user": user}
    if created is not None:
        row["created_at"] = created
    if updated is not None:
        row["updated_at"] = updated
    return row


def self_test() -> int:
    live, ref, green = "a" * 40, "ruley/2561-review-record-slice1", _rec()
    fail: list[str] = []

    def expect(reason: str, sha: str, branch: str, comments: list[dict[str, Any]]) -> None:
        try:
            evaluate(sha, branch, comments)
            fail.append(f"wanted {reason}, got pass")
        except GateError as exc:
            if exc.reason != reason:
                fail.append(f"wanted {reason}, got {exc.reason}")

    try:
        evaluate(live, ref, [_cmt(green)])
        evaluate(live, "cursor/feature", [_cmt(_rec(
            builder={"agent": "cursor", "instance": "w"},
            reviewer={"agent": "cursor", "instance": "r", "github_login": "Rul1an"},
        ))])
        evaluate(live, "feature/fix", [_cmt(_rec(builder={"agent": "human", "instance": "ext"}))])
        evaluate(live, "ruley", [_cmt(_rec(builder={"agent": "human", "instance": "ext"}))])
        evaluate(live, "ruley/", [_cmt(_rec(builder={"agent": "human", "instance": "ext"}))])
    except GateError as exc:
        fail.append(f"GREEN {exc.reason}")

    try:
        validate_record(_rec(head_sha="b" * 40), live_sha=live, branch_ref=ref)
        fail.append("wanted stale_sha, got pass")
    except GateError as exc:
        if exc.reason != "stale_sha":
            fail.append(f"wanted stale_sha, got {exc.reason}")

    reds = [
        ("no_current_record", "c" * 40, ref, [_cmt(green)]),
        ("no_current_record", live, ref, [_cmt(None, body="READY " + live)]),
        ("bot_carrier", live, ref, [_cmt(green, bot=True)]),
        ("bot_carrier", live, ref, [_cmt(green, user_type="Organization")]),
        ("missing_field", live, ref, [_cmt(green, user_type=None)]),
        ("missing_field", live, ref, [_cmt(green, created=None, updated=None)]),
        ("missing_field", live, ref, [_cmt(green, created=None)]),
        ("missing_field", live, ref, [_cmt(green, created="", updated="")]),
        ("identical_writer_reviewer", live, ref, [_cmt(_rec(
            reviewer={"agent": "ruley", "instance": "w1", "github_login": "Rul1an"}))]),
        ("missing_field", live, ref, [_cmt(_rec(verdict=None))]),
        ("missing_field", live, ref, [_cmt(_rec(findings=[], no_findings=False))]),
        ("missing_disposition", live, ref, [_cmt(_rec(
            findings=[{"id": "1", "summary": "x", "disposition": ""}], no_findings=False))]),
        ("did_not_review", live, ref, [_cmt(_rec(review_completed=False))]),
        ("ambiguous_current", live, ref, [_cmt(green), _cmt(green)]),
        ("malformed_record", live, ref, [_cmt(None, body=MARKER + "\n```json\n{not " + live + "\n```\n")]),
        ("edited_current", live, ref, [_cmt(green, edited=True)]),
        ("blocked", live, ref, [_cmt(_rec(verdict="BLOCKED"))]),
        ("branch_prefix_mismatch", live, ref, [_cmt(_rec(builder={"agent": "codex", "instance": "w1"}))]),
        ("branch_prefix_mismatch", live, "codex/foo", [_cmt(_rec(builder={"agent": "cursor", "instance": "w1"}))]),
        ("extra_prose", live, ref, [_cmt(green, extra="\nplease look\n")]),
        ("multiple_fences", live, ref, [_cmt(green, second=True)]),
        ("missing_field", live, ref, [_cmt(_rec(independence={
            "did_not_build": "true", "did_not_author_governing_spec": True}))]),
        ("missing_field", live, ref, [_cmt(_rec(reviewer=[]))]),
    ]
    for row in reds:
        expect(*row)

    if (HTTP_TIMEOUT_S, MAX_RESPONSE_BYTES, COMMENT_PAGE_SIZE, COMMENT_PAGE_MAX) != (30, 8 * 1024 * 1024, 100, 2):
        fail.append("API bound constants drifted")
    pc = open(os.path.join(_root(), ".pre-commit-config.yaml"), encoding="utf-8").read()
    if HOOK_ID not in pc or f"{CHECKER} --self-test" not in pc:
        fail.append("pre-commit hook not registered")
    if "review-record-check.yml" in pc:
        fail.append("slice A must not pin a workflow file")
    boom, urllib.request.urlopen = urllib.request.urlopen, lambda *_a, **_k: (_ for _ in ()).throw(urllib.error.URLError("down"))
    try:
        GitHubApi("o/r", "t").comments(1)
        fail.append("wanted comments_api_failure, got pass")
    except GateError as exc:
        if exc.reason != "comments_api_failure":
            fail.append(f"wanted comments_api_failure, got {exc.reason}")
    finally:
        urllib.request.urlopen = boom

    class _Pages(GitHubApi):
        pages: list[int] = []

        def get(self, path: str) -> Any:
            m = re.search(r"[?&]page=(\d+)", path)
            self.pages.append(int(m.group(1)) if m else 0)
            return [{}] * COMMENT_PAGE_SIZE

    _Pages.pages = []
    try:
        _Pages("o/r", "t").comments(1)
        fail.append("wanted comments_limit, got pass")
    except GateError as exc:
        if exc.reason != "comments_limit":
            fail.append(f"wanted comments_limit, got {exc.reason}")
    if _Pages.pages != [1, 2]:
        fail.append(f"wanted pages [1, 2] immediately, got {_Pages.pages!r}")

    try:
        bounded_json(lambda n: b"x" * n, "pr_api_failure", limit=8)
        fail.append("wanted pr_api_failure overflow, got pass")
    except GateError as exc:
        if exc.reason != "pr_api_failure":
            fail.append(f"wanted pr_api_failure overflow, got {exc.reason}")
    try:
        bounded_json(lambda n: b"\xff\xfe", "comments_api_failure", limit=64)
        fail.append("wanted comments_api_failure decode, got pass")
    except GateError as exc:
        if exc.reason != "comments_api_failure":
            fail.append(f"wanted comments_api_failure decode, got {exc.reason}")

    class _Wired:
        def __enter__(self):
            return self

        def __exit__(self, *_a):
            return False

        def read(self, n: int) -> bytes:
            reads.append(n)
            return b"{}"

    reads, timeouts = [], []

    def _open(_req, timeout=None):
        timeouts.append(timeout)
        return _Wired()

    wired, urllib.request.urlopen = urllib.request.urlopen, _open
    try:
        GitHubApi("o/r", "t").get("/pulls/1")
    finally:
        urllib.request.urlopen = wired
    if timeouts != [HTTP_TIMEOUT_S] or reads != [MAX_RESPONSE_BYTES + 1]:
        fail.append(f"wired get bounds timeout={timeouts!r} read={reads!r}")

    class _Race(GitHubApi):
        n = 0

        def get(self, path: str) -> Any:
            if "comments" in path:
                return []
            type(self).n += 1
            sha = ("a" * 40) if type(self).n == 1 else ("b" * 40)
            return {"head": {"sha": sha, "ref": "ruley/x"}}

    _Race.n = 0
    try:
        live_check(1, api=_Race("o/r", "t"))
        fail.append("wanted head_moved, got pass")
    except GateError as exc:
        if exc.reason != "head_moved":
            fail.append(f"wanted head_moved, got {exc.reason}")

    class _RaceRef(GitHubApi):
        n = 0

        def get(self, path: str) -> Any:
            if "comments" in path:
                return [_cmt(green)]
            type(self).n += 1
            ref = "ruley/x" if type(self).n == 1 else "ruley/y"
            return {"head": {"sha": "a" * 40, "ref": ref}}

    _RaceRef.n = 0
    try:
        live_check(1, api=_RaceRef("o/r", "t"))
        fail.append("wanted head_moved same-sha, got pass")
    except GateError as exc:
        if exc.reason != "head_moved":
            fail.append(f"wanted head_moved same-sha, got {exc.reason}")

    try:
        head_fields({"head": {"sha": None, "ref": "feature/fix"}})
        fail.append("wanted pr_api_failure, got pass")
    except GateError as exc:
        if exc.reason != "pr_api_failure":
            fail.append(f"wanted pr_api_failure, got {exc.reason}")
    if fail:
        print("self-test=failed", file=sys.stderr)
        print("\n".join(f"  {x}" for x in fail), file=sys.stderr)
        return 1
    print("assay-review-record-check self-test=passed")
    return 0


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "--self-test":
        return self_test()
    if len(sys.argv) >= 3 and sys.argv[1] == "--pr":
        try:
            return live_check(int(sys.argv[2]))
        except GateError as exc:
            print(f"review-record-check=fail reason={exc.reason} {exc.detail}", file=sys.stderr)
            return 1
    print("usage: assay_review_record_check.py --self-test | --pr N", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
