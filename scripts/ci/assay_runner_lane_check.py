#!/usr/bin/env python3
"""Enforce the Assay-Runner delegated CI lane contract for pull requests.

The script is intentionally stdlib-only so the GitHub workflow can run it from
the base branch without installing repository dependencies or executing PR code.
"""

from __future__ import annotations

import argparse
import http.client
import json
import os
import re
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from enum import IntEnum
from functools import lru_cache
from pathlib import Path
from typing import Iterable


CONTRACT_DOC = "docs/reference/runner/ci-lanes.md"
DEPENDABOT_FLOW_DOC = "docs/reference/runner/dependabot-lane-flow.md"
GATED_PATHS_DOC = "scripts/ci/assay_runner_gated_paths.json"
DELEGATED_WORKFLOW_NAME = "Runner Spike Delegated"
COMMENT_MARKER = "<!-- assay-runner-lane-check -->"
STATUS_CONTEXT = "lane-check/proof"
# 404 is retryable here because GitHub can briefly return it for freshly
# created PR metadata endpoints such as /pulls/{number}/files.
RETRYABLE_HTTP_CODES = {404, 502, 503, 504}
HTTP_RETRY_ATTEMPTS = 3
# Connection-level failures urllib/http.client can raise on a transient GitHub
# API blip (network reset, DNS hiccup, the remote closing the socket without a
# response). http.client.RemoteDisconnected -- the observed
# "Remote end closed connection without response" crash -- is a
# ConnectionResetError *and* an http.client.HTTPException, which makes it a
# sibling of urllib.error.URLError under OSError, not a subclass: catching
# URLError alone does not cover it. socket.timeout aliases TimeoutError on
# Python 3.10+ but is a distinct class on older runtimes, so list both.
TRANSIENT_REQUEST_ERRORS = (
    urllib.error.URLError,
    http.client.HTTPException,
    ConnectionError,
    TimeoutError,
    socket.timeout,
)


class Gate(IntEnum):
    NONE = 0
    KERNEL_ONLY = 1
    KERNEL_POLICY = 2
    OPENAI_AGENTS_KERNEL_POLICY = 3
    ALL = 4

    @property
    def label(self) -> str:
        return {
            Gate.NONE: "none",
            Gate.KERNEL_ONLY: "kernel-only",
            Gate.KERNEL_POLICY: "kernel-policy",
            Gate.OPENAI_AGENTS_KERNEL_POLICY: "openai-agents-kernel-policy",
            Gate.ALL: "all",
        }[self]


@dataclass(frozen=True)
class Classification:
    gate: Gate
    reasons: tuple[str, ...]


@dataclass(frozen=True)
class PullRequest:
    number: int
    title: str
    body: str
    author_login: str
    head_sha: str
    files: tuple[str, ...]


@dataclass(frozen=True)
class GatedPathConfig:
    all_gate_prefixes: tuple[str, ...]
    all_gate_paths: frozenset[str]


class GitHubApi:
    def __init__(self, repo: str, token: str) -> None:
        self.repo = repo
        self.base_url = f"https://api.github.com/repos/{repo}"
        self.token = token

    def request(self, method: str, path: str, payload: object | None = None) -> object:
        url = path if path.startswith("https://") else f"{self.base_url}{path}"
        headers = {
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Authorization": f"Bearer {self.token}",
        }
        data = None
        if payload is not None:
            data = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        with urlopen_with_retry(request) as response:
            body = response.read().decode("utf-8")
            return json.loads(body) if body else {}

    def paginated(self, path: str) -> list[object]:
        separator = "&" if "?" in path else "?"
        url = f"{self.base_url}{path}{separator}per_page=100"
        results: list[object] = []
        while url:
            request = urllib.request.Request(
                url,
                headers={
                    "Accept": "application/vnd.github+json",
                    "X-GitHub-Api-Version": "2022-11-28",
                    "Authorization": f"Bearer {self.token}",
                },
            )
            with urlopen_with_retry(request) as response:
                page = json.loads(response.read().decode("utf-8"))
                if not isinstance(page, list):
                    raise TypeError(f"Expected list response from {url}")
                results.extend(page)
                url = next_link(response.headers.get("Link", ""))
        return results


def urlopen_with_retry(request: urllib.request.Request):
    last_error: BaseException | None = None
    attempts = HTTP_RETRY_ATTEMPTS if request.get_method() == "GET" else 1
    for attempt in range(attempts):
        try:
            return urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as exc:
            last_error = exc
            # HTTPError is file-like and may hold a connection; close it before
            # retrying or re-raising so repeated transient failures do not leak.
            exc.close()
            if exc.code not in RETRYABLE_HTTP_CODES or attempt == attempts - 1:
                raise
        except TRANSIENT_REQUEST_ERRORS as exc:
            # urllib.error.HTTPError is caught above (it is a URLError subclass
            # but needs the status-code-aware branch); everything else here is
            # a connection-level blip worth retrying on a GET.
            last_error = exc
            if attempt == attempts - 1:
                raise
        time.sleep(1 + attempt)
    assert last_error is not None
    raise last_error


def next_link(header: str) -> str | None:
    for part in header.split(","):
        section = part.strip()
        if 'rel="next"' not in section:
            continue
        match = re.match(r"<([^>]+)>", section)
        if match:
            return match.group(1)
    return None


