#!/usr/bin/env python3
"""Contract tests for the clean-room conformance activation kit."""

from __future__ import annotations

import ast
import gzip
import hashlib
import importlib
import inspect
import io
import json
import re
import os
import shlex
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import time
import unittest
from pathlib import Path
from unittest import mock

CORPUS_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = CORPUS_DIR.parents[1]

# A synthetic string carrying one instance of each thing the canonicalization vectors must never
# contain. Used only to prove the leak patterns in `test_pack_ships_the_canonicalization_vectors...`
# actually match; it is never packed and never read by anything else.
LEAKY_FIXTURE = (
    'see case-07 for the shape; ok-privileged-call is an accept and '
    'bad-arg-drift is a reject; expected_outcome=accept; verdict: pass'
)
BUILD_SCRIPT = CORPUS_DIR / "scripts" / "build_clean_room_pack.py"
SCORE_SCRIPT = CORPUS_DIR / "scripts" / "score_candidate.py"
VALIDATE_SCRIPT = CORPUS_DIR / "scripts" / "validate_run_record.py"
VALIDATE_CANDIDATE_RELEASE = (
    CORPUS_DIR / "scripts" / "validate_candidate_release.py"
)
CANDIDATE_RELEASE = CORPUS_DIR / "candidate-release.json"
RELEASE_WORKFLOW = (
    REPO_ROOT / ".github/workflows/privileged-mcp-action-pack-release.yml"
)
OCI_CANDIDATE_WORKFLOW = (
    REPO_ROOT / ".github/workflows/privileged-mcp-action-oci-candidate.yml"
)
CONFORMANCE_WORKFLOW = (
    REPO_ROOT / ".github/workflows/privileged-mcp-action-conformance.yml"
)
OCI_WORKFLOW_PATH = ".github/workflows/privileged-mcp-action-oci-candidate.yml"
OCI_CHECKOUT_PIN = "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09"
OCI_SETUP_PYTHON_PIN = "actions/setup-python@ece7cb06caefa5fff74198d8649806c4678c61a1"
OCI_UPLOAD_PIN = "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
OCI_EXECUTOR = (
    "conformance/privileged-mcp-action-v0/scripts/oci_candidate_executor.py"
)
OCI_SIGNER_WORKFLOW = (
    "Rul1an/assay/.github/workflows/privileged-mcp-action-pack-release.yml"
)
OCI_PYTHON_VERSION = 'python-version: "3.13.8"'
OCI_SOURCE_DIGEST_SHAPE = '[[ "$source_digest" =~ ^[0-9a-f]{40}$ ]]'
OCI_SOURCE_DIGEST_GUARD = OCI_SOURCE_DIGEST_SHAPE + " || exit 2"
OCI_CAPTURE_UPLOAD_PATH = "${{ runner.temp }}/oci-capture/candidate_capture.v0"
OCI_IMPLEMENTATION_ID_ENV = "IMPLEMENTATION_ID: ${{ inputs.implementation_id }}"
OCI_IMPLEMENTATION_ID_ARGV = '--implementation-id "$IMPLEMENTATION_ID"'
OCI_UPLOAD_STEP = "Upload validated capture"
USES_SHA_RE = re.compile(r"uses:\s*\S+@([0-9a-f]{40}|[^\s#]+)")
# Ordered capture-job shape. Env-key sets and the step-name tuple are one table
# so a skipped setup, extra step, leaked token, or duplicate upload cannot
# look green by matching a looser presence pin.
OCI_STEP_ENV_KEYS = {
    "Check out current main": frozenset(),
    "Set up Python": frozenset(),
    "Require trusted main": frozenset(),
    "Resolve published pack tag": frozenset(),
    "Download attested pack": frozenset({"GH_TOKEN", "TAG"}),
    "Verify pack attestation": frozenset({"GH_TOKEN", "TAG"}),
    "Capture candidate observations": frozenset({"IMPLEMENTATION_ID"}),
    OCI_UPLOAD_STEP: frozenset(),
}
OCI_CAPTURE_STEP_NAMES = tuple(OCI_STEP_ENV_KEYS)
OCI_CAPTURE_JOB = "capture"
OCI_CAPTURE_JOB_KEYS = ("runs-on", "timeout-minutes", "steps")
OCI_DOCUMENT_KEYS = ("name", "on", "permissions", "concurrency", "jobs")
OCI_TAG_BINDING = "TAG: ${{ steps.candidate.outputs.tag }}"
OCI_ARTIFACT_NAME = "name: candidate-capture-v0"
OCI_STEP_KEYS = {
    "Check out current main": ("name", "uses", "with"),
    "Set up Python": ("name", "uses", "with"),
    "Require trusted main": ("name", "run"),
    "Resolve published pack tag": ("name", "id", "run"),
    "Download attested pack": ("name", "env", "run"),
    "Verify pack attestation": ("name", "env", "run"),
    "Capture candidate observations": ("name", "env", "run"),
    OCI_UPLOAD_STEP: ("name", "if", "uses", "with"),
}
OCI_STEP_WITH_KEYS = {
    "Check out current main": ("persist-credentials", "ref"),
    "Set up Python": ("python-version",),
    OCI_UPLOAD_STEP: ("name", "path", "if-no-files-found", "retention-days"),
}
CONFORMANCE_REQUIRED_PATHS = (
    "conformance/privileged-mcp-action-v0/**",
    ".github/actions/privileged-mcp-action-conformance/**",
    ".github/workflows/privileged-mcp-action-conformance.yml",
    ".github/workflows/privileged-mcp-action-pack-release.yml",
    OCI_WORKFLOW_PATH,
    "scripts/ci/check_clean_room_pack_reachable.py",
)
OCI_TOP_LEVEL_PERMISSIONS = ("contents: read",)
_STEP_FIELD_KEYS = frozenset(
    {
        "name",
        "id",
        "if",
        "env",
        "run",
        "uses",
        "with",
        "shell",
        "timeout-minutes",
        "continue-on-error",
        "working-directory",
    }
)

# Normalized `run:` sequences for the named capture steps. Copied from the
# committed YAML after joining `\` continuations and collapsing whitespace;
# not a second parser of workflow intent.
OCI_PINNED_STEP_SEQUENCES = {
    "Require trusted main": (
        "set -euo pipefail",
        'if [[ "$GITHUB_REF" != "refs/heads/main" ]]; then',
        'echo "oci capture must be dispatched from main, got $GITHUB_REF" >&2',
        "exit 2",
        "fi",
        'if [[ "$(git rev-parse HEAD)" != "$GITHUB_SHA" ]]; then',
        'echo "checked-out main does not match the workflow source commit" >&2',
        "exit 2",
        "fi",
    ),
    "Resolve published pack tag": (
        "set -euo pipefail",
        "python3"
        " conformance/privileged-mcp-action-v0/scripts/validate_candidate_release.py"
        " --candidate conformance/privileged-mcp-action-v0/candidate-release.json"
        " --manifest conformance/privileged-mcp-action-v0/MANIFEST.json"
        ' --github-output "$GITHUB_OUTPUT"',
    ),
    "Download attested pack": (
        "set -euo pipefail",
        'mkdir -p "$RUNNER_TEMP/oci-downloads"',
        'gh release download "$TAG" --repo Rul1an/assay'
        " --pattern privileged-mcp-action-v0-clean-room.tar.gz"
        " --pattern SHA256SUMS"
        " --pattern attestation-bundle.json"
        ' --dir "$RUNNER_TEMP/oci-downloads"',
        "(",
        'cd "$RUNNER_TEMP/oci-downloads"',
        "sha256sum -c SHA256SUMS",
        ")",
    ),
    "Verify pack attestation": (
        "set -euo pipefail",
        'source_digest="$(gh api "repos/Rul1an/assay/commits/$TAG" --jq .sha)"',
        OCI_SOURCE_DIGEST_GUARD,
        "gh attestation verify"
        ' "$RUNNER_TEMP/oci-downloads/privileged-mcp-action-v0-clean-room.tar.gz"'
        " --repo Rul1an/assay"
        ' --bundle "$RUNNER_TEMP/oci-downloads/attestation-bundle.json"'
        f" --signer-workflow {OCI_SIGNER_WORKFLOW}"
        ' --source-digest "$source_digest"'
        " --source-ref refs/heads/main"
        " --deny-self-hosted-runners",
    ),
    "Capture candidate observations": (
        "set -euo pipefail",
        'PACK="$RUNNER_TEMP/oci-downloads/privileged-mcp-action-v0-clean-room.tar.gz"',
        'OUTPUT="$RUNNER_TEMP/oci-capture/candidate_capture.v0"',
        'mkdir -p "$(dirname "$OUTPUT")"',
        f"python3 {OCI_EXECUTOR}"
        ' --pack "$PACK"'
        f" {OCI_IMPLEMENTATION_ID_ARGV}"
        ' --output "$OUTPUT"'
        " --timeout-seconds 30",
    ),
}


def _head_commit() -> str:
    """Resolve HEAD, the way the conformance and pack-release workflows already do.

    A literal here cannot work in this repository. Pull requests land as squash merges, so a
    branch commit is never an ancestor of `main`: the pin resolved only while the branch still
    existed on the remote, and ordinary branch cleanup would have broken the required
    activation-kit job on `main` after this merge. It is also structurally unfixable by choosing
    a better literal -- the corpus changes in the same commit the pin would have to name, so no
    pre-existing commit can satisfy it.

    Reading HEAD makes the pairing internally consistent: the pack is built from the checkout
    whose manifest supplies the expectations, which is what these tests are actually about.
    """
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()


SOURCE_COMMIT = _head_commit()
IMPLEMENTATION_COMMIT = "1" * 40


