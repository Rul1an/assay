#!/usr/bin/env python3
"""Exact-head review-record checker used by the Slice B advisory workflow.

When no record names the live head, the checker re-derives the AGENTS.md carry conditions
from the commits themselves rather than reading them out of a posted record's prose; see
`derive_carry`, which quotes the contract it answers.
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from datetime import datetime
from typing import Any

MARKER = "<!-- assay-review-record -->"
SCHEMA = "assay.review-record.v0"
CHECKER = "scripts/ci/assay_review_record_check.py"
HOOK_ID = "assay-review-record-self-test"
PREFIXES = frozenset({"codex", "claude", "cursor", "ruley"})
HEX40 = re.compile(r"^[0-9a-f]{40}$")
IDENTITY_COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:@+-]{0,127}$")
FENCE = re.compile(r"^```(?:json)?\n(.*)\n```$", re.S)
HTTP_TIMEOUT_S = 30
MAX_RESPONSE_BYTES = 8 * 1024 * 1024
COMMENT_PAGE_SIZE = 100
COMMENT_PAGE_MAX = 2
REMOTE = "origin"
GIT_TIMEOUT_S = 60
GIT_FETCH_TIMEOUT_S = 180
# `-C <root>` names the repository to answer about, and these override it. An inherited one
# points git at somebody else's checkout: the pre-commit hook exports GIT_DIR and GIT_INDEX_FILE,
# so a derivation run from there would read the wrong objects rather than none.
GIT_SCOPE_VARS = ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
                  "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE")


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
    agent, instance = _need(obj, "agent", str), _need(obj, "instance", str)
    if not IDENTITY_COMPONENT.fullmatch(agent) or not IDENTITY_COMPONENT.fullmatch(instance):
        raise GateError("malformed_record", label)
    return agent, instance


def validate_record(
    record: dict[str, Any], *, live_sha: str, branch_ref: str, require_ready: bool = True
) -> None:
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
    if require_ready and verdict != "READY":
        raise GateError("blocked", verdict)


def _comment_id(value: Any) -> int | None:
    return value if type(value) is int and value > 0 else None


def _instant(value: Any) -> datetime:
    try:
        when = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
    except ValueError:
        when = None
    if when is None or when.tzinfo is None:
        raise GateError("missing_field", "created_at")
    return when


def _identity(obj: Any) -> tuple[Any, Any] | None:
    return (obj.get("agent"), obj.get("instance")) if isinstance(obj, dict) else None


def resolve_supersedes(
    entries: list[tuple[int | None, str, Any, dict[str, Any]]], *, live_sha: str, branch_ref: str
) -> tuple[set[int], dict[int, GateError]]:
    """Apply each record's `supersedes`; the one rule the gate and the landing helper share.

    `entries` holds (comment id, author login, created_at, record) for every parseable carrier
    naming the live head. Returns the positions retired and, per position, why a declared
    supersede was refused. A refused supersede retires nothing, so its target stays current.
    """
    at = {cid: i for i, (cid, _login, _created, _record) in enumerate(entries) if cid is not None}
    retired: set[int] = set()
    refused: dict[int, GateError] = {}
    for i, (cid, login, created, record) in enumerate(entries):
        if "supersedes" not in record:
            continue
        try:
            target = _comment_id(record["supersedes"])
            if target is None:
                raise GateError("malformed_record", "supersedes")
            if cid is None:
                raise GateError("missing_field", "id")
            validate_record(record, live_sha=live_sha, branch_ref=branch_ref, require_ready=False)
            if target not in at:
                raise GateError("supersede_refused", f"{target} is not a current-head record")
            _tid, t_login, t_created, t_record = entries[at[target]]
            if target >= cid or _instant(t_created) > _instant(created):
                raise GateError("supersede_refused", f"{target} is not older than {cid}")
            reviewer = _identity(record["reviewer"])
            if t_login != login or _identity(t_record.get("reviewer")) != reviewer:
                raise GateError("supersede_refused", f"{target} is another reviewer's record")
            if _identity(t_record.get("builder")) == reviewer:
                raise GateError("supersede_refused", f"{target} declares this reviewer as builder")
        except GateError as exc:
            refused[i] = exc
            continue
        retired.add(at[target])
    return retired, refused


class Git:
    """Bounded `git` access to one checkout: the only subprocess this checker runs."""

    def __init__(self, root: str, timeout: float = GIT_TIMEOUT_S,
                 fetch_timeout: float = GIT_FETCH_TIMEOUT_S) -> None:
        self.root, self.timeout, self.fetch_timeout = root, timeout, fetch_timeout

    def run(self, *args: str, timeout: float | None = None) -> tuple[int, str]:
        try:
            done = subprocess.run(  # noqa: S603 - fixed argv, no shell
                ["git", "-C", self.root, *args], capture_output=True, text=True,
                env={key: value for key, value in os.environ.items()
                     if key not in GIT_SCOPE_VARS},
                timeout=self.timeout if timeout is None else timeout, check=False)
        except (OSError, subprocess.SubprocessError) as exc:
            raise GateError("carry_objects_unavailable", f"git {args[0]}: {exc}") from exc
        return done.returncode, done.stdout.strip()

    def ok(self, *args: str) -> bool:
        return self.run(*args)[0] == 0

    def line(self, *args: str) -> str:
        """One nonempty line, or the objects were not obtainable after all."""
        code, out = self.run(*args)
        if code != 0 or not out:
            raise GateError("carry_objects_unavailable", " ".join(args))
        return out.splitlines()[0].strip()

    def names(self, *args: str) -> list[str]:
        code, out = self.run(*args)
        if code != 0:
            raise GateError("carry_objects_unavailable", " ".join(args))
        return [name for name in out.splitlines() if name]

    def shallow(self) -> bool:
        return self.run("rev-parse", "--is-shallow-repository")[1] == "true"

    def ensure(self, *shas: str) -> None:
        """Obtain each commit and enough history around it to answer ancestry exactly.

        The workflow checks out the PR base at depth 1 from the public `origin` with no
        credentials, so both the commits and the graft boundary are missing. Fetching a sha
        brings the commit; `--unshallow` on the same bounded fetch removes the boundary that
        would otherwise let `merge-base` answer from truncated history — a wrong answer, not
        a missing one.
        """
        if self.shallow():
            self.run("fetch", "--no-tags", "--unshallow", REMOTE, *shas,
                     timeout=self.fetch_timeout)
        for sha in shas:
            if not self.ok("cat-file", "-e", f"{sha}^{{commit}}"):
                self.run("fetch", "--no-tags", REMOTE, sha, timeout=self.fetch_timeout)
            if not self.ok("cat-file", "-e", f"{sha}^{{commit}}"):
                raise GateError("carry_objects_unavailable", sha)
        if self.shallow():
            raise GateError("carry_objects_unavailable", "history is still shallow")


def _require_ancestor(git: Git, reviewed: str, live: str) -> None:
    """Refuse a live head whose history does not contain the reviewed head.

    Reached on its own terms only when the first-parent check above is loosened: a two-parent
    head whose first parent already contains `reviewed` contains it too. It stays as the
    direct statement of "what was reviewed is still in what lands", and it is tested directly.
    """
    if not git.ok("merge-base", "--is-ancestor", reviewed, live):
        raise GateError("carry_not_ancestor", f"{reviewed} is not an ancestor of {live}")


def derive_carry(git: Git, reviewed: str, live: str) -> str:
    """Re-derive both AGENTS.md carry conditions from the commits. Quoting that contract:

        A review is revalidated for a new head only by a recorded equivalence check with two
        conditions: the new head introduces no change, to any file, that is not already on
        `main` - its tree is what merging `main` into the reviewed head produces without
        conflicts - and the advance from the reviewed head to the new head touched no file the
        review covered, meaning the PR's changed files as of the reviewed head. [...] Rewritten
        history (rebase, squash) does not carry a review even when the tree is identical:
        revalidation is for upstream advances only.

    `main` is read as the commit the advance actually merged, which is the live head's second
    parent; a rewritten history has no such parent and is refused before any tree is compared.
    Nothing here reads the record: a record that merely claims a carry gets no credit.
    """
    git.ensure(reviewed, live)
    parents = git.line("rev-list", "--parents", "-n", "1", live).split()[1:]
    if len(parents) != 2:
        raise GateError("carry_not_upstream_merge", f"{live} has {len(parents)} parent(s)")
    first, second = parents
    if not git.ok("merge-base", "--is-ancestor", reviewed, first):
        raise GateError("carry_not_upstream_merge", f"{reviewed} is not in {first}'s history")
    _require_ancestor(git, reviewed, live)
    code, merged = git.run("merge-tree", "--write-tree", second, reviewed)
    if code == 1:
        raise GateError("carry_merge_conflict", f"merging {second} into {reviewed} conflicts")
    if code != 0 or not merged:
        raise GateError("carry_objects_unavailable", f"merge-tree exit {code}")
    produced, landed = merged.splitlines()[0].strip(), git.line("rev-parse", f"{live}^{{tree}}")
    if produced != landed:
        raise GateError("carry_tree_mismatch", f"{produced} != {landed}")
    base = git.line("merge-base", reviewed, second)
    # `--no-renames` on both sides: rename detection reports only a rename's new name, so an
    # upstream rename of a reviewed file would leave the intersection empty and carry a path
    # whose content the review never saw at that name. A rename is a touch.
    reviewed_files = git.names("diff", "--no-renames", "--name-only", base, reviewed)
    advance_files = git.names("diff", "--no-renames", "--name-only", reviewed, live)
    touched = sorted(set(reviewed_files) & set(advance_files))
    if touched:
        raise GateError("carry_touched_reviewed_file", ", ".join(touched))
    return (f"review-record-carry=derived reviewed={reviewed} merged={second} "
            f"conditions=tree-equivalence+no-overlap-re-derived-by-checker "
            f"reviewed_files={len(reviewed_files)} advance_files={len(advance_files)}")


def _carry_candidates(live_sha: str, comments: list[dict[str, Any]]) -> list[str]:
    """Reviewed heads named by records on this PR, newest carrier first, live head excluded."""
    seen: dict[str, int] = {}
    for position, comment in enumerate(comments):
        if not isinstance(comment, dict):
            continue
        body = str(comment.get("body") or "")
        if MARKER not in body:
            continue
        try:
            record = extract_record(body)
        except GateError:
            continue
        if not isinstance(record, dict):
            continue
        sha = str(record.get("head_sha") or "").lower()
        if HEX40.match(sha) and sha != live_sha.lower():
            seen[sha] = position
    return sorted(seen, key=lambda sha: seen[sha], reverse=True)


def _current_sha(body: str, record: dict[str, Any] | None, live: str) -> bool:
    if record and str(record.get("head_sha") or "").lower() == live.lower():
        return True
    return live.lower() in body.lower()


def _evaluate_head(live_sha: str, branch_ref: str, comments: list[dict[str, Any]]) -> None:
    entries: list[tuple[int | None, str, Any, dict[str, Any]]] = []
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
        entries.append((_comment_id(comment.get("id")), login, created, record))
    retired, refused = resolve_supersedes(entries, live_sha=live_sha, branch_ref=branch_ref)
    if refused:
        raise refused[min(refused)]
    current = [record for i, (_cid, _login, _created, record) in enumerate(entries) if i not in retired]
    if not current:
        raise GateError("no_current_record", live_sha)
    if len(current) != 1:
        raise GateError("ambiguous_current", str(len(current)))
    validate_record(current[0], live_sha=live_sha, branch_ref=branch_ref)


def evaluate(
    live_sha: str, branch_ref: str, comments: list[dict[str, Any]], *, git_root: str | None = None
) -> str | None:
    """Judge the live head; fall back to a carry the checker derives itself.

    A record naming the live head is judged exactly as before, pass or fail, and its own text
    never claims a carry into existence. Only when no record names the live head does the
    newest still-valid READY record on an earlier head get tested against the commits, and
    every rule that record had to satisfy on its own head it still has to satisfy here.
    Returns the derived-carry line when one was derived, else `None`.
    """
    try:
        _evaluate_head(live_sha, branch_ref, comments)
        return None
    except GateError as exc:
        if exc.reason != "no_current_record" or git_root is None:
            raise
    for sha in _carry_candidates(live_sha, comments):
        try:
            _evaluate_head(sha, branch_ref, comments)
        except GateError:
            continue
        return derive_carry(Git(git_root), sha, live_sha.lower())
    raise GateError("no_current_record", live_sha)


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
    carried = evaluate(sha2, ref2, comments, git_root=_root())
    if carried:
        print(carried)
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
         user_type="User", created="t0", updated="keep", cid=None):
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
    if cid is not None:
        row["id"] = cid
    if created is not None:
        row["created_at"] = created
    if updated is not None:
        row["updated_at"] = updated
    return row


def _fixture_env() -> dict[str, str]:
    """A hermetic, deterministic environment for the self-test's throwaway repositories."""
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update({
        "GIT_AUTHOR_NAME": "assay-self-test", "GIT_AUTHOR_EMAIL": "self-test@assay.invalid",
        "GIT_COMMITTER_NAME": "assay-self-test", "GIT_COMMITTER_EMAIL": "self-test@assay.invalid",
        "GIT_AUTHOR_DATE": "2026-01-01T00:00:00+0000",
        "GIT_COMMITTER_DATE": "2026-01-01T00:00:00+0000",
        "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_TERMINAL_PROMPT": "0", "GIT_ALLOW_PROTOCOL": "none",
    })
    return env