def starts(path: str, prefix: str) -> bool:
    return path == prefix.rstrip("/") or path.startswith(prefix.rstrip("/") + "/")


@lru_cache(maxsize=1)
def load_gated_path_config() -> GatedPathConfig:
    root = Path(__file__).resolve().parents[2]
    manifest_path = root / GATED_PATHS_DOC
    with manifest_path.open(encoding="utf-8") as handle:
        manifest = json.load(handle)
    return GatedPathConfig(
        all_gate_prefixes=tuple(str(path) for path in manifest["all_gate_prefixes"]),
        all_gate_paths=frozenset(str(path) for path in manifest["all_gate_paths"]),
    )


def classify_file(path: str) -> tuple[Gate, str | None]:
    gated_paths = load_gated_path_config()
    docs_runner_prefixes = (
        "docs/reference/runner/",
        "docs/notes/ASSAY-RUNNER-",
        "docs/ops/ASSAY-RUNNER-",
    )
    if any(path.startswith(prefix) for prefix in docs_runner_prefixes):
        return Gate.NONE, None

    if path in {
        ".github/workflows/assay-runner-lane-check.yml",
        "scripts/ci/assay_runner_lane_check.py",
    }:
        return Gate.NONE, None

    if any(starts(path, prefix) for prefix in gated_paths.all_gate_prefixes):
        return Gate.ALL, f"{path}: shared runner/eBPF/monitor/xtask code requires gates=all"

    if path in gated_paths.all_gate_paths:
        return Gate.ALL, f"{path}: runner workflow, CLI, cgroup, or workspace dependency surface requires gates=all"

    if path.startswith("scripts/ci/runner-spike-kernel-only-"):
        return Gate.KERNEL_ONLY, f"{path}: kernel-only acceptance surface requires gates=kernel-only"

    if path.startswith("scripts/ci/runner-spike-kernel-policy-"):
        return Gate.KERNEL_POLICY, f"{path}: kernel+policy acceptance surface requires gates=kernel-policy"

    if path.startswith("scripts/ci/runner-spike-openai-agents-kernel-policy-"):
        return (
            Gate.OPENAI_AGENTS_KERNEL_POLICY,
            f"{path}: OpenAI Agents kernel+policy acceptance surface requires gates=openai-agents-kernel-policy",
        )

    if path.startswith("scripts/ci/runner-spike-gemini-google-genai-"):
        # Gemini fixture runs under `gates=all` per #1307 non-goal: do not
        # introduce a new narrower delegated gate name. A future narrower
        # gate is a separate coordinated change (workflow inputs.gates enum
        # + ci-lanes.md + classifier).
        return Gate.ALL, f"{path}: Gemini google-genai acceptance surface requires gates=all"

    if path.startswith("scripts/ci/runner-spike-"):
        return Gate.ALL, f"{path}: shared runner-spike script requires gates=all"

    if path == "scripts/ci/assay_runner_delegated_proof_pack.py":
        return Gate.ALL, f"{path}: delegated proof-pack collector requires gates=all"

    if path == "tests/fixtures/runner-spike/kernel-only-agent.sh":
        return Gate.KERNEL_ONLY, f"{path}: kernel-only fixture requires gates=kernel-only"

    if path in {
        "tests/fixtures/runner-spike/mcp-policy-agent.sh",
        "tests/fixtures/runner-spike/mcp_file_server.py",
    }:
        return Gate.KERNEL_POLICY, f"{path}: policy fixture requires gates=kernel-policy"

    if path.startswith("runner-fixtures/openai-agents/"):
        # OpenAI Agents fixture moved from tests/fixtures/runner-spike/
        # openai-agents-js/ to runner-fixtures/openai-agents/ in Phase 2D
        # Slice 5B. The `-js` suffix was dropped because the fixture
        # identity is the runtime, not the implementation language.
        return (
            Gate.OPENAI_AGENTS_KERNEL_POLICY,
            f"{path}: OpenAI Agents fixture requires gates=openai-agents-kernel-policy",
        )

    if path.startswith("runner-fixtures/gemini-google-genai/"):
        # Gemini fixture surface runs under `gates=all` per #1307 non-goal:
        # no new narrower delegated gate name. The fixture moved from
        # `tests/fixtures/runner-spike/gemini-google-genai/` to
        # `runner-fixtures/gemini-google-genai/` in Phase 2D Slice 5A so
        # the fixture package is structured as a Runner-owned asset.
        return Gate.ALL, f"{path}: Gemini google-genai fixture requires gates=all"

    if path.startswith("runner-fixtures/"):
        # Top-level Runner-owned fixture package introduced in Phase 2D
        # Slice 5A. Any new fixture under this directory that has not been
        # given an explicit narrower rule above defaults to gates=all to
        # match the existing fixture-surface discipline.
        return Gate.ALL, f"{path}: runner-fixtures asset requires gates=all"

    if path.startswith("tests/fixtures/runner-spike/"):
        return Gate.ALL, f"{path}: ambiguous runner fixture surface defaults to gates=all"

    return Gate.NONE, None


def classify_files(files: Iterable[str]) -> Classification:
    gate = Gate.NONE
    reasons: list[str] = []
    for path in files:
        file_gate, reason = classify_file(path)
        if file_gate > gate:
            gate = file_gate
        if reason:
            reasons.append(reason)
    return Classification(gate=gate, reasons=tuple(reasons))


