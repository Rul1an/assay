#!/usr/bin/env python3
"""Enforce the Assay-Runner delegated CI lane contract for pull requests.

The script is intentionally stdlib-only so the GitHub workflow can run it from
the base branch without installing repository dependencies or executing PR code.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import http.client
import io
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from dataclasses import dataclass
from enum import IntEnum
from functools import lru_cache
from pathlib import Path
from typing import Iterable, Iterator


CONTRACT_DOC = "docs/reference/runner/ci-lanes.md"
DEPENDABOT_FLOW_DOC = "docs/reference/runner/dependabot-lane-flow.md"
GATED_PATHS_DOC = "scripts/ci/assay_runner_gated_paths.json"
DELEGATED_WORKFLOW_NAME = "Runner Spike Delegated"
DELEGATED_WORKFLOW_PATH = ".github/workflows/runner-spike-delegated.yml"
COMMENT_MARKER = "<!-- assay-runner-lane-check -->"
STATUS_CONTEXT = "lane-check/proof"
PROOF_PACK_SCHEMA = "assay.runner.delegated_proof_pack.v1"
PROOF_PACK_KIND = "delegated_runner_proof_pack"
PROOF_PACK_CLAIM_CEILING = "delegated_gate_execution_only_not_runtime_safety"
# 404 is retryable here because GitHub can briefly return it for freshly
# created PR metadata endpoints such as /pulls/{number}/files.
RETRYABLE_HTTP_CODES = {404, 502, 503, 504}
HTTP_RETRY_ATTEMPTS = 3
PROOF_PACK_ARCHIVE_MAX_BYTES = 100 * 1024 * 1024
PROOF_PACK_REQUIRED_MEMBER_MAX_BYTES = 16 * 1024 * 1024
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
    content_provenance_paths: tuple[str, ...]


@dataclass(frozen=True)
class ContentTreeComparison:
    accepted: bool
    diagnostics: tuple[str, ...]


@dataclass(frozen=True)
class AttestedProofCheck:
    accepted: bool
    run: dict[str, object] | None
    diagnostics: tuple[str, ...]


@dataclass(frozen=True)
class ProofPackArtifact:
    manifest: dict[str, object]
    manifest_bytes: bytes
    checksums: dict[str, str]
    bundle: dict[str, object]
    bundle_bytes: bytes


class DownloadLimitExceeded(Exception):
    pass


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

    def download(self, path_or_url: str, *, max_bytes: int | None = None) -> bytes:
        url = path_or_url if path_or_url.startswith("https://") else f"{self.base_url}{path_or_url}"
        opener = urllib.request.build_opener(_DropAuthorizationOnRedirect)
        request = urllib.request.Request(
            url,
            headers={
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "Authorization": f"Bearer {self.token}",
            },
            method="GET",
        )
        with urlopen_with_retry(request, opener=opener) as response:
            length = response.headers.get("Content-Length")
            if max_bytes is not None and length:
                try:
                    if int(length) > max_bytes:
                        raise DownloadLimitExceeded(
                            f"download size {length} exceeds limit {max_bytes}"
                        )
                except ValueError:
                    pass

            body = bytearray()
            while True:
                read_size = 1024 * 1024
                if max_bytes is not None:
                    read_size = min(read_size, max_bytes - len(body) + 1)
                chunk = response.read(read_size)
                if not chunk:
                    break
                body.extend(chunk)
                if max_bytes is not None and len(body) > max_bytes:
                    raise DownloadLimitExceeded(
                        f"download size exceeds limit {max_bytes}"
                    )
            return bytes(body)

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


class _DropAuthorizationOnRedirect(urllib.request.HTTPRedirectHandler):
    """Do not forward GitHub bearer tokens to artifact archive redirects.

    GitHub's artifact `archive_download_url` is an authenticated API endpoint
    that redirects to a signed blob URL. urllib preserves request headers across
    that redirect, and the blob backend rejects a GitHub `Authorization` header
    even though the signed URL is otherwise valid. Authenticate the GitHub API
    hop, then let the signed archive URL stand on its own.
    """

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        redirected = super().redirect_request(req, fp, code, msg, headers, newurl)
        if redirected is not None:
            redirected.remove_header("Authorization")
        return redirected


def urlopen_with_retry(request: urllib.request.Request, opener=None):
    last_error: BaseException | None = None
    attempts = HTTP_RETRY_ATTEMPTS if request.get_method() == "GET" else 1
    for attempt in range(attempts):
        try:
            if opener is not None:
                return opener.open(request, timeout=30)
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


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def proof_pack_artifact_name(run_id: str) -> str:
    return f"assay-runner-delegated-proof-pack-{run_id}"


def parse_subject_checksums(text: str) -> tuple[dict[str, str], tuple[str, ...]]:
    checksums: dict[str, str] = {}
    diagnostics: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        parts = stripped.split(maxsplit=1)
        if len(parts) != 2:
            diagnostics.append(f"subject-checksums.txt:{line_number}: malformed checksum line")
            continue
        digest, path = parts
        if not re.fullmatch(r"[0-9a-f]{64}", digest):
            diagnostics.append(f"subject-checksums.txt:{line_number}: malformed sha256 digest")
            continue
        if path in checksums and checksums[path] != digest:
            diagnostics.append(f"subject-checksums.txt:{line_number}: duplicate path {path!r} with different digest")
            continue
        checksums[path] = digest
    if not checksums:
        diagnostics.append("subject-checksums.txt: no subjects found")
    return checksums, tuple(diagnostics)


def read_zip_member(
    archive: zipfile.ZipFile,
    name: str,
    *,
    max_bytes: int,
) -> tuple[bytes | None, str | None]:
    try:
        info = archive.getinfo(name)
    except KeyError:
        return None, f"proof artifact missing required file {name!r}"
    if info.file_size > max_bytes:
        return None, f"{name} size {info.file_size} exceeds limit {max_bytes}"

    body = bytearray()
    try:
        with archive.open(info) as handle:
            while True:
                read_size = min(1024 * 1024, max_bytes - len(body) + 1)
                chunk = handle.read(read_size)
                if not chunk:
                    break
                body.extend(chunk)
                if len(body) > max_bytes:
                    return None, f"{name} decompressed size exceeds limit {max_bytes}"
    except (OSError, RuntimeError, NotImplementedError, zipfile.BadZipFile) as exc:
        return None, f"{name} could not be read from proof artifact: {exc}"
    return bytes(body), None


def load_proof_pack_zip(data: bytes) -> tuple[ProofPackArtifact | None, tuple[str, ...]]:
    if len(data) > PROOF_PACK_ARCHIVE_MAX_BYTES:
        return None, (
            f"proof artifact archive size {len(data)} exceeds limit "
            f"{PROOF_PACK_ARCHIVE_MAX_BYTES}",
        )
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile:
        return None, ("proof artifact is not a valid zip archive",)

    with archive:
        required = ("manifest.json", "subject-checksums.txt", "attestation-bundle.json")
        names = set(archive.namelist())
        missing = [name for name in required if name not in names]
        if missing:
            joined = ", ".join(missing)
            return None, (f"proof artifact missing required file(s): {joined}",)

        manifest_bytes, manifest_error = read_zip_member(
            archive,
            "manifest.json",
            max_bytes=PROOF_PACK_REQUIRED_MEMBER_MAX_BYTES,
        )
        checksums_bytes, checksums_error = read_zip_member(
            archive,
            "subject-checksums.txt",
            max_bytes=PROOF_PACK_REQUIRED_MEMBER_MAX_BYTES,
        )
        bundle_bytes, bundle_error = read_zip_member(
            archive,
            "attestation-bundle.json",
            max_bytes=PROOF_PACK_REQUIRED_MEMBER_MAX_BYTES,
        )
        member_errors = [error for error in (manifest_error, checksums_error, bundle_error) if error]
        if member_errors:
            return None, tuple(member_errors)

        try:
            manifest = json.loads(manifest_bytes.decode("utf-8"))
            bundle = json.loads(bundle_bytes.decode("utf-8"))
        except (KeyError, UnicodeDecodeError, json.JSONDecodeError) as exc:
            return None, (f"proof artifact could not be decoded: {exc}",)
    if not isinstance(manifest, dict):
        return None, ("proof manifest is not a JSON object",)
    if not isinstance(bundle, dict):
        return None, ("proof attestation bundle is not a JSON object",)

    try:
        checksums_text = checksums_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        return None, (f"subject-checksums.txt is not utf-8: {exc}",)
    checksums, diagnostics = parse_subject_checksums(checksums_text)
    if diagnostics:
        return None, diagnostics

    return (
        ProofPackArtifact(
            manifest=manifest,
            manifest_bytes=manifest_bytes,
            checksums=checksums,
            bundle=bundle,
            bundle_bytes=bundle_bytes,
        ),
        (),
    )


def download_proof_pack_artifact(
    api: GitHubApi,
    run_id: str,
) -> tuple[ProofPackArtifact | None, tuple[str, ...]]:
    try:
        response = api.request("GET", f"/actions/runs/{run_id}/artifacts")
    except TRANSIENT_REQUEST_ERRORS as exc:
        return None, (f"run {run_id}: could not list artifacts ({exc})",)
    if not isinstance(response, dict):
        return None, (f"run {run_id}: artifacts response is not an object",)
    raw_artifacts = response.get("artifacts")
    if not isinstance(raw_artifacts, list):
        return None, (f"run {run_id}: artifacts response missing artifacts list",)

    expected_name = proof_pack_artifact_name(run_id)
    candidates = [
        artifact
        for artifact in raw_artifacts
        if isinstance(artifact, dict)
        and artifact.get("name") == expected_name
        and artifact.get("expired") is not True
    ]
    if not candidates:
        return None, (f"run {run_id}: proof artifact {expected_name!r} not found or expired",)

    artifact = dict(candidates[0])
    download_url = artifact.get("archive_download_url")
    if not isinstance(download_url, str) or not download_url:
        return None, (f"run {run_id}: proof artifact missing archive_download_url",)
    try:
        data = api.download(download_url, max_bytes=PROOF_PACK_ARCHIVE_MAX_BYTES)
    except DownloadLimitExceeded as exc:
        return None, (f"run {run_id}: proof artifact exceeds size limit ({exc})",)
    except TRANSIENT_REQUEST_ERRORS as exc:
        return None, (f"run {run_id}: could not download proof artifact ({exc})",)
    pack, diagnostics = load_proof_pack_zip(data)
    return pack, tuple(f"run {run_id}: {line}" for line in diagnostics)


def decode_dsse_statement(bundle: dict[str, object]) -> tuple[dict[str, object] | None, tuple[str, ...]]:
    raw_envelope = bundle.get("dsseEnvelope")
    if not isinstance(raw_envelope, dict):
        return None, ("attestation bundle missing dsseEnvelope object",)
    payload = raw_envelope.get("payload")
    if not isinstance(payload, str) or not payload:
        return None, ("attestation bundle missing dsseEnvelope.payload",)
    try:
        statement_bytes = base64.b64decode(payload, validate=True)
        statement = json.loads(statement_bytes.decode("utf-8"))
    except (ValueError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        return None, (f"attestation bundle payload could not be decoded: {exc}",)
    if not isinstance(statement, dict):
        return None, ("attestation statement is not a JSON object",)
    return statement, ()


def statement_subject_digests(statement: dict[str, object]) -> tuple[dict[str, str], tuple[str, ...]]:
    subjects = statement.get("subject")
    if not isinstance(subjects, list):
        return {}, ("attestation statement missing subject list",)
    digests: dict[str, str] = {}
    diagnostics: list[str] = []
    for index, raw_subject in enumerate(subjects):
        if not isinstance(raw_subject, dict):
            diagnostics.append(f"attestation subject[{index}] is not an object")
            continue
        name = raw_subject.get("name")
        raw_digest = raw_subject.get("digest")
        if not isinstance(name, str) or not name:
            diagnostics.append(f"attestation subject[{index}] missing name")
            continue
        if not isinstance(raw_digest, dict):
            diagnostics.append(f"attestation subject {name!r} missing digest object")
            continue
        digest = raw_digest.get("sha256")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            diagnostics.append(f"attestation subject {name!r} missing sha256 digest")
            continue
        digests[name] = digest
    if not digests:
        diagnostics.append("attestation statement has no valid subjects")
    return digests, tuple(diagnostics)


def manifest_subjects(manifest: dict[str, object]) -> tuple[dict[str, str], tuple[str, ...]]:
    raw_pack = manifest.get("proof_pack")
    if not isinstance(raw_pack, dict):
        return {}, ("proof manifest missing proof_pack object",)
    raw_subjects = raw_pack.get("subjects")
    if not isinstance(raw_subjects, list):
        return {}, ("proof manifest missing proof_pack.subjects list",)
    subjects: dict[str, str] = {}
    diagnostics: list[str] = []
    for index, raw_subject in enumerate(raw_subjects):
        if not isinstance(raw_subject, dict):
            diagnostics.append(f"proof_pack.subjects[{index}] is not an object")
            continue
        path = raw_subject.get("path")
        digest = raw_subject.get("sha256")
        if not isinstance(path, str) or not path:
            diagnostics.append(f"proof_pack.subjects[{index}] missing path")
            continue
        if not isinstance(digest, str) or not digest.startswith("sha256:"):
            diagnostics.append(f"proof_pack.subjects[{index}] missing sha256: digest")
            continue
        value = digest.removeprefix("sha256:")
        if not re.fullmatch(r"[0-9a-f]{64}", value):
            diagnostics.append(f"proof_pack.subjects[{index}] malformed sha256 digest")
            continue
        subjects[path] = value
    if not subjects:
        diagnostics.append("proof manifest has no valid proof_pack.subjects")
    return subjects, tuple(diagnostics)


def validate_attestation_statement(
    pack: ProofPackArtifact,
) -> tuple[dict[str, object] | None, tuple[str, ...]]:
    statement, diagnostics = decode_dsse_statement(pack.bundle)
    if statement is None:
        return None, diagnostics

    problems: list[str] = []
    if statement.get("_type") != "https://in-toto.io/Statement/v1":
        problems.append("attestation statement _type is not in-toto Statement v1")
    if statement.get("predicateType") != "https://slsa.dev/provenance/v1":
        problems.append("attestation statement predicateType is not SLSA provenance v1")

    statement_digests, subject_diagnostics = statement_subject_digests(statement)
    problems.extend(subject_diagnostics)
    manifest_digests, manifest_diagnostics = manifest_subjects(pack.manifest)
    problems.extend(manifest_diagnostics)

    expected_manifest_digest = sha256_hex(pack.manifest_bytes)
    recorded_manifest_digest = statement_digests.get("assay-runner-proof-upload/manifest.json")
    if recorded_manifest_digest != expected_manifest_digest:
        problems.append(
            "attestation statement does not bind manifest.json digest "
            f"{expected_manifest_digest}"
        )
    checksummed_manifest_digest = pack.checksums.get("assay-runner-proof-upload/manifest.json")
    if checksummed_manifest_digest != expected_manifest_digest:
        problems.append("subject-checksums.txt does not bind manifest.json digest")

    for path, digest in manifest_digests.items():
        if pack.checksums.get(path) != digest:
            problems.append(f"{path}: manifest digest is not present in subject-checksums.txt")
        if statement_digests.get(path) != digest:
            problems.append(f"{path}: manifest digest is not present in attestation subjects")

    expected_subject_paths = set(manifest_digests)
    expected_subject_paths.add("assay-runner-proof-upload/manifest.json")
    extra_checksums = sorted(set(pack.checksums) - expected_subject_paths)
    if extra_checksums:
        problems.append(f"subject-checksums.txt has unexpected subject(s): {extra_checksums!r}")
    extra_statement_subjects = sorted(set(statement_digests) - expected_subject_paths)
    if extra_statement_subjects:
        problems.append(f"attestation statement has unexpected subject(s): {extra_statement_subjects!r}")

    predicate = statement.get("predicate")
    if not isinstance(predicate, dict):
        problems.append("attestation statement missing predicate object")

    if problems:
        return None, tuple(problems)
    return statement, ()


def run_gh_attestation_verify(
    api: GitHubApi,
    pack: ProofPackArtifact,
) -> tuple[object | None, tuple[str, ...]]:
    if shutil.which("gh") is None:
        return None, ("gh CLI not available for attestation verification",)
    with tempfile.TemporaryDirectory(prefix="assay-lane-attestation-") as tmp:
        root = Path(tmp)
        manifest_path = root / "manifest.json"
        bundle_path = root / "attestation-bundle.json"
        manifest_path.write_bytes(pack.manifest_bytes)
        bundle_path.write_bytes(pack.bundle_bytes)
        env = dict(os.environ)
        if not env.get("GH_TOKEN"):
            env["GH_TOKEN"] = api.token
        try:
            completed = subprocess.run(
                [
                    "gh",
                    "attestation",
                    "verify",
                    str(manifest_path),
                    "--bundle",
                    str(bundle_path),
                    "--repo",
                    api.repo,
                    "--format",
                    "json",
                ],
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=env,
            )
        except OSError as exc:
            return None, (f"gh attestation verify could not run: {exc}",)
    if completed.returncode != 0:
        diagnostic = completed.stderr.strip() or completed.stdout.strip() or "unknown error"
        return None, (f"gh attestation verify failed: {diagnostic}",)
    try:
        parsed = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        return None, (f"gh attestation verify output was not JSON: {exc}",)
    return parsed, ()


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
        content_provenance_paths=tuple(str(path) for path in manifest["content_provenance_paths"]),
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
        # The proof consumer runs on GitHub-hosted Ubuntu, not on the delegated
        # bpf host. Changes to it are covered by the head-running
        # assay-runner-lane-check-self-test pre-commit hook, whose synthetic
        # fixture exercises find_valid_attested_proof end to end.
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


def content_provenance_covers_path(path: str, config: GatedPathConfig | None = None) -> bool:
    gated_paths = config or load_gated_path_config()
    return any(starts(path, prefix) for prefix in gated_paths.content_provenance_paths)


def uncovered_content_provenance_files(files: Iterable[str]) -> tuple[str, ...]:
    gated_paths = load_gated_path_config()
    uncovered: list[str] = []
    for path in files:
        gate, _reason = classify_file(path)
        if gate == Gate.NONE:
            continue
        if not content_provenance_covers_path(path, gated_paths):
            uncovered.append(path)
    return tuple(uncovered)


def extract_proof_path_tree_oids(manifest: dict[str, object]) -> tuple[dict[str, str], tuple[str, ...]]:
    gated_paths = load_gated_path_config()
    raw_content = manifest.get("content_provenance")
    if not isinstance(raw_content, dict):
        return {}, ("proof manifest missing content_provenance object",)
    raw_trees = raw_content.get("path_trees")
    if not isinstance(raw_trees, dict):
        return {}, ("proof manifest missing content_provenance.path_trees object",)

    oids: dict[str, str] = {}
    diagnostics: list[str] = []
    for path in gated_paths.content_provenance_paths:
        raw_entry = raw_trees.get(path)
        if not isinstance(raw_entry, dict):
            diagnostics.append(f"{path}: missing proof tree entry")
            continue
        oid = raw_entry.get("oid")
        error = raw_entry.get("error")
        if not isinstance(oid, str) or not oid:
            diagnostics.append(f"{path}: missing proof tree oid")
            continue
        if error not in (None, ""):
            diagnostics.append(f"{path}: proof tree error {error!r}")
            continue
        oids[path] = oid
    return oids, tuple(diagnostics)


def compare_content_path_trees(
    proof_manifest: dict[str, object],
    current_trees: dict[str, str],
) -> ContentTreeComparison:
    proof_oids, diagnostics = extract_proof_path_tree_oids(proof_manifest)
    if diagnostics:
        return ContentTreeComparison(False, diagnostics)

    mismatches: list[str] = []
    for path, proof_oid in proof_oids.items():
        current_oid = current_trees.get(path)
        if not current_oid:
            mismatches.append(f"{path}: missing current tree oid")
        elif current_oid != proof_oid:
            mismatches.append(f"{path}: proof tree {proof_oid} != current tree {current_oid}")
    if mismatches:
        return ContentTreeComparison(False, tuple(mismatches))
    return ContentTreeComparison(True, ())


def current_content_path_trees(head_sha: str) -> tuple[dict[str, str], tuple[str, ...]]:
    trees: dict[str, str] = {}
    diagnostics: list[str] = []
    for path in load_gated_path_config().content_provenance_paths:
        try:
            oid = subprocess.check_output(
                ["git", "rev-parse", f"{head_sha}:{path}"],
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            diagnostics.append(f"{path}: could not resolve current tree oid at {head_sha}")
            continue
        if not oid:
            diagnostics.append(f"{path}: empty current tree oid at {head_sha}")
            continue
        trees[path] = oid
    return trees, tuple(diagnostics)


def content_tree_proof_accepts_head(
    manifest: dict[str, object],
    pr: PullRequest,
) -> ContentTreeComparison:
    uncovered = uncovered_content_provenance_files(pr.files)
    source = manifest.get("source")
    proof_head = str(source.get("head_sha") or "") if isinstance(source, dict) else ""
    if uncovered and proof_head != pr.head_sha:
        return ContentTreeComparison(
            False,
            tuple(f"{path}: gated path is not covered by content-provenance trees" for path in uncovered),
        )
    try:
        fetch_ref_for_diff(pr.number, pr.head_sha)
    except (OSError, subprocess.CalledProcessError) as exc:
        return ContentTreeComparison(False, (f"could not fetch PR head for content-tree comparison: {exc}",))
    current_trees, diagnostics = current_content_path_trees(pr.head_sha)
    if diagnostics:
        return ContentTreeComparison(False, diagnostics)
    return compare_content_path_trees(manifest, current_trees)


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


GH_ATTESTATION_CLAIM_KEYS = frozenset(
    {
        "githubWorkflowName",
        "githubWorkflowTrigger",
        "githubWorkflowRepository",
        "sourceRepositoryDigest",
        "githubWorkflowSHA",
        "sourceRepositoryRef",
        "runnerEnvironment",
    }
)


def iter_dicts(value: object) -> Iterator[dict[str, object]]:
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from iter_dicts(child)
    elif isinstance(value, list):
        for child in value:
            yield from iter_dicts(child)


def attestation_verified_claims(verification: object) -> dict[str, object]:
    """Return a verified certificate claim object gh exposes.

    `gh attestation verify --format json` has evolved shape across releases;
    the current output nests certificate claims under each verified record.
    Keep wrapper-shape handling tolerant, but fail closed if no object carries
    any of the claim keys the lane-check validates.
    """

    for candidate in iter_dicts(verification):
        if GH_ATTESTATION_CLAIM_KEYS.intersection(candidate):
            return candidate
    return {}


def validate_gh_attestation_claims(
    verification: object,
    manifest: dict[str, object],
    api: GitHubApi,
) -> tuple[str, ...]:
    claims = attestation_verified_claims(verification)
    if not claims:
        return ("gh attestation verify returned no claim object",)

    source = manifest.get("source")
    expected_sha = str(source.get("head_sha") or "") if isinstance(source, dict) else ""
    expected_ref = str(source.get("ref") or "") if isinstance(source, dict) else ""
    expected_workflow_name = (
        str(source.get("workflow_name") or "") if isinstance(source, dict) else DELEGATED_WORKFLOW_NAME
    )
    expected_workflow_sha = str(source.get("workflow_sha") or "") if isinstance(source, dict) else ""

    checks = {
        "githubWorkflowName": expected_workflow_name,
        "githubWorkflowTrigger": "workflow_dispatch",
        "githubWorkflowRepository": api.repo,
        "sourceRepositoryDigest": expected_sha,
        "githubWorkflowSHA": expected_workflow_sha or expected_sha,
        "sourceRepositoryRef": expected_ref,
        "runnerEnvironment": "self-hosted",
    }
    diagnostics: list[str] = []
    for key, expected in checks.items():
        if not expected:
            continue
        if key not in claims:
            diagnostics.append(f"attestation claim {key} missing")
            continue
        got = claims.get(key)
        if got != expected:
            diagnostics.append(f"attestation claim {key}={got!r}, expected {expected!r}")
    return tuple(diagnostics)


def validate_attestation_predicate(
    statement: dict[str, object],
    manifest: dict[str, object],
    run: dict[str, object],
    api: GitHubApi,
) -> tuple[str, ...]:
    source = manifest.get("source")
    if not isinstance(source, dict):
        return ("proof manifest missing source object",)
    predicate = statement.get("predicate")
    if not isinstance(predicate, dict):
        return ("attestation statement missing predicate object",)
    build_definition = predicate.get("buildDefinition")
    run_details = predicate.get("runDetails")
    diagnostics: list[str] = []
    if not isinstance(build_definition, dict):
        diagnostics.append("attestation predicate missing buildDefinition object")
    else:
        external = build_definition.get("externalParameters")
        internal = build_definition.get("internalParameters")
        if not isinstance(external, dict):
            diagnostics.append("attestation predicate missing externalParameters object")
        else:
            workflow = external.get("workflow")
            if not isinstance(workflow, dict):
                diagnostics.append("attestation predicate missing workflow external parameter")
            else:
                expected_repo_url = f"https://github.com/{api.repo}"
                if workflow.get("path") != DELEGATED_WORKFLOW_PATH:
                    diagnostics.append(
                        f"attestation workflow path {workflow.get('path')!r}, "
                        f"expected {DELEGATED_WORKFLOW_PATH!r}"
                    )
                if workflow.get("repository") != expected_repo_url:
                    diagnostics.append(
                        f"attestation workflow repository {workflow.get('repository')!r}, "
                        f"expected {expected_repo_url!r}"
                    )
                if workflow.get("ref") != source.get("ref"):
                    diagnostics.append(
                        f"attestation workflow ref {workflow.get('ref')!r}, "
                        f"expected {source.get('ref')!r}"
                    )
        if not isinstance(internal, dict):
            diagnostics.append("attestation predicate missing internalParameters object")
        else:
            github = internal.get("github")
            if not isinstance(github, dict):
                diagnostics.append("attestation predicate missing github internal parameters")
            else:
                if github.get("event_name") != "workflow_dispatch":
                    diagnostics.append(
                        f"attestation event_name {github.get('event_name')!r}, "
                        "expected 'workflow_dispatch'"
                    )
                if github.get("runner_environment") != "self-hosted":
                    diagnostics.append(
                        f"attestation runner_environment {github.get('runner_environment')!r}, "
                        "expected 'self-hosted'"
                    )

        resolved = build_definition.get("resolvedDependencies")
        if not isinstance(resolved, list):
            diagnostics.append("attestation predicate missing resolvedDependencies list")
        else:
            expected_sha = source.get("head_sha")
            found = False
            for dependency in resolved:
                if not isinstance(dependency, dict):
                    continue
                digest = dependency.get("digest")
                if isinstance(digest, dict) and digest.get("gitCommit") == expected_sha:
                    found = True
                    break
            if not found:
                diagnostics.append("attestation resolvedDependencies missing source head commit")

    if not isinstance(run_details, dict):
        diagnostics.append("attestation predicate missing runDetails object")
    else:
        metadata = run_details.get("metadata")
        if isinstance(metadata, dict):
            invocation = str(metadata.get("invocationId") or "")
            expected_prefix = f"{source.get('run_url')}/attempts/{source.get('run_attempt')}"
            if invocation and invocation != expected_prefix:
                diagnostics.append(
                    f"attestation invocationId {invocation!r}, expected {expected_prefix!r}"
                )

    if str(source.get("run_id") or "") != str(run.get("id") or ""):
        diagnostics.append("proof manifest run_id does not match workflow run")
    if str(source.get("run_url") or "") != str(run.get("html_url") or ""):
        diagnostics.append("proof manifest run_url does not match workflow run")
    if str(source.get("head_sha") or "") != str(run.get("head_sha") or ""):
        diagnostics.append("proof manifest head_sha does not match workflow run")
    if str(source.get("repository") or "") != api.repo:
        diagnostics.append("proof manifest repository does not match current repository")
    if str(source.get("workflow_name") or "") != DELEGATED_WORKFLOW_NAME:
        diagnostics.append("proof manifest workflow_name does not match delegated workflow")
    if str(source.get("workflow_path") or "") != DELEGATED_WORKFLOW_PATH:
        diagnostics.append("proof manifest workflow_path does not match delegated workflow")
    return tuple(diagnostics)


def validate_proof_manifest_semantics(
    manifest: dict[str, object],
    run: dict[str, object],
    expected: Gate,
    pr: PullRequest,
    api: GitHubApi,
) -> tuple[str, ...]:
    diagnostics: list[str] = []
    if manifest.get("schema") != PROOF_PACK_SCHEMA:
        diagnostics.append(f"proof manifest schema is {manifest.get('schema')!r}")
    if manifest.get("kind") != PROOF_PACK_KIND:
        diagnostics.append(f"proof manifest kind is {manifest.get('kind')!r}")
    if manifest.get("claim_ceiling") != PROOF_PACK_CLAIM_CEILING:
        diagnostics.append("proof manifest claim_ceiling is not the delegated gate ceiling")

    source = manifest.get("source")
    if not isinstance(source, dict):
        diagnostics.append("proof manifest missing source object")
    else:
        expected_source = {
            "repository": api.repo,
            "workflow_name": DELEGATED_WORKFLOW_NAME,
            "workflow_path": DELEGATED_WORKFLOW_PATH,
            "run_id": str(run.get("id") or ""),
            "run_url": str(run.get("html_url") or ""),
            "head_sha": str(run.get("head_sha") or ""),
        }
        for key, expected_value in expected_source.items():
            if str(source.get(key) or "") != expected_value:
                diagnostics.append(
                    f"proof manifest source.{key}={source.get(key)!r}, "
                    f"expected {expected_value!r}"
                )

    inputs = manifest.get("inputs")
    if not isinstance(inputs, dict):
        diagnostics.append("proof manifest missing inputs object")
    else:
        gate = str(inputs.get("gates") or "")
        if gate not in accepted_gates(expected):
            diagnostics.append(
                f"proof manifest inputs.gates={gate!r}, "
                f"expected one of {sorted(accepted_gates(expected))!r}"
            )
        if inputs.get("build_ebpf") != "true":
            diagnostics.append("proof manifest inputs.build_ebpf must be 'true'")

    tree_check = content_tree_proof_accepts_head(manifest, pr)
    if not tree_check.accepted:
        diagnostics.extend(tree_check.diagnostics)
    return tuple(diagnostics)


def run_ids_from_event() -> list[str]:
    if os.environ.get("GITHUB_EVENT_NAME") != "workflow_run":
        return []
    run_id = os.environ.get("DELEGATED_WORKFLOW_RUN_ID", "")
    return [run_id] if re.fullmatch(r"[0-9]+", run_id) else []


def find_valid_attested_proof(
    api: GitHubApi,
    run_ids: Iterable[str],
    pr: PullRequest,
    expected: Gate,
) -> AttestedProofCheck:
    diagnostics: list[str] = []
    for run_id in run_ids:
        try:
            run = dict(api.request("GET", f"/actions/runs/{run_id}"))
        except TRANSIENT_REQUEST_ERRORS as exc:
            diagnostics.append(f"run {run_id}: could not read workflow run ({exc})")
            continue
        basic = delegated_run_diagnostic(run, run_id, str(run.get("head_sha") or ""))
        if basic is not None:
            diagnostics.append(basic)
            continue

        pack, pack_diagnostics = download_proof_pack_artifact(api, run_id)
        if pack is None:
            diagnostics.extend(pack_diagnostics)
            continue

        statement, statement_diagnostics = validate_attestation_statement(pack)
        if statement is None:
            diagnostics.extend(f"run {run_id}: {line}" for line in statement_diagnostics)
            continue

        semantic_diagnostics = validate_proof_manifest_semantics(
            pack.manifest,
            run,
            expected,
            pr,
            api,
        )
        if semantic_diagnostics:
            diagnostics.extend(f"run {run_id}: {line}" for line in semantic_diagnostics)
            continue

        predicate_diagnostics = validate_attestation_predicate(statement, pack.manifest, run, api)
        if predicate_diagnostics:
            diagnostics.extend(f"run {run_id}: {line}" for line in predicate_diagnostics)
            continue

        verification, verify_diagnostics = run_gh_attestation_verify(api, pack)
        if verification is None:
            diagnostics.extend(f"run {run_id}: {line}" for line in verify_diagnostics)
            continue
        claim_diagnostics = validate_gh_attestation_claims(verification, pack.manifest, api)
        if claim_diagnostics:
            diagnostics.extend(f"run {run_id}: {line}" for line in claim_diagnostics)
            continue

        return AttestedProofCheck(True, run, tuple(diagnostics))
    return AttestedProofCheck(False, None, tuple(diagnostics))


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
            "PASS: delegated runner proof accepted for this PR."
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

    seen_run_ids: set[str] = set()
    run_ids: list[str] = []
    for run_id in [*run_ids_from_event(), *run_ids_from_text(api.repo, text)]:
        if run_id not in seen_run_ids:
            seen_run_ids.add(run_id)
            run_ids.append(run_id)

    attested = find_valid_attested_proof(api, run_ids, pr, classification.gate)
    valid_run, run_diagnostics = find_valid_delegated_run(api, run_ids, pr.head_sha)
    sha_ok = text_mentions_head_sha(text, pr.head_sha)
    gate_ok = recorded_gate_ok(text, classification.gate)
    fallback_passed = valid_run is not None and sha_ok and gate_ok
    passed = attested.accepted or fallback_passed

    details: list[str] = []
    if not run_ids:
        details.append("No `Runner Spike Delegated` run URL or workflow_run event was available.")
    if not attested.accepted:
        details.append("No attested delegated proof pack matched this PR's gated content.")
    if valid_run is None and not attested.accepted:
        details.append("No legacy delegated run URL matched the PR head SHA.")
    if not sha_ok and not attested.accepted:
        details.append("The PR body/comments do not record the current PR head SHA or its 12-character prefix.")
    if not gate_ok and not attested.accepted:
        details.append(
            f"The PR body/comments do not record `gate: {classification.gate.label}`"
            + (" or `gate: all`." if classification.gate != Gate.ALL else ".")
        )
    if attested.diagnostics and not attested.accepted:
        details.append(
            "Attested proof diagnostics:\n"
            + "\n".join(f"- {line}" for line in attested.diagnostics[:12])
        )
    if run_diagnostics:
        details.append("Legacy run diagnostics:\n" + "\n".join(f"- {line}" for line in run_diagnostics[:8]))
    if attested.accepted and attested.run is not None:
        details.append(f"Matched attested delegated proof pack: {attested.run.get('html_url')}")
    elif fallback_passed and valid_run is not None:
        details.append(f"Matched legacy delegated run proof: {valid_run.get('html_url')}")

    detail = "\n\n".join(details)
    body = comment_body(classification, pr, passed, detail)
    maybe_comment(api, pr.number, comments, body, comment=comment)
    if attested.accepted:
        status_description = f"attested delegated proof accepted: gates={classification.gate.label}"
    elif fallback_passed:
        status_description = f"legacy delegated proof accepted: gates={classification.gate.label}"
    else:
        status_description = f"delegated proof required: gates={classification.gate.label}"
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
    assert "crates/assay-runner-core" in gated_paths.content_provenance_paths
    assert "scripts/ci" in gated_paths.content_provenance_paths

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
    assert uncovered_content_provenance_files(["crates/assay-runner-core/src/lib.rs"]) == ()
    assert uncovered_content_provenance_files(["Cargo.lock"]) == ("Cargo.lock",)
    _test_content_tree_comparison()

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
    _test_attested_proof_pack_helpers()

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
    _test_artifact_redirect_drops_authorization()

    # Phase 2D Slice 6B mechanical absence check: assert that assay-cli no
    # longer consumes the assay-runner-spike wrapper. The check is
    # encoded here in self_test rather than as a runtime workflow step
    # so it travels with the lane-check helper and runs on every PR
    # touching the classifier itself, including any future PR that
    # might silently re-introduce a spike dependency.
    _assert_assay_cli_does_not_consume_spike()


def _test_content_tree_comparison() -> None:
    gated_paths = load_gated_path_config()
    proof_manifest = {
        "content_provenance": {
            "path_trees": {
                path: {"oid": f"oid:{path}", "error": None}
                for path in gated_paths.content_provenance_paths
            }
        }
    }
    current = {
        path: f"oid:{path}"
        for path in gated_paths.content_provenance_paths
    }
    accepted = compare_content_path_trees(proof_manifest, current)
    assert accepted.accepted, accepted.diagnostics

    changed = dict(current)
    changed[gated_paths.content_provenance_paths[0]] = "different"
    mismatch = compare_content_path_trees(proof_manifest, changed)
    assert not mismatch.accepted
    assert "!=" in mismatch.diagnostics[0]

    missing_entry_manifest = {
        "content_provenance": {
            "path_trees": {
                path: {"oid": f"oid:{path}", "error": None}
                for path in gated_paths.content_provenance_paths[1:]
            }
        }
    }
    missing = compare_content_path_trees(missing_entry_manifest, current)
    assert not missing.accepted
    assert "missing proof tree entry" in missing.diagnostics[0]

    error_manifest = {
        "content_provenance": {
            "path_trees": {
                path: {"oid": f"oid:{path}", "error": None}
                for path in gated_paths.content_provenance_paths
            }
        }
    }
    first = gated_paths.content_provenance_paths[0]
    error_manifest["content_provenance"]["path_trees"][first]["error"] = "missing_at_head"
    error = compare_content_path_trees(error_manifest, current)
    assert not error.accepted
    assert "proof tree error" in error.diagnostics[0]


def _test_attested_proof_pack_helpers() -> None:
    manifest = {
        "schema": PROOF_PACK_SCHEMA,
        "kind": PROOF_PACK_KIND,
        "claim_ceiling": PROOF_PACK_CLAIM_CEILING,
        "source": {
            "head_sha": "abc123",
            "ref": "refs/heads/test",
            "repository": "Rul1an/assay",
            "run_attempt": "1",
            "run_id": "12345",
            "run_url": "https://github.com/Rul1an/assay/actions/runs/12345",
            "workflow_name": DELEGATED_WORKFLOW_NAME,
            "workflow_path": DELEGATED_WORKFLOW_PATH,
            "workflow_sha": "abc123",
        },
        "inputs": {"gates": "all", "build_ebpf": "true"},
        "content_provenance": {"path_trees": {}},
        "proof_pack": {
            "subjects": [
                {"path": "target/assay-ebpf.o", "sha256": "sha256:" + "a" * 64},
            ]
        },
    }
    manifest_bytes = json.dumps(manifest, sort_keys=True).encode("utf-8")
    statement = {
        "_type": "https://in-toto.io/Statement/v1",
        "predicateType": "https://slsa.dev/provenance/v1",
        "subject": [
            {
                "name": "assay-runner-proof-upload/manifest.json",
                "digest": {"sha256": sha256_hex(manifest_bytes)},
            },
            {"name": "target/assay-ebpf.o", "digest": {"sha256": "a" * 64}},
        ],
        "predicate": {
            "buildDefinition": {
                "externalParameters": {
                    "workflow": {
                        "ref": "refs/heads/test",
                        "repository": "https://github.com/Rul1an/assay",
                        "path": DELEGATED_WORKFLOW_PATH,
                    }
                },
                "internalParameters": {
                    "github": {
                        "event_name": "workflow_dispatch",
                        "runner_environment": "self-hosted",
                    }
                },
                "resolvedDependencies": [{"digest": {"gitCommit": "abc123"}}],
            },
            "runDetails": {
                "metadata": {
                    "invocationId": "https://github.com/Rul1an/assay/actions/runs/12345/attempts/1"
                }
            },
        },
    }
    bundle = {
        "dsseEnvelope": {
            "payload": base64.b64encode(json.dumps(statement).encode("utf-8")).decode("ascii")
        }
    }
    bundle_bytes = json.dumps(bundle, sort_keys=True).encode("utf-8")
    checksums = (
        f"{sha256_hex(manifest_bytes)}  assay-runner-proof-upload/manifest.json\n"
        f"{'a' * 64}  target/assay-ebpf.o\n"
    )
    archive_buffer = io.BytesIO()
    with zipfile.ZipFile(archive_buffer, "w") as archive:
        archive.writestr("manifest.json", manifest_bytes)
        archive.writestr("subject-checksums.txt", checksums)
        archive.writestr("attestation-bundle.json", bundle_bytes)

    pack, diagnostics = load_proof_pack_zip(archive_buffer.getvalue())
    assert pack is not None, diagnostics
    verified_statement, diagnostics = validate_attestation_statement(pack)
    assert verified_statement is not None, diagnostics
    run = {
        "name": DELEGATED_WORKFLOW_NAME,
        "event": "workflow_dispatch",
        "conclusion": "success",
        "id": 12345,
        "html_url": "https://github.com/Rul1an/assay/actions/runs/12345",
        "head_sha": "abc123",
    }
    assert validate_attestation_predicate(verified_statement, pack.manifest, run, GitHubApi("Rul1an/assay", "t")) == ()
    assert validate_gh_attestation_claims(
        [
            {"attestation": {"bundle": {"mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json"}}},
            {
                "verificationResult": {
                    "githubWorkflowName": DELEGATED_WORKFLOW_NAME,
                    "githubWorkflowTrigger": "workflow_dispatch",
                    "githubWorkflowRepository": "Rul1an/assay",
                    "sourceRepositoryDigest": "abc123",
                    "githubWorkflowSHA": "abc123",
                    "sourceRepositoryRef": "refs/heads/test",
                    "runnerEnvironment": "self-hosted",
                }
            }
        ],
        pack.manifest,
        GitHubApi("Rul1an/assay", "t"),
    ) == ()
    assert "attestation claim githubWorkflowName missing" in validate_gh_attestation_claims(
        [{"verificationResult": {"sourceRepositoryDigest": "abc123"}}],
        pack.manifest,
        GitHubApi("Rul1an/assay", "t"),
    )

    bad_manifest = dict(manifest)
    bad_manifest["proof_pack"] = {"subjects": [{"path": "target/assay-ebpf.o", "sha256": "sha256:" + "b" * 64}]}
    bad_pack = ProofPackArtifact(
        manifest=bad_manifest,
        manifest_bytes=json.dumps(bad_manifest).encode("utf-8"),
        checksums=pack.checksums,
        bundle=pack.bundle,
        bundle_bytes=pack.bundle_bytes,
    )
    assert validate_attestation_statement(bad_pack)[0] is None

    extra_checksum_pack = ProofPackArtifact(
        manifest=pack.manifest,
        manifest_bytes=pack.manifest_bytes,
        checksums={**pack.checksums, "unexpected.bin": "c" * 64},
        bundle=pack.bundle,
        bundle_bytes=pack.bundle_bytes,
    )
    extra_checksum_diagnostics = validate_attestation_statement(extra_checksum_pack)[1]
    assert any("unexpected subject" in line for line in extra_checksum_diagnostics)

    extra_statement = dict(statement)
    extra_statement["subject"] = [
        *statement["subject"],
        {"name": "unexpected.bin", "digest": {"sha256": "c" * 64}},
    ]
    extra_bundle = {
        "dsseEnvelope": {
            "payload": base64.b64encode(json.dumps(extra_statement).encode("utf-8")).decode("ascii")
        }
    }
    extra_statement_pack = ProofPackArtifact(
        manifest=pack.manifest,
        manifest_bytes=pack.manifest_bytes,
        checksums=pack.checksums,
        bundle=extra_bundle,
        bundle_bytes=json.dumps(extra_bundle, sort_keys=True).encode("utf-8"),
    )
    extra_statement_diagnostics = validate_attestation_statement(extra_statement_pack)[1]
    assert any("unexpected subject" in line for line in extra_statement_diagnostics)

    missing_resolved_statement = json.loads(json.dumps(statement))
    del missing_resolved_statement["predicate"]["buildDefinition"]["resolvedDependencies"]
    missing_resolved = validate_attestation_predicate(
        missing_resolved_statement,
        pack.manifest,
        run,
        GitHubApi("Rul1an/assay", "t"),
    )
    assert "attestation predicate missing resolvedDependencies list" in missing_resolved

    oversized_buffer = io.BytesIO()
    with zipfile.ZipFile(oversized_buffer, "w") as oversized_archive:
        oversized_archive.writestr("manifest.json", b"abcd")
    with zipfile.ZipFile(io.BytesIO(oversized_buffer.getvalue())) as oversized_archive:
        _bytes, member_error = read_zip_member(oversized_archive, "manifest.json", max_bytes=3)
    assert member_error is not None and "exceeds limit" in member_error

    corrupt_buffer = io.BytesIO()
    with zipfile.ZipFile(corrupt_buffer, "w", compression=zipfile.ZIP_STORED) as corrupt_archive:
        corrupt_archive.writestr("manifest.json", b'{"ok": true}')
    corrupt_bytes = bytearray(corrupt_buffer.getvalue())
    header_offset = corrupt_bytes.index(b"PK\x03\x04")
    name_length = struct.unpack_from("<H", corrupt_bytes, header_offset + 26)[0]
    extra_length = struct.unpack_from("<H", corrupt_bytes, header_offset + 28)[0]
    payload_offset = header_offset + 30 + name_length + extra_length
    corrupt_bytes[payload_offset] ^= 0xFF
    with zipfile.ZipFile(io.BytesIO(corrupt_bytes)) as corrupt_archive:
        _bytes, corrupt_error = read_zip_member(
            corrupt_archive,
            "manifest.json",
            max_bytes=PROOF_PACK_REQUIRED_MEMBER_MAX_BYTES,
        )
    assert corrupt_error is not None and "could not be read" in corrupt_error

    class _FakeAttestedApi:
        repo = "Rul1an/assay"

        def request(self, method: str, path: str, payload: object | None = None) -> object:
            assert method == "GET"
            if path == "/actions/runs/12345":
                return run
            if path == "/actions/runs/12345/artifacts":
                return {
                    "artifacts": [
                        {
                            "name": proof_pack_artifact_name("12345"),
                            "expired": False,
                            "archive_download_url": (
                                "https://api.github.com/repos/Rul1an/assay/actions/artifacts/1/zip"
                            ),
                        }
                    ]
                }
            raise AssertionError(f"unexpected request path {path!r}")

        def download(self, path_or_url: str, *, max_bytes: int | None = None) -> bytes:
            assert path_or_url.endswith("/actions/artifacts/1/zip")
            assert max_bytes == PROOF_PACK_ARCHIVE_MAX_BYTES
            return archive_buffer.getvalue()

    def _fake_content_tree_accepts_head(
        _manifest: dict[str, object],
        _pr: PullRequest,
    ) -> ContentTreeComparison:
        return ContentTreeComparison(True, ())

    def _fake_gh_verify(_api: GitHubApi, _pack: ProofPackArtifact):
        return (
            [
                {
                    "verificationResult": {
                        "githubWorkflowName": DELEGATED_WORKFLOW_NAME,
                        "githubWorkflowTrigger": "workflow_dispatch",
                        "githubWorkflowRepository": "Rul1an/assay",
                        "sourceRepositoryDigest": "abc123",
                        "githubWorkflowSHA": "abc123",
                        "sourceRepositoryRef": "refs/heads/test",
                        "runnerEnvironment": "self-hosted",
                    }
                }
            ],
            (),
        )

    old_tree_accepts = globals()["content_tree_proof_accepts_head"]
    old_gh_verify = globals()["run_gh_attestation_verify"]
    try:
        globals()["content_tree_proof_accepts_head"] = _fake_content_tree_accepts_head
        globals()["run_gh_attestation_verify"] = _fake_gh_verify
        accepted = find_valid_attested_proof(
            _FakeAttestedApi(),
            ["12345"],
            PullRequest(
                number=7,
                title="synthetic accept path",
                body="",
                author_login="Rul1an",
                head_sha="abc123",
                files=(".github/workflows/runner-spike-delegated.yml",),
            ),
            Gate.ALL,
        )
    finally:
        globals()["content_tree_proof_accepts_head"] = old_tree_accepts
        globals()["run_gh_attestation_verify"] = old_gh_verify
    assert accepted.accepted, accepted.diagnostics

    old_name = os.environ.get("GITHUB_EVENT_NAME")
    old_run_id = os.environ.get("DELEGATED_WORKFLOW_RUN_ID")
    os.environ["GITHUB_EVENT_NAME"] = "workflow_run"
    os.environ["DELEGATED_WORKFLOW_RUN_ID"] = "98765"
    try:
        assert run_ids_from_event() == ["98765"]
    finally:
        if old_name is None:
            os.environ.pop("GITHUB_EVENT_NAME", None)
        else:
            os.environ["GITHUB_EVENT_NAME"] = old_name
        if old_run_id is None:
            os.environ.pop("DELEGATED_WORKFLOW_RUN_ID", None)
        else:
            os.environ["DELEGATED_WORKFLOW_RUN_ID"] = old_run_id


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


def _test_artifact_redirect_drops_authorization() -> None:
    request = urllib.request.Request(
        "https://api.github.com/repos/Rul1an/assay/actions/artifacts/1/zip",
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": "Bearer test-token",
        },
        method="GET",
    )
    redirected = _DropAuthorizationOnRedirect().redirect_request(
        request,
        None,
        302,
        "Found",
        {},
        "https://pipelines.actions.githubusercontent.com/signed/archive.zip?sig=abc",
    )
    assert redirected is not None
    assert redirected.full_url.startswith("https://pipelines.actions.githubusercontent.com/")
    assert redirected.get_header("Authorization") is None
    assert redirected.get_header("Accept") == "application/vnd.github+json"


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
