#!/usr/bin/env python3
"""Host-capability proof gate for diagnostics changes.

Contract: docs/reference/runner/host-capability-proof.md (the RFC owns the rules; this
script must match it). Summary:

- Non-Markdown changes under crates/assay-cli/src/diagnostics/ require a host-capability
  proof: a successful `workflow_dispatch` run of the `host-capability-proof` workflow on
  the PR head SHA, referenced from the PR body or a PR comment.
- The proof is the workflow run and its artifact, validated through the Actions API.
  A pasted JSON block alone never satisfies the gate.
- The checker validates presence and JSON type of the required landlock fields, never
  their values. A red host is also evidence.
- Failures carry a machine-readable reason from an append-only set; removing or renaming
  a reason is a contract change to the RFC first.

This deliberately mirrors scripts/ci/assay_runner_lane_check.py (the proven SHA-binding
mechanics) but is a separate gate: the runner-spike lanes prove kernel capture, this gate
proves host-capability diagnostics were produced on a real host for the exact PR head.
"""

from __future__ import annotations

import argparse
import io
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
import zipfile
from collections.abc import Iterable
from dataclasses import dataclass

PROOF_WORKFLOW_NAME = "host-capability-proof"
ARTIFACT_NAME = "host-capability-proof"
DOCTOR_JSON_NAME = "doctor.json"
META_JSON_NAME = "proof_meta.json"
CONTRACT_DOC = "docs/reference/runner/host-capability-proof.md"
COMMENT_MARKER = "<!-- assay-host-capability-check -->"

TRIGGER_PREFIX = "crates/assay-cli/src/diagnostics/"

# Technical exemption, not a trust statement: a change to the gate cannot be proven by the
# gate it is changing (see the RFC). These files get normal CI like any other file.
SELF_EXEMPT = {
    "scripts/ci/assay_host_capability_check.py",
    ".github/workflows/host-capability-check.yml",
    ".github/workflows/host-capability-proof.yml",
}

# Required fields inside the doctor report's `landlock` object: presence + JSON type only,
# never values. `None` in the type tuple means JSON null is accepted. bool is checked
# before int because bool is a subclass of int in Python.
REQUIRED_FIELDS: dict[str, tuple[type | None, ...]] = {
    "abi_probe_status": (str,),
    "abi_version_source": (str, None),
    "abi_version": (int, None),
    "net_connect_tcp_supported": (bool,),
    "net_bind_tcp_supported": (bool,),
    "net_connect_ruleset_probe": (str,),
    "no_new_privs_settable": (bool,),
}

# Optional fields: absent is fine; when present the type must match.
OPTIONAL_FIELDS: dict[str, tuple[type | None, ...]] = {
    "abi_probe_errno": (int, None),
    "net_connect_ruleset_errno": (int, None),
}

HTTP_RETRY_ATTEMPTS = 3
RETRYABLE_HTTP_CODES = {429, 500, 502, 503, 504}


@dataclass(frozen=True)
class PullRequest:
    number: int
    body: str
    head_sha: str
    files: tuple[str, ...]


@dataclass(frozen=True)
class GateResult:
    ok: bool
    reason: str  # machine-readable, append-only set (see the RFC)
    detail: str  # prose for the human


def proof_required(files: Iterable[str]) -> tuple[bool, list[str]]:
    """Pure path classifier. Markdown anywhere is exempt (pattern-wide, so a future
    diagnostics README never triggers); the gate's own files are a documented technical
    exemption."""
    reasons: list[str] = []
    for path in files:
        if path in SELF_EXEMPT:
            continue
        if path.endswith(".md"):
            continue
        if path.startswith(TRIGGER_PREFIX):
            reasons.append(f"{path}: diagnostics host-capability surface requires host proof")
    return bool(reasons), reasons


def check_landlock_fields(doctor: object) -> GateResult:
    """Presence + type checks on the landlock object. Never value checks: unknown future
    enum strings pass, and a red host (`abi_probe_status: "unsupported"`) is also evidence."""
    if not isinstance(doctor, dict):
        return GateResult(False, "doctor_json_invalid", "doctor output is not a JSON object")
    landlock = doctor.get("landlock")
    if not isinstance(landlock, dict):
        return GateResult(False, "field_missing:landlock", "doctor output has no landlock object")

    def type_ok(value: object, types: tuple[type | None, ...]) -> bool:
        for t in types:
            if t is None:
                if value is None:
                    return True
            elif t is int:
                # bool is a subclass of int; an integer field must not accept true/false.
                if isinstance(value, int) and not isinstance(value, bool):
                    return True
            elif isinstance(value, t):
                return True
        return False

    for name, types in REQUIRED_FIELDS.items():
        if name not in landlock:
            return GateResult(False, f"field_missing:{name}", f"landlock.{name} is required")
        if not type_ok(landlock[name], types):
            return GateResult(
                False, f"field_type:{name}", f"landlock.{name} has the wrong JSON type"
            )
    for name, types in OPTIONAL_FIELDS.items():
        if name in landlock and not type_ok(landlock[name], types):
            return GateResult(
                False, f"field_type:{name}", f"landlock.{name} has the wrong JSON type"
            )
    return GateResult(True, "ok", "all required landlock fields present with expected types")