def _fx(root: str, *args: str, allow_fail: bool = False) -> str:
    done = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", root, *args], env=_fixture_env(), capture_output=True, text=True,
        timeout=GIT_TIMEOUT_S, check=False)
    if done.returncode != 0 and not allow_fail:
        raise RuntimeError(f"fixture `git {' '.join(args)}` failed: {done.stderr.strip()}")
    return done.stdout.strip()


def _fx_write(root: str, name: str, text: str) -> None:
    with open(os.path.join(root, name), "w", encoding="utf-8") as handle:
        handle.write(text)


def _fx_commit(root: str, message: str, files: dict[str, str]) -> str:
    for name, text in files.items():
        _fx_write(root, name, text)
    _fx(root, "add", "--", *files)
    _fx(root, "commit", "-q", "-m", message)
    return _fx(root, "rev-parse", "HEAD")


def _carry_repo(root: str) -> dict[str, str]:
    """One repository holding a reviewed head and every advance shape the carry rules judge."""
    rows = [f"line {index}\n" for index in range(1, 31)]

    def shared(index: int | None = None, text: str = "") -> str:
        edited = list(rows)
        if index is not None:
            edited[index] = text
        return "".join(edited)

    _fx(root, "-c", "init.defaultBranch=main", "init", "-q")
    _fx(root, "config", "core.autocrlf", "false")
    base = _fx_commit(root, "base", {"main.txt": "m0\n", "shared.txt": shared()})
    _fx(root, "checkout", "-q", "-b", "work")
    reviewed = _fx_commit(root, "reviewed work", {
        "feature.txt": "feature\n", "shared.txt": shared(0, "line 1 from the branch\n")})
    _fx(root, "checkout", "-q", "-b", "up-clean", base)
    clean = _fx_commit(root, "upstream adds an untouched file", {"other.txt": "other\n"})
    _fx(root, "checkout", "-q", "-b", "up-overlap", base)
    overlap = _fx_commit(root, "upstream edits a reviewed file", {
        "shared.txt": shared(29, "line 30 from main\n")})
    _fx(root, "checkout", "-q", "-b", "up-conflict", base)
    conflict = _fx_commit(root, "upstream edits a reviewed line", {
        "shared.txt": shared(0, "line 1 from main\n")})
    # A rename is a touch: rename detection would report only the new name, so the old name
    # would drop out of the advance and the carry would take content the review never saw.
    _fx(root, "checkout", "-q", "-b", "up-rename", base)
    _fx(root, "mv", "shared.txt", "moved.txt")
    _fx(root, "commit", "-q", "-m", "upstream renames a reviewed file")
    rename = _fx(root, "rev-parse", "HEAD")
    _fx(root, "checkout", "-q", "-b", "up-rename-edit", base)
    _fx(root, "mv", "shared.txt", "moved.txt")
    _fx_write(root, "moved.txt", shared(29, "line 30 from main after the rename\n"))
    _fx(root, "add", "--", "moved.txt")
    _fx(root, "commit", "-q", "-m", "upstream renames and edits a reviewed file")
    rename_edit = _fx(root, "rev-parse", "HEAD")

    def merged(branch: str, other: str) -> str:
        _fx(root, "checkout", "-q", "-b", branch, "work")
        _fx(root, "merge", "-q", "--no-ff", "-m", "Merge main into work", other)
        return _fx(root, "rev-parse", "HEAD")

    heads = {"base": base, "reviewed": reviewed, "clean": clean,
             "live_clean": merged("live-clean", clean),
             "live_overlap": merged("live-overlap", overlap),
             "live_rename": merged("live-rename", rename),
             "live_rename_edit": merged("live-rename-edit", rename_edit)}
    _fx(root, "checkout", "-q", "-b", "live-conflict", "work")
    _fx(root, "merge", "--no-ff", "-m", "Merge main into work", conflict, allow_fail=True)
    _fx_write(root, "shared.txt", shared(0, "line 1 resolved by hand\n"))
    _fx(root, "add", "--", "shared.txt")
    _fx(root, "commit", "-q", "--no-edit")
    heads["live_conflict"] = _fx(root, "rev-parse", "HEAD")
    _fx(root, "checkout", "-q", "-b", "live-tree", heads["live_clean"])
    _fx_write(root, "sneak.txt", "never on main\n")
    _fx(root, "add", "--", "sneak.txt")
    _fx(root, "commit", "-q", "--amend", "--no-edit")
    heads["live_tree"] = _fx(root, "rev-parse", "HEAD")
    _fx(root, "checkout", "-q", "-b", "live-push", "work")
    heads["live_push"] = _fx_commit(root, "one more push", {"feature.txt": "feature again\n"})
    _fx(root, "checkout", "-q", "-b", "live-rebase", clean)
    _fx(root, "cherry-pick", reviewed)
    heads["live_rebase"] = _fx(root, "rev-parse", "HEAD")
    _fx(root, "checkout", "-q", "-b", "live-backward", clean)
    _fx(root, "merge", "-q", "--no-ff", "-m", "Merge work into main", "work")
    heads["live_backward"] = _fx(root, "rev-parse", "HEAD")
    _fx(root, "checkout", "-q", "--orphan", "lonely")
    _fx(root, "reset", "-q")
    heads["unrelated"] = _fx_commit(root, "unrelated root", {"lonely.txt": "alone\n"})
    return heads