def corpus_digest() -> str:
    """The corpus digest as the manifest declares it.

    Compared against rather than copied: a literal digest in this file is a second statement of
    a value the corpus already carries, and keeping two statements true is the failure this
    branch has hit at every step.
    """
    return json.loads((CORPUS_DIR / "MANIFEST.json").read_text())["corpus_digest"]


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [sys.executable, *args],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and result.returncode != 0:
        raise AssertionError(
            f"command failed ({result.returncode}): {args}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def read_archive(path: Path) -> dict[str, bytes]:
    with tarfile.open(path, "r:gz") as archive:
        return {
            member.name: archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile()
        }


def read_bundle(bundle: bytes) -> tuple[dict, list[dict], bytes]:
    with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
        members = {member.name: member for member in archive.getmembers()}
        manifest = json.load(archive.extractfile(members["manifest.json"]))
        events_bytes = archive.extractfile(members["events.ndjson"]).read()
    return manifest, [json.loads(line) for line in events_bytes.splitlines()], events_bytes


class CleanRoomPackTests(unittest.TestCase):
    def build(self, output: Path) -> None:
        run(
            str(BUILD_SCRIPT),
            "--repo-root",
            str(REPO_ROOT),
            "--source-commit",
            SOURCE_COMMIT,
            "--output",
            str(output),
        )

    def test_pack_digest_binds_the_bytes_the_loader_parsed(self) -> None:
        import capture_candidate

        with tempfile.TemporaryDirectory() as tmp:
            pack_path = Path(tmp) / "pack.tar.gz"
            dest = Path(tmp) / "out"
            dest.mkdir()
            self.build(pack_path)
            original = pack_path.read_bytes()
            digest_a = sha256(original)
            loaded, bound = capture_candidate.load_pack_with_digest(pack_path, dest)
            pack_path.write_bytes(original + b"\x00")
            self.assertEqual(bound, digest_a)
            self.assertEqual(loaded["case_count"], 14)
            self.assertNotEqual(bound, capture_candidate.sha256_file(pack_path))

    def test_pack_is_deterministic_opaque_and_inputs_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            first = Path(tmp) / "first.tar.gz"
            second = Path(tmp) / "second.tar.gz"
            self.build(first)
            self.build(second)

            self.assertEqual(first.read_bytes(), second.read_bytes())
            files = read_archive(first)
            names = sorted(files)
            self.assertIn("privileged-mcp-action-v0/spec.md", names)
            self.assertIn("privileged-mcp-action-v0/descriptor.json", names)
            self.assertIn("privileged-mcp-action-v0/cases.json", names)
            self.assertIn("privileged-mcp-action-v0/README.md", names)

            case_names = [
                name
                for name in names
                if name.startswith("privileged-mcp-action-v0/cases/")
            ]
            self.assertEqual(len(case_names), 14)
            self.assertTrue(
                all(
                    Path(name).name.startswith("case-")
                    and Path(name).name.endswith(".bundle.tar.gz")
                    for name in case_names
                )
            )

            joined_names = "\n".join(names)
            for forbidden in (
                "MANIFEST.json",
                "gen_vectors.py",
                "crates/",
                "ok-",
                "bad-",
            ):
                self.assertNotIn(forbidden, joined_names)

            cases = json.loads(files["privileged-mcp-action-v0/cases.json"])
            self.assertEqual(
                cases["source_corpus_digest"],
                corpus_digest(),
            )
            self.assertRegex(cases["rendered_set_digest"], r"^sha256:[0-9a-f]{64}$")
            self.assertEqual(cases["declared_source_commit"], SOURCE_COMMIT)
            self.assertEqual(cases["case_count"], 14)
            self.assertNotIn("expected", json.dumps(cases))
            self.assertNotIn("description", json.dumps(cases))

            manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
            packed_spec = files["privileged-mcp-action-v0/spec.md"].decode()
            for forbidden in (
                "## Changelog",
                "MANIFEST.json",
                "gen_vectors.py",
                "#1840",
                "13-vector",
                "14 vectors",
            ):
                self.assertNotIn(forbidden, packed_spec)
            for vector in manifest["vectors"]:
                self.assertNotIn(vector["id"], packed_spec)
            source_hashes = {vector["sha256"] for vector in manifest["vectors"]}
            packed_hashes = {
                sha256(files[f"privileged-mcp-action-v0/{case['file']}"])
                for case in cases["cases"]
            }
            self.assertTrue(packed_hashes.isdisjoint(source_hashes))

            for case in cases["cases"]:
                bundle = files[f"privileged-mcp-action-v0/{case['file']}"]
                with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
                    self.assertEqual(
                        [member.name for member in archive.getmembers()],
                        ["manifest.json", "events.ndjson"],
                    )
                    inner = b"".join(
                        archive.extractfile(member).read()
                        for member in archive.getmembers()
                        if member.isfile()
                    )
                self.assertNotIn(b"pmav0-ok-", inner)
                self.assertNotIn(b"pmav0-bad-", inner)

            public_inputs = b"\n".join(
                data for name, data in files.items() if "/cases/" not in name
            )
            for forbidden in (
                b"gen_vectors.py",
                b"first_failure_informative",
                b"ok-005",
                b"bad-105",
                b"bad-108",
            ):
                self.assertNotIn(forbidden, public_inputs)

    def test_pack_ships_the_canonicalization_vectors_without_leaking_answers(self) -> None:
        """The RFC 8785 vectors ship, and shipping them does not make the pack less clean-room.

        A reproducer hits canonicalization before anything else -- the one completed cross-language
        attempt failed there first -- and a wrong canonicalizer makes every later result
        uninterpretable. The vectors are derived from a published RFC, not from this implementation,
        so they remove that wall without answering anything about the profile (#1990).

        What this asserts is the second half of that claim, because the first half is easy to
        believe and the second is the one that could quietly stop being true.
        """
        with tempfile.TemporaryDirectory() as tmp:
            pack = Path(tmp) / "pack.tar.gz"
            self.build(pack)
            files = read_archive(pack)

            vectors_name = "privileged-mcp-action-v0/canonicalization/rfc8785-vectors.json"
            note_name = "privileged-mcp-action-v0/canonicalization/README.md"
            self.assertIn(vectors_name, files)
            self.assertIn(note_name, files)

            # Byte-identical to the file this workspace tests against. If the pack carried a
            # re-serialized copy, a reproducer and this repo would be checking their canonicalizers
            # against two different files, which is the failure the vectors exist to prevent.
            in_repo = (
                REPO_ROOT / "crates/assay-canonical/tests/vectors/rfc8785.json"
            ).read_bytes()
            self.assertEqual(files[vectors_name], in_repo)

            vectors = json.loads(files[vectors_name])
            # Exactly the 31 vectors #1982 landed, with `_about` excluded because it is metadata and
            # not a vector -- that key is also what made the count ambiguous when this was first
            # measured, since the file has 32 keys and 31 vectors.
            #
            # `> 20` was the first version and is too loose. The byte-identity assertion above
            # already catches the pack drifting from the repo, so the case this one owns is the
            # other one: both shrinking together. Ten vectors deleted from the source file would
            # keep the pack byte-identical to it and pass every other check here.
            cases = {k: v for k, v in vectors.items() if k != "_about"}
            self.assertEqual(
                len(cases),
                31,
                f"expected the 31 RFC 8785 vectors, packed {len(cases)}",
            )

            # The clean-room property. The vectors describe byte formation and must not name a
            # profile case, an outcome, or a stage.
            #
            # Anchored patterns, not substrings. A bare "case-" matches "case-insensitive" in a
            # vector's own description of code-unit ordering, which is a false positive that would
            # either be suppressed or would make someone edit a correct vector to appease a test.
            blob = json.dumps(vectors)
            leak_patterns = (
                (r"\bcase-\d", "a profile case id"),
                (r"\b(ok|bad)-[a-z0-9-]{3,}", "a corpus vector name"),
                (r"expected_outcome|\bverdict\b", "an outcome field"),
            )

            # The patterns are self-tested, because a leak check that has never been shown to fire
            # is indistinguishable from one that matches nothing. Planting a leak in the working
            # tree does not exercise them: the pack reads the file from the pinned commit, so the
            # byte-identity assertion above catches that first and these never run.
            for pattern, _ in leak_patterns:
                self.assertIsNotNone(
                    re.search(pattern, LEAKY_FIXTURE),
                    f"leak pattern {pattern!r} matches nothing, so it protects nothing",
                )
            # And it does not fire on the vectors' own prose. "case-insensitive" appears in a
            # description of code-unit ordering and is not a profile case id.
            self.assertIsNotNone(re.search(r"case-insensitive", blob))

            for pattern, what in leak_patterns:
                self.assertIsNone(
                    re.search(pattern, blob),
                    f"vectors leak {what} (matched {pattern!r})",
                )

            note = files[note_name].decode()
            for required in (
                "not progress on the profile",
                "not conformance",
                "derive every profile result yourself",
            ):
                self.assertIn(required, note)

    def test_candidate_release_is_bound_to_the_current_corpus(self) -> None:
        candidate = json.loads(CANDIDATE_RELEASE.read_text())
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        readme = (CORPUS_DIR / "README.md").read_text()
        protocol = (CORPUS_DIR / "CONFORMANCE-PROTOCOL.md").read_text()
        workflow = RELEASE_WORKFLOW.read_text()

        self.assertEqual(
            candidate["schema"],
            "assay.privileged_mcp_action.candidate_release.v0",
        )
        self.assertRegex(
            candidate["tag"],
            r"^privileged-mcp-action-v0-candidate\.[1-9][0-9]*$",
        )
        self.assertEqual(candidate["case_count"], len(manifest["vectors"]))
        self.assertEqual(candidate["corpus_digest"], manifest["corpus_digest"])
        self.assertIn(candidate["tag"], readme)
        self.assertIn(candidate["tag"], protocol)

        self.assertIn("workflow_dispatch:", workflow)
        self.assertNotIn('tags:\n      - "privileged-mcp-action-v0-candidate.*"', workflow)
        self.assertIn("ref: ${{ github.sha }}", workflow)
        self.assertNotIn("ref: main", workflow)
        self.assertIn(
            "conformance/privileged-mcp-action-v0/candidate-release.json",
            workflow,
        )
        self.assertIn("--source-ref refs/heads/main", readme)
        self.assertIn("--source-ref refs/heads/main", protocol)

    def test_candidate_release_validator_rejects_stale_corpus_bindings(self) -> None:
        candidate = json.loads(CANDIDATE_RELEASE.read_text())
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "candidate-release.json"
            for key, stale_value in (
                ("case_count", candidate["case_count"] - 1),
                ("corpus_digest", "sha256:" + "0" * 64),
            ):
                stale = dict(candidate)
                stale[key] = stale_value
                path.write_text(json.dumps(stale))
                result = run(
                    str(VALIDATE_CANDIDATE_RELEASE),
                    "--candidate",
                    str(path),
                    "--manifest",
                    str(CORPUS_DIR / "MANIFEST.json"),
                    check=False,
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn(
                    f"candidate {'case count' if key == 'case_count' else 'corpus digest'} "
                    "does not match manifest",
                    result.stderr,
                )

    def test_candidate_release_validator_recomputes_manifest_digest(self) -> None:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        manifest["vectors"][0]["sha256"] = "sha256:" + "0" * 64
        with tempfile.TemporaryDirectory() as tmp:
            candidate_path = Path(tmp) / "candidate-release.json"
            manifest_path = Path(tmp) / "MANIFEST.json"
            candidate_path.write_text(CANDIDATE_RELEASE.read_text())
            manifest_path.write_text(json.dumps(manifest))
            result = run(
                str(VALIDATE_CANDIDATE_RELEASE),
                "--candidate",
                str(candidate_path),
                "--manifest",
                str(manifest_path),
                check=False,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn(
            "manifest corpus digest does not match ordered vector digests",
            result.stderr,
        )

    def test_archive_metadata_is_normalized(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "pack.tar.gz"
            self.build(output)
            with gzip.GzipFile(fileobj=io.BytesIO(output.read_bytes())) as stream:
                tar_bytes = stream.read()
            with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
                for member in archive.getmembers():
                    self.assertEqual(member.mtime, 0)
                    self.assertEqual(member.uid, 0)
                    self.assertEqual(member.gid, 0)
                    self.assertEqual(member.uname, "")
                    self.assertEqual(member.gname, "")
            files = read_archive(output)
            for name, bundle in files.items():
                if "/cases/" not in name:
                    continue
                with tarfile.open(fileobj=io.BytesIO(bundle), mode="r:gz") as archive:
                    for member in archive.getmembers():
                        self.assertEqual(member.mtime, 0)
                        self.assertEqual(member.uid, 0)
                        self.assertEqual(member.gid, 0)
                        self.assertEqual(member.uname, "")
                        self.assertEqual(member.gname, "")

    def test_rendering_changes_only_stream_identity_and_preserves_integrity_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "pack.tar.gz"
            self.build(output)
            files = read_archive(output)
            cases = json.loads(files["privileged-mcp-action-v0/cases.json"])["cases"]
            manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
            vectors = sorted(manifest["vectors"], key=lambda vector: vector["sha256"])

            for case, vector in zip(cases, vectors, strict=True):
                source = (CORPUS_DIR / vector["file"]).read_bytes()
                rendered = files[f"privileged-mcp-action-v0/{case['file']}"]
                source_manifest, source_events, source_event_bytes = read_bundle(source)
                rendered_manifest, rendered_events, rendered_event_bytes = read_bundle(rendered)

                source_clean = (
                    source_manifest["files"]["events.ndjson"]["sha256"]
                    == sha256(source_event_bytes)
                    and source_manifest["files"]["events.ndjson"]["bytes"]
                    == len(source_event_bytes)
                )
                rendered_clean = (
                    rendered_manifest["files"]["events.ndjson"]["sha256"]
                    == sha256(rendered_event_bytes)
                    and rendered_manifest["files"]["events.ndjson"]["bytes"]
                    == len(rendered_event_bytes)
                )
                self.assertEqual(rendered_clean, source_clean, case["id"])

                self.assertEqual(len(rendered_events), len(source_events))
                for original, opaque in zip(source_events, rendered_events, strict=True):
                    original = dict(original)
                    opaque = dict(opaque)
                    original.pop("id")
                    original.pop("assayrunid")
                    opaque.pop("id")
                    opaque.pop("assayrunid")
                    self.assertEqual(opaque, original, case["id"])

                source_manifest = dict(source_manifest)
                rendered_manifest = dict(rendered_manifest)
                source_manifest.pop("run_id")
                rendered_manifest.pop("run_id")
                source_manifest["files"] = dict(source_manifest["files"])
                rendered_manifest["files"] = dict(rendered_manifest["files"])
                source_manifest["files"].pop("events.ndjson")
                rendered_manifest["files"].pop("events.ndjson")
                self.assertEqual(rendered_manifest, source_manifest, case["id"])

    def test_source_bundle_rejects_surplus_or_oversize_members(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from pack_format import bundle_files, deterministic_tar_gz
        finally:
            sys.path.pop(0)

        manifest = b'{"files":{"events.ndjson":{"bytes":0,"sha256":"sha256:x"}}}\n'
        with self.subTest(reason="surplus"):
            bundle = deterministic_tar_gz(
                {
                    "manifest.json": manifest,
                    "events.ndjson": b"",
                    "surplus": b"x",
                },
                preserve_order=True,
            )
            with self.assertRaisesRegex(ValueError, "surplus"):
                bundle_files(bundle)

        with self.subTest(reason="oversize"):
            bundle = deterministic_tar_gz(
                {
                    "manifest.json": manifest,
                    "events.ndjson": b"x" * (8 * 1024 * 1024 + 1),
                },
                preserve_order=True,
            )
            with self.assertRaisesRegex(ValueError, "exceeds"):
                bundle_files(bundle)

    def test_stream_identity_rewrite_rejects_duplicate_sequences(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from pack_format import deterministic_tar_gz, rewrite_bundle_stream_identity
        finally:
            sys.path.pop(0)

        cases = (
            (
                (
                    b'{"assayrunid":"source","assayseq":1,"id":"source:first"}\n'
                    b'{"assayrunid":"source","assayseq":1,"id":"source:second"}\n'
                ),
                "collide on assayseq 1",
            ),
            (
                b'{"assayrunid":"source","assayseq":"1","id":"source:string"}\n',
                "requires integer assayseq values",
            ),
            (
                b'{"assayrunid":"source","assayseq":[1],"id":"source:list"}\n',
                "requires integer assayseq values",
            ),
        )
        for events, error in cases:
            with self.subTest(error=error):
                manifest = {
                    "run_id": "source",
                    "files": {
                        "events.ndjson": {
                            "bytes": len(events),
                            "sha256": sha256(events),
                        }
                    },
                }
                bundle = deterministic_tar_gz(
                    {
                        "manifest.json": (
                            json.dumps(
                                manifest,
                                separators=(",", ":"),
                                sort_keys=True,
                            ).encode()
                            + b"\n"
                        ),
                        "events.ndjson": events,
                    },
                    preserve_order=True,
                )
                with self.assertRaisesRegex(ValueError, error):
                    rewrite_bundle_stream_identity(bundle, "pmav0-case-001")


class CandidateHarness:
    """Pack build and fake-candidate factory, shared by both scoring shapes.

    A plain mixin rather than a base TestCase: subclassing the combined-mode
    suite would re-run all sixteen of its cases under the split-mode suite.
    """


    @classmethod
    def setUpClass(cls) -> None:
        cls.tmp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.tmp.name)
        cls.pack = cls.root / "pack.tar.gz"
        run(
            str(BUILD_SCRIPT),
            "--repo-root",
            str(REPO_ROOT),
            "--source-commit",
            SOURCE_COMMIT,
            "--output",
            str(cls.pack),
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.tmp.cleanup()

    def candidate(
        self,
        mode: str,
        *,
        oracle_to_rewrite: Path | None = None,
        pack_to_mutate: Path | None = None,
    ) -> Path:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        pack_files = read_archive(self.pack)
        cases = json.loads(pack_files["privileged-mcp-action-v0/cases.json"])["cases"]
        vectors = sorted(manifest["vectors"], key=lambda vector: vector["sha256"])
        expected_by_sha = {
            case["sha256"]: vector["expected"]
            for case, vector in zip(cases, vectors, strict=True)
        }
        script = self.root / f"candidate-{mode}.py"
        script.write_text(
            textwrap.dedent(
                f"""\
                import hashlib
                import json
                from pathlib import Path
                import subprocess
                import sys

                expected = {expected_by_sha!r}
                data = Path(sys.argv[1]).read_bytes()
                digest = "sha256:" + hashlib.sha256(data).hexdigest()
                result = dict(expected[digest])
                mode = {mode!r}
                oracle_to_rewrite = {str(oracle_to_rewrite) if oracle_to_rewrite else None!r}
                pack_to_mutate = {str(pack_to_mutate) if pack_to_mutate else None!r}
                if mode == "rewrite-oracle":
                    manifest_path = Path(oracle_to_rewrite)
                    manifest = json.loads(manifest_path.read_text())
                    for vector in manifest["vectors"]:
                        vector["expected"] = {{"bundle_integrity": "fail"}}
                    manifest_path.write_text(json.dumps(manifest))
                    result = {{"bundle_integrity": "fail"}}
                if mode == "mutate-pack":
                    pack_path = Path(pack_to_mutate)
                    pack_path.write_bytes(pack_path.read_bytes() + b"x")
                if mode == "mismatch" and result.get("verdict") == "valid":
                    result["verdict"] = "invalid"
                    result.pop("claims", None)
                if mode == "malformed":
                    print("not json")
                    raise SystemExit(2)
                if mode == "oversize-integer":
                    sys.stdout.write('{{"attacker_number":' + "9" * 5000 + '}}')
                    raise SystemExit(0)
                if mode == "flood":
                    subprocess.Popen([
                        sys.executable,
                        "-c",
                        "import pathlib,time; time.sleep(0.5); "
                        "pathlib.Path({str(self.root / 'escaped-child')!r}).write_text('escaped')",
                    ])
                    sys.stdout.write("x" * (2 * 1024 * 1024))
                    raise SystemExit(2)
                report = {{
                    "schema": "assay.privileged_mcp_action.verify.report.v0",
                    "profile": "privileged-mcp-action/v0",
                    "non_claims": [
                        "allow does not prove upstream delivery",
                        "deny does not establish maliciousness",
                        "caller-visible denial does not prove external side-effect absence",
                        "bundle integrity does not upgrade source class",
                    ],
                    **result,
                }}
                if result.get("bundle_integrity") == "pass" and result.get("verdict") == "invalid":
                    if mode != "reasonless":
                        report["findings"] = [{{"detail": "candidate explanation"}}]
                else:
                    report["findings"] = []
                if mode == "utf16":
                    sys.stdout.buffer.write(json.dumps(report).encode("utf-16"))
                    raise SystemExit(0)
                print(json.dumps(report))
                if mode == "trailing":
                    print("second document")
                """
            )
        )
        return script

    def score(
        self,
        candidate: Path,
        output: Path,
        *,
        pack: Path | None = None,
        manifest: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return run(
            str(SCORE_SCRIPT),
            "--pack",
            str(pack or self.pack),
            "--manifest",
            str(manifest or CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            shlex.join([sys.executable, str(candidate)]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )


class CandidateScorerTests(CandidateHarness, unittest.TestCase):
    def test_matching_candidate_scores_all_cases(self) -> None:
        output = self.root / "report.json"
        result = self.score(self.candidate("match"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"], {
            "total": 14,
            "match": 14,
            "mismatch": 0,
            "execution_error": 0,
            "harness_error": 0,
            "review_warnings": 0,
        })
        self.assertEqual(
            report["source_corpus_digest"],
            corpus_digest(),
        )
        self.assertRegex(report["rendered_set_digest"], r"^sha256:[0-9a-f]{64}$")
        self.assertEqual(
            {case["case_id"] for case in report["cases"]},
            {f"case-{index:03d}" for index in range(1, 15)},
        )
        self.assertEqual(
            report["implementation"]["reproduction_mode"],
            "blind_from_spec",
        )
        self.assertEqual(
            report["pack_provenance_verification"],
            "not_performed_by_scorer",
        )
        self.assertEqual(run(str(VALIDATE_SCRIPT), str(output)).returncode, 0)
        self.assertNotIn("ok-", output.read_text())
        self.assertNotIn("bad-", output.read_text())

        report["summary"]["match"] -= 1
        tampered = self.root / "tampered-report.json"
        tampered.write_text(json.dumps(report))
        self.assertEqual(
            run(str(VALIDATE_SCRIPT), str(tampered), check=False).returncode,
            2,
        )

    def test_normative_mismatch_fails(self) -> None:
        output = self.root / "mismatch.json"
        result = self.score(self.candidate("mismatch"), output)
        self.assertEqual(result.returncode, 1)
        report = json.loads(output.read_text())
        self.assertGreater(report["summary"]["mismatch"], 0)

    def test_malformed_trailing_or_flooded_output_is_execution_error(self) -> None:
        for mode in ("malformed", "oversize-integer", "trailing", "utf16", "flood"):
            with self.subTest(mode=mode):
                output = self.root / f"{mode}.json"
                result = self.score(self.candidate(mode), output)
                self.assertEqual(result.returncode, 2)
                report = json.loads(output.read_text())
                self.assertGreater(report["summary"]["execution_error"], 0)
                if mode == "flood" and os.name == "posix":
                    time.sleep(2)
                    self.assertFalse((self.root / "escaped-child").exists())

    def test_manifest_pack_desynchronization_is_harness_error(self) -> None:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        manifest["vectors"] = manifest["vectors"][:-1]
        manifest_root = self.root / "canonical"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest))
        output = self.root / "desync.json"

        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(manifest_path),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )

        self.assertEqual(result.returncode, 2)
        report = json.loads(output.read_text())
        self.assertGreater(report["summary"]["harness_error"], 0)
        self.assertEqual(report["summary"]["execution_error"], 0)
        self.assertTrue(report["harness_errors"])
        self.assertEqual(
            report["summary"]["harness_error"],
            sum(case["status"] == "harness_error" for case in report["cases"]),
        )

    def test_global_harness_diagnostic_is_not_a_case_status(self) -> None:
        manifest = json.loads((CORPUS_DIR / "MANIFEST.json").read_text())
        manifest["corpus_digest"] = "sha256:" + "0" * 64
        manifest_root = self.root / "canonical-global-diagnostic"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest))
        output = self.root / "global-diagnostic.json"

        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(manifest_path),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
            check=False,
        )

        self.assertEqual(result.returncode, 2)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"]["match"], 14)
        self.assertEqual(report["summary"]["harness_error"], 0)
        self.assertEqual(len(report["harness_errors"]), 1)
        self.assertEqual(run(str(VALIDATE_SCRIPT), str(output)).returncode, 0)

    def test_run_record_rejects_non_object_cases_and_impossible_observed_shapes(self) -> None:
        output = self.root / "validator-source.json"
        result = self.score(self.candidate("match"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        clean = json.loads(output.read_text())
        mutations = {
            "non-object-case": lambda report: report["cases"].__setitem__(0, "not-an-object"),
            "empty-observed": lambda report: report["cases"][0].__setitem__("observed", {}),
            "relative-source": lambda report: report["implementation"].__setitem__(
                "source", "./verifier"
            ),
            "wrong-harness-count": lambda report: report["summary"].__setitem__(
                "harness_error", 1
            ),
            "boolean-exit-code": lambda report: report["cases"][0].__setitem__(
                "exit_code", True
            ),
            "boolean-summary-count": lambda report: report["summary"].__setitem__(
                "mismatch", False
            ),
            "replaced-non-claims": lambda report: report.__setitem__(
                "non_claims",
                ["certifies security", "certifies compliance", "certifies provider outcomes"],
            ),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                report = json.loads(json.dumps(clean))
                mutate(report)
                path = self.root / f"{name}.json"
                path.write_text(json.dumps(report))
                invalid = run(str(VALIDATE_SCRIPT), str(path), check=False)
                self.assertEqual(invalid.returncode, 2, invalid.stderr)

    def test_standalone_run_record_validator_bounds_bytes_and_nesting(self) -> None:
        oversized = self.root / "oversized-run-record.json"
        oversized.write_bytes(b'{"padding":"' + b"x" * (4 * 1024 * 1024) + b'"}')
        too_deep = self.root / "deep-run-record.json"
        too_deep.write_text("[" * 65 + "0" + "]" * 65)
        # Past CPython's decoder recursion limit and still inside the 4 MiB
        # ceiling: the case the depth scan exists for. Parsed first, this raises
        # RecursionError, which is not a ValueError and reaches the user as a
        # traceback. The capture ceiling is too small to reach here, so this is
        # the only place the scan's purpose can be observed.
        recursive = self.root / "recursive-run-record.json"
        recursive.write_text("[" * 200_000 + "0" + "]" * 200_000)

        for path, diagnostic in (
            (oversized, "exceeds the 4194304-byte limit"),
            (too_deep, "nesting exceeds 64"),
            (recursive, "nesting exceeds 64"),
        ):
            with self.subTest(path=path.name):
                result = run(str(VALIDATE_SCRIPT), str(path), check=False)
                self.assertEqual(result.returncode, 2)
                self.assertIn(diagnostic, result.stderr)
                self.assertNotIn("Traceback", result.stderr)

    def test_candidate_cannot_rewrite_oracle_or_report_a_mutated_pack_hash(self) -> None:
        manifest_root = self.root / "oracle-snapshot"
        manifest_root.mkdir()
        (manifest_root / "vectors").symlink_to(CORPUS_DIR / "vectors")
        manifest_path = manifest_root / "MANIFEST.json"
        manifest_path.write_bytes((CORPUS_DIR / "MANIFEST.json").read_bytes())
        oracle_output = self.root / "oracle-rewrite.json"

        oracle_result = self.score(
            self.candidate("rewrite-oracle", oracle_to_rewrite=manifest_path),
            oracle_output,
            manifest=manifest_path,
        )

        self.assertEqual(oracle_result.returncode, 1)
        oracle_report = json.loads(oracle_output.read_text())
        self.assertGreater(oracle_report["summary"]["mismatch"], 0)
        self.assertEqual(oracle_report["summary"]["harness_error"], 0)

        mutable_pack = self.root / "mutable-pack.tar.gz"
        mutable_pack.write_bytes(self.pack.read_bytes())
        original_pack_hash = sha256(mutable_pack.read_bytes())
        pack_output = self.root / "pack-mutation.json"
        pack_result = self.score(
            self.candidate("mutate-pack", pack_to_mutate=mutable_pack),
            pack_output,
            pack=mutable_pack,
        )

        self.assertEqual(pack_result.returncode, 0, pack_result.stderr)
        self.assertNotEqual(sha256(mutable_pack.read_bytes()), original_pack_hash)
        self.assertEqual(
            json.loads(pack_output.read_text())["pack_sha256"],
            original_pack_hash,
        )

    def test_between_read_pack_swap_cannot_split_parsed_bytes_from_digest(self) -> None:
        """Same-path replacement after parse must not bind a different digest.

        The scorer used to parse one read and hash a second. Replacing the pack
        between those reads recorded digest B for a pack object parsed from A.
        """
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            import capture_candidate
            import score_candidate
        finally:
            sys.path.pop(0)

        mutable = self.root / "between-read-pack.tar.gz"
        original = self.pack.read_bytes()
        mutable.write_bytes(original)
        digest_a = sha256(original)
        digest_b = sha256(original + b"\x00")
        self.assertNotEqual(digest_a, digest_b)

        reads = {"n": 0}
        real_read = capture_candidate.read_pack_bytes

        def read_then_swap(path: Path) -> bytes:
            data = real_read(path)
            reads["n"] += 1
            if reads["n"] == 1:
                path.write_bytes(original + b"\x00")
            return data

        output = self.root / "between-read-report.json"
        argv = [
            str(SCORE_SCRIPT),
            "--pack",
            str(mutable),
            "--manifest",
            str(CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
        ]
        with (
            mock.patch.object(sys, "argv", argv),
            mock.patch.object(
                capture_candidate, "read_pack_bytes", side_effect=read_then_swap
            ),
        ):
            self.assertEqual(score_candidate.main(), 0)

        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["summary"]["match"], 14)
        self.assertEqual(report["pack_sha256"], digest_a)
        self.assertEqual(reads["n"], 1)
        self.assertEqual(sha256(mutable.read_bytes()), digest_b)

    def test_scorer_uses_canonical_load_pack_with_digest(self) -> None:
        """Parse and digest must come from the existing helper, not a second read."""
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            import capture_candidate
            import score_candidate
        finally:
            sys.path.pop(0)

        source = inspect.getsource(score_candidate.main)
        self.assertIn("load_pack_with_digest(", source)
        self.assertNotIn("sha256_file(", source)
        self.assertNotRegex(source, r"(?<![_\w])load_pack\(")
        self.assertIs(
            score_candidate.load_pack_with_digest,
            capture_candidate.load_pack_with_digest,
        )

    def test_timeout_kills_candidate_process_group(self) -> None:
        marker = self.root / "escaped-timeout-child"
        candidate = self.root / "candidate-timeout.py"
        candidate.write_text(
            textwrap.dedent(
                f"""\
                import subprocess
                import sys
                import time

                subprocess.Popen([
                    sys.executable,
                    "-c",
                    "import pathlib,time; time.sleep(1.2); "
                    "pathlib.Path({str(marker)!r}).write_text('escaped')",
                ])
                time.sleep(5)
                """
            )
        )
        bundle = self.root / "ignored.bundle"
        bundle.write_bytes(b"ignored")
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from capture_candidate import CandidateError, run_candidate
        finally:
            sys.path.pop(0)

        with self.assertRaisesRegex(CandidateError, "timed out"):
            run_candidate([sys.executable, str(candidate)], bundle, 1)
        if os.name == "posix":
            time.sleep(1.5)
            self.assertFalse(marker.exists())

    @unittest.skipUnless(os.name == "posix", "process-group containment requires POSIX")
    def test_leader_exit_still_kills_descendants_holding_capture_pipes(self) -> None:
        marker = self.root / "escaped-after-leader-exit"
        candidate = self.root / "candidate-leader-exits.py"
        candidate.write_text(
            textwrap.dedent(
                f"""\
                import subprocess
                import sys

                subprocess.Popen([
                    sys.executable,
                    "-c",
                    "import pathlib,time; time.sleep(1.2); "
                    "pathlib.Path({str(marker)!r}).write_text('escaped'); time.sleep(5)",
                ])
                """
            )
        )
        bundle = self.root / "ignored-after-leader-exit.bundle"
        bundle.write_bytes(b"ignored")
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from capture_candidate import CandidateError, run_candidate
        finally:
            sys.path.pop(0)

        with self.assertRaises(CandidateError):
            run_candidate([sys.executable, str(candidate)], bundle, 5)
        time.sleep(1.5)
        self.assertFalse(marker.exists())

    def test_non_positive_timeout_is_rejected_before_execution(self) -> None:
        output = self.root / "invalid-timeout.json"
        result = run(
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            shlex.join([sys.executable, str(self.candidate("match"))]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--timeout-seconds",
            "0",
            "--output",
            str(output),
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("must be a positive integer", result.stderr)
        self.assertFalse(output.exists())

    def test_capture_failure_is_a_harness_error_not_empty_output(self) -> None:
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            from bounded_process import ProcessCaptureError, run_bounded
        finally:
            sys.path.pop(0)

        with (
            mock.patch(
                "bounded_process._capture_stream",
                side_effect=OSError("synthetic capture failure"),
            ),
            self.assertRaisesRegex(
                ProcessCaptureError,
                r"process output capture failed: stderr, stdout",
            ),
        ):
            run_bounded(
                [sys.executable, "-c", "pass"],
                timeout_seconds=5,
                stdout_limit=1024,
                stderr_limit=1024,
            )

    def test_capture_failure_is_recorded_as_harness_error(self) -> None:
        output = self.root / "capture-harness-error.json"
        sys.path.insert(0, str(CORPUS_DIR / "scripts"))
        try:
            import capture_candidate
            import score_candidate
        finally:
            sys.path.pop(0)

        argv = [
            str(SCORE_SCRIPT),
            "--pack",
            str(self.pack),
            "--manifest",
            str(CORPUS_DIR / "MANIFEST.json"),
            "--entrypoint",
            "unused-candidate",
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            "--output",
            str(output),
        ]
        with (
            mock.patch.object(sys, "argv", argv),
            # Patched where it is executed: capture_observations resolves
            # run_candidate in its own module, not through score_candidate.
            mock.patch.object(
                capture_candidate,
                "run_candidate",
                side_effect=capture_candidate.HarnessError("synthetic capture failure"),
            ),
        ):
            self.assertEqual(score_candidate.main(), 2)

        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["summary"]["harness_error"], 14)
        self.assertEqual(report["summary"]["execution_error"], 0)
        self.assertTrue(
            all(case["status"] == "harness_error" for case in report["cases"])
        )

    def test_reject_reason_is_visible_but_not_scored(self) -> None:
        output = self.root / "reasonless.json"
        result = self.score(self.candidate("reasonless"), output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"]["match"], 14)
        self.assertGreater(report["summary"]["review_warnings"], 0)
        self.assertTrue(
            any(
                "reject_reason_missing" in case.get("review_warnings", [])
                for case in report["cases"]
            )
        )

    def test_duplicate_or_surplus_pack_members_fail_closed(self) -> None:
        files = read_archive(self.pack)
        for mode in ("duplicate", "surplus"):
            with self.subTest(mode=mode):
                tampered = self.root / f"{mode}.tar.gz"
                with tarfile.open(tampered, "w:gz") as archive:
                    for name, data in files.items():
                        info = tarfile.TarInfo(name)
                        info.size = len(data)
                        archive.addfile(info, io.BytesIO(data))
                    if mode == "duplicate":
                        name = "privileged-mcp-action-v0/cases.json"
                        data = files[name]
                    else:
                        name = "privileged-mcp-action-v0/answers.txt"
                        data = b"unexpected"
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))

                output = self.root / f"{mode}-report.json"
                result = self.score(self.candidate("match"), output, pack=tampered)
                self.assertEqual(result.returncode, 2)
                self.assertFalse(output.exists())

    def test_truncated_gzip_pack_is_invalid_input_not_a_mismatch(self) -> None:
        truncated = self.root / "truncated.tar.gz"
        data = self.pack.read_bytes()
        truncated.write_bytes(data[: len(data) // 2])
        output = self.root / "truncated-report.json"

        result = self.score(self.candidate("match"), output, pack=truncated)

        self.assertEqual(result.returncode, 2)
        self.assertNotIn("Traceback", result.stderr)
        self.assertFalse(output.exists())


sys.path.insert(0, str(CORPUS_DIR / "scripts"))
import artifact_io  # noqa: E402
import capture_format  # noqa: E402
import score_candidate  # noqa: E402
import strict_json  # noqa: E402

CAPTURE_SCRIPT = CORPUS_DIR / "scripts" / "capture_candidate.py"
CAPTURE_SCHEMA_DOC = CORPUS_DIR / "capture.schema.json"
ARTIFACT_IO_PATH = CORPUS_DIR / "scripts" / "artifact_io.py"


class SharedArtifactIoContractTests(unittest.TestCase):
    """The byte-I/O rule is shared, not restated by each producer."""

    def artifact_io(self):
        self.assertTrue(ARTIFACT_IO_PATH.is_file(), "shared artifact_io.py is missing")
        importlib.invalidate_caches()
        sys.modules.pop("artifact_io", None)
        return importlib.import_module("artifact_io")

    def test_deterministic_json_bytes_pin_key_order_indent_and_one_lf(self) -> None:
        artifact_io = self.artifact_io()
        self.assertEqual(
            artifact_io.render_deterministic_json_bytes(
                {"z": 3, "a": {"second": 2, "first": 1}}
            ),
            b'{\n  "a": {\n    "first": 1,\n    "second": 2\n  },\n  "z": 3\n}\n',
        )

    def test_every_pretty_json_producer_calls_the_shared_renderer(self) -> None:
        scripts = CORPUS_DIR / "scripts"
        paths = tuple(
            scripts / name
            for name in (
                "pack_format.py",
                "build_clean_room_pack.py",
                "capture_candidate.py",
                "score_candidate.py",
                "oci_candidate_executor.py",
            )
        )
        shared_calls = 0
        inline_pretty_calls = []
        for path in paths:
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            for node in ast.walk(tree):
                if not isinstance(node, ast.Call):
                    continue
                if isinstance(node.func, ast.Name) and node.func.id == "render_deterministic_json_bytes":
                    shared_calls += 1
                if not (
                    isinstance(node.func, ast.Attribute)
                    and isinstance(node.func.value, ast.Name)
                    and node.func.value.id == "json"
                    and node.func.attr == "dumps"
                ):
                    continue
                keywords = {item.arg: item.value for item in node.keywords if item.arg}
                indent = keywords.get("indent")
                sort_keys = keywords.get("sort_keys")
                if (
                    isinstance(indent, ast.Constant)
                    and indent.value == 2
                    and isinstance(sort_keys, ast.Constant)
                    and sort_keys.value is True
                ):
                    inline_pretty_calls.append(f"{path.name}:{node.lineno}")
        self.assertEqual(inline_pretty_calls, [], "inline deterministic renderers drift")
        self.assertEqual(shared_calls, 6, "the six shipped pretty-JSON sites must delegate")

    def test_capture_and_record_outputs_use_one_atomic_writer(self) -> None:
        scripts = CORPUS_DIR / "scripts"
        paths = tuple(
            scripts / name
            for name in (
                "capture_candidate.py",
                "score_candidate.py",
                "oci_candidate_executor.py",
            )
        )
        atomic_calls = 0
        direct_writes = []
        for path in paths:
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            for node in ast.walk(tree):
                if not isinstance(node, ast.Call):
                    continue
                if isinstance(node.func, ast.Name) and node.func.id == "write_regular_file_atomically":
                    atomic_calls += 1
                if isinstance(node.func, ast.Attribute) and node.func.attr == "write_text":
                    direct_writes.append(f"{path.name}:{node.lineno}")
        self.assertEqual(direct_writes, [], "capture/scoring output must not truncate in place")
        self.assertEqual(atomic_calls, 3, "capture, scorer and OCI capture must share the writer")

    def test_failed_atomic_replace_preserves_old_bytes_and_cleans_temp(self) -> None:
        artifact_io = self.artifact_io()
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            output = root / "record.json"
            stale = b'{"stale":true}\n'
            output.write_bytes(stale)
            with mock.patch.object(
                artifact_io.os, "replace", side_effect=OSError("rename interrupted")
            ):
                with self.assertRaisesRegex(OSError, "rename interrupted"):
                    artifact_io.write_regular_file_atomically(output, b'{"new":true}\n')
            self.assertEqual(output.read_bytes(), stale)
            self.assertEqual(
                [p for p in root.iterdir() if p.name.startswith(artifact_io.ARTIFACT_TEMP_PREFIX)],
                [],
            )

    def test_atomic_writer_completes_short_writes(self) -> None:
        artifact_io = self.artifact_io()
        with tempfile.TemporaryDirectory() as raw:
            output = Path(raw) / "record.json"
            expected = b'{"long":"enough to require several writes"}\n'
            real_write = artifact_io.os.write

            def short_write(fd: int, data: bytes) -> int:
                return real_write(fd, data[:3])

            with mock.patch.object(artifact_io.os, "write", side_effect=short_write):
                artifact_io.write_regular_file_atomically(output, expected)

            self.assertEqual(output.read_bytes(), expected)

    def test_zero_progress_write_preserves_old_bytes_and_cleans_temp(self) -> None:
        artifact_io = self.artifact_io()
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            output = root / "record.json"
            stale = b'{"stale":true}\n'
            output.write_bytes(stale)

            with mock.patch.object(artifact_io.os, "write", return_value=0):
                with self.assertRaisesRegex(OSError, "made no progress"):
                    artifact_io.write_regular_file_atomically(output, b'{"new":true}\n')

            self.assertEqual(output.read_bytes(), stale)
            self.assertEqual(
                [p for p in root.iterdir() if p.name.startswith(artifact_io.ARTIFACT_TEMP_PREFIX)],
                [],
            )


class CandidateCaptureTests(CandidateHarness, unittest.TestCase):
    """Split-phase capture and trusted scoring.

    Inherits the pack build and the fake-candidate factory from the combined-mode
    tests rather than restating them: the point of the split is that both phases
    drive the same candidate over the same pack.
    """

    def capture(
        self,
        candidate: Path,
        output: Path,
        *,
        pack: Path | None = None,
        extra: tuple[str, ...] = (),
    ) -> subprocess.CompletedProcess[str]:
        return run(
            str(CAPTURE_SCRIPT),
            "--pack",
            str(pack or self.pack),
            "--entrypoint",
            shlex.join([sys.executable, str(candidate)]),
            "--implementation-name",
            "test implementation",
            "--implementation-source",
            "https://example.test/verifier",
            "--implementation-commit",
            IMPLEMENTATION_COMMIT,
            "--reproduction-mode",
            "blind_from_spec",
            *extra,
            "--output",
            str(output),
            check=False,
        )

    def score_capture(
        self,
        capture: Path,
        output: Path,
        *,
        pack: Path | None = None,
        manifest: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return run(
            str(SCORE_SCRIPT),
            "--pack",
            str(pack or self.pack),
            "--manifest",
            str(manifest or CORPUS_DIR / "MANIFEST.json"),
            "--capture",
            str(capture),
            "--output",
            str(output),
            check=False,
        )

    def valid_capture(self, name: str) -> Path:
        capture = self.root / f"capture-{name}.json"
        result = self.capture(self.candidate("match"), capture)
        self.assertEqual(result.returncode, 0, result.stderr)
        return capture

    def rewrite(self, capture: Path, name: str, mutate) -> Path:
        document = json.loads(capture.read_text())
        mutate(document)
        target = self.root / f"capture-{name}.json"
        target.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
        return target

    def test_capture_observations_runner_is_keyword_only_and_defaults(self) -> None:
        import capture_candidate

        params = inspect.signature(capture_candidate.capture_observations).parameters
        self.assertIn("candidate_runner", params)
        self.assertEqual(params["candidate_runner"].kind, inspect.Parameter.KEYWORD_ONLY)
        self.assertIs(params["candidate_runner"].default, capture_candidate.run_candidate)
        with self.assertRaises(TypeError):
            capture_candidate.capture_observations({}, [], 1, capture_candidate.run_candidate)

    def test_valid_capture_scores_every_case(self) -> None:
        """Positive control for every refusal below.

        Without it, a scorer that refused all captures would satisfy the negative
        cases and prove nothing.
        """
        capture = self.valid_capture("control")
        output = self.root / "capture-control-report.json"
        result = self.score_capture(capture, output)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text())
        self.assertEqual(report["summary"]["match"], 14)
        self.assertEqual(report["summary"]["total"], 14)

    def test_capture_digest_binds_the_single_bounded_read(self) -> None:
        self.assertTrue(
            hasattr(capture_format, "published_rows"),
            "capture loading must delegate its bounded read to published_rows",
        )
        self.assertTrue(
            hasattr(capture_format, "load_capture_with_digest"),
            "capture loading must return the document and digest from one read",
        )
        capture = self.valid_capture("one-read-digest")
        original = capture.read_bytes()
        original_document = json.loads(original)
        swapped_document = json.loads(original)
        swapped_document["implementation"]["name"] = "swapped implementation"
        swapped = artifact_io.render_deterministic_json_bytes(swapped_document)
        reads = {"count": 0}
        real_read = capture_format.published_rows.read_regular_file

        def read_then_swap(path: Path, *, limit: int) -> bytes:
            data = real_read(path, limit=limit)
            reads["count"] += 1
            Path(path).write_bytes(swapped)
            return data

        with mock.patch.object(
            capture_format.published_rows,
            "read_regular_file",
            side_effect=read_then_swap,
        ):
            document, digest = capture_format.load_capture_with_digest(capture)

        capture_format.validate_capture(document)
        self.assertEqual(document, original_document)
        self.assertNotEqual(document, swapped_document)
        self.assertEqual(digest, sha256(original))
        self.assertEqual(reads["count"], 1)
        self.assertEqual(capture.read_bytes(), swapped)
        self.assertNotEqual(digest, sha256(swapped))

    def test_capture_digest_hashes_input_bytes_not_a_reserialization(self) -> None:
        self.assertTrue(
            hasattr(capture_format, "load_capture_with_digest"),
            "capture loading must expose the digest of the exact input bytes",
        )
        capture = self.valid_capture("input-byte-digest")
        document = json.loads(capture.read_text(encoding="utf-8"))
        compact = json.dumps(document, separators=(",", ":")).encode("utf-8")
        capture.write_bytes(compact)

        loaded, digest = capture_format.load_capture_with_digest(capture)

        self.assertEqual(loaded, document)
        self.assertEqual(digest, sha256(compact))
        self.assertNotEqual(digest, sha256(json.dumps(document, sort_keys=True).encode()))
        self.assertEqual(capture_format.load_capture(capture), loaded)

    def test_capture_digest_path_still_rejects_semantically_invalid_input(self) -> None:
        capture = self.valid_capture("digest-validation")
        document = json.loads(capture.read_text(encoding="utf-8"))
        document["schema"] = "unknown.capture.v0"
        capture.write_text(json.dumps(document), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "capture schema mismatch"):
            capture_format.load_capture_with_digest(capture)

    def test_missing_or_duplicated_observation_is_refused_without_a_record(self) -> None:
        capture = self.valid_capture("cardinality")

        def drop(document: object) -> None:
            del document["observations"][6]

        def duplicate(document: object) -> None:
            document["observations"].append(json.loads(json.dumps(document["observations"][6])))

        for name, mutate in (("thirteen", drop), ("fifteen", duplicate)):
            with self.subTest(shape=name):
                hostile = self.rewrite(capture, name, mutate)
                output = self.root / f"capture-{name}-report.json"
                result = self.score_capture(hostile, output)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertFalse(
                    output.exists(),
                    "a capture that does not bind must leave no run record",
                )

    def test_direct_and_split_paths_produce_byte_identical_run_records(self) -> None:
        """The migration guarantee, and the proof that there is one comparison.

        Two records built from the same candidate through different plumbing can
        only be byte-identical if both went through the same scorer.
        """
        candidate = self.candidate("match")
        direct = self.root / "equivalence-direct.json"
        self.assertEqual(self.score(candidate, direct).returncode, 0)

        capture = self.root / "equivalence-capture.json"
        self.assertEqual(self.capture(candidate, capture).returncode, 0)
        split = self.root / "equivalence-split.json"
        self.assertEqual(self.score_capture(capture, split).returncode, 0)

        self.assertEqual(direct.read_bytes(), split.read_bytes())

    def test_run_record_pack_digest_is_recomputed_locally(self) -> None:
        capture = self.valid_capture("digest")
        output = self.root / "digest-report.json"
        self.assertEqual(self.score_capture(capture, output).returncode, 0)
        self.assertEqual(
            json.loads(output.read_text())["pack_sha256"],
            sha256(self.pack.read_bytes()),
        )

    def test_scorer_recomputes_the_pack_digest_rather_than_copying_it(self) -> None:
        """Directly on the scorer, with the CLI's binding guard out of the way.

        Through the CLI a lying capture never reaches this code, so `recomputed`
        and `capture["pack_sha256"]` are equal wherever it can be observed and
        the difference between them is invisible. Calling the scorer with a
        capture that lies is the only way to see which one the record carries.
        """
        capture = json.loads(self.valid_capture("recompute").read_text())
        lie = "sha256:" + "d" * 64
        capture["pack_sha256"] = lie
        recomputed = sha256(self.pack.read_bytes())
        self.assertNotEqual(lie, recomputed)

        pack = {
            "declared_source_commit": capture["pack_declared_source_commit"],
            "source_corpus_digest": capture["source_corpus_digest"],
            "rendered_set_digest": capture["rendered_set_digest"],
            "cases": [
                {"id": observation["case_id"], "sha256": observation["input_sha256"]}
                for observation in capture["observations"]
            ],
        }
        expected = {
            observation["input_sha256"]: observation["observed"]
            for observation in capture["observations"]
        }
        report = score_candidate.score_capture(
            capture,
            pack,
            recomputed,
            expected,
            pack["source_corpus_digest"],
            pack["rendered_set_digest"],
        )
        self.assertEqual(report["pack_sha256"], recomputed)
        self.assertNotEqual(report["pack_sha256"], lie)

    def test_capture_that_does_not_bind_the_pack_is_refused(self) -> None:
        capture = self.valid_capture("binding")
        other = "sha256:" + "b" * 64

        def swap_order(document):
            document["observations"][2], document["observations"][9] = (
                document["observations"][9],
                document["observations"][2],
            )

        def foreign_input_digest(document):
            document["observations"][4]["input_sha256"] = other

        def foreign_pack(document):
            document["pack_sha256"] = other

        def foreign_corpus(document):
            document["source_corpus_digest"] = other

        def foreign_commit(document):
            document["pack_declared_source_commit"] = "c" * 40

        for name, mutate in (
            ("reordered", swap_order),
            ("foreign-input-digest", foreign_input_digest),
            ("foreign-pack", foreign_pack),
            ("foreign-corpus", foreign_corpus),
            ("foreign-commit", foreign_commit),
        ):
            with self.subTest(binding=name):
                hostile = self.rewrite(capture, name, mutate)
                output = self.root / f"bind-{name}.json"
                result = self.score_capture(hostile, output)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertFalse(output.exists())
                self.assertNotIn("Traceback", result.stderr)

    def test_hostile_capture_bytes_fail_closed_without_a_traceback(self) -> None:
        capture = self.valid_capture("hostile")
        valid_text = capture.read_text()
        cases = {
            "duplicate-key": valid_text.replace(
                '"schema":', '"schema": "assay.privileged_mcp_action.candidate_capture.v0",\n  "schema":', 1
            ),
            "non-finite": valid_text.replace('"exit_code": 0', '"exit_code": NaN', 1),
            "float-exit-code": valid_text.replace('"exit_code": 0', '"exit_code": 0.0', 1),
            "oversized-integer": valid_text.replace(
                '"exit_code": 0', '"exit_code": 9007199254740992', 1
            ),
            "out-of-range-exit-code": valid_text.replace('"exit_code": 0', '"exit_code": 4096', 1),
            # Deep enough that json.loads would raise RecursionError, small enough
            # that the byte ceiling cannot be what refuses it. Without both, this
            # case proves the ceiling works and says nothing about the depth scan.
            "deeply-nested": "[" * 30000 + "0" + "]" * 30000,
            "not-utf8": None,
            "not-an-object": "[]",
        }
        for name, text in cases.items():
            with self.subTest(hostile=name):
                path = self.root / f"hostile-{name}.json"
                if text is None:
                    path.write_bytes(b'{"schema": "\xff\xfe"}')
                else:
                    path.write_text(text)
                output = self.root / f"hostile-{name}-report.json"
                result = self.score_capture(path, output)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertFalse(output.exists())
                self.assertNotIn("Traceback", result.stderr)

    def test_capture_over_the_byte_ceiling_is_refused_by_the_ceiling(self) -> None:
        """Oversized but otherwise valid, so only the ceiling can be the refusal.

        A padded piece of junk would also be shape-invalid, and then this case
        would pass with the ceiling removed.
        """
        # A literal, not a multiple of the ceiling: a fixture sized from the
        # constant under test grows with it, and a raised ceiling would then
        # never be observed.
        padding = 200_000
        self.assertGreater(
            padding,
            capture_format.MAX_CAPTURE_BYTES,
            "fixture must exceed the shipped ceiling for this test to mean anything",
        )
        document = json.loads(self.valid_capture("ceiling").read_text())
        document["implementation"]["version"] = "v" * padding
        path = self.root / "capture-over-ceiling.json"
        path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
        capture_format.validate_capture(document)  # valid but for its size

        output = self.root / "over-ceiling-report.json"
        result = self.score_capture(path, output)
        self.assertEqual(result.returncode, 2, result.stdout)
        self.assertFalse(output.exists())
        self.assertIn(str(capture_format.MAX_CAPTURE_BYTES), result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_capture_validator_owns_cardinality_and_ordering(self) -> None:
        """Directly on the validator.

        Through the CLI these refusals are masked by the run-record case count
        and by the pack-binding check, so a CLI-only test cannot tell whether the
        capture format states its own rules.
        """
        valid = json.loads(self.valid_capture("validator").read_text())
        capture_format.validate_capture(valid)

        def dropped(document):
            del document["observations"][6]

        def duplicated(document):
            document["observations"].append(document["observations"][6])

        def reordered(document):
            document["observations"][0], document["observations"][1] = (
                document["observations"][1],
                document["observations"][0],
            )

        def renamed(document):
            document["observations"][3]["case_id"] = "case-999"

        for name, mutate in (
            ("thirteen", dropped),
            ("fifteen", duplicated),
            ("reordered", reordered),
            ("renamed", renamed),
        ):
            with self.subTest(shape=name):
                document = json.loads(json.dumps(valid))
                mutate(document)
                with self.assertRaises(capture_format.CaptureError):
                    capture_format.validate_capture(document)

    def test_strict_loader_owns_the_json_number_domain(self) -> None:
        """Directly on the shared loader.

        Every capture field is separately type-checked, so at capture level this
        rule is defence in depth. It is pinned here because the same loader reads
        run records, where a number can land in a less tightly checked position.
        """
        self.assertEqual(
            strict_json.parse_strict_object(b'{"a": 1, "b": [-9]}', label="probe"),
            {"a": 1, "b": [-9]},
        )
        for payload in (
            b'{"a": 1.0}',
            b'{"a": [1.5]}',
            b'{"a": 9007199254740992}',
            b'{"a": {"b": -9007199254740992}}',
        ):
            with self.subTest(payload=payload):
                with self.assertRaises(ValueError):
                    strict_json.parse_strict_object(payload, label="probe")

    def test_capture_refuses_a_symlink_and_a_missing_file(self) -> None:
        capture = self.valid_capture("symlink-source")
        link = self.root / "capture-symlink.json"
        if link.exists() or link.is_symlink():
            link.unlink()
        link.symlink_to(capture)
        for path in (link, self.root / "capture-absent.json"):
            with self.subTest(path=path.name):
                output = self.root / f"nofollow-{path.name}"
                result = self.score_capture(path, output)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertFalse(output.exists())
                self.assertNotIn("Traceback", result.stderr)

    def test_capture_error_text_is_bounded(self) -> None:
        long_message = "e" * (capture_format.MAX_ERROR_CHARS * 4)
        bounded = capture_format.bound_error(long_message)
        self.assertLessEqual(len(bounded), capture_format.MAX_ERROR_CHARS)
        self.assertEqual(capture_format.bound_error("  short  "), "short")
        self.assertEqual(capture_format.bound_error("   "), "unspecified error")

        capture = self.valid_capture("bounded-error")

        def overlong_error(document):
            document["observations"][0] = {
                "case_id": document["observations"][0]["case_id"],
                "input_sha256": document["observations"][0]["input_sha256"],
                "state": "candidate_error",
                "error": long_message,
            }

        hostile = self.rewrite(capture, "overlong-error", overlong_error)
        output = self.root / "overlong-error-report.json"
        result = self.score_capture(hostile, output)
        self.assertEqual(result.returncode, 2, result.stdout)
        self.assertFalse(output.exists())

    def test_capture_image_binding_uses_the_registry_rule(self) -> None:
        digest_image = "ghcr.io/example/verifier@sha256:" + "1" * 64
        capture = self.root / "capture-image.json"
        result = self.capture(
            self.candidate("match"),
            capture,
            extra=("--implementation-id", "example-verifier",
                   "--implementation-image", digest_image),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        output = self.root / "image-report.json"
        self.assertEqual(self.score_capture(capture, output).returncode, 0)
        implementation = json.loads(output.read_text())["implementation"]
        self.assertEqual(implementation["id"], "example-verifier")
        self.assertEqual(implementation["image"], digest_image)

        def tag_only(document):
            document["implementation"]["image"] = "ghcr.io/example/verifier:latest"

        def half_binding(document):
            document["implementation"]["image"] = None

        for name, mutate in (("tag-only", tag_only), ("half-binding", half_binding)):
            with self.subTest(image=name):
                hostile = self.rewrite(capture, name, mutate)
                bad = self.root / f"image-{name}.json"
                result = self.score_capture(hostile, bad)
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertFalse(bad.exists())

    def test_run_record_omits_the_binding_when_the_capture_declared_none(self) -> None:
        capture = self.valid_capture("no-image")
        self.assertIsNone(json.loads(capture.read_text())["implementation"]["id"])
        output = self.root / "no-image-report.json"
        self.assertEqual(self.score_capture(capture, output).returncode, 0)
        implementation = json.loads(output.read_text())["implementation"]
        self.assertNotIn("id", implementation)
        self.assertNotIn("image", implementation)

    def test_capture_carries_no_oracle_derived_field(self) -> None:
        """Structural, not textual: every key must come from the declared vocabulary."""
        capture = json.loads(self.valid_capture("vocabulary").read_text())
        seen = set()
        pending = [capture]
        while pending:
            current = pending.pop()
            if isinstance(current, dict):
                seen.update(current)
                pending.extend(current.values())
            elif isinstance(current, list):
                pending.extend(current)
        allowed = (
            capture_format.TOP_LEVEL_KEYS
            | capture_format.IMPLEMENTATION_KEYS
            | capture_format.OBSERVED_KEYS
            | capture_format.ERROR_KEYS
            | {"bundle_integrity", "verdict", "claims", "status", "source_class"}
            | {
                "policy_decision_recorded",
                "caller_visible_denial",
                "upstream_delivery",
                "external_side_effect",
            }
        )
        self.assertEqual(seen - allowed, set())
        for forbidden in ("expected", "match", "mismatch", "score", "badge", "summary"):
            self.assertNotIn(forbidden, seen)

    def test_capture_schema_document_matches_the_validator_vocabulary(self) -> None:
        schema = json.loads(CAPTURE_SCHEMA_DOC.read_text())
        self.assertEqual(schema["properties"]["schema"]["const"], capture_format.CAPTURE_SCHEMA)
        self.assertEqual(sorted(capture_format.TOP_LEVEL_KEYS), schema["required"])
        observations = schema["properties"]["observations"]
        self.assertEqual(observations["minItems"], capture_format.EXPECTED_CASE_COUNT)
        self.assertEqual(observations["maxItems"], capture_format.EXPECTED_CASE_COUNT)
        self.assertEqual(
            schema["properties"]["capture_non_claims"]["prefixItems"],
            [{"const": text} for text in capture_format.CAPTURE_NON_CLAIMS],
        )
        observed = schema["$defs"]["observed"]["properties"]["exit_code"]
        self.assertEqual(observed["minimum"], capture_format.MIN_EXIT_CODE)
        self.assertEqual(observed["maximum"], capture_format.MAX_EXIT_CODE)
        self.assertEqual(
            schema["$defs"]["errored"]["properties"]["error"]["maxLength"],
            capture_format.MAX_ERROR_CHARS,
        )

    def test_capture_phase_completes_with_the_oracle_absent_from_its_filesystem(self) -> None:
        """The point of the split, measured rather than asserted.

        The probe reports whether it could open the canonical MANIFEST.json by
        the same relative path the composite action uses. The combined-mode arm
        is the positive control: without it, a probe that always reported
        `False` would pass this test while proving nothing.
        """
        probe = self.root / "oracle-probe.py"
        marker = self.root / "oracle-probe-marker.json"
        probe.write_text(
            textwrap.dedent(
                f"""\
                import json, os
                from pathlib import Path
                oracle = Path("conformance/privileged-mcp-action-v0/MANIFEST.json")
                marker = Path({str(marker)!r})
                try:
                    readable = len(json.loads(oracle.read_text())["vectors"]) > 0
                except OSError:
                    readable = False
                seen = json.loads(marker.read_text()) if marker.exists() else []
                seen.append({{"cwd": os.getcwd(), "oracle_readable": readable}})
                marker.write_text(json.dumps(seen))
                print(json.dumps({{"bundle_integrity": "fail"}}))
                """
            )
        )

        elsewhere = self.root / "no-oracle-here"
        elsewhere.mkdir(exist_ok=True)
        capture = self.root / "oracle-absent-capture.json"
        split = subprocess.run(
            [
                sys.executable,
                str(CAPTURE_SCRIPT),
                "--pack", str(self.pack),
                "--entrypoint", shlex.join([sys.executable, str(probe)]),
                "--implementation-name", "oracle probe",
                "--implementation-source", "https://example.test/verifier",
                "--implementation-commit", IMPLEMENTATION_COMMIT,
                "--reproduction-mode", "blind_from_spec",
                "--output", str(capture),
            ],
            cwd=elsewhere,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(split.returncode, 0, split.stderr)
        split_observations = json.loads(marker.read_text())
        marker.unlink()

        self.assertEqual(len(split_observations), 14)
        self.assertTrue(
            all(not seen["oracle_readable"] for seen in split_observations),
            "the capture phase must not carry the oracle on its filesystem",
        )

        report = self.root / "oracle-absent-report.json"
        self.assertEqual(self.score_capture(capture, report).returncode, 1)
        self.assertEqual(json.loads(report.read_text())["summary"]["total"], 14)

        combined = self.root / "oracle-present-report.json"
        self.score(probe, combined)
        combined_observations = json.loads(marker.read_text())
        self.assertEqual(len(combined_observations), 14)
        self.assertTrue(
            all(seen["oracle_readable"] for seen in combined_observations),
            "positive control: the probe must be able to discriminate",
        )


def _active_lines(text: str) -> list[str]:
    """YAML/script lines that can execute. Full-line comments and blanks do not."""
    lines = []
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        lines.append(stripped)
    return lines


def _keys_at_indent(
    block: str, indent: int, *, under: str | None = None
) -> tuple[str, ...]:
    """Active mapping keys at exactly `indent` spaces. One allowlist extractor.

    `under` limits collection to children of that parent key (parent at indent-2).
    """
    keys: list[str] = []
    parent_indent = indent - 2
    in_under = under is None
    key_prefix = " " * indent
    parent_prefix = " " * parent_indent if parent_indent >= 0 else ""
    for raw in block.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if under is not None:
            if parent_indent <= 0:
                at_parent = not raw.startswith(" ")
            else:
                at_parent = raw.startswith(parent_prefix) and not raw.startswith(
                    parent_prefix + " "
                )
            if at_parent:
                in_under = stripped.split(":", 1)[0] == under
                continue
        if not in_under:
            continue
        if indent == 0:
            at_key = not raw.startswith(" ")
        else:
            at_key = raw.startswith(key_prefix) and not raw.startswith(key_prefix + " ")
        if not at_key:
            continue
        if stripped.startswith("- "):
            item = stripped[2:].lstrip()
            if item.startswith("name:"):
                keys.append("name")
            continue
        keys.append(stripped.split(":", 1)[0])
    return tuple(keys)


def _on_paths(text: str, trigger: str) -> tuple[str, ...]:
    """Quoted path-filter entries under `on.<trigger>.paths`."""
    paths: list[str] = []
    in_on = False
    in_trigger = False
    in_paths = False
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if raw == "on:":
            in_on = True
            in_trigger = False
            in_paths = False
            continue
        if not in_on:
            continue
        if not raw.startswith(" "):
            break
        if raw.startswith("  ") and not raw.startswith("   "):
            in_trigger = stripped.split(":", 1)[0] == trigger
            in_paths = False
            continue
        if not in_trigger:
            continue
        if raw.startswith("    ") and not raw.startswith("     "):
            in_paths = stripped.rstrip(":") == "paths" or stripped.split(":", 1)[0] == "paths"
            continue
        if in_paths and raw.startswith("      - "):
            item = stripped[2:].strip()
            if item.startswith('"') and item.endswith('"'):
                item = item[1:-1]
            paths.append(item)
    return tuple(paths)


def _named_step_names(text: str) -> list[str]:
    """Active `- name:` values in document order."""
    names: list[str] = []
    for line in _active_lines(text):
        match = re.match(r"^-\s+name:\s*(.+)$", line)
        if match:
            names.append(match.group(1).strip())
    return names


def _top_level_mapping_children(text: str, header: str) -> tuple[str, ...]:
    """Active 2-space lines under column-0 `header:`, or a same-line scalar."""
    children: list[str] = []
    in_map = False
    prefix = f"{header}:"
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if not raw.startswith(" ") and raw.startswith(prefix):
            in_map = True
            scalar = raw[len(prefix) :].strip()
            if scalar:
                children.append(scalar)
            continue
        if not in_map:
            continue
        if raw.startswith("  ") and not raw.startswith("   "):
            children.append(stripped)
        elif not raw.startswith(" "):
            in_map = False
    return tuple(children)


def _top_level_mapping_keys(text: str, header: str) -> tuple[str, ...]:
    """Document-order keys of a column-0 mapping."""
    return tuple(child.split(":", 1)[0] for child in _top_level_mapping_children(text, header))


def _has_job_level_permissions(text: str) -> bool:
    """True when a job mapping (exactly 4-space keys) declares `permissions:`."""
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if raw.startswith("    permissions:") and not raw.startswith("     "):
            return True
    return False


def _job_mapping_keys(text: str, job: str) -> tuple[str, ...]:
    """Document-order 4-space keys under the `  {job}:` mapping."""
    keys: list[str] = []
    in_job = False
    for raw in text.splitlines():
        stripped = raw.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if raw.startswith("  ") and not raw.startswith("   "):
            in_job = stripped.split(":", 1)[0] == job
            continue
        if not in_job:
            continue
        if raw.startswith("    ") and not raw.startswith("     "):
            keys.append(stripped.split(":", 1)[0])
        elif not raw.startswith(" "):
            in_job = False
    return tuple(keys)


def _named_step_env_keys(block: str) -> frozenset[str]:
    """Active `env:` mapping keys inside one named step block."""
    keys: set[str] = set()
    in_env = False
    for line in _active_lines(block):
        key = line.split(":", 1)[0]
        if key == "env":
            in_env = True
            continue
        if not in_env:
            continue
        if key in _STEP_FIELD_KEYS:
            break
        keys.add(key)
    return frozenset(keys)


def _named_step_block(text: str, name: str) -> str | None:
    """Full named step mapping, from `- name:` through the next sibling step."""
    marker = f"      - name: {name}\n"
    start = text.find(marker)
    if start < 0:
        return None
    rest = text[start + len(marker) :]
    nxt = re.search(r"(?m)^      - ", rest)
    tail = rest if nxt is None else rest[: nxt.start()]
    return marker + tail


def _named_step_run_body(text: str, name: str) -> str | None:
    """Literal `run:` body of a named step. None if the step or body is missing."""
    block = _named_step_block(text, name)
    if block is None:
        return None
    run_m = re.search(r"(?m)^        run:\s*\|\s*$", block)
    if run_m is None:
        return None
    after = block[run_m.end() :]
    if after.startswith("\n"):
        after = after[1:]
    body: list[str] = []
    for line in after.splitlines():
        if line.startswith("          ") or line == "":
            body.append(line)
            continue
        break
    return "\n".join(body)


def _normalized_command_sequence(run_body: str) -> tuple[str, ...]:
    """Join `\\` continuations, drop comments, collapse insignificant whitespace."""
    commands: list[str] = []
    pending: list[str] = []
    for line in _active_lines(run_body):
        continued = line.endswith("\\")
        piece = line[:-1].rstrip() if continued else line
        if piece:
            pending.append(piece)
        if continued:
            continue
        if pending:
            commands.append(re.sub(r"\s+", " ", " ".join(pending)).strip())
            pending = []
    if pending:
        commands.append(re.sub(r"\s+", " ", " ".join(pending)).strip())
    return tuple(commands)


def oci_candidate_workflow_problems(
    text: str, *, skip: frozenset[str] = frozenset()
) -> list[str]:
    """Structural pins for the trusted-main OCI capture workflow. One function."""
    problems: list[str] = []

    def omitted(label: str) -> bool:
        return any(label == token or label.startswith(token) for token in skip)

    active = _active_lines(text)
    active_text = "\n".join(active)
    if "workflow_dispatch:" not in active_text:
        problems.append("missing workflow_dispatch")
    if re.search(r"(?m)^  pull_request", text) or "pull_request_target:" in active_text:
        problems.append("untrusted pull_request trigger")
    if re.search(r"(?m)^  push:", text):
        problems.append("push trigger")
    in_on = False
    for line in text.splitlines():
        stripped = line.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if line == "on:":
            in_on = True
            continue
        if not in_on:
            continue
        if line.startswith("  ") and not line.startswith("   "):
            key = stripped.split(":", 1)[0]
            if key != "workflow_dispatch":
                problems.append(f"extra trigger: {key}")
        elif not line.startswith(" "):
            in_on = False
    if "runs-on: ubuntu-24.04" not in active_text:
        problems.append("missing ubuntu-24.04")
    if re.search(r"runs-on:\s*.*self-hosted", active_text):
        problems.append("self-hosted runner")
    if (
        re.search(r"(?m)^\S+:\s*write\s*$", active_text)
        or "id-token:" in active_text
        or "attestations:" in active_text
    ):
        problems.append("excess permissions")
    if "contents: read" not in active_text:
        problems.append("missing contents:read")
    if "secrets." in active_text:
        problems.append("explicit secrets")
    if 'if [[ "$GITHUB_REF" != "refs/heads/main" ]]' not in active_text:
        problems.append("missing main ref guard")
    if "git rev-parse HEAD" not in active_text or "$GITHUB_SHA" not in active_text:
        problems.append("missing HEAD/SHA guard")
    if "persist-credentials: false" not in active_text:
        problems.append("missing persist-credentials:false")
    if "candidate-release.json" not in active_text:
        problems.append("missing candidate-release.json")
    if "validate_candidate_release.py" not in active_text:
        problems.append("missing validate_candidate_release.py")
    if "attestation-bundle.json" not in active_text:
        problems.append("missing attestation-bundle.json")
    if not any(re.match(r"^(-\s+)?gh attestation verify(\s|\\|$)", line) for line in active):
        problems.append("missing gh attestation verify")
    if "--bundle" not in active_text:
        problems.append("missing --bundle")
    if f"--signer-workflow {OCI_SIGNER_WORKFLOW}" not in active_text:
        problems.append("missing or wrong signer-workflow")
    if "--source-digest" not in active_text:
        problems.append("missing --source-digest")
    if OCI_SOURCE_DIGEST_SHAPE not in active_text:
        problems.append("missing source_digest regex")
    elif OCI_SOURCE_DIGEST_GUARD not in active_text:
        problems.append("source_digest check not fail-closed")
    if "--source-ref refs/heads/main" not in active_text:
        problems.append("missing --source-ref")
    if "--deny-self-hosted-runners" not in active_text:
        problems.append("missing --deny-self-hosted-runners")
    if "release_attestation_enforce.sh" in active_text:
        problems.append("software-release verifier used for pack")
    if not any(
        re.match(rf"^(-\s+)?python3\s+{re.escape(OCI_EXECUTOR)}(\s|\\|$)", line)
        for line in active
    ):
        problems.append("missing canonical executor")
    if (
        "--pack" not in active_text
        or "--implementation-id" not in active_text
        or "--output" not in active_text
    ):
        problems.append("missing executor argv")
    if "--timeout-seconds 30" not in active_text:
        problems.append("missing --timeout-seconds 30")
    if "--implementation-image" in active_text or "--registry" in active_text:
        problems.append("direct image or registry override")
    if "python3 - <<" in active_text:
        problems.append("inline Python")
    if OCI_IMPLEMENTATION_ID_ENV not in active_text:
        problems.append("missing implementation_id env binding")
    if OCI_IMPLEMENTATION_ID_ARGV not in active_text:
        problems.append("implementation_id not quoted argv")
    for line in active:
        if "${{ inputs." not in line:
            continue
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*:\s*\$\{\{ inputs\.", line):
            continue
        problems.append("inputs interpolated in run script")
        break
    if "continue-on-error" in active_text:
        problems.append("continue-on-error swallows failure")
    if "|| true" in active_text:
        problems.append("|| true swallows failure")
    for pin, label in (
        (OCI_CHECKOUT_PIN, "unpinned or wrong checkout"),
        (OCI_SETUP_PYTHON_PIN, "unpinned or wrong setup-python"),
        (OCI_UPLOAD_PIN, "unpinned or wrong upload-artifact"),
    ):
        if not any(re.match(rf"^(-\s+)?uses:\s*{re.escape(pin)}", line) for line in active):
            problems.append(label)
    if OCI_PYTHON_VERSION not in active_text:
        problems.append("missing python 3.13.8")
    upload_steps = [
        block
        for block in re.split(r"(?m)^(?=      - )", text)
        if any(
            re.match(rf"^(-\s+)?uses:\s*{re.escape(OCI_UPLOAD_PIN)}", line)
            for line in _active_lines(block)
        )
    ]
    upload_ifs = [
        line
        for block in upload_steps
        for line in _active_lines(block)
        if line.startswith("if:")
    ]
    if not omitted("upload not gated on success") and upload_ifs != ["if: success()"]:
        problems.append("upload not gated on success")
    if "candidate_capture.v0" not in active_text:
        problems.append("missing fixed capture name")
    if OCI_CAPTURE_UPLOAD_PATH not in active_text:
        problems.append("missing exact capture upload path")
    if "retention-days: 7" not in active_text:
        problems.append("missing retention-days: 7")
    if "if-no-files-found: error" not in active_text:
        problems.append("missing if-no-files-found:error")
    if "timeout-minutes: 25" not in active_text:
        problems.append("missing timeout-minutes: 25")
    for line in active:
        for match in USES_SHA_RE.finditer(line):
            pin = match.group(1)
            if not re.fullmatch(r"[0-9a-f]{40}", pin):
                problems.append(f"unpinned uses: {match.group(0)}")
    for name in _named_step_names(text):
        if name == OCI_UPLOAD_STEP:
            continue
        block = _named_step_block(text, name)
        if block is not None and any(
            re.match(r"^if:", line) for line in _active_lines(block)
        ):
            problems.append(f"conditional step: {name}")
    for name, allowed in OCI_PINNED_STEP_SEQUENCES.items():
        body = _named_step_run_body(text, name)
        if body is None or _normalized_command_sequence(body) != allowed:
            problems.append(f"unexpected run sequence: {name}")
    if _top_level_mapping_keys(text, "jobs") != (OCI_CAPTURE_JOB,):
        problems.append("unexpected jobs")
    if _job_mapping_keys(text, OCI_CAPTURE_JOB) != OCI_CAPTURE_JOB_KEYS:
        problems.append("unexpected capture job keys")
    if tuple(_named_step_names(text)) != OCI_CAPTURE_STEP_NAMES:
        problems.append("unexpected step names")
    if _top_level_mapping_children(text, "permissions") != OCI_TOP_LEVEL_PERMISSIONS:
        problems.append("unexpected top-level permissions")
    if _has_job_level_permissions(text):
        problems.append("job-level permissions")
    for name, allowed in OCI_STEP_ENV_KEYS.items():
        block = _named_step_block(text, name)
        if block is None or _named_step_env_keys(block) != allowed:
            problems.append(f"unexpected env keys: {name}")
    if not omitted("unexpected top-level keys"):
        if _keys_at_indent(text, 0) != OCI_DOCUMENT_KEYS:
            problems.append("unexpected top-level keys")
    if not omitted("unexpected step keys"):
        for name, allowed in OCI_STEP_KEYS.items():
            block = _named_step_block(text, name)
            actual = ("name",) + _keys_at_indent(block, 8) if block is not None else ()
            if block is None or actual != allowed:
                problems.append(f"unexpected step keys: {name}")
    if not omitted("unexpected with keys"):
        for name, allowed in OCI_STEP_WITH_KEYS.items():
            block = _named_step_block(text, name)
            actual = (
                _keys_at_indent(block, 10, under="with") if block is not None else ()
            )
            if block is None or actual != allowed:
                problems.append(f"unexpected with keys: {name}")
    if not omitted("unexpected TAG bindings"):
        if sum(1 for line in active if line == OCI_TAG_BINDING) != 2:
            problems.append("unexpected TAG bindings")
    if not omitted("missing exact artifact name"):
        if sum(1 for line in active if line == OCI_ARTIFACT_NAME) != 1:
            problems.append("missing exact artifact name")
    return problems


class PrivilegedMcpActionOciCandidateWorkflowContract(unittest.TestCase):
    """Capture-only trusted-main workflow. Structural; no live dispatch."""

    def test_workflow_file_exists(self) -> None:
        self.assertTrue(
            OCI_CANDIDATE_WORKFLOW.is_file(),
            f"missing {OCI_CANDIDATE_WORKFLOW}",
        )

    def test_trusted_main_capture_contract(self) -> None:
        text = OCI_CANDIDATE_WORKFLOW.read_text(encoding="utf-8")
        self.assertEqual(oci_candidate_workflow_problems(text), [])

    def test_mutations_fail_independently(self) -> None:
        text = OCI_CANDIDATE_WORKFLOW.read_text(encoding="utf-8")
        self.assertEqual(oci_candidate_workflow_problems(text), [])
        drops = (
            ('if [[ "$GITHUB_REF" != "refs/heads/main" ]]', "missing main ref guard"),
            ("gh attestation verify", "missing gh attestation verify"),
            ("attestation-bundle.json", "missing attestation-bundle.json"),
            ("--source-digest", "missing --source-digest"),
            ("^[0-9a-f]{40}$", "missing source_digest regex"),
            (" || exit 2", "source_digest check not fail-closed"),
            ("--source-ref refs/heads/main", "missing --source-ref"),
            (f"--signer-workflow {OCI_SIGNER_WORKFLOW}", "missing or wrong signer-workflow"),
            ("--deny-self-hosted-runners", "missing --deny-self-hosted-runners"),
            (OCI_EXECUTOR, "missing canonical executor"),
            ("--implementation-id", "missing executor argv"),
            (OCI_IMPLEMENTATION_ID_ENV, "missing implementation_id env binding"),
            (OCI_IMPLEMENTATION_ID_ARGV, "implementation_id not quoted argv"),
            (OCI_SETUP_PYTHON_PIN, "unpinned or wrong setup-python"),
            (OCI_PYTHON_VERSION, "missing python 3.13.8"),
            (OCI_CAPTURE_UPLOAD_PATH, "missing exact capture upload path"),
            ("retention-days: 7", "missing retention-days: 7"),
            ("if: success()", "upload not gated on success"),
        )
        for needle, expected in drops:
            with self.subTest(drop=needle):
                mutated = text.replace(needle, "")
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        comment_out = (
            (
                OCI_SOURCE_DIGEST_GUARD,
                f"# {OCI_SOURCE_DIGEST_GUARD}",
                "missing source_digest regex",
            ),
            (
                "--source-ref refs/heads/main",
                "# --source-ref refs/heads/main",
                "missing --source-ref",
            ),
            (
                f"--signer-workflow {OCI_SIGNER_WORKFLOW}",
                f"# --signer-workflow {OCI_SIGNER_WORKFLOW}",
                "missing or wrong signer-workflow",
            ),
            (
                "--bundle",
                "# --bundle",
                "missing --bundle",
            ),
            (
                "contents: read",
                "# contents: read",
                "missing contents:read",
            ),
        )
        for needle, replacement, expected in comment_out:
            with self.subTest(comment_out=expected):
                mutated = text.replace(needle, replacement)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        decorative = (
            (
                "gh attestation verify \\",
                "echo gh attestation verify \\",
                "missing gh attestation verify",
            ),
            (
                f"python3 {OCI_EXECUTOR} \\",
                f"echo python3 {OCI_EXECUTOR} \\",
                "missing canonical executor",
            ),
            (
                "if: success()",
                "# if: success()",
                "upload not gated on success",
            ),
        )
        for needle, replacement, expected in decorative:
            with self.subTest(decorative=expected):
                mutated = text.replace(needle, replacement, 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        extra = text + "\n      --implementation-image ghcr.io/example/x@sha256:" + "ab" * 32 + "\n"
        extra = extra.replace(OCI_EXECUTOR, "python3 -c 'pass'", 1)
        with self.subTest(kind="inline-and-image"):
            problems = oci_candidate_workflow_problems(extra)
            self.assertTrue(any("canonical executor" in p or "inline" in p for p in problems))
            self.assertTrue(any("image or registry" in p for p in problems))
        software = text.replace(
            "gh attestation verify",
            "bash scripts/ci/release_attestation_enforce.sh",
            1,
        )
        with self.subTest(kind="software-release-verifier"):
            problems = oci_candidate_workflow_problems(software)
            self.assertTrue(any("software-release verifier" in p for p in problems))
        inserts = (
            ("on:\n", "on:\n  pull_request:\n    branches: [main]\n", "untrusted pull_request trigger"),
            ("on:\n", "on:\n  schedule:\n    - cron: '0 0 * * *'\n", "extra trigger: schedule"),
            ("on:\n", "on:\n  repository_dispatch:\n", "extra trigger: repository_dispatch"),
            ("on:\n", "on:\n  workflow_call:\n", "extra trigger: workflow_call"),
            ("ubuntu-24.04", "self-hosted", "self-hosted runner"),
            ("permissions:\n  contents: read\n", "permissions:\n  contents: write\n", "excess permissions"),
            (
                "permissions:\n  contents: read\n",
                "permissions:\n  contents: read\n  packages: write\n",
                "excess permissions",
            ),
            ("permissions:\n", "env:\n  TOKEN: ${{ secrets.FOO }}\npermissions:\n", "explicit secrets"),
            (OCI_CHECKOUT_PIN, "actions/checkout@v5", "unpinned uses:"),
            (OCI_SETUP_PYTHON_PIN, "actions/setup-python@v6", "unpinned or wrong setup-python"),
            (OCI_PYTHON_VERSION, 'python-version: "3.12.0"', "missing python 3.13.8"),
        )
        for needle, replacement, expected in inserts:
            with self.subTest(insert=expected):
                mutated = text.replace(needle, replacement, 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        with self.subTest(kind="inputs-in-run"):
            mutated = text.replace(OCI_IMPLEMENTATION_ID_ENV + "\n", "", 1).replace(
                OCI_IMPLEMENTATION_ID_ARGV,
                "--implementation-id ${{ inputs.implementation_id }}",
                1,
            )
            problems = oci_candidate_workflow_problems(mutated)
            self.assertTrue(
                any("inputs interpolated in run script" in problem for problem in problems),
                f"expected inputs interpolated in {problems}",
            )
        swallow_steps = (
            "Resolve published pack tag",
            "Download attested pack",
            "Verify pack attestation",
            "Capture candidate observations",
        )
        for step in swallow_steps:
            with self.subTest(continue_on_error=step):
                needle = f"      - name: {step}\n"
                mutated = text.replace(needle, needle + "        continue-on-error: true\n", 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any("continue-on-error swallows failure" in problem for problem in problems),
                    f"expected continue-on-error in {problems}",
                )
        swallow_commands = (
            "validate_candidate_release.py",
            "gh release download",
            "gh attestation verify",
            OCI_EXECUTOR,
        )
        for cmd in swallow_commands:
            with self.subTest(or_true=cmd):
                mutated = text.replace(cmd, f"{cmd} || true", 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any("|| true swallows failure" in problem for problem in problems),
                    f"expected || true in {problems}",
                )
        verify_cmd = (
            "          gh attestation verify \\\n"
            '            "$RUNNER_TEMP/oci-downloads/privileged-mcp-action-v0-clean-room.tar.gz" \\\n'
            "            --repo Rul1an/assay \\\n"
            '            --bundle "$RUNNER_TEMP/oci-downloads/attestation-bundle.json" \\\n'
            f"            --signer-workflow {OCI_SIGNER_WORKFLOW} \\\n"
            '            --source-digest "$source_digest" \\\n'
            "            --source-ref refs/heads/main \\\n"
            "            --deny-self-hosted-runners"
        )
        executor_cmd = (
            f"          python3 {OCI_EXECUTOR} \\\n"
            '            --pack "$PACK" \\\n'
            '            --implementation-id "$IMPLEMENTATION_ID" \\\n'
            '            --output "$OUTPUT" \\\n'
            "            --timeout-seconds 30"
        )
        reachability = (
            (
                "verify_if_false",
                verify_cmd,
                f"          if false; then\n{verify_cmd}\n          fi",
                "unexpected run sequence: Verify pack attestation",
            ),
            (
                "executor_if_false",
                executor_cmd,
                f"          if false; then\n{executor_cmd}\n          fi",
                "unexpected run sequence: Capture candidate observations",
            ),
            (
                "exit_0_before_verify",
                "          gh attestation verify \\",
                "          exit 0\n          gh attestation verify \\",
                "unexpected run sequence: Verify pack attestation",
            ),
        )
        for kind, needle, replacement, expected in reachability:
            with self.subTest(kind=kind):
                self.assertIn(needle, text)
                mutated = text.replace(needle, replacement, 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        step_conditionals = (
            (
                "checkout_if_false",
                "Check out current main",
                "conditional step: Check out current main",
            ),
            (
                "setup_python_if_false",
                "Set up Python",
                "conditional step: Set up Python",
            ),
            (
                "trusted_main_if_false",
                "Require trusted main",
                "conditional step: Require trusted main",
            ),
            (
                "verify_step_if_false",
                "Verify pack attestation",
                "conditional step: Verify pack attestation",
            ),
            (
                "capture_step_if_false",
                "Capture candidate observations",
                "conditional step: Capture candidate observations",
            ),
        )
        for kind, step, expected in step_conditionals:
            with self.subTest(kind=kind):
                needle = f"      - name: {step}\n"
                self.assertIn(needle, text)
                mutated = text.replace(needle, needle + "        if: false\n", 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        capture_job = "  capture:\n"
        capture_env = "          IMPLEMENTATION_ID: ${{ inputs.implementation_id }}\n"
        executor_line = f"          python3 {OCI_EXECUTOR} \\\n"
        shape = (
            (
                "job_write_all",
                capture_job,
                "  capture:\n    permissions: write-all\n",
                "unexpected capture job keys",
            ),
            (
                "job_env_token",
                capture_job,
                "  capture:\n    env:\n      GH_TOKEN: ${{ github.token }}\n",
                "unexpected capture job keys",
            ),
            (
                "curl_bash_pre_guard",
                "    steps:\n",
                "    steps:\n      - name: Pre-guard\n        run: curl | bash\n",
                "unexpected step names",
            ),
            (
                "capture_gh_token",
                capture_env,
                capture_env + "          GH_TOKEN: ${{ github.token }}\n",
                "unexpected env keys",
            ),
            (
                "inline_python_direct",
                executor_line,
                "          python3 - <<'PY'\n          print(0)\n          PY\n" + executor_line,
                "inline Python",
            ),
        )
        for kind, needle, replacement, expected in shape:
            with self.subTest(kind=kind):
                self.assertIn(needle, text)
                mutated = text.replace(needle, replacement, 1)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
        with self.subTest(kind="duplicate_upload_always"):
            mutated = text + (
                f"\n      - name: {OCI_UPLOAD_STEP}\n"
                "        if: always()\n"
                f"        uses: {OCI_UPLOAD_PIN}\n"
            )
            problems = oci_candidate_workflow_problems(mutated)
            self.assertTrue(
                any("unexpected step names" in problem for problem in problems),
                f"expected unexpected step names in {problems}",
            )
        allowlist = (
            (
                "workflow_env_token",
                "on:\n",
                "env:\n  GH_TOKEN: ${{ github.token }}\non:\n",
                "unexpected top-level keys",
            ),
            (
                "upload_if_or_failure",
                "        if: success()\n",
                "        if: success() || failure()\n",
                "upload not gated on success",
            ),
            (
                "tag_literal",
                f"          {OCI_TAG_BINDING}\n",
                "          TAG: privileged-mcp-action-v0-candidate.4\n",
                "unexpected TAG bindings",
            ),
            (
                "artifact_name",
                f"          {OCI_ARTIFACT_NAME}\n",
                "          name: candidate-capture-v0-extra\n",
                "missing exact artifact name",
            ),
            (
                "capture_working_directory",
                "      - name: Capture candidate observations\n",
                "      - name: Capture candidate observations\n"
                "        working-directory: /tmp\n",
                "unexpected step keys",
            ),
            (
                "checkout_extra_with",
                "          persist-credentials: false\n",
                "          persist-credentials: false\n"
                "          repository: example/evil\n",
                "unexpected with keys",
            ),
            (
                "unknown_top_level",
                "on:\n",
                "assay-note: ignore\non:\n",
                "unexpected top-level keys",
            ),
            (
                "unknown_step_timeout",
                "      - name: Require trusted main\n",
                "      - name: Require trusted main\n        timeout-minutes: 5\n",
                "unexpected step keys",
            ),
        )
        for kind, needle, replacement, expected in allowlist:
            with self.subTest(kind=kind):
                self.assertIn(needle, text)
                mutated = text.replace(needle, replacement)
                problems = oci_candidate_workflow_problems(mutated)
                self.assertTrue(
                    any(expected in problem for problem in problems),
                    f"expected {expected!r} in {problems}",
                )
                restored = oci_candidate_workflow_problems(
                    mutated, skip=frozenset({expected})
                )
                self.assertEqual(
                    restored,
                    [],
                    f"{kind} must be [] when {expected!r} is skipped: {restored}",
                )

    def test_conformance_path_filters_include_oci_workflow(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        self.assertEqual(_on_paths(text, "pull_request"), CONFORMANCE_REQUIRED_PATHS)
        self.assertEqual(_on_paths(text, "push"), CONFORMANCE_REQUIRED_PATHS)


if __name__ == "__main__":
    unittest.main()