class GitHubApi:
    def __init__(self, repo: str, token: str) -> None:
        self.repo = repo
        self.base_url = f"https://api.github.com/repos/{repo}"
        self.token = token

    def request(self, path: str) -> object:
        url = path if path.startswith("https://") else f"{self.base_url}{path}"
        request = urllib.request.Request(url, headers=self._headers())
        with urlopen_with_retry(request) as response:
            body = response.read().decode("utf-8")
            return json.loads(body) if body else {}

    def paginated(self, path: str) -> list[object]:
        separator = "&" if "?" in path else "?"
        url = f"{self.base_url}{path}{separator}per_page=100"
        results: list[object] = []
        while url:
            request = urllib.request.Request(url, headers=self._headers())
            with urlopen_with_retry(request) as response:
                page = json.loads(response.read().decode("utf-8"))
                if not isinstance(page, list):
                    raise TypeError(f"Expected list response from {url}")
                results.extend(page)
                url = next_link(response.headers.get("Link", ""))
        return results

    def download_artifact_zip(self, archive_url: str) -> bytes:
        """Two-hop download: the API endpoint answers 302 to a pre-signed blob URL. The
        Authorization header must go to api.github.com only, never to the second host, so
        redirects are handled manually instead of letting urllib forward headers."""

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: N802
                return None

        opener = urllib.request.build_opener(NoRedirect)
        request = urllib.request.Request(archive_url, headers=self._headers())
        try:
            with opener.open(request, timeout=60) as response:
                return response.read()
        except urllib.error.HTTPError as exc:
            if exc.code in (301, 302, 303, 307, 308):
                location = exc.headers.get("Location", "")
                exc.close()
                if not location:
                    raise
                plain = urllib.request.Request(location)
                with urllib.request.urlopen(plain, timeout=60) as response:
                    return response.read()
            raise

    def _headers(self) -> dict[str, str]:
        return {
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Authorization": f"Bearer {self.token}",
        }


def urlopen_with_retry(request: urllib.request.Request):
    last_error: BaseException | None = None
    for attempt in range(HTTP_RETRY_ATTEMPTS):
        try:
            return urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as exc:
            last_error = exc
            exc.close()
            if exc.code not in RETRYABLE_HTTP_CODES or attempt == HTTP_RETRY_ATTEMPTS - 1:
                raise
        except (urllib.error.URLError, TimeoutError) as exc:
            last_error = exc
            if attempt == HTTP_RETRY_ATTEMPTS - 1:
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


def load_pr(api: GitHubApi, number: int) -> PullRequest:
    pr = api.request(f"/pulls/{number}")
    files = api.paginated(f"/pulls/{number}/files")
    return PullRequest(
        number=number,
        body=str(pr.get("body") or ""),
        head_sha=str(pr["head"]["sha"]),
        files=tuple(str(item["filename"]) for item in files),
    )


def combined_evidence_text(api: GitHubApi, pr: PullRequest) -> str:
    chunks = [pr.body]
    try:
        comments = api.paginated(f"/issues/{pr.number}/comments")
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        print(f"warning: could not read PR comments: {exc}", file=sys.stderr)
        comments = []
    chunks.extend(str(dict(c).get("body") or "") for c in comments)
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


def run_diagnostic(run: dict[str, object], run_id: str, head_sha: str) -> tuple[str, str] | None:
    """None when the run is a valid proof run; otherwise (reason, detail)."""
    name = str(run.get("name") or "")
    event = str(run.get("event") or "")
    conclusion = str(run.get("conclusion") or "")
    run_head = str(run.get("head_sha") or "")
    if name != PROOF_WORKFLOW_NAME:
        return ("run_wrong_workflow", f"run {run_id}: workflow is {name!r}, expected {PROOF_WORKFLOW_NAME!r}")
    if event != "workflow_dispatch":
        return ("run_not_dispatch", f"run {run_id}: event is {event!r}, expected 'workflow_dispatch'")
    if run_head != head_sha:
        return ("head_sha_mismatch", f"run {run_id}: head_sha {run_head} does not match PR head {head_sha}")
    if conclusion != "success":
        return ("run_not_success", f"run {run_id}: conclusion is {conclusion!r}, expected 'success'")
    return None