def self_test() -> int:
    live, ref, green = "a" * 40, "ruley/2561-review-record-slice1", _rec()
    fail: list[str] = []

    def result(thunk: Any) -> tuple[str, str]:
        # A crash is a result too: it comes back as its exception type, so the case that caused it
        # is named in a failure line instead of ending the self-test in a traceback naming none.
        try:
            note = thunk()
        except GateError as exc:
            return exc.reason, exc.detail
        except Exception as exc:  # noqa: BLE001 - reported as the case's result, never passed
            return type(exc).__name__, str(exc)
        return "pass", str(note or "")

    def outcome(sha: str, branch: str, comments: list[dict[str, Any]]) -> tuple[str, str]:
        return result(lambda: evaluate(sha, branch, comments))

    def expect(reason: str, sha: str, branch: str, comments: list[dict[str, Any]]) -> None:
        got, _detail = outcome(sha, branch, comments)
        if got != reason:
            fail.append(f"wanted {reason}, got {got}")

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

    # Same-head supersede. `bad` is the PR #2896 shape: parseable, but its findings carry
    # claim/status keys, so validate_record() refuses it while it still names the live head.
    def at(minute: int) -> str:
        return f"2026-09-10T17:{minute:02d}:00Z"

    def by(record: dict[str, Any], cid: int, minute: int, **kw: Any) -> dict[str, Any]:
        return _cmt(record, cid=cid, created=at(minute), **kw)

    bad = _rec(findings=[{"claim": 1, "status": "holds"}], no_findings=False)
    fix = _rec(supersedes=101)
    other = {"agent": "codex", "instance": "r2", "github_login": "Rul1an"}
    found = [{"id": "F1", "summary": "x", "disposition": "fixed"}]
    greens = [
        ("supersede a malformed same-head record", [by(bad, 101, 11), by(fix, 102, 19)]),
        ("supersede a valid BLOCKED record", [by(_rec(verdict="BLOCKED"), 101, 11), by(fix, 102, 19)]),
        ("supersede chain", [by(bad, 101, 11), by(_rec(verdict="BLOCKED", findings=found,
                             no_findings=False, supersedes=101), 102, 12), by(_rec(supersedes=102), 103, 13)]),
        ("same-second repost ordered by id", [by(bad, 101, 11), by(fix, 102, 11)]),
        ("older-head history is not current", [by(_rec(head_sha="b" * 40), 100, 5), by(green, 101, 11)]),
    ]
    for label, comments in greens:
        got, detail = outcome(live, ref, comments)
        if got != "pass":
            fail.append(f"GREEN {label}: {got} {detail}")

    self_review = _rec(builder={"agent": "ruley", "instance": "w1"},
                       reviewer={"agent": "ruley", "instance": "w1", "github_login": "Rul1an"})
    as_builder = {"agent": "ruley", "instance": "w1", "github_login": "Rul1an"}
    supersede_reds = [
        ("different reviewers stay ambiguous", "ambiguous_current",
         [by(green, 101, 11), by(_rec(reviewer=other), 102, 19)]),
        ("different reviewer cannot supersede", "supersede_refused",
         [by(green, 101, 11), by(_rec(reviewer=other, supersedes=101), 102, 19)]),
        ("different github login cannot supersede", "supersede_refused",
         [by(_rec(reviewer={"agent": "cursor", "instance": "r1", "github_login": "Other"}), 101, 11,
             login="Other"), by(fix, 102, 19)]),
        ("builder as reviewer cannot supersede", "identical_writer_reviewer",
         [by(bad, 101, 11), by(_rec(reviewer=as_builder, supersedes=101), 102, 19)]),
        ("builder under another reviewer identity cannot supersede", "supersede_refused",
         [by(bad, 101, 11), by(_rec(builder={"agent": "ruley", "instance": "w2"}, reviewer=as_builder,
                                    supersedes=101), 102, 19)]),
        ("builder cannot supersede its own self-review", "supersede_refused",
         [by(self_review, 101, 11), by(_rec(builder={"agent": "ruley", "instance": "w2"},
                                            reviewer=as_builder, supersedes=101), 102, 19)]),
        ("non-existent target", "supersede_refused", [by(bad, 101, 11), by(_rec(supersedes=999), 102, 19)]),
        ("newer target", "supersede_refused", [by(_rec(supersedes=102), 101, 11), by(bad, 102, 19)]),
        ("self target", "supersede_refused", [by(_rec(supersedes=101), 101, 11)]),
        ("target created later despite lower id", "supersede_refused", [by(bad, 101, 30), by(fix, 102, 19)]),
        ("older-head target", "supersede_refused",
         [by(_rec(head_sha="b" * 40), 101, 11), by(fix, 102, 19)]),
        ("non-record target", "supersede_refused",
         [{"id": 101, "body": "looks good", "user": {"login": "Rul1an", "type": "User"},
           "created_at": at(11), "updated_at": at(11)}, by(fix, 102, 19)]),
        ("invalid superseding record", "missing_disposition",
         [by(bad, 101, 11), by(_rec(findings=[{"claim": 1}], no_findings=False, supersedes=101), 102, 19)]),
        ("invalid record in a chain retires nothing", "missing_disposition",
         [by(bad, 101, 11), by(_rec(findings=[{"claim": 1}], no_findings=False, supersedes=101), 102, 12),
          by(_rec(supersedes=102), 103, 13)]),
        ("superseding record without comment id", "missing_field", [by(bad, 101, 11), _cmt(fix, created=at(19))]),
        ("timestamp without a zone", "missing_field", [_cmt(bad, cid=101, created="2026-09-10T17:11:00"),
                                                       by(fix, 102, 19)]),
        ("unparsable timestamp", "missing_field", [_cmt(bad, cid=101, created="t0"), by(fix, 102, 19)]),
        ("edited superseded record", "edited_current", [by(bad, 101, 11, edited=True), by(fix, 102, 19)]),
        ("edited superseding record", "edited_current", [by(bad, 101, 11), by(fix, 102, 19, edited=True)]),
        ("bot carrier cannot be superseded", "bot_carrier", [by(bad, 101, 11, bot=True), by(fix, 102, 19)]),
        ("login-mismatched carrier cannot be superseded", "login_mismatch",
         [by(_rec(reviewer={"agent": "cursor", "instance": "r1", "github_login": "Typo"}), 101, 11),
          by(fix, 102, 19)]),
        ("unparsable carrier cannot be superseded", "extra_prose",
         [_cmt(bad, cid=101, created=at(11), extra="\nsorry\n"), by(fix, 102, 19)]),
        ("two supersedes of one target", "ambiguous_current",
         [by(bad, 101, 11), by(fix, 102, 19), by(fix, 103, 20)]),
        ("superseding BLOCKED still fails", "blocked",
         [by(green, 101, 11), by(_rec(verdict="BLOCKED", supersedes=101), 102, 19)]),
    ] + [
        (f"supersedes={value!r}", "malformed_record", [by(bad, 101, 11), by(_rec(supersedes=value), 102, 19)])
        for value in ("101", True, 0, -1, 101.0, None, [101])
    ]
    for label, reason, comments in supersede_reds:
        got, detail = outcome(live, ref, comments)
        if got != reason:
            fail.append(f"{label}: wanted {reason}, got {got} {detail}".rstrip())

    # Derived carry. Every case builds a real repository, so what is judged is what git answers
    # about real commits; no record's text is consulted for a condition anywhere below.
    tmp = tempfile.mkdtemp(prefix="assay-review-record-carry-")
    try:
        at = _carry_repo(tmp)
        reviewed, live = at["reviewed"], at["live_clean"]

        def carried(head: str, *records: dict[str, Any]) -> Any:
            return lambda: evaluate(head, ref, list(records), git_root=tmp)

        def posted(head: str, **over: Any) -> dict[str, Any]:
            return _cmt(_rec(head_sha=head, **over))

        older = posted(reviewed)
        claims = posted(live, carry={"conditions_hold": True, "note": "both conditions checked"})
        carry_cases = [
            ("an upstream-advance merge carries", "pass", carried(live, older)),
            ("an unobtainable reviewed head", "carry_objects_unavailable",
             carried(live, posted("e" * 40))),
            ("a further push is not an upstream merge", "carry_not_upstream_merge",
             carried(at["live_push"], older)),
            ("a rebase does not carry", "carry_not_upstream_merge",
             carried(at["live_rebase"], older)),
            ("a merge taken on the main side does not carry", "carry_not_upstream_merge",
             carried(at["live_backward"], older)),
            ("a hand-resolved conflict does not carry", "carry_merge_conflict",
             carried(at["live_conflict"], older)),
            ("an amended merge does not carry", "carry_tree_mismatch",
             carried(at["live_tree"], older)),
            ("an upstream edit to a reviewed file does not carry", "carry_touched_reviewed_file",
             carried(at["live_overlap"], older)),
            ("an upstream rename of a reviewed file does not carry",
             "carry_touched_reviewed_file", carried(at["live_rename"], older)),
            ("an upstream rename that also edits a reviewed file does not carry",
             "carry_touched_reviewed_file", carried(at["live_rename_edit"], older)),
            ("a head outside the reviewed history is refused", "carry_not_ancestor",
             lambda: _require_ancestor(Git(tmp), at["unrelated"], live)),
            ("a BLOCKED record does not carry", "no_current_record",
             carried(live, posted(reviewed, verdict="BLOCKED"))),
            ("an edited record does not carry", "no_current_record",
             carried(live, _cmt(_rec(head_sha=reviewed), edited=True))),
            ("a self-reviewed record does not carry", "no_current_record",
             carried(live, posted(reviewed, reviewer={
                 "agent": "ruley", "instance": "w1", "github_login": "Rul1an"}))),
            ("a bot-carried record does not carry", "no_current_record",
             carried(live, _cmt(_rec(head_sha=reviewed), bot=True))),
            ("no record at all still fails as before", "no_current_record", carried(live)),
            ("a live-head record is judged as before", "pass", carried(live, claims, older)),
            ("a live-head record's carry claim earns nothing", "blocked",
             carried(live, posted(live, verdict="BLOCKED"), older)),
        ]
        for label, reason, thunk in carry_cases:
            got, detail = result(thunk)
            if got != reason:
                fail.append(f"carry: {label}: wanted {reason}, got {got} {detail}".rstrip())
        _got, note = result(carried(live, older))
        wanted = (f"reviewed={reviewed}", f"merged={at['clean']}", "re-derived-by-checker",
                  "reviewed_files=2", "advance_files=1")
        for token in wanted:
            if token not in note:
                fail.append(f"carry: derived line omits {token}: {note!r}")
        _got, quiet = result(carried(live, claims, older))
        if quiet:
            fail.append(f"carry: a live-head record derived a carry line: {quiet!r}")
        decoy, os.environ["GIT_DIR"] = os.environ.get("GIT_DIR"), os.path.join(tmp, "not-a-git-dir")
        try:
            got, detail = result(carried(live, older))
            if got != "pass":
                fail.append(f"carry: an inherited GIT_DIR redirected the derivation: {got} {detail}")
        finally:
            if decoy is None:
                del os.environ["GIT_DIR"]
            else:
                os.environ["GIT_DIR"] = decoy
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if (HTTP_TIMEOUT_S, MAX_RESPONSE_BYTES, COMMENT_PAGE_SIZE, COMMENT_PAGE_MAX) != (30, 8 * 1024 * 1024, 100, 2):
        fail.append("API bound constants drifted")
    if (GIT_TIMEOUT_S, GIT_FETCH_TIMEOUT_S, REMOTE) != (60, 180, "origin"):
        fail.append("git bound constants drifted")
    pc = open(os.path.join(_root(), ".pre-commit-config.yaml"), encoding="utf-8").read()
    if HOOK_ID not in pc or f"{CHECKER} --self-test" not in pc:
        fail.append("pre-commit hook not registered")
    if "review-record-check\\.yml" not in pc:
        fail.append("review-record workflow is not covered by pre-commit")
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
