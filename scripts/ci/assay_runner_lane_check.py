#!/usr/bin/env python3
"""Enforce the Assay-Runner delegated CI lane contract for pull requests.

The script is intentionally stdlib-only so the GitHub workflow can run it from
the base branch without installing repository dependencies or executing PR code.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from enum import IntEnum
from typing import Iterable


CONTRACT_DOC = "docs/reference/runner/ci-lanes.md"
DEPENDABOT_FLOW_DOC = "docs/reference/runner/dependabot-lane-flow.md"
DELEGATED_WORKFLOW_NAME = "Runner Spike Delegated"
COMMENT_MARKER = "<!-- assay-runner-lane-check -->"


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
        with urllib.request.urlopen(request, timeout=30) as response:
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
            with urllib.request.urlopen(request, timeout=30) as response:
                page = json.loads(response.read().decode("utf-8"))
                if not isinstance(page, list):
                    raise TypeError(f"Expected list response from {url}")
                results.extend(page)
                url = next_link(response.headers.get("Link", ""))
        return results


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


def classify_file(path: str) -> tuple[Gate, str | None]:
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

    all_prefixes = (
        "crates/assay-runner-spike/",
        "crates/assay-runner-schema/",
        "crates/assay-runner-core/",
        "crates/assay-runner-linux/",
        "crates/assay-monitor/",
        "crates/assay-ebpf/",
        "crates/assay-xtask/",
    )
    if any(starts(path, prefix) for prefix in all_prefixes):
        return Gate.ALL, f"{path}: shared runner/eBPF/monitor/xtask code requires gates=all"

    if path in {
        ".github/workflows/runner-spike-delegated.yml",
        ".github/workflows/runner-spike-sdk.yml",
        "crates/assay-cli/src/cli/commands/runner_spike.rs",
        "crates/assay-cli/src/cgroup.rs",
        "Cargo.toml",
        "Cargo.lock",
    }:
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


def load_issue_comments(api: GitHubApi, number: int) -> list[dict[str, object]]:
    return [dict(item) for item in api.paginated(f"/issues/{number}/comments")]


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
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
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


def run_check(api: GitHubApi, pr_number: int, *, comment: bool) -> int:
    pr = load_pr(api, pr_number)
    comments = load_issue_comments(api, pr.number)
    classification = classify_files(pr.files)
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
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        print(f"warning: could not post/update PR comment: {exc}", file=sys.stderr)


def self_test() -> None:
    cases = [
        (["docs/reference/runner/ci-lanes.md"], Gate.NONE),
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

    # Phase 2D Slice 6B mechanical absence check: assert that assay-cli no
    # longer consumes the assay-runner-spike wrapper. The check is
    # encoded here in self_test rather than as a runtime workflow step
    # so it travels with the lane-check helper and runs on every PR
    # touching the classifier itself, including any future PR that
    # might silently re-introduce a spike dependency.
    _assert_assay_cli_does_not_consume_spike()


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
        # Match `assay-runner-spike` only when it appears as a top-level
        # dependency key, not when it appears inside a comment or string.
        # Cargo dep keys look like: `assay-runner-spike = ...`
        if re.search(r"(?m)^assay-runner-spike\s*=", cargo_text):
            raise AssertionError(
                "Assay still consumes spike internals: "
                "`crates/assay-cli/Cargo.toml` declares `assay-runner-spike` "
                "as a dependency. Phase 2D Slice 6B requires assay-cli to "
                "depend on assay-runner-{schema,core,linux} directly. "
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
    parser.add_argument("--pr-number", type=int, default=int(os.environ.get("PR_NUMBER", "0") or "0"))
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    parser.add_argument("--comment", action="store_true")
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
    if args.pr_number <= 0:
        print("--pr-number or PR_NUMBER is required", file=sys.stderr)
        return 2

    return run_check(GitHubApi(args.repo, token), args.pr_number, comment=args.comment)


if __name__ == "__main__":
    raise SystemExit(main())