def validate_proof(api: GitHubApi, pr: PullRequest, text: str) -> GateResult:
    run_ids = run_ids_from_text(api.repo, text)
    if not run_ids:
        return GateResult(
            False,
            "no_proof_marker",
            "no workflow-run URL found in the PR body or comments; a pasted JSON block alone "
            "never satisfies the gate",
        )
    diagnostics: list[str] = []
    for run_id in run_ids:
        try:
            run = dict(api.request(f"/actions/runs/{run_id}"))
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
            diagnostics.append(f"run {run_id}: could not read workflow run ({exc})")
            continue
        diagnostic = run_diagnostic(run, run_id, pr.head_sha)
        if diagnostic is not None:
            diagnostics.append(diagnostic[1])
            continue
        return validate_artifact(api, run_id, pr.head_sha)
    return GateResult(
        False,
        "run_not_found",
        "no referenced workflow run validates for this PR head:\n" + "\n".join(diagnostics),
    )


def validate_artifact(api: GitHubApi, run_id: str, head_sha: str) -> GateResult:
    try:
        listing = dict(api.request(f"/actions/runs/{run_id}/artifacts"))
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        return GateResult(False, "artifact_missing", f"could not list artifacts for run {run_id}: {exc}")
    artifacts = [dict(a) for a in listing.get("artifacts", []) if isinstance(a, dict)]
    match = next((a for a in artifacts if str(a.get("name")) == ARTIFACT_NAME), None)
    if match is None:
        return GateResult(
            False, "artifact_missing", f"run {run_id} has no artifact named {ARTIFACT_NAME!r}"
        )
    try:
        blob = api.download_artifact_zip(str(match["archive_download_url"]))
        archive = zipfile.ZipFile(io.BytesIO(blob))
        doctor = json.loads(archive.read(DOCTOR_JSON_NAME).decode("utf-8"))
    except (KeyError, ValueError, urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        return GateResult(
            False, "artifact_unreadable", f"could not read {DOCTOR_JSON_NAME} from run {run_id}: {exc}"
        )
    # Defense in depth: the artifact records the SHA it was produced on; it must agree with
    # the run metadata the gate already matched against the PR head.
    try:
        meta = json.loads(archive.read(META_JSON_NAME).decode("utf-8"))
        meta_sha = str(meta.get("head_sha") or "")
        if meta_sha and meta_sha != head_sha:
            return GateResult(
                False,
                "meta_sha_mismatch",
                f"artifact {META_JSON_NAME} records head_sha {meta_sha}, expected {head_sha}",
            )
    except KeyError:
        pass  # meta file is recommended, not required by the contract
    except ValueError as exc:
        return GateResult(False, "artifact_unreadable", f"{META_JSON_NAME} is not valid JSON: {exc}")
    return check_landlock_fields(doctor)


def self_test() -> int:
    failures: list[str] = []

    classification_cases = [
        (["crates/assay-cli/src/diagnostics/probes.rs"], True),
        (["crates/assay-cli/src/diagnostics/landlock_net_smoke.rs"], True),
        (["crates/assay-cli/src/diagnostics/format.rs"], True),
        (["crates/assay-cli/src/diagnostics/README.md"], False),
        (["crates/assay-cli/src/cli/commands/run.rs"], False),
        (["CHANGELOG.md"], False),
        (["docs/reference/runner/host-capability-proof.md"], False),
        (["scripts/ci/assay_host_capability_check.py"], False),
        ([".github/workflows/host-capability-check.yml"], False),
        ([".github/workflows/host-capability-proof.yml"], False),
        (["docs/intro.md", "crates/assay-cli/src/diagnostics/report.rs"], True),
        ([], False),
    ]
    for files, expected in classification_cases:
        required, _ = proof_required(files)
        if required != expected:
            failures.append(f"classification: {files} -> {required}, expected {expected}")

    good = {
        "landlock": {
            "abi_probe_status": "ok",
            "abi_version_source": "landlock_create_ruleset_version",
            "abi_version": 4,
            "net_connect_tcp_supported": True,
            "net_bind_tcp_supported": True,
            "net_connect_ruleset_probe": "usable",
            "no_new_privs_settable": True,
            "abi_probe_errno": None,
            "net_connect_ruleset_errno": None,
        }
    }
    field_cases: list[tuple[str, object, str]] = [
        ("good report passes", good, "ok"),
        # A red host is also evidence: unsupported status and unknown future enum pass.
        (
            "red host passes",
            {"landlock": {**good["landlock"], "abi_probe_status": "unsupported", "abi_version": None}},
            "ok",
        ),
        (
            "unknown future enum passes",
            {"landlock": {**good["landlock"], "net_connect_ruleset_probe": "future_state"}},
            "ok",
        ),
        (
            "missing required field fails",
            {"landlock": {k: v for k, v in good["landlock"].items() if k != "net_bind_tcp_supported"}},
            "field_missing:net_bind_tcp_supported",
        ),
        (
            "boolean field with wrong type fails",
            {"landlock": {**good["landlock"], "no_new_privs_settable": "true"}},
            "field_type:no_new_privs_settable",
        ),
        (
            "abi_version must not accept a boolean",
            {"landlock": {**good["landlock"], "abi_version": True}},
            "field_type:abi_version",
        ),
        (
            "errno as string fails when present",
            {"landlock": {**good["landlock"], "abi_probe_errno": "13"}},
            "field_type:abi_probe_errno",
        ),
        (
            "errno absent is fine",
            {"landlock": {k: v for k, v in good["landlock"].items() if k != "abi_probe_errno"}},
            "ok",
        ),
        ("missing landlock object fails", {}, "field_missing:landlock"),
        ("non-object doctor fails", [], "doctor_json_invalid"),
    ]
    for label, doctor, expected_reason in field_cases:
        result = check_landlock_fields(doctor)
        if result.reason != expected_reason:
            failures.append(f"fields: {label}: got {result.reason}, expected {expected_reason}")

    run_cases = [
        ({"name": "other", "event": "workflow_dispatch", "conclusion": "success", "head_sha": "a" * 40}, "run_wrong_workflow"),
        ({"name": PROOF_WORKFLOW_NAME, "event": "push", "conclusion": "success", "head_sha": "a" * 40}, "run_not_dispatch"),
        ({"name": PROOF_WORKFLOW_NAME, "event": "workflow_dispatch", "conclusion": "success", "head_sha": "b" * 40}, "head_sha_mismatch"),
        ({"name": PROOF_WORKFLOW_NAME, "event": "workflow_dispatch", "conclusion": "failure", "head_sha": "a" * 40}, "run_not_success"),
        ({"name": PROOF_WORKFLOW_NAME, "event": "workflow_dispatch", "conclusion": "success", "head_sha": "a" * 40}, None),
    ]
    for run, expected in run_cases:
        got = run_diagnostic(run, "1", "a" * 40)
        got_reason = got[0] if got else None
        if got_reason != expected:
            failures.append(f"run: {run} -> {got_reason}, expected {expected}")

    if failures:
        for failure in failures:
            print(f"SELF-TEST FAIL: {failure}", file=sys.stderr)
        return 1
    print("self-test: all cases passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--pr", type=int)
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.pr is None:
        parser.error("--pr or --self-test is required")

    repo = os.environ.get("GITHUB_REPOSITORY", "")
    token = os.environ.get("GITHUB_TOKEN", "")
    if not repo or not token:
        print("GITHUB_REPOSITORY and GITHUB_TOKEN are required", file=sys.stderr)
        return 2

    api = GitHubApi(repo, token)
    pr = load_pr(api, args.pr)
    required, reasons = proof_required(pr.files)

    if not required:
        print("PASS: no host-capability proof required for this PR.")
        return 0

    print("Host-capability proof required:")
    for reason in reasons[:12]:
        print(f"  - {reason}")

    text = combined_evidence_text(api, pr)
    result = validate_proof(api, pr, text)
    if result.ok:
        print(f"PASS: host-capability proof validated for head {pr.head_sha}.")
        print(f"  {result.detail}")
        return 0

    print(f"FAIL: host-capability proof missing or invalid.", file=sys.stderr)
    print(f"reason={result.reason}", file=sys.stderr)
    print(result.detail, file=sys.stderr)
    print(
        f"\nDispatch the {PROOF_WORKFLOW_NAME!r} workflow on this PR's head SHA and reference the "
        f"run URL in the PR body or a comment:\n\n"
        f"Host-capability proof:\n"
        f"- workflow-run: https://github.com/{repo}/actions/runs/<run_id>\n"
        f"- host: assay-bpf-runner\n"
        f"- command: assay doctor --format json\n"
        f"- sha: {pr.head_sha}\n\n"
        f"Contract: {CONTRACT_DOC}",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