def accepted_gates(expected: Gate) -> set[str]:
    if expected == Gate.KERNEL_ONLY:
        return {"kernel-only", "all"}
    if expected == Gate.KERNEL_POLICY:
        return {"kernel-policy", "all"}
    if expected == Gate.OPENAI_AGENTS_KERNEL_POLICY:
        return {"openai-agents-kernel-policy", "all"}
    if expected == Gate.ALL:
        return {"all"}
    return set()


def load_pr(api: GitHubApi, number: int) -> PullRequest:
    try:
        pr = api.request("GET", f"/pulls/{number}")
        files = api.paginated(f"/pulls/{number}/files")
        return PullRequest(
            number=number,
            title=str(pr.get("title") or ""),
            body=str(pr.get("body") or ""),
            author_login=str((pr.get("user") or {}).get("login") or ""),
            head_sha=str(pr["head"]["sha"]),
            files=tuple(str(item["filename"]) for item in files),
        )
    except TRANSIENT_REQUEST_ERRORS as exc:
        fallback = load_pr_from_event_and_git(number)
        if fallback is not None:
            print(
                f"warning: could not read GitHub PR metadata; "
                f"classifying changed files from local git diff fallback: {exc}",
                file=sys.stderr,
            )
            return fallback
        raise


def resolve_pr_number_from_event(api: GitHubApi) -> str | None:
    event_name = os.environ.get("GITHUB_EVENT_NAME", "")
    event_path = os.environ.get("GITHUB_EVENT_PATH", "")
    if not event_path:
        return None
    try:
        with open(event_path, encoding="utf-8") as handle:
            event = json.load(handle)
    except (OSError, json.JSONDecodeError):
        return None

    if event_name == "pull_request":
        raw_pr = event.get("pull_request") or {}
        number = raw_pr.get("number")
        return str(number) if number else None

    if event_name == "workflow_dispatch":
        value = (event.get("inputs") or {}).get("pr_number", "")
        return str(value) if value else None

    if event_name != "workflow_run":
        return None

    workflow_run = event.get("workflow_run") or {}
    pull_requests = workflow_run.get("pull_requests") or []
    if pull_requests:
        number = pull_requests[0].get("number")
        return str(number) if number else None

    # Manual workflow_dispatch runs can omit workflow_run.pull_requests.
    # Fall back to GitHub's commit-associated PR endpoint for the run SHA.
    head_sha = str(workflow_run.get("head_sha") or "")
    if not head_sha:
        return None
    try:
        pulls = api.request("GET", f"/commits/{head_sha}/pulls")
    except TRANSIENT_REQUEST_ERRORS as exc:
        print(f"warning: could not resolve PR for delegated run: {exc}", file=sys.stderr)
        return None
    if not isinstance(pulls, list) or not pulls:
        return None
    number = dict(pulls[0]).get("number")
    return str(number) if number else None


def load_pr_from_event_and_git(number: int) -> PullRequest | None:
    event_path = os.environ.get("GITHUB_EVENT_PATH", "")
    if not event_path:
        return None
    try:
        with open(event_path, encoding="utf-8") as handle:
            event = json.load(handle)
    except (OSError, json.JSONDecodeError):
        return None

    raw_pr = event.get("pull_request")
    if not isinstance(raw_pr, dict) or int(raw_pr.get("number") or 0) != number:
        return None

    base_sha = str((raw_pr.get("base") or {}).get("sha") or "")
    head_sha = str((raw_pr.get("head") or {}).get("sha") or "")
    if not base_sha or not head_sha:
        return None

    try:
        fetch_ref_for_diff(number, head_sha)
        files_output = subprocess.check_output(
            ["git", "diff", "--name-only", f"{base_sha}...{head_sha}"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError):
        return None

    return PullRequest(
        number=number,
        title=str(raw_pr.get("title") or ""),
        body=str(raw_pr.get("body") or ""),
        author_login=str((raw_pr.get("user") or {}).get("login") or ""),
        head_sha=head_sha,
        files=tuple(line for line in files_output.splitlines() if line),
    )


def fetch_ref_for_diff(number: int, head_sha: str) -> None:
    try:
        subprocess.run(
            ["git", "cat-file", "-e", f"{head_sha}^{{commit}}"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return
    except (OSError, subprocess.CalledProcessError):
        pass

    subprocess.run(
        ["git", "fetch", "--depth=1", "origin", f"pull/{number}/head"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def load_issue_comments(api: GitHubApi, number: int) -> list[dict[str, object]]:
    return [dict(item) for item in api.paginated(f"/issues/{number}/comments")]


def safe_load_issue_comments(api: GitHubApi, number: int) -> list[dict[str, object]]:
    try:
        return load_issue_comments(api, number)
    except TRANSIENT_REQUEST_ERRORS as exc:
        # A transient comments-list failure (e.g. http.client.RemoteDisconnected
        # mid-pagination) must not crash the lane check: degrade to body-only
        # evidence, which is still checked downstream.
        print(
            f"warning: could not read PR comments; PR body evidence will still be checked: {exc}",
            file=sys.stderr,
        )
        return []


def combined_evidence_text(pr: PullRequest, comments: Iterable[dict[str, object]]) -> str:
    chunks = [pr.body]
    chunks.extend(str(comment.get("body") or "") for comment in comments)
    return "\n\n".join(chunks)


def run_ids_from_text(repo: str, text: str) -> list[str]:
    escaped = re.escape(repo)
    pattern = re.compile(rf"https://github\.com/{escaped}/actions/runs/([0-9]+)")
    seen: set[str] = set()
    run_ids: list[str] = []
    for run_id in pattern.findall(text):
        if run_id not in seen:
            seen.add(run_id)
            run_ids.append(run_id)
    return run_ids


def text_mentions_head_sha(text: str, sha: str) -> bool:
    return sha in text or sha[:12] in text


def find_valid_delegated_run(
    api: GitHubApi,
    run_ids: Iterable[str],
    head_sha: str,
) -> tuple[dict[str, object] | None, list[str]]:
    diagnostics: list[str] = []
    for run_id in run_ids:
        try:
            run = dict(api.request("GET", f"/actions/runs/{run_id}"))
        except TRANSIENT_REQUEST_ERRORS as exc:
            diagnostics.append(f"run {run_id}: could not read workflow run ({exc})")
            continue
        diagnostic = delegated_run_diagnostic(run, run_id, head_sha)
        if diagnostic is not None:
            diagnostics.append(diagnostic)
            continue
        return run, diagnostics
    return None, diagnostics


def delegated_run_diagnostic(run: dict[str, object], run_id: str, head_sha: str) -> str | None:
    name = str(run.get("name") or "")
    event = str(run.get("event") or "")
    conclusion = str(run.get("conclusion") or "")
    run_head = str(run.get("head_sha") or "")
    if name != DELEGATED_WORKFLOW_NAME:
        return f"run {run_id}: workflow name is {name!r}, expected {DELEGATED_WORKFLOW_NAME!r}"
    if event != "workflow_dispatch":
        return f"run {run_id}: event is {event!r}, expected 'workflow_dispatch'"
    if run_head != head_sha:
        return f"run {run_id}: head_sha {run_head} does not match PR head {head_sha}"
    if conclusion != "success":
        return f"run {run_id}: conclusion is {conclusion!r}, expected 'success'"
    return None


def recorded_gate_ok(text: str, expected: Gate) -> bool:
    gates = accepted_gates(expected)
    if not gates:
        return True
    for gate in gates:
        if re.search(rf"\b(?:gate|gates)\s*[:=]\s*`?{re.escape(gate)}`?\b", text, re.IGNORECASE):
            return True
    return False


def comment_body(classification: Classification, pr: PullRequest, passed: bool, detail: str) -> str:
    if classification.gate == Gate.NONE:
        status = "PASS: no delegated runner proof required for this PR."
        proof = ""
    else:
        status = (
            "PASS: delegated runner proof recorded and matched this PR head."
            if passed
            else "FAIL: delegated runner proof is required before merge."
        )
        dependabot = dependabot_guidance(pr, classification) if not passed else ""
        proof = f"""

Expected delegated gate: `{classification.gate.label}`

Record proof in the PR body or a PR comment using:

```text
Assay-Runner delegated proof:
- gate: {classification.gate.label}
- run: https://github.com/Rul1an/assay/actions/runs/<run_id>
- sha: {pr.head_sha}
```
{dependabot}
"""
    reasons = "\n".join(f"- {reason}" for reason in classification.reasons[:12])
    if not reasons:
        reasons = "- No runner-impacting paths detected."
    return f"""{COMMENT_MARKER}
### Assay-Runner Lane Check

{status}

{detail}
{proof}
Changed-path classification:
{reasons}

Contract: [`{CONTRACT_DOC}`](https://github.com/Rul1an/assay/blob/main/{CONTRACT_DOC})
"""


def is_dependabot_pr(pr: PullRequest) -> bool:
    # GitHub normally reports Dependabot as dependabot[bot]; keep the app form
    # for older API surfaces that expose the GitHub App slug instead.
    return pr.author_login in {"dependabot[bot]", "app/dependabot"}


def dependabot_guidance(pr: PullRequest, classification: Classification) -> str:
    if not is_dependabot_pr(pr):
        return ""
    return f"""
Dependabot maintainer flow:

1. Review the dependency surface and update any coupled runner fixture
   assertions on a maintainer branch if needed.
2. Dispatch `Runner Spike Delegated` manually with `gates={classification.gate.label}`
   after the PR head SHA is final.
3. Add a maintainer comment with the run URL, `gate: {classification.gate.label}`,
   and the current PR head SHA. Dependabot does not need to edit its own PR body.
4. If Dependabot rebases or force-pushes, rerun the delegated gate because the
   recorded proof must match the new head SHA.

Flow reference:
[`{DEPENDABOT_FLOW_DOC}`](https://github.com/Rul1an/assay/blob/main/{DEPENDABOT_FLOW_DOC})
"""


def post_or_update_comment(
    api: GitHubApi,
    pr_number: int,
    comments: Iterable[dict[str, object]],
    body: str,
) -> None:
    existing_id: int | None = None
    for comment in comments:
        if COMMENT_MARKER in str(comment.get("body") or ""):
            existing_id = int(comment["id"])
            break
    if existing_id is not None:
        api.request("PATCH", f"/issues/comments/{existing_id}", {"body": body})
    else:
        api.request("POST", f"/issues/{pr_number}/comments", {"body": body})


def status_target_url() -> str:
    server_url = os.environ.get("GITHUB_SERVER_URL", "https://github.com")
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    if repo and run_id:
        return f"{server_url}/{repo}/actions/runs/{run_id}"
    return ""


def post_commit_status(api: GitHubApi, sha: str, passed: bool, description: str) -> None:
    payload: dict[str, object] = {
        "state": "success" if passed else "failure",
        "context": STATUS_CONTEXT,
        "description": description[:140],
    }
    target_url = status_target_url()
    if target_url:
        payload["target_url"] = target_url
    api.request("POST", f"/statuses/{sha}", payload)


def maybe_status(
    api: GitHubApi,
    sha: str,
    passed: bool,
    description: str,
    *,
    status: bool,
) -> None:
    if not status:
        return
    try:
        post_commit_status(api, sha, passed, description)
    except TRANSIENT_REQUEST_ERRORS as exc:
        print(f"warning: could not post commit status: {exc}", file=sys.stderr)


def run_check(api: GitHubApi, pr_number: int, *, comment: bool, status: bool) -> int:
    pr = load_pr(api, pr_number)
    classification = classify_files(pr.files)
    comments = safe_load_issue_comments(api, pr.number)
    text = combined_evidence_text(pr, comments)

    print(f"Assay-Runner lane check for PR #{pr.number}")
    print(f"Head SHA: {pr.head_sha}")
    print(f"Changed files: {len(pr.files)}")
    print(f"Expected delegated gate: {classification.gate.label}")
    for reason in classification.reasons:
        print(f"- {reason}")

    if classification.gate == Gate.NONE:
        body = comment_body(classification, pr, True, "No runner-impacting paths were detected.")
        maybe_comment(api, pr.number, comments, body, comment=comment and existing_lane_comment(comments))
        maybe_status(
            api,
            pr.head_sha,
            True,
            "no delegated runner proof required",
            status=status,
        )
        return 0

    run_ids = run_ids_from_text(api.repo, text)
    valid_run, run_diagnostics = find_valid_delegated_run(api, run_ids, pr.head_sha)
    sha_ok = text_mentions_head_sha(text, pr.head_sha)
    gate_ok = recorded_gate_ok(text, classification.gate)
    passed = valid_run is not None and sha_ok and gate_ok

    details: list[str] = []
    if valid_run is None:
        details.append("No successful `Runner Spike Delegated` workflow_dispatch run URL matched the PR head SHA.")
    if not sha_ok:
        details.append("The PR body/comments do not record the current PR head SHA or its 12-character prefix.")
    if not gate_ok:
        details.append(
            f"The PR body/comments do not record `gate: {classification.gate.label}`"
            + (" or `gate: all`." if classification.gate != Gate.ALL else ".")
        )
    if run_diagnostics:
        details.append("Run diagnostics:\n" + "\n".join(f"- {line}" for line in run_diagnostics[:8]))
    if passed:
        details.append(f"Matched delegated run: {valid_run.get('html_url')}")

    detail = "\n\n".join(details)
    body = comment_body(classification, pr, passed, detail)
    maybe_comment(api, pr.number, comments, body, comment=comment)
    status_description = (
        f"delegated proof accepted: gates={classification.gate.label}"
        if passed
        else f"delegated proof required: gates={classification.gate.label}"
    )
    maybe_status(api, pr.head_sha, passed, status_description, status=status)

    if passed:
        return 0
    print(detail, file=sys.stderr)
    return 1


def existing_lane_comment(comments: Iterable[dict[str, object]]) -> bool:
    return any(COMMENT_MARKER in str(comment.get("body") or "") for comment in comments)


def maybe_comment(
    api: GitHubApi,
    pr_number: int,
    comments: Iterable[dict[str, object]],
    body: str,
    *,
    comment: bool,
) -> None:
    if not comment:
        return
    try:
        post_or_update_comment(api, pr_number, comments, body)
    except TRANSIENT_REQUEST_ERRORS as exc:
        print(f"warning: could not post/update PR comment: {exc}", file=sys.stderr)


def self_test() -> None:
    gated_paths = load_gated_path_config()
    assert "crates/assay-runner-core/" in gated_paths.all_gate_prefixes
    assert ".github/actions/canonical-ebpf-build/action.yml" in gated_paths.all_gate_paths
    assert GATED_PATHS_DOC in gated_paths.all_gate_paths

    cases = [
        (["docs/reference/runner/ci-lanes.md"], Gate.NONE),
        (["docs/reference/observability/claim-boundary-positioning.md"], Gate.NONE),
        (["tests/fixtures/runner-spike/kernel-only-agent.sh"], Gate.KERNEL_ONLY),
        (["tests/fixtures/runner-spike/mcp-policy-agent.sh"], Gate.KERNEL_POLICY),
        (["runner-fixtures/openai-agents/package-lock.json"], Gate.OPENAI_AGENTS_KERNEL_POLICY),
        (["crates/assay-monitor/src/lib.rs"], Gate.ALL),
        (["crates/assay-xtask/src/main.rs"], Gate.ALL),
        (["scripts/ci/runner-spike-sdk-policy-correlation.sh"], Gate.ALL),
        (["tests/fixtures/runner-spike/kernel-only-agent.sh", "crates/assay-ebpf/src/main.rs"], Gate.ALL),
        # Gemini Python google-genai fixture paths route to gates=all per
        # #1307 (no new narrower delegated gate name). The Gemini fixture
        # moved from tests/fixtures/runner-spike/gemini-google-genai/ to
        # runner-fixtures/gemini-google-genai/ in Phase 2D Slice 5A.
        (["runner-fixtures/gemini-google-genai/fixture.py"], Gate.ALL),
        (["runner-fixtures/gemini-google-genai/sdk-policy-agent.sh"], Gate.ALL),
        (["scripts/ci/runner-spike-gemini-google-genai-acceptance.sh"], Gate.ALL),
        (
            ["scripts/ci/runner-spike-openai-agents-kernel-policy-hidden-write-three-run-determinism.sh"],
            Gate.OPENAI_AGENTS_KERNEL_POLICY,
        ),
        (["scripts/ci/assay_runner_delegated_proof_pack.py"], Gate.ALL),
        (["scripts/ci/assay_runner_gated_paths.json"], Gate.ALL),
        ([".github/actions/canonical-ebpf-build/action.yml"], Gate.ALL),
        # Read-only contract validators under scripts/ci/ do not exercise
        # the kernel, eBPF, or runner runtime path. They project over
        # normalized evidence files only. The cross-runtime-diff v0
        # validator was added by the Phase 2C implementation slice
        # (cross-runtime-diff-v0.md) and must not silently elevate to a
        # delegated gate via a future classifier refactor.
        (["scripts/ci/assay_runner_cross_runtime_diff_validate.py"], Gate.NONE),
        (["scripts/ci/assay_runner_capability_diff_validate.py"], Gate.NONE),
        # Phase 2D Slice 1 extracted the v0 schema data structures from
        # crates/assay-runner-spike/ into crates/assay-runner-schema/.
        # The schema crate hosts contract types that the runner archive
        # asserts at delegated acceptance time, so schema changes are
        # runner-impacting and require gates=all just like the spike crate.
        (["crates/assay-runner-schema/src/lib.rs"], Gate.ALL),
        (["crates/assay-runner-schema/Cargo.toml"], Gate.ALL),
        # Phase 2D Slice 2 extracted runner orchestration, archive
        # assembly, and layer normalizers from crates/assay-runner-spike/
        # into crates/assay-runner-core/. Core hosts the mechanics half
        # of the measured-run path and must be classified the same as
        # the spike crate for delegated proof requirements.
        (["crates/assay-runner-core/src/lib.rs"], Gate.ALL),
        (["crates/assay-runner-core/Cargo.toml"], Gate.ALL),
        # Phase 2D Slice 3 introduced crates/assay-runner-linux/ as the
        # Linux platform adapter for the Assay-Runner candidate. It
        # currently hosts cgroup placement only; future macOS/Windows
        # spikes belong in separate crates per
        # platform-and-extraction-readiness.md. Linux platform changes
        # require gates=all because cgroup placement is on the delegated
        # acceptance path.
        (["crates/assay-runner-linux/src/lib.rs"], Gate.ALL),
        (["crates/assay-runner-linux/Cargo.toml"], Gate.ALL),
    ]
    for files, expected in cases:
        got = classify_files(files).gate
        if got != expected:
            raise AssertionError(f"{files}: expected {expected.label}, got {got.label}")
    text = """
    Assay-Runner delegated proof:
    - gate: all
    - run: https://github.com/Rul1an/assay/actions/runs/26202770332
    - sha: abcdef1234567890
    """
    assert run_ids_from_text("Rul1an/assay", text) == ["26202770332"]
    assert text_mentions_head_sha(text, "abcdef1234567890ffffffffffffffffffffffff")
    assert recorded_gate_ok(text, Gate.KERNEL_ONLY)
    assert recorded_gate_ok("- gates=all", Gate.OPENAI_AGENTS_KERNEL_POLICY)
    assert not recorded_gate_ok("- gate: kernel-only", Gate.ALL)
    assert not recorded_gate_ok("- gate: kernel-only", Gate.OPENAI_AGENTS_KERNEL_POLICY)

    valid_run = {
        "name": DELEGATED_WORKFLOW_NAME,
        "event": "workflow_dispatch",
        "head_sha": "abc123",
        "conclusion": "success",
    }
    assert delegated_run_diagnostic(valid_run, "1", "abc123") is None
    assert "head_sha" in delegated_run_diagnostic({**valid_run, "head_sha": "def456"}, "1", "abc123")
    assert "conclusion" in delegated_run_diagnostic({**valid_run, "conclusion": "failure"}, "1", "abc123")
    assert "workflow name" in delegated_run_diagnostic({**valid_run, "name": "CI"}, "1", "abc123")
    _test_event_resolution_and_status_payload()

    dependabot_pr = PullRequest(
        number=1,
        title="Bump @openai/agents",
        body="",
        author_login="dependabot[bot]",
        head_sha="abc123",
        files=("runner-fixtures/openai-agents/package-lock.json",),
    )
    guidance = dependabot_guidance(
        dependabot_pr,
        Classification(Gate.OPENAI_AGENTS_KERNEL_POLICY, ()),
    )
    assert "Dependabot maintainer flow" in guidance
    assert "gates=openai-agents-kernel-policy" in guidance

    _test_connection_resilience()

    # Phase 2D Slice 6B mechanical absence check: assert that assay-cli no
    # longer consumes the assay-runner-spike wrapper. The check is
    # encoded here in self_test rather than as a runtime workflow step
    # so it travels with the lane-check helper and runs on every PR
    # touching the classifier itself, including any future PR that
    # might silently re-introduce a spike dependency.
    _assert_assay_cli_does_not_consume_spike()


def _test_event_resolution_and_status_payload() -> None:
    from tempfile import NamedTemporaryFile

    class _FakeApi:
        def __init__(self) -> None:
            self.requests: list[tuple[str, str, object | None]] = []

        def request(self, method: str, path: str, payload: object | None = None) -> object:
            self.requests.append((method, path, payload))
            if path == "/commits/deadbeef/pulls":
                return [{"number": 79}]
            return {}

    def with_event(event_name: str, event: object) -> str | None:
        old_name = os.environ.get("GITHUB_EVENT_NAME")
        old_path = os.environ.get("GITHUB_EVENT_PATH")
        with NamedTemporaryFile("w", encoding="utf-8") as handle:
            json.dump(event, handle)
            handle.flush()
            os.environ["GITHUB_EVENT_NAME"] = event_name
            os.environ["GITHUB_EVENT_PATH"] = handle.name
            try:
                return resolve_pr_number_from_event(_FakeApi())
            finally:
                if old_name is None:
                    os.environ.pop("GITHUB_EVENT_NAME", None)
                else:
                    os.environ["GITHUB_EVENT_NAME"] = old_name
                if old_path is None:
                    os.environ.pop("GITHUB_EVENT_PATH", None)
                else:
                    os.environ["GITHUB_EVENT_PATH"] = old_path

    assert with_event("pull_request", {"pull_request": {"number": 42}}) == "42"
    assert with_event("workflow_dispatch", {"inputs": {"pr_number": "43"}}) == "43"
    assert with_event("workflow_run", {"workflow_run": {"pull_requests": [{"number": 44}]}}) == "44"
    assert with_event("workflow_run", {"workflow_run": {"head_sha": "deadbeef"}}) == "79"

    fake = _FakeApi()
    old_repo = os.environ.pop("GITHUB_REPOSITORY", None)
    old_run_id = os.environ.pop("GITHUB_RUN_ID", None)
    try:
        post_commit_status(fake, "abc123", True, "ok")
    finally:
        if old_repo is not None:
            os.environ["GITHUB_REPOSITORY"] = old_repo
        if old_run_id is not None:
            os.environ["GITHUB_RUN_ID"] = old_run_id
    assert fake.requests == [
        (
            "POST",
            "/statuses/abc123",
            {
                "state": "success",
                "context": STATUS_CONTEXT,
                "description": "ok",
            },
        )
    ]


def _test_connection_resilience() -> None:
    """Regression for the assay PR #1706 lane-check crash: a transient
    connection-level error (`http.client.RemoteDisconnected`,
    "Remote end closed connection without response") must be retried in the
    request helper and, if it persists on the comments listing, degrade to
    body-only evidence rather than crashing the whole lane-check job.
    """

    def remote_disconnected() -> http.client.RemoteDisconnected:
        return http.client.RemoteDisconnected(
            "Remote end closed connection without response"
        )

    # (1) urlopen_with_retry recovers when a GET blips once then succeeds.
    class _FakeResponse:
        pass

    attempts_seen = {"count": 0}

    def flaky_urlopen(_request, timeout=None):
        attempts_seen["count"] += 1
        if attempts_seen["count"] == 1:
            raise remote_disconnected()
        return _FakeResponse()

    request = urllib.request.Request(
        "https://api.github.com/repos/x/y/issues/1/comments"
    )
    orig_urlopen = urllib.request.urlopen
    orig_sleep = time.sleep
    try:
        urllib.request.urlopen = flaky_urlopen
        time.sleep = lambda _seconds: None
        recovered = urlopen_with_retry(request)
    finally:
        urllib.request.urlopen = orig_urlopen
        time.sleep = orig_sleep
    assert isinstance(recovered, _FakeResponse)
    assert attempts_seen["count"] == 2

    # (2) A persistent connection failure on a GET is retried the bounded
    #     number of times and then re-raised for the caller to handle.
    persistent = {"count": 0}

    def always_disconnect(_request, timeout=None):
        persistent["count"] += 1
        raise remote_disconnected()

    reraised = False
    orig_urlopen = urllib.request.urlopen
    orig_sleep = time.sleep
    try:
        urllib.request.urlopen = always_disconnect
        time.sleep = lambda _seconds: None
        try:
            urlopen_with_retry(request)
        except http.client.RemoteDisconnected:
            reraised = True
    finally:
        urllib.request.urlopen = orig_urlopen
        time.sleep = orig_sleep
    assert reraised
    assert persistent["count"] == HTTP_RETRY_ATTEMPTS

    # (3) safe_load_issue_comments degrades to body-only evidence (returns [])
    #     when the comments call keeps failing with a connection-level error,
    #     instead of raising and crashing the lane check.
    class _RemoteDisconnectedApi:
        def paginated(self, _path):
            raise remote_disconnected()

    assert safe_load_issue_comments(_RemoteDisconnectedApi(), 1) == []


def _assert_assay_cli_does_not_consume_spike() -> None:
    """Slice 6B invariant: assay-cli must consume the Runner candidate
    only through its public schema/core/linux crates, never through
    the assay-runner-spike compatibility wrapper.

    Two mechanical conditions:

    1. `crates/assay-cli/Cargo.toml` does not declare
       `assay-runner-spike` as a dependency.
    2. No source file under `crates/assay-cli/` references the
       `assay_runner_spike::` path.

    Either condition appearing means external-style consumption has
    silently regressed; the assertion failure message names exactly
    which condition fires.
    """
    import re
    from pathlib import Path

    root = Path(__file__).resolve().parents[2]

    # Read with explicit utf-8 encoding so the self-test does not raise
    # UnicodeDecodeError under a non-UTF-8 locale (Rust source files in
    # the assay-cli crate contain non-ASCII characters such as em-dashes
    # in comments). On a genuine decoding failure, surface a clear
    # AssertionError naming the file rather than letting a UnicodeDecodeError
    # traceback hide the self-test contract.
    cli_cargo = root / "crates" / "assay-cli" / "Cargo.toml"
    if cli_cargo.exists():
        try:
            cargo_text = cli_cargo.read_text(encoding="utf-8")
        except UnicodeDecodeError as exc:
            raise AssertionError(
                f"Could not read {cli_cargo} as utf-8 while checking the "
                f"Slice 6B spike-absence invariant: {exc}. The Cargo.toml "
                "must be valid utf-8."
            ) from exc
        # Match `assay-runner-spike` as a dependency in any of the
        # common Cargo.toml forms a regression could re-introduce it
        # under. Covers:
        #   1. Inline form (with or without leading whitespace):
        #      `assay-runner-spike = ...`
        #      `  assay-runner-spike = { workspace = true }`
        #   2. Section-header form, for [dependencies], [dev-dependencies],
        #      [build-dependencies], and target-conditional dependencies:
        #      `[dependencies.assay-runner-spike]`
        #      `[dev-dependencies.assay-runner-spike]`
        #      `[build-dependencies.assay-runner-spike]`
        #      `[target.'cfg(unix)'.dependencies.assay-runner-spike]`
        # Comments and string literals do not match because the regex is
        # anchored at the start of a line via (?m)^.
        spike_inline = r"(?m)^\s*assay-runner-spike\s*="
        spike_table_header = (
            r"(?m)^\s*\[(?:dependencies|dev-dependencies|"
            r"build-dependencies|target\.[^\]]+?\.dependencies)"
            r"\.assay-runner-spike\]"
        )
        if re.search(spike_inline, cargo_text) or re.search(
            spike_table_header, cargo_text
        ):
            raise AssertionError(
                "Assay still consumes spike internals: "
                "`crates/assay-cli/Cargo.toml` declares `assay-runner-spike` "
                "as a dependency (inline or table-header form). Phase 2D "
                "Slice 6B requires assay-cli to depend on "
                "assay-runner-{schema,core,linux} directly. "
                "See docs/reference/runner/assay-consumes-runner-external.md."
            )

    cli_src = root / "crates" / "assay-cli" / "src"
    if cli_src.is_dir():
        offenders: list[str] = []
        for path in cli_src.rglob("*.rs"):
            try:
                content = path.read_text(encoding="utf-8")
            except OSError:
                continue
            except UnicodeDecodeError as exc:
                raise AssertionError(
                    f"Could not read {path} as utf-8 while checking the "
                    f"Slice 6B spike-absence invariant: {exc}. assay-cli "
                    "source files must be valid utf-8."
                ) from exc
            if "assay_runner_spike::" in content:
                offenders.append(str(path.relative_to(root)))
        if offenders:
            joined = ", ".join(offenders)
            raise AssertionError(
                "Assay still consumes spike internals: "
                f"the following file(s) under `crates/assay-cli/src/` "
                f"reference `assay_runner_spike::`: {joined}. "
                "Phase 2D Slice 6B requires assay-cli to import from "
                "assay-runner-{schema,core,linux} directly. See "
                "docs/reference/runner/assay-consumes-runner-external.md."
            )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--resolve-pr-from-event", action="store_true")
    parser.add_argument("--pr-number", type=int, default=int(os.environ.get("PR_NUMBER", "0") or "0"))
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--comment", action="store_true")
    parser.add_argument("--status", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("self-test ok")
        return 0

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        print("GITHUB_TOKEN is required", file=sys.stderr)
        return 2
    if not args.repo:
        print("--repo or GITHUB_REPOSITORY is required", file=sys.stderr)
        return 2
    api = GitHubApi(args.repo, token)
    if args.resolve_pr_from_event:
        pr_number = resolve_pr_number_from_event(api) or ""
        print(f"pr_number={pr_number}")
        return 0
    if args.pr_number <= 0:
        print("--pr-number or PR_NUMBER is required", file=sys.stderr)
        return 2

    return run_check(api, args.pr_number, comment=args.comment, status=args.status)


if __name__ == "__main__":
    raise SystemExit(main())
