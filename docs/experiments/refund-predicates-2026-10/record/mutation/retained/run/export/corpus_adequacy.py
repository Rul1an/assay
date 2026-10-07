#!/usr/bin/env python3
"""Mutation adequacy for a published conformance corpus, driven by a manifest.

Standard library only. No Assay import, no pip install, no network.

    python3 corpus_adequacy.py <manifest.json>
    python3 corpus_adequacy.py <manifest.json> --json

WHY A CORPUS NEEDS THIS AND A TEST SUITE DOES NOT
-------------------------------------------------
Outcome coverage is not rule coverage. A corpus can reach every declared
outcome while some rule never decides anything, because another rule reaches
the same outcome first on every vector it would have caught. Mutation adequacy
is the criterion that finds that, and it outperforms structural criteria such
as line and branch coverage for exactly this question.

The bar here is higher than the usual one. In ordinary mutation testing the
artifact under test is a test suite, a surviving mutant is a gap in confidence,
and a score near 80% is a working target. Here the artifact under test is a
*published corpus whose digest is the contract*. After a valid run, a surviving
mutant means only that the declared mutation did not change the declared
outcome on the pinned inputs. Reading that as a hole in the contract also
requires a faithful owner-pinned declaration and rule ownership. So the
required score is 100% of non-equivalent mutants.

WHAT THIS TOOL CAN AND CANNOT GENERALIZE, STATED PLAINLY
---------------------------------------------------------
It cannot infer a corpus's rules from arbitrary source. That would be a static
analysis project, and a tool that guessed would report a score it had not
earned. What is portable is the *method*, so a corpus that wants to be measured
declares its own mutants in a manifest:

  - one mutant per DECLARED RULE, not per line. A surviving mutant then names
    the rule an implementation could omit, instead of naming a line number.
    Generic operators would produce mostly-equivalent noise over rules this
    small.
  - ordinal axes are PERMUTED, not deleted. Deletion is the wrong operator for
    a ladder: the ordering lives in a table a comparison reads, not in a branch
    a mutant can cut. Declare flatten-to-top, flatten-to-bottom and invert.
  - equivalent mutants are DECLARED WITH A REASON, never inferred. Deciding
    mutant equivalence is undecidable in general, so a tool that claimed to
    detect it would be lying.
    - child termination is classified before stdout is parsed. Default
    accepted_exit_codes is [0]. Opt-in unproved_exit_codes is [] and must
    be disjoint. A parseable report on an undeclared code, a signal, or a
    missing code is not an outcome. A declared-unproved exit is not an
    outcome either, even when stdout is valid JSON. An observed unexpected
    exit or signal on an ordinary mutant may be a kill with that class
    named; a control abnormality is control-error and is not a score.

WHAT THE PERCENTAGE IS A PERCENTAGE OF
---------------------------------------
100% here means 100% of the rules THE AUTHOR DECLARED. It does not mean 100% of
the rules the implementation has. A rule nobody declared is invisible to this
check, and there is no honest mechanical fix for that inside a manifest-driven
design: the manifest is written by the same hand as the corpus.

That is why the report never prints a bare percentage. It prints the numerator,
the denominator, the count declared equivalent, the count declared out of scope,
and the ratio between excluded and measured. A score reported without those is a
percentage target wearing a different coat, because an author can exclude almost
everything and still print 100%.

For the same reason an out_of_scope mutant must carry a stated reason. It leaves
the denominator exactly as a declared-equivalent one does, so it carries the same
obligation.

SELF-COVERAGE, APPLIED TO THIS CHECK
-------------------------------------
A group present in the corpus that declares no mutant is the same defect one
level up: the check would silently cover less than its name claims. That is a
hard failure here, not a warning.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import stat
import sys
import subprocess
import tempfile
try:
    import fcntl                       # POSIX advisory locks
except ImportError:                          # pragma: no cover - non-POSIX
    fcntl = None
from collections import namedtuple
from pathlib import Path
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_run import (  # noqa: E402
    OUTPUT_CAP_BYTES, _OutputTooLarge, _run_capped,
)
from isolated_tree import IsolationError, IsolatedMutationTree  # noqa: E402

SCHEMA = "corpus-adequacy.manifest.v0"
MANIFEST_V1_SCHEMA = "corpus-adequacy.manifest.v1"
ERROR_SCHEMA = "corpus-adequacy.error.v0"
REPORT_SCHEMA = "corpus-adequacy.report.v0"
SURVIVORS_SCHEMA = "corpus-adequacy.survivors.v0"
KILLS_SCHEMA = "corpus-adequacy.kills.v0"
RULES_SCHEMA = "corpus-adequacy.rules.v0"
DIFF_SCHEMA = "corpus-adequacy.diff.v0"
INSPECT_SCHEMA = "corpus-adequacy.inspect.v0"
ANCHOR_EXCERPT_MAX = 200
# One place. The report, --version, and CHANGELOG name this.
# A tag v+VERSION exists only after the documented cut.
# A SHA pin is exact and opaque; this is the name a measurement can quote.
VERSION = "0.8.0"

# Every shipped runtime source, in one ordered explicit set. HEAD byte equality
# and the content digest read this same tuple, so a runtime file added without
# declaring it here cannot be silently excluded from tool identity. Enumerating
# sys.modules instead would absorb whatever the measured candidate imports.
TOOL_SOURCE_PATHS = (
    "bounded_run.py",
    "contained_contract.py",
    "corpus_adequacy.py",
    "execution_observation.py",
    "isolated_tree.py",
    "module_child.py",
    "observation_session.py",
)
# The digest is domain-tagged so it cannot be confused with a bare concatenation
# of the same bytes under some other rule.
TOOL_SOURCE_DIGEST_TAG = b"corpus-adequacy.tool-source.v0\n"
_COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
_MANIFEST_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
REPORT_VERDICTS = frozenset({
    "equivalent",
    "unproved",
    "killed",
    "unexercised",
    "known-hole",
    "silent",
    "survived",
    "control-killed",
    "control-SURVIVED",
    "control-unchanged",
    "control-MOVED",
    "control-error",
})
TOOL_SOURCE_STATES = frozenset({"exact", "dirty", "unresolved"})
DIFF_NON_CLAIMS = (
    "This projection does not establish causation for any verdict transition.",
    "This projection does not establish mutation identity beyond the report label.",
    "This projection does not recompute corpus identity; corpus_digest is an author-declared string.",
    "This projection does not establish that an inadequate or unproved input is a valid completed comparison.",
)
_DIFF_TOP_KEYS = frozenset({
    "schema", "old_input", "new_input", "identity", "rows", "counts", "non_claims",
})
_DIFF_INPUT_KEYS = frozenset({"adequate", "control_status", "unproved"})
_DIFF_IDENTITY_KEYS = frozenset({"manifest_sha256", "corpus_digest", "tool"})
_DIFF_COMPONENT_KEYS = frozenset({"old", "new", "status"})
_DIFF_TOOL_KEYS = frozenset({
    "tool_version", "tool_commit", "tool_source_state", "tool_content_sha256",
})
_DIFF_ROW_KEYS = frozenset({
    "label", "presence", "old", "new", "verdict_transition",
    "changed_fields", "acknowledgement_retired",
})
_DIFF_COUNT_KEYS = frozenset({
    "common", "added", "removed", "verdict_changed", "verdict_same",
    "acknowledgement_retired",
})
_REPORT_BOOL_KEYS = ("adequate", "diagnostic_channel_declared")
_REPORT_INT_KEYS = (
    "killed", "survived", "silent", "known_holes", "acknowledged_digests",
    "equivalent", "unexercised_out_of_scope", "unproved", "declared_total",
)
_RULE_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$")

RULE_GROUPS_MAX = 128
RULE_ROWS_PER_GROUP_MAX = 1024
RULE_ROWS_TOTAL_MAX = 4096
RULE_MUTANTS_PER_ROW_MAX = 256
RULE_MUTANT_REFERENCES_MAX = 8192
CLASS_PROVENANCE_SCHEMA = "corpus-adequacy.class-provenance.v0"
CLASS_ATTEMPT_SCHEMA = "corpus-adequacy.class-attempt.v0"
CLASS_IDS = frozenset({"declared", "independent", "held_out", "real_fault", "adaptive"})
CLASS_EFFECTIVE_IDS = CLASS_IDS | frozenset({"unknown"})
CLASS_VISIBILITY_STATUSES = frozenset({
    "declared", "hidden-until-freeze", "disclosed-before-freeze", "unknown",
})
CLASS_INPUT_CAP_BYTES = OUTPUT_CAP_BYTES
CLASS_EVENTS_MAX = 64
CLASS_EXPECTED_DISTINCTIONS_MAX = RULE_MUTANT_REFERENCES_MAX
CLASS_ROWS_MAX = RULE_MUTANT_REFERENCES_MAX
CLASS_AUTHORING_ID_MAX = 256
CLASS_NON_CLAIMS = (
    "No evidence class proves rule completeness, corpus quality, implementation correctness, real-world prevalence, security, conformance, or author independence.",
    "Each denominator is one declared selection and is not a population estimate.",
    "This artifact defines one class only; no aggregate score or overall adequacy exists.",
)
_CLASS_REPOSITORY_RE = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$")
_CLASS_ORIGIN_BY_REQUEST = {
    "declared": "authored",
    "independent": "authored",
    "held_out": "authored",
    "real_fault": "historical_fault",
    "adaptive": "adaptive",
}
_CLASS_EVENT_NAMES = frozenset({
    "selection-committed", "candidate-frozen", "selection-disclosed",
})
_CLASS_RELATIONSHIPS = frozenset({"same", "independent", "unknown"})
_CLASS_PROVENANCE_KEYS = frozenset({
    "schema", "class_id", "requested_class", "manifest_sha256",
    "mutation_bundle_sha256", "candidate_freeze", "authoring",
    "visibility_events", "origin", "expected_distinctions", "non_claims",
})
_CLASS_FREEZE_KEYS = frozenset({
    "candidate", "corpus", "observation_declaration_sha256",
})
_CLASS_SOURCE_KEYS = frozenset({"repository", "commit", "tree_sha256"})
_CLASS_AUTHORING_KEYS = frozenset({
    "mutation_author", "candidate_builder", "relationship",
    "candidate_outcomes_seen",
})
_CLASS_EVENT_KEYS = frozenset({
    "ordinal", "event", "mutation_bundle_sha256", "actor",
    "predecessor_event_sha256",
})
_CLASS_AUTHORED_ORIGIN_KEYS = frozenset({"kind", "source"})
_CLASS_FAULT_ORIGIN_KEYS = frozenset({
    "kind", "repository", "faulty_commit", "fixed_commit", "reference",
})
_CLASS_ADAPTIVE_ORIGIN_KEYS = frozenset({
    "kind", "source", "predecessor_attempt_sha256",
})
_CLASS_DISTINCTION_KEYS = frozenset({"group", "label", "channel", "member"})
_CLASS_ATTEMPT_KEYS = frozenset({
    "schema", "attempt_id", "class_id", "provenance_sha256", "manifest_sha256",
    "report_sha256", "environment_sha256", "effective_class",
    "visibility_status", "predecessor_attempt_sha256", "status", "result",
    "rows", "non_claims",
})
_CLASS_RESULT_KEYS = frozenset({
    "control_status", "killed", "survived", "silent", "equivalent",
    "unexercised_out_of_scope", "unproved", "known_holes", "denominator",
    "score_percent", "adequate", "failures",
})
_ORDINARY_VERDICT_TO_COUNT = {
    "killed": "killed",
    "survived": "survived",
    "silent": "silent",
    "equivalent": "equivalent",
    "unexercised": "unexercised_out_of_scope",
    "unproved": "unproved",
    "known-hole": "known_holes",
}

OPERATOR_PROFILE_KEY = "execution_profile"
MINIMUM_PROFILE_KEY = "minimum_execution_profile"
CLOSED_EXECUTION_PROFILES = frozenset(
    {"trusted-local", "contained-oci-v0", "contained-oci-v1"})
_PROFILE_STRENGTH = {"trusted-local": 0, "contained-oci-v0": 1, "contained-oci-v1": 2}
# Profiles that must never reach a local backend or the module runner.
_CONTAINED_PROFILES = frozenset({"contained-oci-v0", "contained-oci-v1"})
# Profiles the engine may execute. A closed member left out of this set
# resolves, orders and refuses like any other but is refused before a backend
# is called. contained-oci-v1 is in it (#102 A3) because its consumers now
# select by the resolved profile, and it executes only through a backend
# that declares its profile (see _require_contained_execution).
_EXECUTABLE_PROFILES = frozenset(
    {"trusted-local", "contained-oci-v0", "contained-oci-v1"})
# Contained profiles whose backend must declare the profile it was built for.
# A contained-oci-v0 backend may still be undeclared, as every v0 caller was
# before backends declared anything; a declared one must still match.
_DECLARATION_REQUIRED_PROFILES = frozenset({"contained-oci-v1"})
# The attribute a contained backend declares its profile on.
BACKEND_PROFILE_ATTRIBUTE = "execution_profile"
# A backend that sets this attribute to True receives one more keyword per call, `step`, naming
# what the engine is running (#185). A backend without it is called exactly as before.
BACKEND_STEP_ATTRIBUTE = "accepts_step"
_UNDECLARED = object()


class ManifestError(Exception):
    """The manifest does not describe a measurable corpus."""


class ReportEncodingError(ValueError):
    """A successful report cannot be represented by the v0 byte contract."""


def require_shape(obj, expected, where: str) -> None:
    """One boundary: a container is the declared JSON kind, or the run does not start.

    Walking .items() or a non-object entry is a traceback, not a measurement refusal.
    """
    if expected is dict:
        if not isinstance(obj, dict):
            raise ManifestError("%s must be an object, got %s" % (where, type(obj).__name__))
        return
    if expected is list:
        if not isinstance(obj, list):
            raise ManifestError("%s must be an array, got %s" % (where, type(obj).__name__))
        return
    raise TypeError("require_shape expected dict or list")


def load_json_document(raw, *, root=None, where: str):
    """One strict decoder; optional root is checked exactly once."""
    def refuse_const(value):
        raise ManifestError("non-finite JSON number %s" % value)

    def no_duplicate_keys(pairs):
        obj = {}
        for key, value in pairs:
            if key in obj:
                raise ManifestError("duplicate JSON key %r" % key)
            obj[key] = value
        return obj

    def refuse_nonfinite(value):
        stack = [value]
        while stack:
            item = stack.pop()
            if isinstance(item, float) and (
                    item != item or item in (float("inf"), -float("inf"))):
                raise ManifestError("non-finite JSON number %r" % item)
            if isinstance(item, dict):
                stack.extend(item.values())
            elif isinstance(item, list):
                stack.extend(item)
        return value

    try:
        text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
    except UnicodeDecodeError:
        raise ManifestError("%s input is not UTF-8" % where) from None
    try:
        doc = refuse_nonfinite(json.loads(
            text, parse_constant=refuse_const, object_pairs_hook=no_duplicate_keys))
    except RecursionError as exc:
        raise ManifestError(str(exc)) from None
    if root is not None:
        require_shape(doc, root, where)
    return doc


def error_envelope(exc: BaseException, *, operation: str) -> dict:
    """Parseable --json body for a run that never produced a report.

    `operation` is the verb (`measure`, `project`, `inspect`, or `invoke`). One envelope, no
    second parser rule. The field and stderr share that verb.
    """
    if operation not in ("measure", "project", "inspect", "invoke"):
        raise ValueError("error_envelope operation must be measure, project, inspect or invoke")
    return {
        "schema": ERROR_SCHEMA,
        "ok": False,
        "error": "could not %s: %s" % (operation, exc),
        "exit": 2,
    }


def _git_bytes(root: Path, *args: str) -> subprocess.CompletedProcess:
    """Plumbing, bytes out. Porcelain is never the source of truth here.

    `git status` answers whether git considers a path modified, which is an
    index-aware question. Tool identity asks a different one: are the bytes on
    disk the bytes HEAD committed. Only plumbing can answer that directly.
    """
    env = os.environ.copy()
    env["GIT_TERMINAL_PROMPT"] = "0"
    return subprocess.run(
        ["git", "-C", str(root), *args],
        capture_output=True, timeout=10, env=env,
    )


def _runtime_source_bytes(root: Path) -> list | None:
    """Raw bytes of every declared runtime path, in declared order.

    None when the executing surface is not the declared surface: a missing
    path, a symlink, a directory, or a path that resolves outside the tool
    root. Those bytes are not this checkout's bytes, so there is nothing
    honest to attribute to HEAD and nothing honest to digest.
    """
    try:
        root_real = root.resolve()
    except OSError:
        return None
    collected = []
    for rel in TOOL_SOURCE_PATHS:
        path = root / rel
        try:
            st = path.lstat()
        except OSError:
            return None
        if not stat.S_ISREG(st.st_mode):
            return None
        try:
            if path.resolve().parent != root_real:
                return None
            collected.append((rel, path.read_bytes()))
        except OSError:
            return None
    return collected


def _tool_content_digest(sources: list | None) -> str | None:
    """sha256 over an ordered, length-delimited (path, bytes) stream.

    Length delimiting is what makes the stream unambiguous: without it a
    rename plus a matching edit could produce one concatenation two ways.
    """
    if sources is None:
        return None
    digest = hashlib.sha256()
    digest.update(TOOL_SOURCE_DIGEST_TAG)
    for rel, data in sources:
        raw = rel.encode("utf-8")
        digest.update(b"%d\n" % len(raw))
        digest.update(raw)
        digest.update(b"%d\n" % len(data))
        digest.update(data)
    return "sha256:" + digest.hexdigest()


def _tool_source_state(root: Path, sources: list | None) -> tuple:
    """(state, commit). exact only when every declared runtime file is
    byte-identical to `HEAD:./<path>`.

    Two failures are kept apart. `unresolved` means the comparison could not
    be established: no git, an unresolvable HEAD, a HEAD that is not a commit
    id, or a `git show` that failed. `dirty` means it was established and the
    worktree differs. Neither yields a commit, because a commit id next to
    bytes it does not name is the defect this function exists to remove.

    `HEAD:./<path>` is resolved relative to the tool root, so a copy of this
    tool sitting inside an unrelated repository compares against nothing and
    reports unresolved rather than borrowing that repository's HEAD.

    A checkout filter that rewrites line endings on the way to the worktree
    makes those bytes genuinely differ from the committed bytes; this reports
    dirty there. That direction is the safe one: never a false exact.
    """
    try:
        head = _git_bytes(root, "rev-parse", "HEAD")
    except (OSError, subprocess.TimeoutExpired):
        return "unresolved", None
    if head.returncode != 0:
        return "unresolved", None
    commit = head.stdout.decode("utf-8", "replace").strip()
    if _COMMIT_RE.match(commit) is None:
        return "unresolved", None
    committed = {}
    for rel in TOOL_SOURCE_PATHS:
        try:
            shown = _git_bytes(root, "show", "HEAD:./%s" % rel)
        except (OSError, subprocess.TimeoutExpired):
            return "unresolved", None
        if shown.returncode != 0:
            return "unresolved", None
        committed[rel] = shown.stdout
    if sources is None:
        return "dirty", None
    for rel, data in sources:
        if data != committed[rel]:
            return "dirty", None
    return "exact", commit


def tool_identity(root: Path | None = None) -> dict:
    """One producer. What a pinned measurement may claim about its own bytes.

    CI pins this tool by commit SHA. That is exact and opaque; the version
    constant is the name a report can quote. `tool_commit` stays semantically
    a commit: it is the 40-hex HEAD only when every declared runtime source is
    byte-identical to that commit, and null otherwise. `tool_source_state`
    says which case it was, and `tool_content_sha256` keeps the executing
    bytes addressable even when no commit may be named. A checkout without
    `.git` still carries the version and the digest.

    Reading the sources and comparing them are not one instant, so the
    snapshot is re-read once the comparison is done and any observed change
    fails closed. That is a narrowed window, not an atomic snapshot: a change
    made and reverted entirely between the two reads is not detectable, and in
    that case the bytes on disk did equal HEAD at both observations.

    Non-claims: this is not an attestation, signature, or SBOM; it does not
    prove the recorded bytes are the code objects already loaded in
    sys.modules; and it does not make the checkout or its environment
    reproducible.
    """
    root = Path(__file__).resolve().parent if root is None else Path(root)
    sources = _runtime_source_bytes(root)
    state, commit = _tool_source_state(root, sources)
    if state == "exact" and _runtime_source_bytes(root) != sources:
        # The snapshot was read before the comparison ran, so the bytes could
        # move underneath it. Re-reading proves the snapshot still describes
        # the disk. It differs here, and the snapshot equalled HEAD, so the
        # bytes now on disk provably do not: dirty is measured, not hedged.
        # No single byte-state can be addressed either, so the digest is
        # dropped rather than naming bytes that have already been replaced.
        state, commit, sources = "dirty", None, None
    return {
        "tool_version": VERSION,
        "tool_commit": commit,
        "tool_source_state": state,
        "tool_content_sha256": _tool_content_digest(sources),
    }


def _with_tool_identity(report: dict) -> dict:
    report.update(tool_identity())
    return report


# One sentence for every runner. The module report carried an older, shorter
# version that predated the silent class, so a module consumer read a different
# description of the same number.
SCORE_MEANS = ("percent of author-declared in-scope rules killed; NOT percent of "
               "the rules the implementation actually has. Silent mutants count "
               "in the denominator and never the numerator. Without "
               "diagnostic_from the silent class is unreachable, so a zero there "
               "means it was not measured, not that none exist")



SURVIVED_OBLIGATION = (
    "After a valid run, the declared mutation did not change the declared "
    "outcome on the pinned inputs. A contract-hole reading also needs a "
    "faithful owner-pinned declaration and rule ownership.")
SILENT_OBLIGATION = (
    "After a valid run, the declared mutation moved only the diagnostic "
    "channel. Silent is diagnostic-only and never a numerator. A "
    "contract-hole reading also needs a faithful owner-pinned declaration "
    "and rule ownership.")



def _refuse_nonregular(path: Path) -> ManifestError:
    return ManifestError(
        "input %s is not a regular file; refusing to follow a symlink or "
        "open a non-file" % path)


def read_bounded_regular_file(path: Path, *, cap: int | None = None) -> bytes:
    """Read one regular file without following a symlink.

    Size is refused from fstat before any parse, using OUTPUT_CAP_BYTES
    unless a caller passes cap. Bytes are gathered in a loop; one os.read
    is not a complete-read guarantee. The caller may json.loads the
    returned bytes; this function never does.

    When O_NOFOLLOW is missing, lstat/open/fstat identity parity is the
    fallback: a symlink or non-regular path is refused, and a file whose
    (st_dev, st_ino) changed between lstat and fstat is refused.
    """
    if cap is None:
        cap = OUTPUT_CAP_BYTES
    path = Path(path)
    nofollow = getattr(os, "O_NOFOLLOW", None)
    flags = os.O_RDONLY
    if hasattr(os, "O_BINARY"):
        flags |= os.O_BINARY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NONBLOCK"):
        # A FIFO is openable and parks open() until a writer arrives, so the
        # S_ISREG check below would never run. Opening non-blocking makes the
        # refusal immediate; on a regular file the flag has no effect.
        flags |= os.O_NONBLOCK
    identity = None
    if nofollow is not None:
        flags |= nofollow
    else:
        try:
            before = os.lstat(path)
        except OSError:
            raise _refuse_nonregular(path) from None
        if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
            raise _refuse_nonregular(path)
        identity = (before.st_dev, before.st_ino)
    try:
        fd = os.open(path, flags)
    except OSError:
        raise _refuse_nonregular(path) from None
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            raise _refuse_nonregular(path)
        if identity is not None and (st.st_dev, st.st_ino) != identity:
            raise ManifestError(
                "input %s changed between lstat and open; refusing" % path)
        if st.st_size > cap:
            raise ManifestError(
                "input %s exceeds the input cap of %d bytes" % (path, cap))
        data = bytearray()
        while len(data) <= cap:
            chunk = os.read(fd, min(65536, cap + 1 - len(data)))
            if not chunk:
                break
            data.extend(chunk)
        if len(data) > cap:
            raise ManifestError(
                "input %s exceeds the input cap of %d bytes" % (path, cap))
        return bytes(data)
    finally:
        os.close(fd)


def _file_sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def _control_stripped_one_line(text: str) -> str:
    return "".join(ch for ch in text if ord(ch) >= 32 and ch != "\x7f")




def _parse_projection_json(raw: bytes):
    """One strict parser for projection bytes, sharing the JSON decoder."""
    return load_json_document(raw, where="projection")


def _require_anchor_manifest(manifest_obj) -> dict:
    """Typed mutants map before any .get on a group or entry."""
    if not isinstance(manifest_obj, dict):
        raise ManifestError("manifest must be an object")
    mutants = manifest_obj.get("mutants")
    if mutants is None:
        return manifest_obj
    if not isinstance(mutants, dict):
        raise ManifestError("manifest.mutants must be an object")
    for group, entries in mutants.items():
        if not isinstance(entries, list):
            raise ManifestError("manifest.mutants[%r] must be a list" % group)
        for i, entry in enumerate(entries):
            if not isinstance(entry, dict):
                raise ManifestError(
                    "manifest.mutants[%r][%d] must be an object" % (group, i))
    return manifest_obj


def _lookup_manifest_anchor(manifest_obj: dict, group: str, label: str):
    entries = (manifest_obj.get("mutants") or {}).get(group) or []
    for entry in entries:
        if entry.get("label") == label:
            return entry.get("anchor")
    return None


def _apply_anchor(finding: dict, manifest_obj: dict) -> None:
    raw_anchor = _lookup_manifest_anchor(manifest_obj, finding["group"], finding["rule"])
    if not isinstance(raw_anchor, str):
        return
    if len(raw_anchor) > ANCHOR_EXCERPT_MAX:
        finding["anchor_omitted"] = "oversized"
        return
    excerpt = _control_stripped_one_line(raw_anchor)
    if excerpt:
        finding["anchor_excerpt"] = excerpt
    # Intentional omission: a within-cap anchor that is empty after control
    # stripping gets neither excerpt nor omitted reason. That is not a
    # missing field and not a new survivors key.


REPORT_MISSING_KEY = "report missing key"
REPORT_EXTRA_KEY = "report extra key"
MUTANT_MISSING_KEY = "mutant missing key"
MUTANT_EXTRA_KEY = "mutant extra key"


def _runner_specific_report_key(runner):
    """The one process/batch-only report.v0 field. Derived once."""
    if runner in ("process", "batch"):
        return "originals_unverified_against_head"
    return None


def _scored_denominator(killed: int, survived: int, silent: int) -> int:
    """The one denominator a score is a fraction of: killed, survived and silent.

    `silent` sits beside `survived` and never in the numerator. Equivalent,
    out-of-scope, unproved, acknowledged holes and controls stay outside it.
    Every place that divides by the denominator or prints it calls this, so a
    printed fraction cannot disagree with the percentage printed beside it.
    """
    return killed + survived + silent


def _report_v0_document(manifest_path: Path, m: dict, *,
                        killed: int, survived: int, silent: int, equivalent: int,
                        out_of_scope: int, unproved: int, known_holes: int,
                        score, results: list, failures: list,
                        control_status: str = "absent-or-invalid",
                        originals_unverified_against_head=None) -> dict:
    """The one report.v0 document. Consumer keys are derived from this function.

    Process and batch always emit `originals_unverified_against_head` (`[]`
    when the caller supplies nothing). Module never emits it.
    Identity keys are present so `_with_tool_identity` only overwrites values.
    """
    denom = _scored_denominator(killed, survived, silent)
    report = {
        "schema": REPORT_SCHEMA,
        "manifest": str(manifest_path),
        "manifest_sha256": m.get("_manifest_sha256"),
        "runner": m["runner"],
        "control_status": control_status,
        "killed": killed,
        "survived": survived,
        "silent": silent,
        "diagnostic_channel_declared": m.get("diagnostic_from") is not None,
        "known_holes": known_holes,
        "corpus_digest": m.get("_corpus_digest"),
        "acknowledged_digests": len(m.get("known_holes", {})),
        "hole_ratio": None if denom == 0 else round(known_holes / denom, 2),
        "equivalent": equivalent,
        "unexercised_out_of_scope": out_of_scope,
        "unproved": unproved,
        "declared_total": (killed + survived + silent + equivalent + out_of_scope
                           + unproved + known_holes),
        "out_of_scope_ratio": None if denom == 0 else round(out_of_scope / denom, 2),
        "score_percent": score,
        "score_means": SCORE_MEANS,
        "mutants": results,
        "failures": failures,
        "adequate": not failures,
        "tool_version": None,
        "tool_commit": None,
        "tool_source_state": None,
        "tool_content_sha256": None,
    }
    extra = _runner_specific_report_key(m["runner"])
    if extra is not None:
        report[extra] = (
            [] if originals_unverified_against_head is None
            else originals_unverified_against_head)
    return report


def _report_v0_keys(runner):
    """Closed top-level set from one produced document, not a second key table."""
    m = {"runner": runner, "known_holes": {}, "_corpus_digest": None,
         "_manifest_sha256": None}
    return frozenset(_report_v0_document(
        Path("m.json"), m,
        killed=0, survived=0, silent=0, equivalent=0, out_of_scope=0,
        unproved=0, known_holes=0, score=None, results=[], failures=[]))


def _report_v0_required_keys(runner):
    return _report_v0_keys(runner)


def _report_v0_allowed_keys(runner):
    return _report_v0_keys(runner)


_MUTANT_ROW_BASE_KEYS = frozenset({"group", "label", "verdict", "moved", "how"})
_DIAGNOSTIC_OPTIONAL_VERDICTS = frozenset({"unexercised", "known-hole"})


def _mutant_row_optional_keys(verdict):
    """Fields the producer writes only for some verdicts."""
    optional = set()
    if verdict == "killed":
        optional.add("raised")
    if verdict in _DIAGNOSTIC_OPTIONAL_VERDICTS:
        optional.add("moved_diagnostic")
    return frozenset(optional)


def _mutant_row_required_keys(verdict):
    """Producer-required row keys for one verdict."""
    required = set(_MUTANT_ROW_BASE_KEYS)
    if verdict != "equivalent":
        required.add("scope")
    if verdict == "silent":
        required.add("moved_diagnostic")
    return frozenset(required)


def _mutant_row_allowed_keys(verdict):
    """Closed mutant-row set: required plus verdict-optional producer fields."""
    return _mutant_row_required_keys(verdict) | _mutant_row_optional_keys(verdict)


def _require_closed_keys(obj, required, allowed, *, missing_token, extra_token):
    """One missing-vs-extra comparison. Distinct tokens; no shared substring."""
    present = set(obj)
    missing = required - present
    extra = present - allowed
    if missing:
        raise ManifestError("%s %s" % (missing_token, sorted(missing)[0]))
    if extra:
        raise ManifestError("%s %s" % (extra_token, sorted(extra)[0]))


def _require_canonical_sha256(value, where: str) -> None:
    if not isinstance(value, str) or _MANIFEST_DIGEST_RE.fullmatch(value) is None:
        raise ManifestError("%s is not a canonical sha256 digest" % where)


def _require_sha256_or_null(value, where: str) -> None:
    if value is None:
        return
    _require_canonical_sha256(value, where)


def _require_corpus_digest(value, where: str) -> None:
    if value is not None and (not isinstance(value, str) or not value):
        raise ManifestError("%s must be null or a non-empty string" % where)


def _require_tool_identity_forms(fields, where: str) -> None:
    version = fields.get("tool_version")
    if not isinstance(version, str) or not version:
        raise ManifestError("%s.tool_version must be a non-empty string" % where)
    state = fields.get("tool_source_state")
    if state not in TOOL_SOURCE_STATES:
        raise ManifestError("%s.tool_source_state is not a producer tool state" % where)
    commit = fields.get("tool_commit")
    if commit is not None and (
            not isinstance(commit, str) or _COMMIT_RE.fullmatch(commit) is None):
        raise ManifestError("%s.tool_commit is not a 40-hex commit id" % where)
    _require_sha256_or_null(fields.get("tool_content_sha256"), "%s.tool_content_sha256" % where)
    content = fields.get("tool_content_sha256")
    if state == "exact":
        if commit is None or content is None:
            raise ManifestError("%s tool identity is not producer-consistent" % where)
        return
    if commit is not None:
        raise ManifestError("%s tool identity is not producer-consistent" % where)


def _require_report_rows(report) -> list:
    """Refuse hostile report shapes before they become KeyError or []."""
    if not isinstance(report, dict) or report.get("schema") != REPORT_SCHEMA:
        raise ManifestError(
            "report input must be %s, got %r"
            % (REPORT_SCHEMA, report.get("schema") if isinstance(report, dict) else type(report).__name__))
    runner = report.get("runner") if isinstance(report.get("runner"), str) else ""
    _require_closed_keys(
        report, _report_v0_required_keys(runner), _report_v0_allowed_keys(runner),
        missing_token=REPORT_MISSING_KEY, extra_token=REPORT_EXTRA_KEY)
    for key in _REPORT_BOOL_KEYS:
        if type(report[key]) is not bool:
            raise ManifestError("report.%s must be a bool" % key)
    for key in _REPORT_INT_KEYS:
        if type(report[key]) is not int or report[key] < 0:
            raise ManifestError("report.%s must be a non-negative int" % key)
    if not isinstance(report.get("control_status"), str) or not report["control_status"]:
        raise ManifestError("report.control_status must be a non-empty string")
    _require_canonical_sha256(report.get("manifest_sha256"), "report.manifest_sha256")
    _require_corpus_digest(report.get("corpus_digest"), "report.corpus_digest")
    _require_tool_identity_forms(report, "report")
    mutants = report.get("mutants")
    if not isinstance(mutants, list):
        raise ManifestError("report.mutants must be a list")
    seen_labels = set()
    for i, row in enumerate(mutants):
        if not isinstance(row, dict):
            raise ManifestError("report.mutants[%d] must be an object" % i)
        verdict = row.get("verdict") if isinstance(row.get("verdict"), str) else ""
        _require_closed_keys(
            row, _mutant_row_required_keys(verdict), _mutant_row_allowed_keys(verdict),
            missing_token=MUTANT_MISSING_KEY, extra_token=MUTANT_EXTRA_KEY)
        for key in ("group", "label", "verdict"):
            val = row.get(key)
            if not isinstance(val, str) or not val:
                raise ManifestError(
                    "report.mutants[%d].%s must be a non-empty string" % (i, key))
        if row["verdict"] not in REPORT_VERDICTS:
            raise ManifestError(
                "report.mutants[%d].verdict is not a producer verdict" % i)
        for key in ("moved", "moved_diagnostic"):
            if key in row and (type(row[key]) is not int or row[key] < 0):
                raise ManifestError(
                    "report.mutants[%d].%s must be a non-negative int" % (i, key))
        label = row["label"]
        if label in seen_labels:
            raise ManifestError("duplicate mutant label %r" % label)
        seen_labels.add(label)
    return mutants


def survivor_findings(report, manifest=None):
    """Project survived and silent rows. Pure; does not measure or mutate.

    `rule` is the mutant label. Counts come from the findings, not from
    producer summary fields. Producer failures stay out of the projection.
    A Path `manifest` is read through `read_bounded_regular_file`; its exact
    on-disk bytes must match `report.manifest_sha256` before an anchor is
    emitted. A reserialized object digest is never used.
    """
    mutants = _require_report_rows(report)
    findings = []
    for row in mutants:
        verdict = row.get("verdict")
        if verdict == "survived":
            obligation = SURVIVED_OBLIGATION
        elif verdict == "silent":
            obligation = SILENT_OBLIGATION
        else:
            continue
        findings.append({
            "rule": row["label"],
            "group": row["group"],
            "verdict": verdict,
            "moved": row.get("moved", 0),
            "moved_diagnostic": row.get("moved_diagnostic", 0),
            "obligation": obligation,
        })
    findings.sort(key=lambda f: (f["group"], f["rule"]))
    if manifest is not None:
        raw = read_bounded_regular_file(Path(manifest))
        if _file_sha256(raw) == report.get("manifest_sha256"):
            manifest_obj = _require_anchor_manifest(_parse_projection_json(raw))
            for finding in findings:
                _apply_anchor(finding, manifest_obj)
    return {
        "schema": SURVIVORS_SCHEMA,
        "source_schema": REPORT_SCHEMA,
        "manifest_sha256": report.get("manifest_sha256"),
        "survived": sum(f["verdict"] == "survived" for f in findings),
        "silent": sum(f["verdict"] == "silent" for f in findings),
        "finding_count": len(findings),
        "findings": findings,
    }


def classify_kill(row, runner):
    """Say what a killed row's own record shows about how it was killed.

    Returns ``(basis, classes)``. The unit is one mutant row. ``moved-outcome``:
    at least one declared outcome moved, whatever else happened; any termination
    or raise recorded beside it is kept in ``classes``. ``without-moved-outcome``:
    no outcome moved and the row records why the mutant was still distinguished,
    as termination classes (process, batch, module) or ``raises`` (module
    entrypoint exceptions, from the structured ``raised`` list). ``unclassified``:
    no outcome moved and the row does not record a supported reason; nothing is
    inferred from free text. The kill rule itself is not consulted or changed.
    """
    if row.get("verdict") != "killed":
        raise ValueError("classify_kill takes killed rows only, got %r" % row.get("verdict"))
    moved = row.get("moved")
    if type(moved) is not int or moved < 0:
        return ("unclassified", ())
    how = row.get("how")
    classes = None
    if runner == "module":
        if row.get("raised"):
            classes = ("raises",)
        elif how in TERMINATED_KINDS:
            classes = (how,)
    elif runner in ("process", "batch") and isinstance(how, str):
        # The only record of the termination classes on these runners is `how`,
        # which the producer writes as the sorted, comma-joined class names.
        parts = how.split(", ")
        if parts and all(part in TERMINATED_KINDS for part in parts):
            classes = tuple(sorted(set(parts)))
    if moved > 0:
        return ("moved-outcome", classes or ())
    if classes:
        return ("without-moved-outcome", classes)
    return ("unclassified", ())


def _kill_basis_counts(rows, runner) -> dict:
    """The one tally of classify_kill that the text summary and kills.v0 share."""
    kills, groups = [], {}
    without = unclassified = also = 0
    for row in rows:
        if row.get("verdict") != "killed":
            continue
        basis, classes = classify_kill(row, runner)
        kills.append({"rule": row["label"], "group": row["group"], "basis": basis,
                      "classes": list(classes), "moved": row.get("moved")})
        if basis == "without-moved-outcome":
            without += 1
            groups[classes] = groups.get(classes, 0) + 1
        elif basis == "unclassified":
            unclassified += 1
        elif classes:
            also += 1
    kills.sort(key=lambda k: (k["group"], k["rule"]))
    return {
        "killed": len(kills),
        "without_moved_outcome": without,
        # Disjoint: each kill without a moved outcome sits in exactly one exact class set.
        "without_moved_outcome_by_classes": [
            {"classes": list(classes), "kills": count} for classes, count in sorted(groups.items())],
        "moved_outcome_also_terminated": also,
        "unclassified": unclassified,
        "kills": kills,
    }


def kill_evidence(report, *, report_sha256) -> dict:
    """Project kills.v0 from an existing report. Pure; does not measure or rescore.

    Counts come from the rows through classify_kill, not from producer summary
    fields. `report_sha256` names the exact report bytes this describes.
    """
    rows = _require_report_rows(report)
    _require_canonical_sha256(report_sha256, "report_sha256")
    return {
        "schema": KILLS_SCHEMA,
        "source_schema": REPORT_SCHEMA,
        "report_sha256": report_sha256,
        "manifest_sha256": report.get("manifest_sha256"),
        "runner": report.get("runner"),
        **_kill_basis_counts(rows, report.get("runner")),
    }


def _kill_basis_text(counts) -> str:
    """Suffix for the score's parentheses; empty when every kill moved an outcome."""
    parts = []
    if counts["without_moved_outcome"]:
        parts.append("%d without a moved outcome: %s" % (
            counts["without_moved_outcome"],
            ", ".join("%d %s" % (g["kills"], "+".join(g["classes"]))
                      for g in counts["without_moved_outcome_by_classes"])))
    if counts["unclassified"]:
        parts.append("%d unclassified" % counts["unclassified"])
    return "".join("; " + part for part in parts)


def _kill_also_terminated_line(counts):
    """A moved-outcome kill keeps its termination or raise visible, outside the subset."""
    also = [k for k in counts["kills"] if k["basis"] == "moved-outcome" and k["classes"]]
    if not also:
        return None
    by_class = {}
    for kill in also:
        label = "+".join(kill["classes"])
        by_class[label] = by_class.get(label, 0) + 1
    return "%d kill%s with a moved outcome also terminated or raised: %s." % (
        len(also), "" if len(also) == 1 else "s",
        ", ".join("%d %s" % (n, label) for label, n in sorted(by_class.items())))


def rule_inventory_projection(report, manifest) -> dict:
    """Project rules.v0 from one exact-byte-matched manifest; never executes a run."""
    _require_report_rows(report)
    digest = report.get("manifest_sha256")
    if not isinstance(digest, str) or _MANIFEST_DIGEST_RE.fullmatch(digest) is None:
        raise ManifestError("report.manifest_sha256 is not a canonical sha256 digest")
    if isinstance(manifest, (str, Path)):
        raw = read_bounded_regular_file(Path(manifest))
    elif type(manifest) is bytes:
        raw = manifest
    else:
        raise ManifestError("rules manifest input must be bytes or a path")
    if _file_sha256(raw) != digest:
        raise ManifestError("report/manifest exact-byte digest mismatch")
    manifest_obj = _parse_projection_json(raw)
    require_shape(manifest_obj, dict, "manifest")
    inventory = rule_inventory_index(manifest_obj)
    return {
        "schema": RULES_SCHEMA,
        "source_schema": REPORT_SCHEMA,
        "manifest_schema": manifest_obj.get("schema"),
        "manifest_sha256": digest,
        "inventory": inventory,
    }


def encode_report_v0(report: dict) -> bytes:
    """Return the sole byte representation of a successful report.

    UTF-8, sorted keys, two-space indentation and one trailing LF are part of
    the v0 byte contract. Error envelopes have their own schema and must never
    be addressable as successful report bytes through this function.
    """
    if report.get("schema") != REPORT_SCHEMA:
        raise ValueError("encode_report_v0 accepts only %s" % REPORT_SCHEMA)
    try:
        return (json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "report contains text that cannot be encoded as valid UTF-8") from None



def encode_survivors_v0(doc: dict) -> bytes:
    """Sole byte form of a survivors.v0 projection. Never calls encode_report_v0."""
    if doc.get("schema") != SURVIVORS_SCHEMA:
        raise ValueError("encode_survivors_v0 accepts only %s" % SURVIVORS_SCHEMA)
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "survivors projection contains text that cannot be encoded as valid UTF-8") from None


def encode_kills_v0(doc: dict) -> bytes:
    """Sole byte form of a kills.v0 projection. Never calls encode_report_v0."""
    if doc.get("schema") != KILLS_SCHEMA:
        raise ValueError("encode_kills_v0 accepts only %s" % KILLS_SCHEMA)
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "kills projection contains text that cannot be encoded as valid UTF-8") from None


def _require_rules_v0_document(doc: dict) -> None:
    require_shape(doc, dict, "rules projection")
    top_keys = frozenset({
        "schema", "source_schema", "manifest_schema", "manifest_sha256", "inventory"})
    _require_closed_keys(
        doc, top_keys, top_keys,
        missing_token="rules projection missing key", extra_token="rules projection extra key")
    if doc.get("schema") != RULES_SCHEMA or doc.get("source_schema") != REPORT_SCHEMA:
        raise ManifestError("rules projection has an invalid schema")
    inventory = doc["inventory"]
    if inventory is None:
        if doc.get("manifest_schema") != SCHEMA:
            raise ManifestError("only manifest.v0 may project an absent inventory")
        return
    require_shape(inventory, dict, "inventory")
    inv_keys = frozenset({
        "rules_declared", "rules_mutation_linked", "rules_excluded",
        "linked_mutants", "groups", "rules"})
    _require_closed_keys(
        inventory, inv_keys, inv_keys,
        missing_token="inventory missing key", extra_token="inventory extra key")
    if doc.get("manifest_schema") != MANIFEST_V1_SCHEMA:
        raise ManifestError("only manifest.v1 may project a rule inventory")
    count_keys = (
        "rules_declared", "rules_mutation_linked", "rules_excluded", "linked_mutants")
    for key in count_keys:
        if type(inventory[key]) is not int or inventory[key] < 0:
            raise ManifestError("inventory.%s must be a non-negative int" % key)
    require_shape(inventory["groups"], list, "inventory.groups")
    require_shape(inventory["rules"], list, "inventory.rules")
    group_keys = frozenset({"group", *count_keys})
    group_order = []
    for i, group in enumerate(inventory["groups"]):
        require_shape(group, dict, "inventory.groups[%d]" % i)
        _require_closed_keys(
            group, group_keys, group_keys,
            missing_token="group missing key", extra_token="group extra key")
        _utf8_len(group["group"], "inventory.groups[%d].group" % i,
                  maximum=128)
        group_order.append(group["group"])
        for key in count_keys:
            if type(group[key]) is not int or group[key] < 0:
                raise ManifestError("inventory.groups[%d].%s must be a non-negative int" % (i, key))
        if group["rules_declared"] != group["rules_mutation_linked"] + group["rules_excluded"]:
            raise ManifestError("inventory group rule-count invariant failed")
    if group_order != sorted(group_order) or len(group_order) != len(set(group_order)):
        raise ManifestError("inventory groups are not unique exact Unicode order")
    linked_from_rows = 0
    mutated_rows = 0
    excluded_rows = 0
    by_group = {}
    order = []
    for i, row in enumerate(inventory["rules"]):
        require_shape(row, dict, "inventory.rules[%d]" % i)
        disposition = row.get("disposition")
        base = {"group", "id", "text", "url", "disposition"}
        expected = frozenset(base | ({"mutants"} if disposition == "mutated" else {"reason"}))
        if disposition not in ("mutated", "excluded"):
            raise ManifestError("inventory.rules[%d] has invalid disposition" % i)
        _require_closed_keys(
            row, expected, expected,
            missing_token="rule missing key", extra_token="rule extra key")
        _utf8_len(row["group"], "inventory.rules[%d].group" % i, maximum=128)
        if not isinstance(row["id"], str) or _RULE_ID_RE.fullmatch(row["id"]) is None:
            raise ManifestError("inventory.rules[%d].id is invalid" % i)
        order.append((row["group"], row["id"]))
        counts = by_group.setdefault(row["group"], {
            "rules_declared": 0, "rules_mutation_linked": 0,
            "rules_excluded": 0, "linked_mutants": 0})
        counts["rules_declared"] += 1
        if disposition == "mutated":
            require_shape(row["mutants"], list, "inventory.rules[%d].mutants" % i)
            for j, label in enumerate(row["mutants"]):
                _utf8_len(label, "inventory.rules[%d].mutants[%d]" % (i, j),
                          maximum=None)
            if row["mutants"] != sorted(row["mutants"]):
                raise ManifestError("inventory rule mutants are not in exact label order")
            linked_from_rows += len(row["mutants"])
            mutated_rows += 1
            counts["rules_mutation_linked"] += 1
            counts["linked_mutants"] += len(row["mutants"])
        else:
            excluded_rows += 1
            counts["rules_excluded"] += 1
    if order != sorted(order) or len(order) != len(set(order)):
        raise ManifestError("inventory rules are not unique exact (group, id) order")
    if inventory["rules_declared"] != len(inventory["rules"]):
        raise ManifestError("inventory declared-rule invariant failed")
    if inventory["rules_mutation_linked"] != mutated_rows:
        raise ManifestError("inventory mutation-linked-rule invariant failed")
    if inventory["rules_excluded"] != excluded_rows:
        raise ManifestError("inventory excluded-rule invariant failed")
    if inventory["linked_mutants"] != linked_from_rows:
        raise ManifestError("inventory linked-mutant invariant failed")
    for key in count_keys:
        if inventory[key] != sum(group[key] for group in inventory["groups"]):
            raise ManifestError("inventory top-level %s invariant failed" % key)
    if set(by_group) != {group["group"] for group in inventory["groups"]
                         if group["rules_declared"]}:
        raise ManifestError("inventory group summaries do not match projected rules")
    for group in inventory["groups"]:
        if group["rules_declared"] and any(
                group[key] != by_group[group["group"]][key] for key in count_keys):
            raise ManifestError("inventory group %r count invariant failed" % group["group"])


def encode_rules_v0(doc: dict) -> bytes:
    """Sole closed UTF-8 byte form of a rules.v0 projection."""
    _require_rules_v0_document(doc)
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "rules projection contains text that cannot be encoded as valid UTF-8") from None


def _identity_component(old, new, status: str) -> dict:
    return {"old": old, "new": new, "status": status}


def _manifest_identity_status(old, new) -> str:
    return "same" if old == new else "changed"


def _corpus_identity_status(old, new) -> str:
    if old is None or new is None:
        return "undeclared"
    return "same" if old == new else "changed"


def _tool_component_status(old, new) -> str:
    if old is None or new is None:
        return "unresolved"
    return "same" if old == new else "changed"


def _row_changed_fields(old_row, new_row, presence: str) -> list:
    if presence != "common":
        present = new_row if presence == "added" else old_row
        return sorted(key for key in present if key != "label")
    keys = (set(old_row) | set(new_row)) - {"label"}
    changed = []
    for key in sorted(keys):
        old_present = key in old_row
        new_present = key in new_row
        if old_present != new_present:
            changed.append(key)
        elif old_row[key] != new_row[key]:
            changed.append(key)
    return changed


def _verdict_transition(old_row, new_row, presence: str) -> str:
    if presence != "common":
        return "unavailable"
    if old_row["verdict"] == new_row["verdict"]:
        return "same"
    return "changed"


def _acknowledgement_retired(old_row, new_row, presence: str, manifest_changed: bool) -> bool:
    return (
        presence == "common"
        and old_row["verdict"] == "known-hole"
        and new_row["verdict"] != "known-hole"
        and manifest_changed
    )


def _input_block(report: dict) -> dict:
    return {
        "adequate": report["adequate"],
        "control_status": report["control_status"],
        "unproved": report["unproved"],
    }


def diff_reports(old_report, new_report) -> dict:
    """Compare two validated report.v0 documents. Never executes a run."""
    old_rows = _require_report_rows(old_report)
    new_rows = _require_report_rows(new_report)
    old_by = {row["label"]: row for row in old_rows}
    new_by = {row["label"]: row for row in new_rows}
    added = set(new_by) - set(old_by)
    removed = set(old_by) - set(new_by)
    manifest_changed = old_report["manifest_sha256"] != new_report["manifest_sha256"]
    if (added or removed) and not manifest_changed:
        raise ManifestError("added or removed labels require a changed manifest")
    rows = []
    for label in sorted(set(old_by) | set(new_by)):
        if label in old_by and label in new_by:
            presence = "common"
            old_row, new_row = old_by[label], new_by[label]
        elif label in new_by:
            presence = "added"
            old_row, new_row = None, new_by[label]
        else:
            presence = "removed"
            old_row, new_row = old_by[label], None
        rows.append({
            "label": label,
            "presence": presence,
            "old": old_row,
            "new": new_row,
            "verdict_transition": _verdict_transition(old_row, new_row, presence),
            "changed_fields": _row_changed_fields(old_row, new_row, presence),
            "acknowledgement_retired": _acknowledgement_retired(
                old_row, new_row, presence, manifest_changed),
        })
    counts = {
        "common": sum(row["presence"] == "common" for row in rows),
        "added": sum(row["presence"] == "added" for row in rows),
        "removed": sum(row["presence"] == "removed" for row in rows),
        "verdict_changed": sum(
            row["presence"] == "common" and row["verdict_transition"] == "changed"
            for row in rows),
        "verdict_same": sum(
            row["presence"] == "common" and row["verdict_transition"] == "same"
            for row in rows),
        "acknowledgement_retired": sum(row["acknowledgement_retired"] for row in rows),
    }
    tool_old, tool_new = {}, {}
    tool_status = {}
    for key in ("tool_version", "tool_commit", "tool_source_state", "tool_content_sha256"):
        tool_old[key] = old_report[key]
        tool_new[key] = new_report[key]
        tool_status[key] = _tool_component_status(old_report[key], new_report[key])
    return {
        "schema": DIFF_SCHEMA,
        "old_input": _input_block(old_report),
        "new_input": _input_block(new_report),
        "identity": {
            "manifest_sha256": _identity_component(
                old_report["manifest_sha256"], new_report["manifest_sha256"],
                _manifest_identity_status(
                    old_report["manifest_sha256"], new_report["manifest_sha256"])),
            "corpus_digest": _identity_component(
                old_report["corpus_digest"], new_report["corpus_digest"],
                _corpus_identity_status(
                    old_report["corpus_digest"], new_report["corpus_digest"])),
            "tool": {
                key: _identity_component(tool_old[key], tool_new[key], tool_status[key])
                for key in (
                    "tool_version", "tool_commit", "tool_source_state", "tool_content_sha256")
            },
        },
        "rows": rows,
        "counts": counts,
        "non_claims": list(DIFF_NON_CLAIMS),
    }


def _require_diff_input_block(block: dict, name: str) -> None:
    if type(block["adequate"]) is not bool:
        raise ManifestError("%s.adequate must be a bool" % name)
    if not isinstance(block["control_status"], str) or not block["control_status"]:
        raise ManifestError("%s.control_status must be a non-empty string" % name)
    if type(block["unproved"]) is not int:
        raise ManifestError("%s.unproved must be an int" % name)
    if block["unproved"] < 0:
        raise ManifestError("%s.unproved must be a non-negative int" % name)


def _require_diff_component_status(component: dict, status_fn, where: str) -> None:
    if component.get("status") != status_fn(component.get("old"), component.get("new")):
        raise ManifestError("%s has an invalid status" % where)


def _require_diff_identity_values(identity: dict) -> None:
    for side in ("old", "new"):
        _require_canonical_sha256(
            identity["manifest_sha256"][side], "identity.manifest_sha256." + side)
        _require_corpus_digest(
            identity["corpus_digest"][side], "identity.corpus_digest." + side)
        _require_tool_identity_forms(
            {key: identity["tool"][key][side] for key in _DIFF_TOOL_KEYS},
            "identity.tool." + side)


def _require_diff_mutant_row(row, where: str) -> None:
    require_shape(row, dict, where)
    verdict = row.get("verdict") if isinstance(row.get("verdict"), str) else ""
    _require_closed_keys(
        row, _mutant_row_required_keys(verdict), _mutant_row_allowed_keys(verdict),
        missing_token=MUTANT_MISSING_KEY, extra_token=MUTANT_EXTRA_KEY)
    for key in ("group", "label", "verdict"):
        val = row.get(key)
        if not isinstance(val, str) or not val:
            raise ManifestError("%s.%s must be a non-empty string" % (where, key))
    if row["verdict"] not in REPORT_VERDICTS:
        raise ManifestError("%s.verdict is not a producer verdict" % where)
    for key in ("moved", "moved_diagnostic"):
        if key in row and type(row[key]) is not int:
            raise ManifestError("%s.%s must be an int" % (where, key))


def _require_diff_v0_document(doc: dict) -> None:
    require_shape(doc, dict, "diff projection")
    _require_closed_keys(
        doc, _DIFF_TOP_KEYS, _DIFF_TOP_KEYS,
        missing_token="diff projection missing key", extra_token="diff projection extra key")
    if doc.get("schema") != DIFF_SCHEMA:
        raise ManifestError("diff projection has an invalid schema")
    for name in ("old_input", "new_input"):
        require_shape(doc[name], dict, name)
        _require_closed_keys(
            doc[name], _DIFF_INPUT_KEYS, _DIFF_INPUT_KEYS,
            missing_token=name + " missing key", extra_token=name + " extra key")
        _require_diff_input_block(doc[name], name)
    identity = doc["identity"]
    require_shape(identity, dict, "identity")
    _require_closed_keys(
        identity, _DIFF_IDENTITY_KEYS, _DIFF_IDENTITY_KEYS,
        missing_token="identity missing key", extra_token="identity extra key")
    for name, status_fn in (
            ("manifest_sha256", _manifest_identity_status),
            ("corpus_digest", _corpus_identity_status)):
        require_shape(identity[name], dict, "identity." + name)
        _require_closed_keys(
            identity[name], _DIFF_COMPONENT_KEYS, _DIFF_COMPONENT_KEYS,
            missing_token="identity component missing key",
            extra_token="identity component extra key")
        _require_diff_component_status(identity[name], status_fn, "identity." + name)
    require_shape(identity["tool"], dict, "identity.tool")
    _require_closed_keys(
        identity["tool"], _DIFF_TOOL_KEYS, _DIFF_TOOL_KEYS,
        missing_token="identity.tool missing key", extra_token="identity.tool extra key")
    for name in _DIFF_TOOL_KEYS:
        require_shape(identity["tool"][name], dict, "identity.tool." + name)
        _require_closed_keys(
            identity["tool"][name], _DIFF_COMPONENT_KEYS, _DIFF_COMPONENT_KEYS,
            missing_token="identity component missing key",
            extra_token="identity component extra key")
        _require_diff_component_status(
            identity["tool"][name], _tool_component_status, "identity.tool." + name)
    _require_diff_identity_values(identity)
    require_shape(doc["rows"], list, "rows")
    manifest_changed = identity["manifest_sha256"]["status"] == "changed"
    labels = []
    for i, row in enumerate(doc["rows"]):
        where = "rows[%d]" % i
        require_shape(row, dict, where)
        _require_closed_keys(
            row, _DIFF_ROW_KEYS, _DIFF_ROW_KEYS,
            missing_token="diff row missing key", extra_token="diff row extra key")
        label = row["label"]
        if not isinstance(label, str) or not label:
            raise ManifestError("%s.label must be a non-empty string" % where)
        presence = row["presence"]
        old_row, new_row = row["old"], row["new"]
        if presence == "common":
            if old_row is None or new_row is None:
                raise ManifestError("%s presence/value mismatch" % where)
        elif presence == "added":
            if old_row is not None or new_row is None:
                raise ManifestError("%s presence/value mismatch" % where)
        elif presence == "removed":
            if old_row is None or new_row is not None:
                raise ManifestError("%s presence/value mismatch" % where)
        else:
            raise ManifestError("%s has invalid presence" % where)
        if presence in ("added", "removed") and not manifest_changed:
            raise ManifestError("added or removed labels require a changed manifest")
        if old_row is not None:
            _require_diff_mutant_row(old_row, where + ".old")
            if old_row["label"] != label:
                raise ManifestError("%s.old label does not match" % where)
        if new_row is not None:
            _require_diff_mutant_row(new_row, where + ".new")
            if new_row["label"] != label:
                raise ManifestError("%s.new label does not match" % where)
        if row["verdict_transition"] != _verdict_transition(old_row, new_row, presence):
            raise ManifestError("%s has invalid verdict_transition" % where)
        require_shape(row["changed_fields"], list, where + ".changed_fields")
        if row["changed_fields"] != _row_changed_fields(old_row, new_row, presence):
            raise ManifestError("%s has invalid changed_fields" % where)
        if type(row["acknowledgement_retired"]) is not bool:
            raise ManifestError("%s.acknowledgement_retired must be a bool" % where)
        if row["acknowledgement_retired"] != _acknowledgement_retired(
                old_row, new_row, presence, manifest_changed):
            raise ManifestError("%s has invalid acknowledgement_retired" % where)
        labels.append(label)
    if labels != sorted(labels) or len(labels) != len(set(labels)):
        raise ManifestError("diff rows are not unique exact label order")
    require_shape(doc["counts"], dict, "counts")
    _require_closed_keys(
        doc["counts"], _DIFF_COUNT_KEYS, _DIFF_COUNT_KEYS,
        missing_token="counts missing key", extra_token="counts extra key")
    for key in _DIFF_COUNT_KEYS:
        value = doc["counts"][key]
        if type(value) is not int or value < 0:
            raise ManifestError("counts.%s must be a non-negative int" % key)
    expected_counts = {
        "common": sum(row["presence"] == "common" for row in doc["rows"]),
        "added": sum(row["presence"] == "added" for row in doc["rows"]),
        "removed": sum(row["presence"] == "removed" for row in doc["rows"]),
        "verdict_changed": sum(
            row["presence"] == "common" and row["verdict_transition"] == "changed"
            for row in doc["rows"]),
        "verdict_same": sum(
            row["presence"] == "common" and row["verdict_transition"] == "same"
            for row in doc["rows"]),
        "acknowledgement_retired": sum(row["acknowledgement_retired"] for row in doc["rows"]),
    }
    if doc["counts"] != expected_counts:
        raise ManifestError("diff counts invariant failed")
    if list(doc["non_claims"]) != list(DIFF_NON_CLAIMS):
        raise ManifestError("diff projection has invalid non_claims")


def encode_diff_v0(doc: dict) -> bytes:
    """Sole closed UTF-8 byte form of a diff.v0 projection. Never calls encode_report_v0."""
    _require_diff_v0_document(doc)
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "diff projection contains text that cannot be encoded as valid UTF-8") from None


def _read_class_input(path) -> bytes:
    return read_bounded_regular_file(Path(path), cap=CLASS_INPUT_CAP_BYTES)


def _require_digest_match(raw: bytes, expected, where: str) -> str:
    digest = _file_sha256(raw)
    if not isinstance(expected, str) or digest != expected:
        raise ManifestError("%s digest mismatch" % where)
    return digest


def _parse_class_object(raw: bytes):
    try:
        doc = _parse_projection_json(raw)
    except ManifestError:
        raise
    except (json.JSONDecodeError, ValueError) as exc:
        raise ManifestError("class artifact is not valid JSON: %s" % exc) from None
    if not isinstance(doc, dict):
        raise ManifestError(
            "class artifact must be a JSON object, got %s" % type(doc).__name__)
    return doc


def _encode_class_artifact_v0(doc: dict) -> bytes:
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "class artifact contains text that cannot be encoded as valid UTF-8") from None


def _require_canonical_class_bytes(raw: bytes, encoded: bytes) -> None:
    if raw != encoded:
        raise ManifestError("class artifact input is not the canonical codec byte form")


def _require_exact_bool(value, where: str) -> bool:
    if type(value) is not bool:
        raise ManifestError("%s must be a JSON boolean" % where)
    return value


def _require_exact_nonneg_int(value, where: str) -> int:
    if type(value) is not int or value < 0:
        raise ManifestError("%s must be a non-negative integer" % where)
    return value


def _require_class_id_token(value, where: str) -> str:
    if not isinstance(value, str) or _RULE_ID_RE.fullmatch(value) is None:
        raise ManifestError("%s does not match the closed id syntax" % where)
    return value


def _require_repository_identity(value, where: str) -> str:
    _utf8_len(value, where, maximum=256, nonblank=True)
    if _CLASS_REPOSITORY_RE.fullmatch(value) is None:
        raise ManifestError("%s must be a syntactic owner/name repository identity" % where)
    return value


def _require_commit_id(value, where: str) -> str:
    if not isinstance(value, str) or _COMMIT_RE.fullmatch(value) is None:
        raise ManifestError("%s is not a 40-hex commit id" % where)
    return value


def _require_source_identity(obj, where: str) -> None:
    require_shape(obj, dict, where)
    _require_closed_keys(
        obj, _CLASS_SOURCE_KEYS, _CLASS_SOURCE_KEYS,
        missing_token="%s missing key" % where, extra_token="%s extra key" % where)
    _require_repository_identity(obj["repository"], where + ".repository")
    _require_commit_id(obj["commit"], where + ".commit")
    _require_canonical_sha256(obj["tree_sha256"], where + ".tree_sha256")


def _require_class_non_claims(value, where: str) -> None:
    if not isinstance(value, list) or list(value) != list(CLASS_NON_CLAIMS):
        raise ManifestError("%s must be the fixed class non_claims list" % where)


def _require_score_percent_form(value, where: str):
    if value is None:
        return None
    if type(value) is bool or type(value) not in (int, float):
        raise ManifestError("%s must be null or a finite number in [0, 100]" % where)
    if value != value or value < 0 or value > 100:
        raise ManifestError("%s must be null or a finite number in [0, 100]" % where)
    return value


def _distinction_sort_key(row: dict):
    member = row.get("member")
    return (row.get("group") or "", row.get("label") or "",
            row.get("channel") or "", "" if member is None else member)


def _require_visibility_events(events) -> None:
    require_shape(events, list, "visibility_events")
    if not events or len(events) > CLASS_EVENTS_MAX:
        raise ManifestError(
            "visibility_events must contain 1-%d events" % CLASS_EVENTS_MAX)
    for i, event in enumerate(events):
        where = "visibility_events[%d]" % i
        require_shape(event, dict, where)
        _require_closed_keys(
            event, _CLASS_EVENT_KEYS, _CLASS_EVENT_KEYS,
            missing_token="%s missing key" % where, extra_token="%s extra key" % where)
        ordinal = event["ordinal"]
        if type(ordinal) is not int or ordinal != i:
            raise ManifestError(
                "%s.ordinal must be the contiguous integer %d" % (where, i))
        if event["event"] not in _CLASS_EVENT_NAMES:
            raise ManifestError("%s.event is not a closed visibility event" % where)
        _require_canonical_sha256(
            event["mutation_bundle_sha256"], where + ".mutation_bundle_sha256")
        _utf8_len(event["actor"], where + ".actor", maximum=CLASS_AUTHORING_ID_MAX)
        pred = event["predecessor_event_sha256"]
        if i == 0:
            if pred is not None:
                raise ManifestError(
                    "%s.predecessor_event_sha256 must be null at ordinal 0" % where)
        else:
            _require_canonical_sha256(pred, where + ".predecessor_event_sha256")


def _visibility_event_sha256(event: dict) -> str:
    return _file_sha256(_encode_class_artifact_v0(event))


_HELD_OUT_CHAIN_NAMES = (
    "selection-committed", "candidate-frozen", "selection-disclosed",
)
_PRE_FREEZE_CHAIN_NAMES = (
    "selection-committed", "selection-disclosed", "candidate-frozen",
)
_VISIBILITY_PATTERNS = ("held_out_chain", "pre_freeze", "commit_only")
_VISIBILITY_RELATIONSHIPS = ("same", "independent", "unknown")


def _require_visibility_chain_v0(provenance) -> list[str]:
    """Refuse a broken visibility chain; return the ordered event names.

    Encode, load, and classification consume this result. Predecessor,
    bundle, duplicate, and start-of-chain checks live only here.
    """
    events = provenance["visibility_events"]
    bundle = provenance["mutation_bundle_sha256"]
    names = []
    for i, event in enumerate(events):
        if event["mutation_bundle_sha256"] != bundle:
            raise ManifestError("visibility event mutation bundle drifted")
        if i > 0:
            expected = _visibility_event_sha256(events[i - 1])
            if event["predecessor_event_sha256"] != expected:
                raise ManifestError(
                    "visibility predecessor digest does not match the preceding event")
        name = event["event"]
        if name in names:
            raise ManifestError("duplicate visibility event")
        names.append(name)
    if not names or names[0] != "selection-committed":
        raise ManifestError(
            "visibility events are reordered: chain must start with selection-committed")
    return names


def _visibility_pattern(names: list[str]) -> str:
    if names == list(_HELD_OUT_CHAIN_NAMES):
        return "held_out_chain"
    if names == list(_PRE_FREEZE_CHAIN_NAMES):
        return "pre_freeze"
    if names == ["selection-committed"]:
        return "commit_only"
    if "candidate-frozen" not in names:
        raise ManifestError("visibility chain is missing candidate-frozen")
    raise ManifestError("visibility events are missing or reordered")


def _build_requested_class_transition_v0() -> dict:
    """Complete monotone table: preserve or weaken, never promote."""
    table = {}
    for requested in sorted(CLASS_IDS):
        for pattern in _VISIBILITY_PATTERNS:
            for relationship in _VISIBILITY_RELATIONSHIPS:
                key = (requested, pattern, relationship)
                if requested == "held_out":
                    if pattern == "held_out_chain":
                        table[key] = ("held_out", "hidden-until-freeze")
                    elif pattern == "pre_freeze":
                        table[key] = {
                            "same": ("declared", "disclosed-before-freeze"),
                            "independent": ("independent", "disclosed-before-freeze"),
                            "unknown": ("unknown", "disclosed-before-freeze"),
                        }[relationship]
                    else:
                        table[key] = "refuse-missing-freeze"
                    continue
                if relationship == "unknown":
                    table[key] = ("unknown", "unknown")
                else:
                    table[key] = (requested, "declared")
    return table


_REQUESTED_CLASS_TRANSITION_V0 = _build_requested_class_transition_v0()


def _classify_visibility_v0(provenance, names: list[str]) -> tuple[str, str]:
    """Derive effective class from an already-validated visibility chain.

    `names` is the ordered event-name list from the semantic chain
    validator. This function does not restate predecessor, bundle,
    duplicate, or start-of-chain checks. Establishes declared artifact
    order and relationship metadata only. It does not authenticate people
    or prove absence of undisclosed access. Held-out and pre-freeze
    mappings apply only to a requested held_out selection. Other requested
    classes preserve or weaken; they cannot promote to held_out or
    independent.
    """
    pattern = _visibility_pattern(names)
    requested = provenance["requested_class"]
    relationship = provenance["authoring"]["relationship"]
    outcome = _REQUESTED_CLASS_TRANSITION_V0[(requested, pattern, relationship)]
    if outcome == "refuse-missing-freeze":
        raise ManifestError("visibility chain is missing candidate-frozen")
    return outcome


def _require_origin(origin, requested: str) -> None:
    require_shape(origin, dict, "origin")
    kind = origin.get("kind")
    expected_kind = _CLASS_ORIGIN_BY_REQUEST.get(requested)
    if kind != expected_kind:
        raise ManifestError(
            "origin.kind must be %s for requested_class %s" % (expected_kind, requested))
    if kind == "authored":
        _require_closed_keys(
            origin, _CLASS_AUTHORED_ORIGIN_KEYS, _CLASS_AUTHORED_ORIGIN_KEYS,
            missing_token="origin missing key", extra_token="origin extra key")
        _require_source_identity(origin["source"], "origin.source")
        return
    if kind == "historical_fault":
        _require_closed_keys(
            origin, _CLASS_FAULT_ORIGIN_KEYS, _CLASS_FAULT_ORIGIN_KEYS,
            missing_token="origin missing key", extra_token="origin extra key")
        _require_repository_identity(origin["repository"], "origin.repository")
        _require_commit_id(origin["faulty_commit"], "origin.faulty_commit")
        _require_commit_id(origin["fixed_commit"], "origin.fixed_commit")
        if origin["faulty_commit"] == origin["fixed_commit"]:
            raise ManifestError("origin.faulty_commit must differ from origin.fixed_commit")
        _require_rule_url(origin["reference"], "origin.reference")
        return
    if kind == "adaptive":
        _require_closed_keys(
            origin, _CLASS_ADAPTIVE_ORIGIN_KEYS, _CLASS_ADAPTIVE_ORIGIN_KEYS,
            missing_token="origin missing key", extra_token="origin extra key")
        _require_source_identity(origin["source"], "origin.source")
        _require_canonical_sha256(
            origin["predecessor_attempt_sha256"], "origin.predecessor_attempt_sha256")
        return
    raise ManifestError("origin.kind is not a closed origin variant")


def _require_expected_distinctions_shape(rows) -> None:
    require_shape(rows, list, "expected_distinctions")
    if not rows or len(rows) > CLASS_EXPECTED_DISTINCTIONS_MAX:
        raise ManifestError(
            "expected_distinctions must contain 1-%d rows"
            % CLASS_EXPECTED_DISTINCTIONS_MAX)
    seen = []
    for i, row in enumerate(rows):
        where = "expected_distinctions[%d]" % i
        require_shape(row, dict, where)
        _require_closed_keys(
            row, _CLASS_DISTINCTION_KEYS, _CLASS_DISTINCTION_KEYS,
            missing_token="%s missing key" % where, extra_token="%s extra key" % where)
        _utf8_len(row["group"], where + ".group", maximum=128)
        _utf8_len(row["label"], where + ".label", maximum=None)
        if row["channel"] not in ("outcome", "diagnostic"):
            raise ManifestError("%s.channel must be outcome or diagnostic" % where)
        member = row["member"]
        if member is not None:
            _utf8_len(member, where + ".member", maximum=None)
        key = _distinction_sort_key(row)
        if key in seen:
            raise ManifestError("%s repeats the distinction tuple" % where)
        seen.append(key)
    expected = sorted(rows, key=_distinction_sort_key)
    if rows != expected:
        raise ManifestError("expected_distinctions are not in canonical Unicode order")


def _require_authoring(authoring, requested: str) -> None:
    require_shape(authoring, dict, "authoring")
    _require_closed_keys(
        authoring, _CLASS_AUTHORING_KEYS, _CLASS_AUTHORING_KEYS,
        missing_token="authoring missing key", extra_token="authoring extra key")
    _utf8_len(authoring["mutation_author"], "authoring.mutation_author",
              maximum=CLASS_AUTHORING_ID_MAX)
    _utf8_len(authoring["candidate_builder"], "authoring.candidate_builder",
              maximum=CLASS_AUTHORING_ID_MAX)
    if authoring["relationship"] not in _CLASS_RELATIONSHIPS:
        raise ManifestError("authoring.relationship is not a closed relationship")
    _require_exact_bool(authoring["candidate_outcomes_seen"],
                        "authoring.candidate_outcomes_seen")
    if authoring["relationship"] == "independent":
        if (authoring["mutation_author"] == authoring["candidate_builder"]
                or authoring["candidate_outcomes_seen"] is not False):
            raise ManifestError(
                "independent provenance cannot validate as independent")
        if requested == "independent":
            return
    if requested == "independent" and authoring["relationship"] != "independent":
        raise ManifestError(
            "requested_class independent requires relationship independent")


def _require_class_provenance_v0_document(doc) -> tuple[str, str]:
    require_shape(doc, dict, "class provenance")
    _require_closed_keys(
        doc, _CLASS_PROVENANCE_KEYS, _CLASS_PROVENANCE_KEYS,
        missing_token="class provenance missing key",
        extra_token="class provenance extra key")
    if doc.get("schema") != CLASS_PROVENANCE_SCHEMA:
        raise ManifestError("class provenance schema must be %s" % CLASS_PROVENANCE_SCHEMA)
    _require_class_id_token(doc["class_id"], "class_id")
    if doc["requested_class"] not in CLASS_IDS:
        raise ManifestError("requested_class is not a closed class id")
    _require_canonical_sha256(doc["manifest_sha256"], "manifest_sha256")
    _require_canonical_sha256(doc["mutation_bundle_sha256"], "mutation_bundle_sha256")
    freeze = doc["candidate_freeze"]
    require_shape(freeze, dict, "candidate_freeze")
    _require_closed_keys(
        freeze, _CLASS_FREEZE_KEYS, _CLASS_FREEZE_KEYS,
        missing_token="candidate_freeze missing key",
        extra_token="candidate_freeze extra key")
    _require_source_identity(freeze["candidate"], "candidate_freeze.candidate")
    _require_source_identity(freeze["corpus"], "candidate_freeze.corpus")
    _require_canonical_sha256(
        freeze["observation_declaration_sha256"],
        "candidate_freeze.observation_declaration_sha256")
    _require_authoring(doc["authoring"], doc["requested_class"])
    _require_visibility_events(doc["visibility_events"])
    names = _require_visibility_chain_v0(doc)
    classified = _classify_visibility_v0(doc, names)
    _require_origin(doc["origin"], doc["requested_class"])
    _require_expected_distinctions_shape(doc["expected_distinctions"])
    _require_class_non_claims(doc["non_claims"], "non_claims")
    return classified


def _ordinary_declared_mutant(manifest: dict, group: str, label: str) -> dict:
    found = None
    for declared_group, entries in (manifest.get("mutants") or {}).items():
        require_shape(entries, list, "mutants[%s]" % declared_group)
        for entry in entries:
            if entry.get("label") == label:
                if declared_group != group:
                    raise ManifestError(
                        "expected distinction label %r is not in group %r" % (label, group))
                found = entry
    if found is None:
        for declared_group, entries in (manifest.get("equivalent") or {}).items():
            for entry in entries:
                if entry.get("label") == label:
                    raise ManifestError(
                        "expected distinction names equivalent mutation %r" % label)
        raise ManifestError("expected distinction names unknown mutation %r" % label)
    if found.get("control") is True:
        raise ManifestError("expected distinction names a control %r" % label)
    if found.get("scope", "declared") != "declared":
        raise ManifestError(
            "expected distinction names an out-of-scope mutation %r" % label)
    return found


def _bind_expected_distinctions(doc: dict, manifest: dict) -> None:
    runner = manifest.get("runner", "module")
    for i, dist in enumerate(doc["expected_distinctions"]):
        where = "expected_distinctions[%d]" % i
        mutant = _ordinary_declared_mutant(manifest, dist["group"], dist["label"])
        channel = dist["channel"]
        member = dist["member"]
        if runner == "module":
            if channel != "outcome" or member is not None:
                raise ManifestError(
                    "%s: module provenance admits only channel=outcome with member=null"
                    % where)
            continue
        if manifest.get("outcome_parse") == "test-names":
            if channel == "diagnostic":
                raise ManifestError(
                    "%s: test-names provenance cannot use channel=diagnostic" % where)
            mover = mutant.get("expected_mover")
            if member is None:
                if mover is not None:
                    raise ManifestError(
                        "%s: a declared expected_mover requires that test member" % where)
                continue
            if not isinstance(mover, str) or member != mover:
                raise ManifestError(
                    "%s: test-names member must equal the existing expected_mover" % where)
            continue
        if channel == "outcome":
            if member is None:
                continue
            members = selector_members(manifest.get("outcome_from"))
            if member not in members:
                raise ManifestError(
                    "%s: outcome member %r is not declared in outcome_from"
                    % (where, member))
            continue
        if member is None:
            raise ManifestError("%s: diagnostic member is required" % where)
        diagnostic = manifest.get("diagnostic_from")
        if diagnostic is None:
            raise ManifestError("%s: diagnostic channel is not declared" % where)
        members = selector_members(diagnostic)
        if member not in members:
            raise ManifestError(
                "%s: diagnostic member %r is not declared in diagnostic_from"
                % (where, member))


def encode_class_provenance_v0(doc: dict) -> bytes:
    _require_class_provenance_v0_document(doc)
    return _encode_class_artifact_v0(doc)


def load_class_provenance_v0(provenance_path, *, manifest_path, mutation_bundle_path) -> dict:
    raw = _read_class_input(provenance_path)
    doc = _parse_class_object(raw)
    _require_class_provenance_v0_document(doc)
    _require_canonical_class_bytes(raw, _encode_class_artifact_v0(doc))
    manifest_raw = _read_class_input(manifest_path)
    _require_digest_match(manifest_raw, doc["manifest_sha256"], "manifest")
    _parse_class_object(manifest_raw)
    manifest = load_manifest_bytes(manifest_raw, Path(manifest_path))
    _bind_expected_distinctions(doc, manifest)
    bundle_raw = _read_class_input(mutation_bundle_path)
    _require_digest_match(bundle_raw, doc["mutation_bundle_sha256"], "mutation bundle")
    return doc


def _control_statuses_from_rows(rows: list) -> list[str]:
    statuses = []
    for row in rows:
        verdict = row.get("verdict")
        if verdict == "control-error":
            statuses.append("error")
        elif verdict == "control-SURVIVED":
            statuses.append("survived")
        elif verdict == "control-MOVED":
            statuses.append("moved")
        elif verdict in ("control-killed", "control-unchanged"):
            statuses.append("killed")
        elif isinstance(verdict, str) and verdict.startswith("control-"):
            raise ManifestError("unsupported control verdict %r" % verdict)
    return statuses


def _class_counts_from_rows(rows: list) -> dict:
    counts = {
        "killed": 0, "survived": 0, "silent": 0, "equivalent": 0,
        "unexercised_out_of_scope": 0, "unproved": 0, "known_holes": 0,
    }
    for row in rows:
        verdict = row["verdict"]
        if isinstance(verdict, str) and verdict.startswith("control-"):
            continue
        field = _ORDINARY_VERDICT_TO_COUNT.get(verdict)
        if field is None:
            raise ManifestError("mutant verdict %r is not an ordinary class count" % verdict)
        counts[field] += 1
    return counts


def _class_report_summary_parity(report: dict) -> None:
    rows = report["mutants"]
    if len(rows) > CLASS_ROWS_MAX:
        raise ManifestError("report.mutants exceeds %d rows" % CLASS_ROWS_MAX)
    counts = _class_counts_from_rows(rows)
    for field, value in counts.items():
        if report[field] != value:
            raise ManifestError("report.%s does not match mutant rows" % field)
    declared_total = sum(counts.values())
    if report["declared_total"] != declared_total:
        raise ManifestError("report.declared_total does not match mutant rows")
    if type(report["failures"]) is not list or any(
            not isinstance(item, str) for item in report["failures"]):
        raise ManifestError("report.failures must be a list of strings")
    denom = _scored_denominator(counts["killed"], counts["survived"], counts["silent"])
    expected_score = None if denom == 0 else round(100.0 * counts["killed"] / denom, 1)
    expected_score = _score_or_none(expected_score, rows, report["failures"])
    if report["score_percent"] != expected_score:
        raise ManifestError("report.score_percent does not match derived score")
    if report["adequate"] is not (not report["failures"]):
        raise ManifestError("report.adequate does not match failures")
    statuses = _control_statuses_from_rows(rows)
    derived_control = _control_status(statuses, len(statuses))
    if report["control_status"] != derived_control:
        raise ManifestError("report.control_status does not match control rows")


def _class_attempt_status(report: dict) -> str:
    if (report["score_percent"] is None
            or report["unproved"] != 0
            or report["control_status"] != "killed"):
        return "unproved"
    if any(row.get("verdict") in ("control-error", "control-MOVED")
           for row in report["mutants"]):
        return "unproved"
    if any("UNMUTATED" in item for item in report["failures"]):
        return "unproved"
    return "completed"


def _class_result_from_report(report: dict) -> dict:
    counts = _class_counts_from_rows(report["mutants"])
    return {
        "control_status": report["control_status"],
        "killed": counts["killed"],
        "survived": counts["survived"],
        "silent": counts["silent"],
        "equivalent": counts["equivalent"],
        "unexercised_out_of_scope": counts["unexercised_out_of_scope"],
        "unproved": counts["unproved"],
        "known_holes": counts["known_holes"],
        "denominator": _scored_denominator(
            counts["killed"], counts["survived"], counts["silent"]),
        "score_percent": report["score_percent"],
        "adequate": report["adequate"],
        "failures": list(report["failures"]),
    }


def _require_class_result(result, rows, status: str) -> None:
    require_shape(result, dict, "result")
    _require_closed_keys(
        result, _CLASS_RESULT_KEYS, _CLASS_RESULT_KEYS,
        missing_token="result missing key", extra_token="result extra key")
    _utf8_len(result["control_status"], "result.control_status", maximum=None)
    for key in ("killed", "survived", "silent", "equivalent",
                "unexercised_out_of_scope", "unproved", "known_holes",
                "denominator"):
        _require_exact_nonneg_int(result[key], "result.%s" % key)
    _require_score_percent_form(result["score_percent"], "result.score_percent")
    _require_exact_bool(result["adequate"], "result.adequate")
    if type(result["failures"]) is not list or any(
            not isinstance(item, str) for item in result["failures"]):
        raise ManifestError("result.failures must be a list of strings")
    counts = _class_counts_from_rows(rows)
    for key, value in counts.items():
        if result[key] != value:
            raise ManifestError("result.%s does not match attempt rows" % key)
    denom = _scored_denominator(counts["killed"], counts["survived"], counts["silent"])
    if result["denominator"] != denom:
        raise ManifestError("result.denominator does not match scored rows")
    if result["adequate"] is not (not result["failures"]):
        raise ManifestError("result.adequate does not match failures")
    if status not in ("completed", "unproved"):
        raise ManifestError("status must be completed or unproved")


def _require_class_attempt_v0_document(doc) -> dict:
    require_shape(doc, dict, "class attempt")
    _require_closed_keys(
        doc, _CLASS_ATTEMPT_KEYS, _CLASS_ATTEMPT_KEYS,
        missing_token="class attempt missing key",
        extra_token="class attempt extra key")
    if doc.get("schema") != CLASS_ATTEMPT_SCHEMA:
        raise ManifestError("class attempt schema must be %s" % CLASS_ATTEMPT_SCHEMA)
    _require_class_id_token(doc["attempt_id"], "attempt_id")
    _require_class_id_token(doc["class_id"], "class_id")
    for key in ("provenance_sha256", "manifest_sha256", "report_sha256",
                "environment_sha256"):
        _require_canonical_sha256(doc[key], key)
    if doc["effective_class"] not in CLASS_EFFECTIVE_IDS:
        raise ManifestError("effective_class is not a closed class id")
    if doc["visibility_status"] not in CLASS_VISIBILITY_STATUSES:
        raise ManifestError("visibility_status is not a closed visibility status")
    _require_sha256_or_null(
        doc["predecessor_attempt_sha256"], "predecessor_attempt_sha256")
    require_shape(doc["rows"], list, "rows")
    if len(doc["rows"]) > CLASS_ROWS_MAX:
        raise ManifestError("rows exceeds %d entries" % CLASS_ROWS_MAX)
    for i, row in enumerate(doc["rows"]):
        require_shape(row, dict, "rows[%d]" % i)
        verdict = row.get("verdict") if isinstance(row.get("verdict"), str) else ""
        _require_closed_keys(
            row, _mutant_row_required_keys(verdict), _mutant_row_allowed_keys(verdict),
            missing_token="mutant missing key", extra_token="mutant extra key")
    _require_class_result(doc["result"], doc["rows"], doc["status"])
    expected_status = "unproved"
    result = doc["result"]
    if (result["score_percent"] is not None
            and result["unproved"] == 0
            and result["control_status"] == "killed"
            and not any(row.get("verdict") in ("control-error", "control-MOVED")
                        for row in doc["rows"])
            and not any("UNMUTATED" in item for item in result["failures"])):
        expected_status = "completed"
    if doc["status"] != expected_status:
        raise ManifestError("status does not match result")
    _require_class_non_claims(doc["non_claims"], "non_claims")
    return doc


def encode_class_attempt_v0(doc: dict) -> bytes:
    _require_class_attempt_v0_document(doc)
    return _encode_class_artifact_v0(doc)


def _classification_pair(classification) -> tuple[str, str]:
    require_shape(classification, dict, "classification")
    keys = frozenset({"effective_class", "visibility_status"})
    _require_closed_keys(
        classification, keys, keys,
        missing_token="classification missing key",
        extra_token="classification extra key")
    effective = classification["effective_class"]
    visibility = classification["visibility_status"]
    if effective not in CLASS_EFFECTIVE_IDS:
        raise ManifestError("classification.effective_class is not a closed class id")
    if visibility not in CLASS_VISIBILITY_STATUSES:
        raise ManifestError(
            "classification.visibility_status is not a closed visibility status")
    return effective, visibility


def _derive_class_attempt_v0(*, attempt_id, provenance_raw, manifest_raw,
                             report_raw, environment_raw, predecessor,
                             classification=None) -> dict:
    if type(provenance_raw) is not bytes or type(manifest_raw) is not bytes:
        raise ManifestError("class attempt inputs must be bytes")
    if type(report_raw) is not bytes or type(environment_raw) is not bytes:
        raise ManifestError("class attempt inputs must be bytes")
    provenance = _parse_class_object(provenance_raw)
    classified = _require_class_provenance_v0_document(provenance)
    _require_canonical_class_bytes(provenance_raw, _encode_class_artifact_v0(provenance))
    _require_digest_match(manifest_raw, provenance["manifest_sha256"], "manifest")
    _parse_class_object(manifest_raw)
    manifest = load_manifest_bytes(manifest_raw, Path("manifest.json"))
    _bind_expected_distinctions(provenance, manifest)
    report = _parse_class_object(report_raw)
    _require_report_rows(report)
    _class_report_summary_parity(report)
    if report.get("manifest_sha256") != provenance["manifest_sha256"]:
        raise ManifestError("report/provenance manifest digest mismatch")
    if classification is not None:
        claimed = _classification_pair(classification)
        if claimed != classified:
            raise ManifestError(
                "classification does not match the visibility classifier")
    effective, visibility = classified
    _require_sha256_or_null(predecessor, "predecessor_attempt_sha256")
    result = _class_result_from_report(report)
    status = _class_attempt_status(report)
    doc = {
        "schema": CLASS_ATTEMPT_SCHEMA,
        "attempt_id": attempt_id,
        "class_id": provenance["class_id"],
        "provenance_sha256": _file_sha256(provenance_raw),
        "manifest_sha256": _file_sha256(manifest_raw),
        "report_sha256": _file_sha256(report_raw),
        "environment_sha256": _file_sha256(environment_raw),
        "effective_class": effective,
        "visibility_status": visibility,
        "predecessor_attempt_sha256": predecessor,
        "status": status,
        "result": result,
        "rows": copy.deepcopy(report["mutants"]),
        "non_claims": list(CLASS_NON_CLAIMS),
    }
    _require_class_attempt_v0_document(doc)
    return doc


def derive_class_attempt_v0(*, attempt_id, provenance_raw, manifest_raw,
                            report_raw, environment_raw, predecessor) -> dict:
    return _derive_class_attempt_v0(
        attempt_id=attempt_id,
        provenance_raw=provenance_raw,
        manifest_raw=manifest_raw,
        report_raw=report_raw,
        environment_raw=environment_raw,
        predecessor=predecessor,
    )


def load_class_attempt_v0(attempt_path, *, provenance_path, manifest_path,
                          report_path, environment_path, classifier=None) -> dict:
    attempt_raw = _read_class_input(attempt_path)
    doc = _parse_class_object(attempt_raw)
    _require_class_attempt_v0_document(doc)
    _require_canonical_class_bytes(attempt_raw, encode_class_attempt_v0(doc))
    provenance_raw = _read_class_input(provenance_path)
    _require_digest_match(provenance_raw, doc["provenance_sha256"], "provenance")
    provenance = _parse_class_object(provenance_raw)
    classified = _require_class_provenance_v0_document(provenance)
    _require_canonical_class_bytes(provenance_raw, _encode_class_artifact_v0(provenance))
    if provenance["class_id"] != doc["class_id"]:
        raise ManifestError("provenance class_id does not match the attempt")
    manifest_raw = _read_class_input(manifest_path)
    _require_digest_match(manifest_raw, doc["manifest_sha256"], "manifest")
    if provenance["manifest_sha256"] != doc["manifest_sha256"]:
        raise ManifestError("provenance/attempt manifest digest mismatch")
    _parse_class_object(manifest_raw)
    manifest = load_manifest_bytes(manifest_raw, Path(manifest_path))
    _bind_expected_distinctions(provenance, manifest)
    report_raw = _read_class_input(report_path)
    _require_digest_match(report_raw, doc["report_sha256"], "report")
    report = _parse_class_object(report_raw)
    _require_report_rows(report)
    _class_report_summary_parity(report)
    if report.get("manifest_sha256") != doc["manifest_sha256"]:
        raise ManifestError("report/manifest digest mismatch")
    environment_raw = _read_class_input(environment_path)
    _require_digest_match(environment_raw, doc["environment_sha256"], "environment")
    expected_result = _class_result_from_report(report)
    if doc["result"] != expected_result:
        raise ManifestError("attempt result does not match the validated report")
    if doc["rows"] != report["mutants"]:
        raise ManifestError("attempt rows do not match the validated report")
    if doc["status"] != _class_attempt_status(report):
        raise ManifestError("attempt status does not match the validated report")
    if classified != (doc["effective_class"], doc["visibility_status"]):
        raise ManifestError(
            "stored effective class or visibility status does not match the classifier")
    if classifier is not None:
        extra = classifier(provenance, report)
        if (extra != classified
                and extra != {
                    "effective_class": doc["effective_class"],
                    "visibility_status": doc["visibility_status"],
                }):
            raise ManifestError("classifier does not match stored class status")
    return doc


def _control_result(group: str, label: str, scope: str, *, polarity: str,
                    changed: bool, moved: int, error=None):
    """Map one declared polarity and observation to its row and direct status."""
    if error is not None:
        status, verdict, how = "error", "control-error", str(error)
    elif polarity == "inert" and changed:
        status, verdict, how = (
            "moved", "control-MOVED",
            "pinned outcomes moved under a declared inert transformation")
    elif polarity == "inert":
        status, verdict, how = (
            "killed", "control-unchanged",
            "pinned outcomes stayed unchanged under a declared inert transformation")
    elif changed:
        status, verdict, how = (
            "killed", "control-killed", "harness detects a change on this path")
    else:
        status, verdict, how = (
            "survived", "control-SURVIVED", "THE HARNESS DETECTS NOTHING")
    return ({"group": group, "label": label, "verdict": verdict,
             "scope": scope, "moved": moved, "how": how}, status)


def _record_control(results: list, statuses: list[str], group: str, label: str,
                    scope: str, *, polarity: str, changed: bool, moved: int,
                    error=None) -> str:
    """Record the row and direct status together so neither path can omit one."""
    row, status = _control_result(
        group, label, scope, polarity=polarity, changed=changed,
        moved=moved, error=error)
    results.append(row)
    statuses.append(status)
    return status


def _control_polarity(mut: dict) -> str:
    """Return the validated polarity; legacy controls are positive."""
    return mut.get("control_polarity", "positive")


def _declared_control_count(m: dict) -> int:
    return sum(bool(mut.get("control"))
               for group in m["mutants"].values() for mut in group)


def partition_declared_mutants(mutants: dict) -> tuple:
    """Stable split: every control, then every ordinary mutant.

    Walks sorted group names, then declaration order. Relative order is
    preserved within each partition. No controls leaves the current
    sorted-group walk unchanged.
    """
    if type(mutants) is not dict:
        raise ManifestError("mutants must be an object")
    items = [(group, mut) for group in sorted(mutants) for mut in mutants[group]]
    controls = [(group, mut) for group, mut in items if mut.get("control")]
    ordinary = [(group, mut) for group, mut in items if not mut.get("control")]
    return controls, ordinary


def ordered_declared_mutants(mutants: dict, mutation_order=None) -> list:
    """Return the complete control-first schedule, optionally by exact labels."""
    controls, ordinary = partition_declared_mutants(mutants)
    declared = controls + ordinary
    if mutation_order is None:
        return declared
    if not isinstance(mutation_order, (list, tuple)) or not all(
            isinstance(label, str) and label for label in mutation_order):
        raise ManifestError("mutation_order must be a list of declared labels")
    labels = [label_identity(mut) for _group, mut in declared]
    if len(set(mutation_order)) != len(mutation_order):
        raise ManifestError("mutation_order repeats a declared label")
    if len(mutation_order) != len(labels) or set(mutation_order) != set(labels):
        raise ManifestError("mutation_order must name every declared mutant once")
    by_label = {label_identity(mut): (group, mut) for group, mut in declared}
    ordered = [by_label[label] for label in mutation_order]
    control_count = len(controls)
    if any(not mut.get("control") for _group, mut in ordered[:control_count]) or any(
            mut.get("control") for _group, mut in ordered[control_count:]):
        raise ManifestError("mutation_order must place every control before ordinary mutants")
    return ordered


def _append_group_equivalents(results: list, m: dict, group: str,
                              baselines: dict, emitted: set) -> int:
    """Emit one group's declared equivalents once.

    Equivalents are declarations, not ordinary executions. The ordinary wave
    calls this after each group so no-control row order stays group then
    that group's equivalents. A later pass over remaining baseline groups
    keeps declarations on a control abort that never entered wave 1.
    """
    if group not in baselines or group in emitted:
        return 0
    emitted.add(group)
    added = 0
    for eq in m["equivalent"].get(group, []):
        results.append({"group": group, "label": eq["label"], "verdict": "equivalent",
                        "how": eq["reason"], "moved": 0})
        added += 1
    return added


def _control_status(statuses: list[str], declared_count: int) -> str:
    """Summarise all declared controls without asking a consumer to scan rows.

    Error outranks an incomplete observation, which outranks survived, moved,
    and killed. A missing row means a declared control was stale,
    unloadable or otherwise unmeasured, so a partial set cannot report killed.
    """
    if "error" in statuses:
        return "error"
    if declared_count == 0 or len(statuses) != declared_count:
        return "absent-or-invalid"
    if "survived" in statuses:
        return "survived"
    if "moved" in statuses:
        return "moved"
    return "killed"


def _control_barrier_allows_ordinary(statuses: list[str], declared_count: int) -> bool:
    """Allow ordinary evidence only after every declared control was killed."""
    return declared_count == 0 or _control_status(statuses, declared_count) == "killed"


def _report_v0(manifest_path: Path, m: dict, *,
               killed: int, survived: int, silent: int, equivalent: int,
               out_of_scope: int, unproved: int, known_holes: int,
               score, results: list, failures: list,
               control_status: str = "absent-or-invalid",
               originals_unverified_against_head=None) -> dict:
    """The one `report.v0` shape, for every runner.

    There were two of these, and they drifted: the module one omitted `runner`,
    so a report could not name its own producer and a downstream consumer
    re-read the manifest to recover the field. They had also diverged on three
    expressions that happened to agree numerically only because the module
    runner cannot produce a silent mutant.

    `runner` is read from the manifest rather than passed by the caller.
    `load_manifest` defaults it to `module`, so it is always populated, and a
    caller that cannot supply it cannot supply the wrong one either.

    Process and batch always emit `originals_unverified_against_head`. Module
    never does. The document itself is built by `_report_v0_document`; this
    wrapper only stamps tool identity.
    """
    return _with_tool_identity(_report_v0_document(
        manifest_path, m,
        killed=killed, survived=survived, silent=silent, equivalent=equivalent,
        out_of_scope=out_of_scope, unproved=unproved, known_holes=known_holes,
        score=score, results=results, failures=failures,
        control_status=control_status,
        originals_unverified_against_head=originals_unverified_against_head))


def format_tool_identity(identity: dict | None = None) -> str:
    """Render one producer result. This never resolves identity itself.

    A second resolution here would be a second answer to the same question,
    and the two could disagree with each other inside one report.
    """
    identity = identity if identity is not None else tool_identity()
    commit = identity.get("tool_commit") or "none"
    state = identity.get("tool_source_state") or "unresolved"
    content = identity.get("tool_content_sha256") or "none"
    return "corpus-adequacy %s commit=%s source=%s content=%s" % (
        identity["tool_version"], commit, state, content,
    )


def _req(obj: dict, key: str, where: str):
    if key not in obj:
        raise ManifestError("%s: missing required key %r" % (where, key))
    return obj[key]


def _diagnostic_note(m: dict, moved_diag: list) -> dict:
    """`moved_diagnostic` on rows whose verdict is not itself about diagnostics.

    An excluded or acknowledged row that the diagnostics DID move is a different
    fact from one nothing moved, and the row is the only place a reader can see
    which it was. Present only where a channel was declared, so absent and zero
    do not blur.
    """
    if m.get("diagnostic_from") is None:
        return {}
    return {"moved_diagnostic": len(moved_diag)}


def _diagnostic_suffix(moved_diag: list) -> str:
    """Said in the row's own `how`, because the verdict alone would overstate."""
    if not moved_diag:
        return ""
    return (". The declared diagnostic channel moved on %d vector(s); the pinned "
            "outcomes did not, and this verdict is not scored" % len(moved_diag))


def selector_members(sel) -> list:
    """The declared members of a selector, whether it is a scalar or a list.

    One function so the reader, the presence rule and the manifest validation
    cannot disagree about what a selector declares.
    """
    return sel if isinstance(sel, list) else [sel]


def label_identity(entry: dict, where: str = "entry") -> str:
    """Return the exact string identity used for declarations and acknowledgements."""
    label = entry["label"]
    if not isinstance(label, str) or not label.strip():
        raise ManifestError("%s: label must be a non-empty string" % where)
    return label


def _resolved_contained_source(path: Path, repo_root: Path) -> Path:
    """Resolve one source, refusing roots and targets outside the declared boundary."""
    root = repo_root.resolve()
    if not root.is_dir():
        raise ManifestError("repo_root must be an existing directory: %s" % root)
    resolved = path.resolve()
    try:
        resolved.relative_to(root)
    except ValueError:
        raise ManifestError(
            "implementation source %s resolves outside repo_root %s" % (path, root))
    if not resolved.is_file():
        raise ManifestError("implementation source not found: %s" % resolved)
    return resolved


def _require_unique_labels(m: dict) -> None:
    """Reject ambiguous mutant and per-digest acknowledgement declarations."""
    seen_mutants = {}
    for declaration in ("mutants", "equivalent"):
        for group, entries in m[declaration].items():
            for i, entry in enumerate(entries):
                ident = label_identity(entry, "%s[%s][%d]" % (declaration, group, i))
                if ident in seen_mutants:
                    previous_declaration, previous_group = seen_mutants[ident]
                    raise ManifestError(
                        "mutant label %r is declared more than once (already in %s group %r). "
                        "Labels are unique across the manifest, so one known-hole "
                        "acknowledgement cannot name two mutants"
                        % (ident, previous_declaration, previous_group))
                seen_mutants[ident] = (declaration, group)

    for digest, entries in m["known_holes"].items():
        seen_acknowledgements = set()
        for i, entry in enumerate(entries):
            ident = label_identity(entry, "known_holes[%s][%d]" % (digest, i))
            if ident in seen_acknowledgements:
                raise ManifestError(
                    "known_holes[%s] repeats acknowledgement %r. Each mutant may be "
                    "acknowledged at most once for one corpus digest" % (digest, ident))
            seen_acknowledgements.add(ident)


def _require_expected_mover(m: dict, mutant: dict) -> None:
    if "expected_mover" not in mutant:
        return
    name = mutant["expected_mover"]
    if (m["runner"] != "batch" or m.get("outcome_parse") != "test-names"
            or mutant.get("control") or not isinstance(name, str)
            or not name.strip()):
        raise ManifestError(
            "expected_mover requires a nonempty test name on a non-control "
            "batch test-names mutant")


def _utf8_len(value, where: str, *, maximum: int | None,
              nonblank: bool = True) -> int:
    if not isinstance(value, str):
        raise ManifestError("%s must be a string" % where)
    if nonblank and not value.strip():
        raise ManifestError("%s must not be empty or whitespace-only" % where)
    try:
        size = len(value.encode("utf-8"))
    except UnicodeEncodeError:
        raise ManifestError("%s is not valid UTF-8" % where) from None
    if maximum is not None and size > maximum:
        raise ManifestError("%s exceeds %d UTF-8 bytes" % (where, maximum))
    return size


def _require_rule_url(value, where: str) -> None:
    if value is None:
        return
    _utf8_len(value, where, maximum=2048, nonblank=True)
    try:
        parsed = urlsplit(value)
        hostname = parsed.hostname
    except ValueError as exc:
        raise ManifestError("%s must be an absolute HTTPS URL: %s" % (where, exc)) from None
    if (parsed.scheme != "https" or not hostname or parsed.username is not None
            or parsed.password is not None):
        raise ManifestError(
            "%s must be an absolute HTTPS URL with a hostname and no userinfo" % where)


def _eligible_rule_labels(manifest: dict) -> tuple[dict, dict]:
    """Return exact eligible labels by group and every declared label's group/class."""
    eligible = {}
    declared = {}
    for declaration in ("mutants", "equivalent"):
        groups = manifest.get(declaration, {})
        require_shape(groups, dict, declaration)
        for group, entries in groups.items():
            require_shape(entries, list, "%s[%s]" % (declaration, group))
            for i, entry in enumerate(entries):
                require_shape(entry, dict, "%s[%s][%d]" % (declaration, group, i))
                label = label_identity(entry, "%s[%s][%d]" % (declaration, group, i))
                _utf8_len(label, "%s[%s][%d].label" % (declaration, group, i),
                          maximum=None)
                if label in declared:
                    raise ManifestError("mutant label %r is declared more than once" % label)
                if declaration == "equivalent":
                    is_eligible = True
                else:
                    scope = entry.get("scope", "declared")
                    if scope not in ("declared", "out_of_scope"):
                        raise ManifestError(
                            "%s[%s][%d].scope must be declared or out_of_scope"
                            % (declaration, group, i))
                    is_eligible = (entry.get("control", False) is False
                                   and scope == "declared")
                declared[label] = (group, is_eligible)
                if is_eligible:
                    eligible.setdefault(group, set()).add(label)
    return eligible, declared


def rule_inventory_index(manifest) -> dict | None:
    """Validate and project the one author-declared rule inventory, without side effects."""
    require_shape(manifest, dict, "manifest")
    schema = manifest.get("schema")
    if schema == SCHEMA:
        return None
    if schema != MANIFEST_V1_SCHEMA:
        raise ManifestError(
            "schema must be %r or %r, got %r" % (SCHEMA, MANIFEST_V1_SCHEMA, schema))
    if "rules" not in manifest:
        raise ManifestError("manifest.v1: missing required key 'rules'")
    rules = manifest["rules"]
    require_shape(rules, dict, "rules")
    if len(rules) > RULE_GROUPS_MAX:
        raise ManifestError("rules exceeds the maximum of %d groups" % RULE_GROUPS_MAX)

    eligible, declared = _eligible_rule_labels(manifest)
    all_eligible = {(group, label) for group, labels in eligible.items() for label in labels}
    linked = set()
    projected_rules = []
    groups = []
    total_rows = 0
    total_references = 0
    mutated_keys = frozenset({"id", "text", "url", "disposition", "mutants"})
    excluded_keys = frozenset({"id", "text", "url", "disposition", "reason"})

    for group in rules:
        _utf8_len(group, "rules group", maximum=128, nonblank=True)
    for group in sorted(rules):
        rows = rules[group]
        require_shape(rows, list, "rules[%r]" % group)
        if len(rows) > RULE_ROWS_PER_GROUP_MAX:
            raise ManifestError(
                "rules[%r] exceeds the maximum of %d rows"
                % (group, RULE_ROWS_PER_GROUP_MAX))
        total_rows += len(rows)
        if total_rows > RULE_ROWS_TOTAL_MAX:
            raise ManifestError("rules exceeds the maximum of %d total rows" % RULE_ROWS_TOTAL_MAX)
        seen_ids = set()
        group_rows = []
        group_mutated = 0
        group_excluded = 0
        group_links = 0
        for i, source_row in enumerate(rows):
            where = "rules[%r][%d]" % (group, i)
            require_shape(source_row, dict, where)
            disposition = source_row.get("disposition")
            if disposition == "mutated":
                required = mutated_keys
            elif disposition == "excluded":
                required = excluded_keys
            else:
                raise ManifestError("%s.disposition must be mutated or excluded" % where)
            _require_closed_keys(
                source_row, required, required,
                missing_token="rule missing key", extra_token="rule extra key")
            rule_id = source_row["id"]
            if not isinstance(rule_id, str) or _RULE_ID_RE.fullmatch(rule_id) is None:
                raise ManifestError("%s.id does not match the closed rule id syntax" % where)
            if rule_id in seen_ids:
                raise ManifestError("rules[%r] repeats rule id %r" % (group, rule_id))
            seen_ids.add(rule_id)
            _utf8_len(source_row["text"], where + ".text", maximum=8192)
            _require_rule_url(source_row["url"], where + ".url")
            row = copy.deepcopy(source_row)
            if disposition == "excluded":
                _utf8_len(source_row["reason"], where + ".reason", maximum=2048)
                group_excluded += 1
            else:
                refs = source_row["mutants"]
                require_shape(refs, list, where + ".mutants")
                if not refs or len(refs) > RULE_MUTANTS_PER_ROW_MAX:
                    raise ManifestError(
                        "%s.mutants must contain 1-%d labels"
                        % (where, RULE_MUTANTS_PER_ROW_MAX))
                row_seen = set()
                for j, label in enumerate(refs):
                    label_where = "%s.mutants[%d]" % (where, j)
                    _utf8_len(label, label_where, maximum=None)
                    if label in row_seen:
                        raise ManifestError("%s repeats mutation label %r" % (where, label))
                    row_seen.add(label)
                    declaration = declared.get(label)
                    if declaration is None:
                        raise ManifestError(
                            "%s references unknown mutation label %r" % (where, label))
                    declared_group, is_eligible = declaration
                    if declared_group != group:
                        raise ManifestError(
                            "%s references cross-group mutation label %r" % (where, label))
                    if not is_eligible:
                        raise ManifestError(
                            "%s references a control or out_of_scope label %r" % (where, label))
                    identity = (group, label)
                    if identity in linked:
                        raise ManifestError("mutation label %r is linked by two rules" % label)
                    linked.add(identity)
                total_references += len(refs)
                if total_references > RULE_MUTANT_REFERENCES_MAX:
                    raise ManifestError(
                        "rules exceeds the maximum of %d mutation-label references"
                        % RULE_MUTANT_REFERENCES_MAX)
                row["mutants"] = sorted(refs)
                group_mutated += 1
                group_links += len(refs)
            row["group"] = group
            group_rows.append(row)
        group_rows.sort(key=lambda item: item["id"])
        projected_rules.extend(group_rows)
        groups.append({
            "group": group,
            "rules_declared": len(group_rows),
            "rules_mutation_linked": group_mutated,
            "rules_excluded": group_excluded,
            "linked_mutants": group_links,
        })

    missing = sorted(all_eligible - linked)
    if missing:
        group, label = missing[0]
        raise ManifestError(
            "eligible mutation label %r in group %r is not linked by any rule" % (label, group))
    return {
        "rules_declared": total_rows,
        "rules_mutation_linked": sum(g["rules_mutation_linked"] for g in groups),
        "rules_excluded": sum(g["rules_excluded"] for g in groups),
        "linked_mutants": total_references,
        "groups": groups,
        "rules": projected_rules,
    }


def _require_manifest_profile_declaration(manifest: dict) -> None:
    """Manifest may state only its minimum; operator selection is external."""
    if OPERATOR_PROFILE_KEY in manifest:
        raise ManifestError(
            "manifest must not declare operator key %s (got %r); "
            "candidates may state only %s"
            % (OPERATOR_PROFILE_KEY, manifest[OPERATOR_PROFILE_KEY],
               MINIMUM_PROFILE_KEY))


_MAX_EXACT_TIMEOUT_SECONDS = (1 << 53) - 1


def _require_positive_timeout(value, where: str, *, allow_none: bool = False) -> None:
    """Require a positive integer at most the conservative binary64 bound 2^53 - 1."""
    if allow_none and value is None:
        return
    if (type(value) is not int or value <= 0
            or value > _MAX_EXACT_TIMEOUT_SECONDS):
        raise ManifestError(
            "%s must be a positive integer no greater than %d"
            % (where, _MAX_EXACT_TIMEOUT_SECONDS))


def _require_argv(value, where: str, *, allow_empty: bool,
                  allow_none: bool = False) -> None:
    """Require a JSON argv array without coercing scalars or member types."""
    if allow_none and value is None:
        return
    if (not isinstance(value, list) or (not allow_empty and not value)
            or not all(isinstance(member, str) and member for member in value)):
        qualifier = "possibly empty" if allow_empty else "non-empty"
        raise ManifestError(
            "%s must be a %s JSON array of non-empty strings" % (where, qualifier))


def _require_control_boolean(value, where: str) -> None:
    if type(value) is not bool:
        raise ManifestError("%s control must be a boolean" % where)


def parse_manifest_declaration(manifest_bytes: bytes) -> dict:
    """Validate only declarations carried by exact manifest bytes.

    This boundary is intentionally free of path resolution and file reads. The
    normal measurement loader and static inspection both call it, so manifest
    rules have one implementation while only measurement proceeds to binding.
    """
    if type(manifest_bytes) is not bytes:
        raise ManifestError("manifest bytes must be bytes")
    m = load_json_document(manifest_bytes, root=dict, where="manifest")
    m["_declared_selector_keys"] = tuple(
        key for key in ("outcome_from", "diagnostic_from", "outcome_parse")
        if key in m)
    if m.get("schema") not in (SCHEMA, MANIFEST_V1_SCHEMA):
        raise ManifestError(
            "schema must be %r or %r, got %r"
            % (SCHEMA, MANIFEST_V1_SCHEMA, m.get("schema")))
    _require_manifest_profile_declaration(m)
    # Exact on-disk bytes are the input parsed above. Whitespace and key order
    # therefore remain addressable rather than being silently canonicalised.
    m["_manifest_sha256"] = "sha256:" + hashlib.sha256(manifest_bytes).hexdigest()
    _req(m, "vectors", "manifest")
    if m.get("runner", "module") == "module":
        _req(m, "implementation", "manifest")
    m.setdefault("entrypoint", "evaluate")
    # group_key is OPTIONAL. A corpus with no axis column is one group; forcing it to invent
    # an axis would be the tool bending the corpus to fit itself.
    m.setdefault("group_key", None)
    m.setdefault("id_key", "vector_id")
    m.setdefault("inputs_key", "inputs")
    m.setdefault("vectors_key", "vectors")
    # Which vector fields are passed positionally to the entrypoint. Signatures differ between
    # corpora and a fixed arity would exclude every corpus that did not guess the same one.
    m.setdefault("entrypoint_args",
                 [k for k in (m["group_key"], m["inputs_key"]) if k is not None])
    m.setdefault("default_group", "all")
    if MINIMUM_PROFILE_KEY in m:
        m[MINIMUM_PROFILE_KEY] = _canonical_execution_profile(
            m[MINIMUM_PROFILE_KEY], which=MINIMUM_PROFILE_KEY)
    m.setdefault("known_holes", {})
    require_shape(m["known_holes"], dict, "known_holes")
    m["_corpus_digest"] = None
    if m["known_holes"]:
        for key in ("corpus_digest_file", "corpus_digest_key"):
            _req(m, key, "manifest (known_holes declared)")
        for digest, entries in m["known_holes"].items():
            require_shape(entries, list, "known_holes[%s]" % digest)
            for i, e in enumerate(entries):
                require_shape(e, dict, "known_holes[%s][%d]" % (digest, i))
                for key in ("label", "reason", "recorded"):
                    _req(e, key, "known_holes[%s][%d]" % (digest, i))
                if not str(e["reason"]).strip():
                    raise ManifestError("known_holes[%s][%d] %r: a hole needs a stated reason"
                                        % (digest, i, e["label"]))
    m.setdefault("runner", "module")
    if m["runner"] not in ("module", "process", "batch"):
        raise ManifestError("runner must be module, process or batch, got %r" % m["runner"])
    if m.get("outcome_parse") == "test-names" and m["runner"] != "batch":
        raise ManifestError(
            "outcome_parse test-names is implemented only for runner=batch, "
            "not runner=%s" % m["runner"])
    if m["runner"] == "module" and m.get("diagnostic_from") is not None:
        # Silently ignoring it would be this tool's own failure mode: a manifest
        # declaring a channel that is never read, reporting `silent: 0` as though
        # the class had been measured.
        raise ManifestError(
            "diagnostic_from is not implemented for runner=module; the module runner "
            "reads one callable result, so there is no second channel to compare")
    if m["runner"] == "module" and "unproved_exit_codes" in m:
        raise ManifestError(
            "unproved_exit_codes is not implemented for runner=module; "
            "the module runner has no process/batch exit-code policy")
    if m["runner"] in ("process", "batch"):
        _req(m, "entrypoint_command", "manifest (runner=%s)" % m["runner"])
        if m.get("outcome_parse") != "test-names":
            _req(m, "outcome_from", "manifest (runner=%s)" % m["runner"])
        m.setdefault("outcome_from", [])
        # Optional second channel. Declaring it buys the `silent` verdict: a
        # mutant that moves nothing here is `survived`, one that moves only here
        # is `silent`. Refused beside `test-names`, where the names ARE the
        # outcome and there is no second channel to read.
        if m.get("diagnostic_from") is not None:
            if m.get("outcome_parse") == "test-names":
                raise ManifestError(
                    "diagnostic_from needs a JSON outcome; it cannot be read beside "
                    "outcome_parse test-names, where the test names are the outcome")
            if not isinstance(m["diagnostic_from"], (str, list)):
                raise ManifestError("diagnostic_from must be a string or a list of strings")
            sel = m["diagnostic_from"]
            sel = sel if isinstance(sel, list) else [sel]
            if not sel or not all(isinstance(k, str) and k for k in sel):
                raise ManifestError("diagnostic_from names no readable member")
            oc = m["outcome_from"] if isinstance(m["outcome_from"], list) else [m["outcome_from"]]
            overlap = sorted(set(sel) & set(oc))
            if overlap:
                # A member on both channels can never produce `silent`: any move
                # in it is already a move in the outcome, so the class would be
                # unreachable and the manifest would read as covering more.
                raise ManifestError(
                    "diagnostic_from and outcome_from both name %s; a member read as the "
                    "outcome can never be a silent-only move" % overlap)
        # A batch corpus is exercised as a unit, so there is nothing to build and no
        # per-vector path. Anything else must declare its build.
        if m["runner"] == "process":
            _req(m, "build", "manifest (runner=process)")
        m.setdefault("build", [])
        m.setdefault("repo_root", ".")
        m.setdefault("vector_path_key", "path")
        m.setdefault("build_timeout", 1800)
        m.setdefault("vector_timeout", 120)
        m["accepted_exit_codes"] = accepted_exit_codes(m)
        m["unproved_exit_codes"] = unproved_exit_codes(m)
        overlap = sorted(set(m["accepted_exit_codes"]) & set(m["unproved_exit_codes"]))
        if overlap:
            raise ManifestError(
                "unproved_exit_codes overlaps accepted_exit_codes: %s" % overlap)
    # One deadline per child, on every runner. The module runner has a child too.
    m.setdefault("vector_timeout", 120)
    _require_positive_timeout(m["vector_timeout"], "vector_timeout")
    if "build_timeout" in m:
        _require_positive_timeout(m["build_timeout"], "build_timeout")
    if "build" in m:
        _require_argv(m["build"], "build", allow_empty=True)
    if "entrypoint_command" in m:
        _require_argv(
            m["entrypoint_command"], "entrypoint_command", allow_empty=False)
    m.setdefault("mutants", {})
    m.setdefault("equivalent", {})
    require_shape(m["mutants"], dict, "mutants")
    require_shape(m["equivalent"], dict, "equivalent")
    if not m["mutants"]:
        raise ManifestError("manifest declares no mutants; there is nothing to measure")
    for group, entries in m["mutants"].items():
        require_shape(entries, list, "mutants[%s]" % group)
        for i, e in enumerate(entries):
            require_shape(e, dict, "mutants[%s][%d]" % (group, i))
            for key in ("label", "anchor", "replacement"):
                _req(e, key, "mutants[%s][%d]" % (group, i))
            e.setdefault("scope", "declared")
            e.setdefault("control", False)
            _require_control_boolean(e["control"], "mutants[%s][%d]" % (group, i))
            _require_expected_mover(m, e)
            if "control_polarity" in e and e["control"] is not True:
                raise ManifestError(
                    "mutants[%s][%d] %r: control_polarity requires control: true"
                    % (group, i, e["label"]))
            if ("control_polarity" in e
                    and e["control_polarity"] not in ("positive", "inert")):
                raise ManifestError(
                    "mutants[%s][%d] %r: control_polarity must be positive or inert"
                    % (group, i, e["label"]))
            if e["control"] and e["scope"] != "declared":
                raise ManifestError("mutants[%s][%d] %r: a control cannot be out_of_scope"
                                    % (group, i, e["label"]))
            if e["scope"] not in ("declared", "out_of_scope"):
                raise ManifestError("mutants[%s][%d] %r: scope must be declared or out_of_scope"
                                    % (group, i, e["label"]))
            # An out-of-scope mutant leaves the denominator exactly as an equivalent one does,
            # so it carries the same obligation: a stated reason, never a bare exclusion.
            if e["scope"] == "out_of_scope" and not str(e.get("reason", "")).strip():
                raise ManifestError(
                    "mutants[%s][%d] %r: an out_of_scope mutant needs a stated reason. It leaves "
                    "the denominator like an equivalent one, so it carries the same obligation"
                    % (group, i, e["label"]))
            if not e["anchor"]:
                raise ManifestError(
                    "mutants[%s][%d] %r: the anchor is empty. An empty anchor matches everywhere, "
                    "corrupts the source and is then counted as a kill" % (group, i, e["label"]))
            if e["anchor"] == e["replacement"]:
                raise ManifestError(
                    "mutants[%s][%d] %r: anchor and replacement are identical, so it mutates nothing"
                    % (group, i, e["label"]))
    for group, entries in m["equivalent"].items():
        require_shape(entries, list, "equivalent[%s]" % group)
        for i, e in enumerate(entries):
            require_shape(e, dict, "equivalent[%s][%d]" % (group, i))
            for key in ("label", "reason"):
                _req(e, key, "equivalent[%s][%d]" % (group, i))
            if not str(e["reason"]).strip():
                raise ManifestError(
                    "equivalent[%s][%d] %r: an equivalence needs a stated reason, never a bare claim"
                    % (group, i, e["label"]))
    _require_unique_labels(m)
    m["_rule_inventory"] = rule_inventory_index(m)
    return m


def bind_manifest_files(declaration: dict, artifact_path: Path, *,
                        path_root: Path | None = None) -> dict:
    """Bind a validated declaration to filesystem inputs for measurement."""
    # Binding adds only top-level private fields. Keep the validated declaration
    # observable as the parser returned it while preserving nested identities.
    m = declaration.copy()
    path = Path(artifact_path)
    base = Path(path_root) if path_root is not None else path.parent
    m["_impl_path"] = ((base / m["implementation"]).resolve()
                       if m.get("implementation") else None)
    m["_vectors_path"] = (base / m["vectors"]).resolve()
    if m["known_holes"]:
        dp = (base / m["corpus_digest_file"]).resolve()
        if not dp.is_file():
            raise ManifestError("corpus_digest_file not found: %s" % dp)
        digest_doc = load_json_document(
            dp.read_bytes(), root=dict, where="corpus_digest_file")
        key = m["corpus_digest_key"]
        if key not in digest_doc:
            raise ManifestError("corpus_digest_key %r is missing" % key)
        digest_value = digest_doc[key]
        if not isinstance(digest_value, str):
            raise ManifestError(
                "corpus_digest_key %r must be a string, got %s"
                % (key, type(digest_value).__name__))
        m["_corpus_digest"] = digest_value
    if m["runner"] in ("process", "batch"):
        m["_repo_root"] = (base / m["repo_root"]).resolve()
        srcs = m.get("implementation_sources") or [m["implementation"]]
        m["_source_paths"] = []
        for source in srcs:
            declared = base / source
            m["_source_paths"].append(
                _resolved_contained_source(declared, m["_repo_root"]))
    return m


def load_manifest_bytes(manifest_bytes: bytes, artifact_path: Path, *,
                        path_root: Path | None = None) -> dict:
    """Parse exact manifest bytes once, then bind their logical paths."""
    return bind_manifest_files(
        parse_manifest_declaration(manifest_bytes), artifact_path, path_root=path_root)


_INSPECT_TOP_KEYS = frozenset({
    "schema", "manifest", "declared", "statically_checked", "runtime_unchecked",
    "review_judgment", "execution_authorized", "non_claims",
})
_INSPECT_MANIFEST_KEYS = frozenset({"schema", "sha256", "bytes"})
_INSPECT_DECLARED_KEYS = frozenset({
    "runner", "implementation", "implementation_sources", "vectors", "selectors",
    "controls", "minimum_execution_profile", "operator_execution_profile", "deadlines",
    "commands", "resource_limits", "corpus_digest_file", "rule_inventory",
})
_INSPECT_STATUS_KEYS = frozenset({"status", "value"})
_INSPECT_SELECTOR_KEYS = frozenset({"outcome_from", "diagnostic_from", "outcome_parse"})
_INSPECT_CONTROL_KEYS = frozenset({"group", "label", "polarity"})
_INSPECT_DEADLINE_KEYS = frozenset({"build_timeout", "vector_timeout"})
_INSPECT_COMMAND_KEYS = frozenset({"build", "entrypoint_command"})
_INSPECT_INVENTORY_KEYS = frozenset({
    "rules_declared", "rules_mutation_linked", "rules_excluded", "linked_mutants", "groups"})
_INSPECT_INVENTORY_GROUP_KEYS = frozenset({
    "group", "rules_declared", "rules_mutation_linked", "rules_excluded", "linked_mutants"})
_INSPECT_STATIC_BASE = (
    "json", "manifest-schema", "declaration-shapes", "selector-contract",
    "mutant-label-identity", "control-declarations", "minimum-profile-syntax",
)
_INSPECT_RUNTIME_UNCHECKED = (
    "declared-path-resolution", "source-file-existence-and-bytes", "vector-document",
    "corpus-digest-file", "operator-profile-selection", "profile-downgrade",
    "runner-and-build", "baseline", "controls", "mutants", "containment-envelope",
)
_INSPECT_REVIEW_JUDGMENT = (
    "manifest-trust", "selector-fidelity", "mutation-rule-ownership",
    "rule-inventory-completeness", "execution-permission",
)
_INSPECT_NON_CLAIMS = (
    "No manifest execution or execution authorization is established.",
    "No sandbox or readiness result is established.",
    "No corpus adequacy or rule completeness is established.",
    "No owner ratification or runtime validation is established.",
)


def _inspect_inventory_summary(inventory):
    if inventory is None:
        return None
    return {key: copy.deepcopy(inventory[key]) for key in (
        "rules_declared", "rules_mutation_linked", "rules_excluded",
        "linked_mutants", "groups")}


def inspect_manifest_declaration(manifest_bytes: bytes) -> dict:
    """Project validated manifest declarations without binding or execution."""
    m = parse_manifest_declaration(manifest_bytes)
    controls = []
    for group, entries in m["mutants"].items():
        for entry in entries:
            if entry.get("control"):
                controls.append({
                    "group": group,
                    "label": label_identity(entry),
                    "polarity": _control_polarity(entry),
                })
    present_selectors = frozenset(m["_declared_selector_keys"])
    selectors = {
        key: {
            "status": "declared" if key in present_selectors else "absent",
            "value": copy.deepcopy(m.get(key)) if key in present_selectors else None,
        }
        for key in ("outcome_from", "diagnostic_from", "outcome_parse")
    }
    static_tail = ("rule-inventory" if m["schema"] == MANIFEST_V1_SCHEMA
                   else "rule-inventory-absent")
    return {
        "schema": INSPECT_SCHEMA,
        "manifest": {
            "schema": m["schema"],
            "sha256": m["_manifest_sha256"],
            "bytes": len(manifest_bytes),
        },
        "declared": {
            "runner": m["runner"],
            "implementation": m.get("implementation"),
            "implementation_sources": copy.deepcopy(m.get("implementation_sources")),
            "vectors": m["vectors"],
            "selectors": selectors,
            "controls": controls,
            "minimum_execution_profile": m.get(MINIMUM_PROFILE_KEY),
            "operator_execution_profile": {
                "status": "not-supplied-to-inspection", "value": None},
            "deadlines": {
                "build_timeout": m.get("build_timeout"),
                "vector_timeout": m["vector_timeout"],
            },
            "commands": {
                "build": copy.deepcopy(m.get("build")),
                "entrypoint_command": copy.deepcopy(m.get("entrypoint_command")),
            },
            "resource_limits": {
                "status": "not-represented-in-manifest", "value": None},
            "corpus_digest_file": {
                "status": ("declared" if "corpus_digest_file" in m else "absent"),
                "value": copy.deepcopy(m.get("corpus_digest_file")),
            },
            "rule_inventory": _inspect_inventory_summary(m["_rule_inventory"]),
        },
        "statically_checked": list(_INSPECT_STATIC_BASE + (static_tail,)),
        "runtime_unchecked": list(_INSPECT_RUNTIME_UNCHECKED),
        "review_judgment": list(_INSPECT_REVIEW_JUDGMENT),
        "execution_authorized": False,
        "non_claims": list(_INSPECT_NON_CLAIMS),
    }


def _require_inspect_v0(doc: dict) -> None:
    require_shape(doc, dict, "inspection")
    _require_closed_keys(doc, _INSPECT_TOP_KEYS, _INSPECT_TOP_KEYS,
                         missing_token="inspection missing key",
                         extra_token="inspection extra key")
    if doc["schema"] != INSPECT_SCHEMA:
        raise ManifestError("inspection has an invalid schema")
    _require_closed_keys(doc["manifest"], _INSPECT_MANIFEST_KEYS, _INSPECT_MANIFEST_KEYS,
                         missing_token="inspection manifest missing key",
                         extra_token="inspection manifest extra key")
    _require_closed_keys(doc["declared"], _INSPECT_DECLARED_KEYS, _INSPECT_DECLARED_KEYS,
                         missing_token="inspection declared missing key",
                         extra_token="inspection declared extra key")
    manifest = doc["manifest"]
    if manifest["schema"] not in (SCHEMA, MANIFEST_V1_SCHEMA):
        raise ManifestError("inspection manifest schema is invalid")
    _require_canonical_sha256(manifest["sha256"], "inspection manifest sha256")
    if type(manifest["bytes"]) is not int or manifest["bytes"] < 0:
        raise ManifestError("inspection manifest bytes must be a non-negative integer")
    declared = doc["declared"]
    if declared["runner"] not in ("module", "process", "batch"):
        raise ManifestError("inspection runner is invalid")
    for key in ("implementation", "vectors", "minimum_execution_profile"):
        if declared[key] is not None and not isinstance(declared[key], str):
            raise ManifestError("inspection declared %s must be a string or null" % key)
    sources = declared["implementation_sources"]
    if sources is not None and (not isinstance(sources, list)
                                or not all(isinstance(v, str) for v in sources)):
        raise ManifestError("inspection implementation_sources must be strings or null")
    require_shape(declared["selectors"], dict, "inspection selectors")
    _require_closed_keys(declared["selectors"], _INSPECT_SELECTOR_KEYS, _INSPECT_SELECTOR_KEYS,
                         missing_token="inspection selectors missing key",
                         extra_token="inspection selectors extra key")
    for key, selector in declared["selectors"].items():
        require_shape(selector, dict, "inspection selector %s" % key)
        _require_closed_keys(
            selector, _INSPECT_STATUS_KEYS, _INSPECT_STATUS_KEYS,
            missing_token="inspection selector missing key",
            extra_token="inspection selector extra key")
        status, value = selector["status"], selector["value"]
        if status == "absent":
            if value is not None:
                raise ManifestError("absent inspection selector %s must be null" % key)
            continue
        if status != "declared":
            raise ManifestError("inspection selector %s status is invalid" % key)
        if key == "outcome_parse":
            if not isinstance(value, str) or not value:
                raise ManifestError(
                    "declared inspection selector outcome_parse must be a string")
            continue
        valid_string = isinstance(value, str) and bool(value)
        valid_list = (isinstance(value, list)
                      and all(isinstance(member, str) and member for member in value))
        if not (valid_string or valid_list):
            raise ManifestError(
                "declared inspection selector %s must be a string or list of strings" % key)
        if key == "diagnostic_from" and value == []:
            raise ManifestError(
                "declared inspection selector diagnostic_from must not be empty")
    require_shape(declared["controls"], list, "inspection controls")
    for control in declared["controls"]:
        require_shape(control, dict, "inspection control")
        _require_closed_keys(control, _INSPECT_CONTROL_KEYS, _INSPECT_CONTROL_KEYS,
                             missing_token="inspection control missing key",
                             extra_token="inspection control extra key")
        if not isinstance(control["group"], str):
            raise ManifestError("inspection control group must be a string")
        label_identity(control, "inspection control")
        if control["polarity"] not in ("positive", "inert"):
            raise ManifestError("inspection control polarity is invalid")
    for key, keys in (("deadlines", _INSPECT_DEADLINE_KEYS),
                      ("commands", _INSPECT_COMMAND_KEYS)):
        require_shape(declared[key], dict, "inspection %s" % key)
        _require_closed_keys(declared[key], keys, keys,
                             missing_token="inspection %s missing key" % key,
                             extra_token="inspection %s extra key" % key)
    _require_positive_timeout(
        declared["deadlines"]["build_timeout"], "inspection build_timeout",
        allow_none=declared["runner"] == "module")
    _require_positive_timeout(
        declared["deadlines"]["vector_timeout"], "inspection vector_timeout")
    _require_argv(
        declared["commands"]["build"], "inspection build", allow_empty=True,
        allow_none=declared["runner"] == "module")
    _require_argv(
        declared["commands"]["entrypoint_command"],
        "inspection entrypoint_command", allow_empty=False,
        allow_none=declared["runner"] == "module")
    inventory = declared["rule_inventory"]
    if manifest["schema"] == SCHEMA:
        if inventory is not None:
            raise ManifestError("inspection manifest.v0 inventory must be null")
    else:
        require_shape(inventory, dict, "inspection rule inventory")
        _require_closed_keys(inventory, _INSPECT_INVENTORY_KEYS, _INSPECT_INVENTORY_KEYS,
                             missing_token="inspection inventory missing key",
                             extra_token="inspection inventory extra key")
        require_shape(inventory["groups"], list, "inspection inventory groups")
        for group in inventory["groups"]:
            _require_closed_keys(group, _INSPECT_INVENTORY_GROUP_KEYS,
                                 _INSPECT_INVENTORY_GROUP_KEYS,
                                 missing_token="inspection inventory group missing key",
                                 extra_token="inspection inventory group extra key")
    for key, expected in (("statically_checked",
                           _INSPECT_STATIC_BASE + (("rule-inventory",)
                           if doc["manifest"]["schema"] == MANIFEST_V1_SCHEMA
                           else ("rule-inventory-absent",))),
                          ("runtime_unchecked", _INSPECT_RUNTIME_UNCHECKED),
                          ("review_judgment", _INSPECT_REVIEW_JUDGMENT),
                          ("non_claims", _INSPECT_NON_CLAIMS)):
        if doc[key] != list(expected):
            raise ManifestError("inspection %s is not the closed v0 vocabulary" % key)
    if doc["execution_authorized"] is not False:
        raise ManifestError("inspection cannot authorize execution")
    for key, status in (("operator_execution_profile", "not-supplied-to-inspection"),
                        ("resource_limits", "not-represented-in-manifest")):
        value = doc["declared"][key]
        _require_closed_keys(value, _INSPECT_STATUS_KEYS, _INSPECT_STATUS_KEYS,
                             missing_token="inspection status missing key",
                             extra_token="inspection status extra key")
        if value != {"status": status, "value": None}:
            raise ManifestError("inspection %s is not unavailable" % key)
    digest_file = declared["corpus_digest_file"]
    _require_closed_keys(digest_file, _INSPECT_STATUS_KEYS, _INSPECT_STATUS_KEYS,
                         missing_token="inspection corpus digest file missing key",
                         extra_token="inspection corpus digest file extra key")
    if digest_file["status"] == "absent":
        if digest_file["value"] is not None:
            raise ManifestError("absent inspection corpus digest file must be null")
    elif digest_file["status"] == "declared":
        if not isinstance(digest_file["value"], str) or not digest_file["value"]:
            raise ManifestError("declared inspection corpus digest file must be a string")
    else:
        raise ManifestError("inspection corpus digest file status is invalid")


def encode_inspect_v0(doc: dict) -> bytes:
    _require_inspect_v0(doc)
    try:
        return (json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8")
    except UnicodeEncodeError:
        raise ReportEncodingError(
            "inspection contains text that cannot be encoded as valid UTF-8") from None


def load_manifest(path: Path) -> dict:
    path = Path(path)
    return load_manifest_bytes(read_bounded_regular_file(path), path)


def _acknowledged_holes(m: dict) -> dict:
    """Holes acknowledged against the DECLARED corpus digest.

    STATED PRECISELY, because the earlier wording here was false. This pins to a
    digest STRING read from a file the manifest itself names. It is an
    author-supplied claim about the corpus, not a measurement of it: nothing here
    recomputes the digest from the vectors. Point the file at a stale value, or
    leave it untouched while the corpus moves, and every acknowledgement survives
    a corpus it no longer describes.

    So the expiry is only as strong as the honesty of that file. That is the same
    declared-versus-observed gap this tool exists to find, one level up, in its
    own implementation. Whether the tool should recompute the digest is a contract
    decision with a canonicalisation question attached, and it is not made here.
    """
    if not m.get("_corpus_digest"):
        return {}
    return {label_identity(e): e
            for e in m["known_holes"].get(m["_corpus_digest"], [])}


def _group_of(v: dict, m: dict) -> str:
    return v[m["group_key"]] if m["group_key"] else m["default_group"]


# ---------------------------------------------------------------------------
# module runner: the corpus runs in a disposable child, never in this process
# ---------------------------------------------------------------------------

MODULE_CHILD = Path(__file__).resolve().parent / "module_child.py"
MODULE_CHILD_SCHEMA = "corpus-adequacy.module-child.v0"

# Abnormal TERMINATION of the child, observed before a word of its output is
# read. The unmutated run completed on these same vectors and this one did not,
# so the harness distinguished the mutant, and that is a kill with the class
# named. Everything else -- a child that exited 0 leaving nothing parseable, a
# child that could not be started at all -- is a failure of the MEASUREMENT.
# Reporting one of those as a kill would credit the corpus with catching
# something it was never shown, which is the over-claim this tool exists to
# find, one level up in its own implementation. Those become unproved, and an
# unproved mutant already fails the run.
TERMINATED_KINDS = frozenset({"timeout", "output-cap", "unexpected-exit", "signal"})


def _child_failure_is_termination(kind: str) -> bool:
    """Whether a completed baseline makes this child failure a mutation kill."""
    return kind in TERMINATED_KINDS


_ModuleRun = namedtuple(
    "_ModuleRun", "outcomes raised unsupported load_error entrypoint_missing abnormal")


def _module_abnormal(kind: str) -> _ModuleRun:
    """A child that did not report. Never an outcome, on any of the three roles."""
    return _ModuleRun({}, [], [], None, False, kind)


def child_module_result(raw: str, count: int):
    """Validate the child's typed JSON before any of it becomes an outcome.

    Same contract as child_outcome on the process path: anything the child did
    not say EXACTLY is a kind, never a value. Empty output is the case that
    decides whether this tool is honest, because a child that was killed, or
    that called os._exit, leaves nothing behind -- and reading nothing as "no
    outcome moved" reports a rule as covered on the strength of silence.
    """
    if not raw.strip():
        return None, "no-result"
    try:
        doc = json.loads(raw)
    except Exception:  # noqa: BLE001 - unreadable output is a parse-error
        return None, "parse-error"
    if not isinstance(doc, dict) or doc.get("schema") != MODULE_CHILD_SCHEMA:
        return None, "parse-error"
    # Outcome VALUES are the corpus's own and are not constrained here. Their
    # keys, and the bookkeeping around them, are this protocol's business.
    outcomes = doc.get("outcomes")
    if not isinstance(outcomes, dict) or not all(type(k) is str for k in outcomes):
        return None, "parse-error"
    for key in ("raised", "unsupported"):
        seq = doc.get(key)
        if not isinstance(seq, list) or not all(type(x) is str for x in seq):
            return None, "parse-error"
    if doc.get("load_error") is not None and type(doc.get("load_error")) is not str:
        return None, "parse-error"
    if type(doc.get("entrypoint_missing")) is not bool:
        return None, "parse-error"
    seen = list(outcomes) + doc["raised"] + doc["unsupported"]
    if not all(k.isascii() and k.isdigit() and str(int(k)) == k and int(k) < count
               for k in seen):
        return None, "parse-error"
    index = [int(k) for k in seen]
    if len(set(index)) != len(index):
        return None, "parse-error"
    if doc["load_error"] is None and not doc["entrypoint_missing"]:
        # A child that ran accounts for every vector exactly once. Silence about
        # a vector reads as "unchanged", which is a false survivor wearing the
        # shape of a measurement.
        if sorted(index) != list(range(count)):
            return None, "parse-error"
    return doc, None


def _module_outcomes(m: dict, source: str, tag: str, vectors: list, tmp: Path) -> _ModuleRun:
    """Load one variant of the implementation and collect its outcomes, in a child.

    Corpus source -- mutated corpus source, at that -- is arbitrary code, and
    every way that ends badly was observed on this runner while it ran here: an
    endless mutant hung the tool with no report, SystemExit chose the tool's
    exit code, os._exit(0) ended it at exit 0 with no report at all, a printing
    mutant put 6.3 MB on the tool's own stdout, and a spawned descendant
    outlived the run.

    So the boundary is the one the process and batch runners already had, taken
    through the same _run_capped rather than written a second time: one
    deadline, one output ceiling, one POSIX process-group kill. Windows keeps
    bounded_run's stated non-claim -- the direct child is killed, the process
    tree is not.

    WHAT THIS DOES AND DOES NOT CLAIM
    ---------------------------------
    Process isolation for a trusted-local corpus, not a sandbox. The child
    inherits this process's filesystem, network, environment and credentials,
    and nothing here bounds its memory or its descriptors.

    The protocol channel is not authenticated either. Fd 1 is duplicated before
    any corpus code runs and the original is pointed at stderr, which stops
    accidental pollution; it does not stop a child that scans its descriptors,
    finds the duplicate, and writes a well-formed payload of its choosing.

    So the claim is narrower than "a misbehaving corpus cannot make this run
    say something untrue", which is what an earlier draft of this docstring
    said. What is claimed: the classes measured here -- direct-child timeout,
    output-cap breach, abnormal termination and protocol failure -- are
    fail-closed, so none can be read as a clean result. Same-user
    parent signalling (e.g. kill(getppid())), session escape and
    host resource exhaustion remain outside the process-isolation
    claim. A corpus written to forge a verdict is also outside that
    claim, and nothing in this file would detect one.
    """
    if not MODULE_CHILD.is_file():
        raise ManifestError("the module child shim is missing: %s" % MODULE_CHILD)
    request = tmp / ("request_%s.json" % tag)
    request.write_text(json.dumps({
        "source": source, "tag": tag, "work_dir": str(tmp),
        "entrypoint": m["entrypoint"], "arg_keys": list(m["entrypoint_args"]),
        "vectors": vectors,
        "sys_path": [p for p in sys.path if isinstance(p, str)],
    }), encoding="utf-8")
    cmd = [sys.executable, str(MODULE_CHILD), str(request)]
    try:
        p = _run_capped(cmd, Path.cwd(), timeout=m["vector_timeout"])
    except subprocess.TimeoutExpired:
        return _module_abnormal("timeout")
    except _OutputTooLarge:
        return _module_abnormal("output-cap")
    except OSError:
        return _module_abnormal("incomplete")
    # Classify the child before reading a word of its output, exactly as the
    # process path does: a parseable report on a code we did not accept is not
    # an outcome. The shim is ours, so the only accepted code is 0; the
    # manifest's accepted_exit_codes describe the corpus's own checker.
    kind = classify(p.returncode, [0])
    if kind != "ok":
        return _module_abnormal(kind)
    doc, kind = child_module_result(p.stdout, len(vectors))
    if kind:
        return _module_abnormal(kind)
    vids = [v[m["id_key"]] for v in vectors]
    return _ModuleRun(
        outcomes={vids[int(k)]: v for k, v in doc["outcomes"].items()},
        raised=[vids[int(k)] for k in doc["raised"]],
        unsupported=[vids[int(k)] for k in doc["unsupported"]],
        load_error=doc["load_error"],
        entrypoint_missing=doc["entrypoint_missing"],
        abnormal=None)


CLOSED_UNPROVED_REASONS = (
    "timeout",
    "output-cap",
    "inner-exit",
    "empty-or-missing",
    "malformed",
    "projection",
    # The daemon reported an OOM kill in the contained candidate's cgroup (#102 C): named,
    # not scored, and not a claim that the measured process was the one killed.
    "oom-killed-reported",
    # Contained setup never became ready (unavailable or refused), so no candidate ran to
    # an outcome. Without this token the sanitizer reported such a run as "malformed".
    "setup",
    # Closed tokens selected from wrapper-owned reserved exit statuses. These distinguish
    # where its bounded protocol stopped without retaining child or host text.
    "candidate-preflight",
    "candidate-copy",
    "candidate-build",
    "candidate-report-missing",
    "candidate-report-empty",
    "candidate-report-read",
    "candidate-readback-hold",
)


def sanitize_unproved_reason(value):
    """Return a harness-owned reason token, or None. Never copies child bytes."""
    if value in CLOSED_UNPROVED_REASONS:
        return value
    return None


def classify(returncode, accepted, unproved=()) -> str:
    """ok | unproved | unexpected-exit | signal | incomplete. Never reads stdout.

    Signals and None never become ok or unproved, even if a policy is malformed.
    """
    if returncode is None or type(returncode) is not int:
        return "incomplete"
    if returncode < 0:
        return "signal"
    allowed = {code for code in (accepted or []) if type(code) is int and code >= 0}
    if returncode in allowed:
        return "ok"
    declared_unproved = {
        code for code in (unproved or []) if type(code) is int and code >= 0}
    if returncode in declared_unproved:
        return "unproved"
    return "unexpected-exit"


def _unique_nonneg_exit_codes(raw, key: str) -> list[int]:
    """Unique nonnegative ints. Bools and signals are refused."""
    if not isinstance(raw, list):
        raise ManifestError(
            "%s must be an array of unique nonnegative integers" % key)
    seen: set[int] = set()
    codes: list[int] = []
    for i, value in enumerate(raw):
        if type(value) is bool:
            raise ManifestError(
                "%s[%d] must be a nonnegative integer, got bool" % (key, i))
        if type(value) is not int:
            raise ManifestError(
                "%s[%d] must be a nonnegative integer, got %s"
                % (key, i, type(value).__name__))
        if value < 0:
            raise ManifestError(
                "%s[%d] is %s; signals are never accepted" % (key, i, value))
        if value in seen:
            raise ManifestError("%s repeats %d" % (key, value))
        seen.add(value)
        codes.append(value)
    return codes


def accepted_exit_codes(m: dict) -> list[int]:
    """One process/batch policy: unique nonnegative ints, plus parse rules.

    Default is [0]. Bools are excluded (JSON true is not exit 1). Signals are
    never accepted. outcome_parse test-names requires 101.
    JSON outcome_from has no protocol ID, so extra codes such as 2 are declared
    explicitly. This repository ships no manifests and does not infer codes
    from a command name.
    """
    codes = _unique_nonneg_exit_codes(
        m.get("accepted_exit_codes", [0]), "accepted_exit_codes")
    if m.get("outcome_parse") == "test-names":
        if 101 not in codes:
            raise ManifestError(
                "outcome_parse test-names requires accepted_exit_codes to include 101")
    return codes


def unproved_exit_codes(m: dict) -> list[int]:
    """Opt-in incomplete-inner class. Default []. Same integer rule as accepted."""
    return _unique_nonneg_exit_codes(
        m.get("unproved_exit_codes", []), "unproved_exit_codes")


def child_outcome(m: dict, completed: subprocess.CompletedProcess):
    """Classify returncode against the accepted policy, then parse stdout.

    An accepted code with empty or malformed output is still a parse-error.
    A declared-unproved exit never reaches the parse, even with valid JSON.
    """
    kind = classify(
        completed.returncode,
        m["accepted_exit_codes"],
        m.get("unproved_exit_codes") or [],
    )
    if kind != "ok":
        return None, None, kind
    if m.get("outcome_parse") == "test-names":
        out = completed.stdout + completed.stderr
        failed = sorted(set(re.findall(m.get("failed_test_pattern",
                                             r"^test (\S+) \.\.\. FAILED$"), out, re.M)))
        ran = sum(int(x) for x in re.findall(r"^test result: \w+\. (\d+) passed", out, re.M))
        if ran == 0 and not failed:
            return None, None, "parse-error"
        # `test-names` exposes no separate diagnostic channel: the names ARE the
        # outcome. A corpus wanting the silent class here must emit JSON.
        return tuple(failed), None, None
    try:
        doc = json.loads(completed.stdout)
    except Exception:  # noqa: BLE001 - unreadable output is a parse-error
        return None, None, "parse-error"
    if not isinstance(doc, dict):
        return None, None, "parse-error"
    def _read(name, sel):
        # Presence is recorded for EVERY selector, not only the outcome. A member
        # nothing emits compares None to None on every mutant, and that is the
        # same defect whichever selector declared it: on `outcome_from` it makes
        # the score over-generous, on `diagnostic_from` it makes the `silent`
        # class unreachable while the report still says the channel was declared.
        sl = selector_members(sel)
        m.setdefault("_selector_keys_seen", {}).setdefault(name, set()).update(
            k for k in sl if k in doc)
        if m.get("runner") == "batch":
            vals = [doc.get(k) for k in sl]
            return tuple(tuple(v) if isinstance(v, list) else v for v in vals)
        if isinstance(sel, list):
            return tuple(doc.get(k) for k in sel)
        return doc.get(sel)

    diag = (_read("diagnostic_from", m["diagnostic_from"])
            if m.get("diagnostic_from") is not None else None)
    return _read("outcome_from", m["outcome_from"]), diag, None


# ---------------------------------------------------------------------------
# process runner: a compiled implementation behind a command line
# ---------------------------------------------------------------------------


class _TreeLock:
    """One mutation run at a time per repository.

    _SourceGuard restores what THIS run mutated. It cannot see another run, and
    two runs over one working tree corrupt each other in two ways. The visible
    one is a wrong score: run A applies a mutant, run B reads the tree and finds
    its own anchor gone, and reports `anchor not found` or a plausible number
    over a smaller denominator. The silent one is worse: run A captures its
    "originals" while run B has a mutant applied, and A's restore then writes
    B's mutant back to the tree as though it were the original. That is a
    disabled rule left in a working tree, which is the exact outcome
    _SourceGuard exists to prevent.

    So the lock is taken BEFORE the isolated copy, not after: a tree
    observed outside the lock can change before the copy is made, which
    is the same bug one step earlier.

    Non-blocking on purpose. A run that queued and started twenty minutes later
    would measure a tree nobody chose for it.
    """

    def __init__(self, repo_root: Path) -> None:
        key = hashlib.sha256(str(repo_root.resolve()).encode("utf-8")).hexdigest()[:16]
        self.path = Path(tempfile.gettempdir()) / ("corpus-adequacy-%s.lock" % key)
        self.repo_root = repo_root
        self._fh = None
        self.held = False
        self.unavailable = fcntl is None

    def __enter__(self) -> "_TreeLock":
        if self.unavailable:
            raise ManifestError(
                "no advisory lock on this platform, so a process or batch run "
                "cannot exclude a concurrent writer. Refusing before source "
                "copy, build, or mutation of %s"
                % self.repo_root)
        self._fh = self._open_lockfile()
        try:
            fcntl.flock(self._fh, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            self._fh.close()
            self._fh = None
            raise ManifestError(
                "another corpus-adequacy run holds the lock on %s. It is mutating the "
                "tree you would measure, so this run would score a mixture of two "
                "mutants rather than either one. Wait for it, or measure a separate "
                "checkout." % self.repo_root)
        self.held = True
        return self

    def _open_lockfile(self):
        """POSIX lock open: no follow, no truncate. Fail-closed on a symlink."""
        nofollow = getattr(os, "O_NOFOLLOW", None)
        if nofollow is None:
            raise ManifestError(
                "no O_NOFOLLOW on this platform, so the lock path cannot be "
                "opened without following a symlink. Refusing before isolation of %s"
                % self.repo_root)
        flags = os.O_RDWR | os.O_CREAT | nofollow
        if hasattr(os, "O_CLOEXEC"):
            flags |= os.O_CLOEXEC
        try:
            fd = os.open(self.path, flags, 0o600)
        except OSError:
            raise ManifestError(
                "lock path %s is not a regular file; refusing before isolation of %s"
                % (self.path, self.repo_root))
        try:
            st = os.fstat(fd)
            if not stat.S_ISREG(st.st_mode):
                os.close(fd)
                raise ManifestError(
                    "lock path %s is not a regular file; refusing before isolation of %s"
                    % (self.path, self.repo_root))
            return os.fdopen(fd, "r+")
        except ManifestError:
            raise
        except Exception:
            os.close(fd)
            raise

    def __exit__(self, *exc) -> None:
        if self._fh is not None:
            fcntl.flock(self._fh, fcntl.LOCK_UN)
            self._fh.close()
            self._fh = None
        self.held = False


class _SourceGuard:
    """Restores every mutated source file while this Python process can unwind.

    The adapter edits files in the working tree. The originals are captured up
    front and rewritten in a finally block, covering normal return and ordinary
    Python exceptions. SIGKILL, power loss, and host termination cannot run that
    finally block, so this guard is not durable crash recovery.
    """

    def __init__(self, paths: list[Path], repo_root: Path | None = None) -> None:
        self.repo_root = repo_root
        self.original = {}
        for path in paths:
            current = (_resolved_contained_source(path, repo_root)
                       if repo_root is not None else path)
            self.original[path] = current.read_bytes()
        self.unverified: list[str] = []
        if repo_root is not None:
            self._verify_against_head(repo_root)

    def _verify_against_head(self, repo_root: Path) -> None:
        """The captured original must be the committed one, checked AT capture time.

        Checking `git status` before constructing this guard leaves a window: a
        concurrent actor can mutate a file between the check and the capture, and
        then the mutant IS the original, so restore writes it back and the run
        scores against a rule that was already deleted. That window was not
        theoretical. A run of this tool reported `anchor not found in any declared
        source` for two anchors that occur exactly once in the committed file,
        because a second run had a mutant applied at the moment of capture.

        The lock above stops another instance of THIS tool. It cannot stop a hand
        edit, an editor autosave, or an instance that started before the lock
        existed, so the content is compared rather than assumed.
        """
        for path in self.original:
            current = _resolved_contained_source(path, repo_root)
            rel = current.relative_to(repo_root.resolve())
            try:
                out = subprocess.run(["git", "-C", str(repo_root), "show", "HEAD:%s" % rel],
                                     capture_output=True, timeout=60)
            except (OSError, subprocess.TimeoutExpired):
                self.unverified.append(str(rel))
                continue
            if out.returncode != 0:
                self.unverified.append(str(rel))   # untracked or no git: cannot compare
                continue
            if out.stdout != self.original[path]:
                raise ManifestError(
                    "%s does not match HEAD at the moment its original was captured. "
                    "Something changed it between the dirty check and now, so this run "
                    "would treat that change as the original and restore it afterwards. "
                    "Refusing rather than measuring a tree it did not read." % rel)

    def restore(self) -> None:
        for path, data in self.original.items():
            current = (_resolved_contained_source(path, self.repo_root)
                       if self.repo_root is not None else path)
            if current.read_bytes() != data:
                current.write_bytes(data)

    def verify_clean(self) -> list[str]:
        leaked = []
        for path, data in self.original.items():
            current = (_resolved_contained_source(path, self.repo_root)
                       if self.repo_root is not None else path)
            if current.read_bytes() != data:
                leaked.append(str(path))
        return leaked


def _build(m: dict) -> tuple[bool, str]:
    if not m.get("build"):
        return True, "nothing to build"      # an interpreted corpus has no build step
    try:
        p = _run_capped(list(m["build"]), m["_repo_root"], timeout=m["build_timeout"])
    except _OutputTooLarge:
        return False, "build output exceeded the ceiling"
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, "build could not complete: %r" % (exc,)
    if p.returncode != 0:
        tail = [ln for ln in (p.stdout + p.stderr).splitlines()
                if "error" in ln.lower()][-3:]
        return False, " | ".join(tail)[:300] or "build exited %d" % p.returncode
    return True, "built"


def _batch_outcome(m: dict) -> tuple[dict, dict[str, str]]:
    """Run the command ONCE over the whole corpus.

    Some corpora are consumed as a unit: the checker takes the vector file and
    reports a summary. Per-vector invocation is not available there, so the unit
    IS the outcome. Discrimination then depends entirely on the summary naming
    which cases moved -- a checker reporting only a boolean would make every
    mutant either kill everything or nothing, and this runner would be measuring
    almost nothing. That limitation belongs to the corpus, and it is stated in the
    report rather than hidden by a per-vector shape this corpus does not have.
    """
    cmd = [str(x) for x in m["entrypoint_command"]]
    try:
        p = _run_capped(cmd, m["_repo_root"], timeout=m["vector_timeout"])
    except subprocess.TimeoutExpired:
        return {}, {}, {"<batch>": "timeout"}
    except _OutputTooLarge:
        return {}, {}, {"<batch>": "output-cap"}
    except OSError:
        return {}, {}, {"<batch>": "incomplete"}
    value, diag, kind = child_outcome(m, p)
    if kind:
        return {}, {}, {"<batch>": kind}
    return {"<batch>": value}, {"<batch>": diag}, {}


def _process_outcomes(m: dict, vectors: list[dict]) -> tuple[dict, dict, dict]:
    """Run the built command once per vector. Classify the child before parse."""
    if m["runner"] == "batch":
        return _batch_outcome(m)
    outcomes, diags, raised = {}, {}, {}
    for v in vectors:
        vid = v[m["id_key"]]
        cmd = [str(x).replace("{vector}", str((m["_repo_root"] / v[m["vector_path_key"]]).resolve()))
               for x in m["entrypoint_command"]]
        try:
            p = _run_capped(cmd, m["_repo_root"], timeout=m["vector_timeout"])
        except subprocess.TimeoutExpired:
            raised[vid] = "timeout"
            continue
        except _OutputTooLarge:
            raised[vid] = "output-cap"
            continue
        except OSError:
            raised[vid] = "incomplete"
            continue
        value, diag, kind = child_outcome(m, p)
        if kind:
            raised[vid] = kind
        else:
            outcomes[vid] = value
            diags[vid] = diag
    return outcomes, diags, raised



def _require_declared_vector_keys(row: dict, m: dict, where: str) -> None:
    if m["id_key"] not in row:
        raise ManifestError("%s missing %r" % (where, m["id_key"]))
    if m["group_key"] is not None and m["group_key"] not in row:
        raise ManifestError("%s missing %r" % (where, m["group_key"]))


def load_vector_document(m: dict) -> list:
    """Shared vectors JSON for process and module. One decode, then list/object."""
    raw = m["_vectors_path"].read_bytes()
    doc = load_json_document(raw, where="vectors")
    if m["runner"] == "batch":
        return [{m["id_key"]: "<batch>",
                 (m["group_key"] or "_g"): m["default_group"]}]
    if isinstance(doc, dict):
        if m["vectors_key"] not in doc:
            raise ManifestError("vectors key %r is missing" % m["vectors_key"])
        selected = doc[m["vectors_key"]]
    elif isinstance(doc, list):
        selected = doc
    else:
        raise ManifestError(
            "vectors must be an object or array, got %s" % type(doc).__name__)
    require_shape(selected, list, "vectors")
    for i, row in enumerate(selected):
        where = "vectors[%d]" % i
        require_shape(row, dict, where)
        _require_declared_vector_keys(row, m, where)
    return selected


def _default_execution_backend(m: dict, vectors=None, *, rebuild=True):
    """Host `_build` plus the current process/batch child-outcome contract.

    `vectors is None` is compile/build only, matching the unmutated tree gate.
    `rebuild=False` reuses that build for another baseline group. Mutant steps
    always pass `rebuild=True`.
    A caller may compile or run elsewhere and still return one
    `_ProcessExecution`. The backend never scores,
    never writes a denominator, and never emits a report.
    """
    if rebuild:
        built, detail = _build(m)
        if not built or vectors is None:
            return _ProcessExecution(
                built, detail, {}, {}, {}, {})
    else:
        detail = "reused unmutated build"
        if vectors is None:
            return _ProcessExecution(
                True, detail, {}, {}, {}, {})
    outcomes, diagnostics, raised = _process_outcomes(m, vectors)
    seen = copy.deepcopy(m.get("_selector_keys_seen", {}))
    return _ProcessExecution(
        True, detail, outcomes, diagnostics, raised, seen)


def _new_process_tally() -> dict:
    return {
        "results": [],
        "killed": 0,
        "survived": 0,
        "equivalent": 0,
        "out_of_scope": 0,
        "unproved": 0,
        "silent": 0,
        "known_holes": 0,
        "control_statuses": [],
        "failures": [],
    }


class _ProcessReportAccumulator:
    """Own the only mutable process score state and its final transition."""

    def __init__(self):
        self.state = _new_process_tally()

    def finalize(self, m: dict, acknowledged: dict, declared_controls: int) -> dict:
        return _finalize_process_tally(
            self.state, m, acknowledged, declared_controls)


_EXECUTION_MANIFEST_KEYS = (
    "_repo_root", "_source_paths", "build", "build_timeout", "runner",
    "entrypoint_command", "vector_timeout", "id_key", "vector_path_key",
    "accepted_exit_codes", "unproved_exit_codes", "outcome_parse",
    "failed_test_pattern", "outcome_from", "diagnostic_from",
)


_ProcessExecution = namedtuple(
    "_ProcessExecution",
    "built detail outcomes diagnostics raised selector_keys_seen",
)


def _snapshot_process_execution(result) -> _ProcessExecution:
    """Validate and detach one backend result before trusted state sees it."""
    if not isinstance(result, _ProcessExecution):
        raise ManifestError("execution backend returned an invalid result")
    if type(result.built) is not bool or not isinstance(result.detail, str):
        raise ManifestError("execution backend returned an invalid result")
    mappings = (
        result.outcomes, result.diagnostics, result.raised,
        result.selector_keys_seen,
    )
    if any(not isinstance(value, dict) for value in mappings):
        raise ManifestError("execution backend returned an invalid result")
    return _ProcessExecution(
        result.built,
        result.detail,
        copy.deepcopy(result.outcomes),
        copy.deepcopy(result.diagnostics),
        copy.deepcopy(result.raised),
        copy.deepcopy(result.selector_keys_seen),
    )


def _execution_manifest(m: dict) -> dict:
    """Project only fields the build and child-outcome contract consumes."""
    return {key: copy.deepcopy(m[key]) for key in _EXECUTION_MANIFEST_KEYS if key in m}


class _ProcessMutationSession:
    """Trusted schedule and scoring state shared by every process mutation.

    Backends receive a fresh manifest copy for each execution. They may perform
    compilation and child execution, but cannot rewrite the schedule,
    denominator, provenance, or report state retained here.
    """

    def __init__(self, manifest: dict, backend, accumulator: _ProcessReportAccumulator,
                 acknowledged: dict, declared_controls: int):
        self.manifest = copy.deepcopy(manifest)
        self.backend = backend
        # The default local backend never takes a step; only a supplied backend can declare one.
        self.accepts_step = (backend is not _default_execution_backend
                             and _backend_accepts_step(backend))
        self.accumulator = accumulator
        self.acknowledged = acknowledged
        self.declared_controls = declared_controls
        self.baselines = {}

    def _backend_kwargs(self, vectors, step):
        return {"step": dict(step)} if self.accepts_step else {}

    def _snapshot_execution(self, result):
        return _snapshot_process_execution(result)

    def execute(self, vectors=None, *, rebuild=True, record_selectors=False, step):
        execution_manifest = _execution_manifest(self.manifest)
        execution_vectors = copy.deepcopy(vectors)
        sources = [
            _resolved_contained_source(path, self.manifest["_repo_root"])
            for path in self.manifest["_source_paths"]
        ]
        source_guard = _SourceGuard(sources, repo_root=None)
        try:
            step_kwargs = self._backend_kwargs(execution_vectors, step)
            if source_guard.verify_clean():
                raise ManifestError("declared source changed while binding execution request")
            result = self._snapshot_execution(self.backend(
                execution_manifest, execution_vectors, rebuild=rebuild, **step_kwargs))
            changed = source_guard.verify_clean()
            if changed:
                raise ManifestError(
                    "execution backend changed a declared source: %s" % changed)
        finally:
            source_guard.restore()
            leaked = source_guard.verify_clean()
            if leaked:
                raise ManifestError("declared source restoration failed: %s" % leaked)
        if record_selectors:
            trusted_seen = self.manifest.setdefault("_selector_keys_seen", {})
            for selector in ("outcome_from", "diagnostic_from"):
                trusted_seen.setdefault(selector, set()).update(
                    result.selector_keys_seen.get(selector, set()))
        return result


def _backend_accepts_step(backend) -> bool:
    """True only for an explicit boolean True; any other declared value is refused uncalled."""
    declared = getattr(backend, BACKEND_STEP_ATTRIBUTE, _UNDECLARED)
    if declared is _UNDECLARED:
        return False
    if declared is not True:
        raise ManifestError(
            "execution backend declares %s=%r; only True is accepted, so it is not called"
            % (BACKEND_STEP_ATTRIBUTE, declared if isinstance(declared, (bool, str, int))
               else type(declared).__name__))
    return True


def _step(kind: str, group=None, mutation_id=None) -> dict:
    """The engine's own name for one backend call: no label text, path or host value."""
    return {"kind": kind, "group": group, "id": mutation_id}


def _finalize_process_tally(tally: dict, m: dict, acknowledged: dict,
                            declared_controls: int) -> dict:
    """One process/batch score and failure closer. Backends do not call this."""
    results = tally["results"]
    failures = tally["failures"]
    linger = {label_identity(r): r["verdict"] for r in results
              if label_identity(r) in acknowledged and r["verdict"] != "known-hole"}
    if linger:
        failures.append("known_holes acknowledge rules that are no longer holes: %s. Remove "
                        "them; an acknowledgement pointing at nothing hides the next regression"
                        % sorted("%s (now %s)" % kv for kv in linger.items()))
    control_status = _control_status(tally["control_statuses"], declared_controls)
    killed = tally["killed"]
    survived = tally["survived"]
    silent = tally["silent"]
    # `silent` sits in the denominator beside `survived` and NEVER in the
    # numerator. A mutant the declared outcome channel could not see is a rule an
    # implementer can delete while reproducing every pinned outcome, whatever the
    # diagnostics did; counting it killed would inflate the score by exactly the
    # rules the corpus fails to force.
    denom = _scored_denominator(killed, survived, silent)
    # No denominator means no measurement. Printing 100% over zero is the same
    # defect as excluding everything and printing 100%. An unmutated or control
    # abnormality fail-closes the run: there is no adequacy score.
    score = _score_or_none(None if denom == 0 else round(100.0 * killed / denom, 1),
                           results, failures)
    unproved = tally["unproved"]
    if unproved:
        failures.append("%d mutant(s) never ran, so this corpus was not measured against them"
                        % unproved)
    if survived:
        failures.append("%d mutant(s) survived; the required score is 100%% of non-equivalent "
                        "mutants" % survived)
    if silent:
        failures.append("%d mutant(s) were silent: no declared outcome moved, only a declared "
                        "diagnostic. The rule is not forced by the outcomes this corpus pins, so "
                        "it counts against the score; either write a vector that moves an outcome "
                        "or declare the diagnostic channel part of the pinned surface" % silent)
    if denom == 0 and (declared_controls == 0 or control_status == "killed"):
        failures.append(null_result_reading(
            tally["known_holes"], tally["equivalent"], tally["out_of_scope"]))
    return {
        "killed": killed,
        "survived": survived,
        "silent": silent,
        "equivalent": tally["equivalent"],
        "out_of_scope": tally["out_of_scope"],
        "unproved": unproved,
        "known_holes": tally["known_holes"],
        "score": score,
        "results": results,
        "failures": failures,
        "control_status": control_status,
    }


def _expected_mover_attribution(mutant: dict, baseline: dict, outcome: dict) -> tuple[bool, str]:
    """Attribute only the failed-test tuples already parsed by child_outcome."""
    name = mutant["expected_mover"]
    observed = sorted(set(baseline["<batch>"]) ^ set(outcome["<batch>"]))
    return name in observed, "expected mover %r; observed changed names %r" % (name, observed)


def _execute_mutation_observation(session, group: str, mut: dict) -> dict:
    """Apply one unique source replacement, execute and restore; never compare.

    Both scored and observation consumers use these exact execution facts.
    A backend exception still unwinds source restoration before it propagates.
    """
    m = session.manifest
    vectors = session.baselines[group][0]
    sources = [_resolved_contained_source(sp, m["_repo_root"])
               for sp in m["_source_paths"]]
    hits = [(sp, sp.read_text(encoding="utf-8").count(mut["anchor"]))
            for sp in sources]
    total = sum(n for _, n in hits)
    if total != 1:
        detail = ("%s / %s: anchor not found in any declared source" %
                  (group, mut["label"]) if total == 0 else
                  "%s / %s: the anchor occurs %d times across the declared sources, so "
                  "the substitution would pick one arbitrarily. Make it unique" %
                  (group, mut["label"], total))
        return {"anchor_hits": total, "application": "failed", "execution": None,
                "restored": True, "detail": detail}
    target = next(sp for sp, n in hits if n == 1)
    target = _resolved_contained_source(target, m["_repo_root"])
    step_guard = _SourceGuard(sources, repo_root=None)
    original = target.read_text(encoding="utf-8")
    target = _resolved_contained_source(target, m["_repo_root"])
    mutated = original.replace(mut["anchor"], mut["replacement"], 1)
    try:
        target.write_text(mutated, encoding="utf-8")
        execution = session.execute(vectors, rebuild=True, step=_step(
            "control" if mut.get("control") else "mutant", group, mut.get("id")))
    finally:
        step_guard.restore()
        leaked = step_guard.verify_clean()
        if leaked:
            raise ManifestError("declared source restoration failed: %s" % leaked)
    return {"anchor_hits": 1, "application": "applied", "execution": execution,
            "restored": True, "detail": None}


def _run_mutation_step(session: _ProcessMutationSession, group: str, mut: dict) -> None:
    """Score the shared guarded execution facts using the existing comparator."""
    m = session.manifest
    tally = session.accumulator.state
    acknowledged = session.acknowledged
    vectors, baseline, baseline_diag = session.baselines[group]
    scope = mut.get("scope", "declared")
    facts = _execute_mutation_observation(session, group, mut)
    if facts["execution"] is None:
        detail = facts["detail"]
        if mut.get("control"):
            _record_control(
                tally["results"], tally["control_statuses"],
                group, mut["label"], scope, polarity=_control_polarity(mut), changed=False,
                moved=0, error=detail)
            tally["failures"].append(
                "control %r ended abnormally (%s); that is not a kill and "
                "this run has no adequacy score" % (mut["label"], detail))
        else:
            tally["failures"].append(detail)
        return
    execution = facts["execution"]
    out = execution.outcomes
    out_diag = execution.diagnostics
    raised = execution.raised
    if not execution.built:
        # Cursor's ruling on the design: rustc exit 1 yields no verdict, so
        # the corpus never saw this mutant. Counting it killed would let a
        # typo in the substitution print as "rule covered". Measure a
        # load-bearing arm with a variant that COMPILES, or declare it
        # equivalent.
        if mut.get("control"):
            _record_control(
                tally["results"], tally["control_statuses"],
                group, mut["label"], scope,
                polarity=_control_polarity(mut), changed=False,
                moved=0, error=execution.detail)
            tally["failures"].append(
                "control %r ended abnormally (%s); that is not a kill and "
                "this run has no adequacy score"
                % (mut["label"], execution.detail))
            return
        tally["results"].append({"group": group, "label": mut["label"],
                                 "verdict": "unproved", "scope": scope, "moved": 0,
                                 "how": "the mutant does not build, so the corpus was "
                                        "never run against it: %s"
                                        % execution.detail})
        tally["unproved"] += 1
        return

    moved = [vid for vid, val in out.items() if baseline.get(vid) != val]
    # The silent class, adopted from the forcing gate in
    # `astrogilda/aee-conformance` (see Related work): a mutant that
    # moves no declared outcome but does move a declared diagnostic
    # is a different finding from one nothing noticed at all. It is
    # NOT a kill -- the corpus's own verdict channel did not see the
    # rule go -- so it scores as a survivor, and it is named because
    # the repair differs: a survivor needs a new vector, a silent one
    # may only need the corpus to make its diagnostics normative.
    moved_diag = ([vid for vid, val in out_diag.items()
                   if baseline_diag.get(vid) != val]
                  if m.get("diagnostic_from") is not None else [])
    if mut.get("control"):
        # Only a successfully parsed outcome change proves the control
        # bites. An abnormal child is control-error, not a kill, and
        # invalidates the run.
        if raised:
            how = ", ".join(sorted(set(raised.values())))
            _record_control(
                tally["results"], tally["control_statuses"],
                group, mut["label"], scope,
                polarity=_control_polarity(mut), changed=False,
                moved=0, error=how)
            tally["failures"].append(
                "control %r ended abnormally (%s); that is not a kill and "
                "this run has no adequacy score" % (mut["label"], how))
            return
        changed = bool(moved)
        status = _record_control(
            tally["results"], tally["control_statuses"],
            group, mut["label"], scope, polarity=_control_polarity(mut),
            changed=changed, moved=len(moved))
        if status == "survived":
            tally["failures"].append(
                "control %r survived: the harness cannot detect a change on this "
                "path, so every other verdict in this run is meaningless"
                % mut["label"])
        elif status == "moved":
            tally["failures"].append(
                "inert control %r moved: pinned outcomes changed under a declared "
                "inert transformation, so this run has no adequacy score"
                % mut["label"])
        return
    unmeasured = sorted({
        kind for kind in raised.values() if not _child_failure_is_termination(kind)
    })
    if unmeasured:
        how = ", ".join(unmeasured)
        tally["results"].append({
            "group": group, "label": mut["label"],
            "verdict": "unproved", "scope": scope, "moved": 0,
            "how": "the measurement did not complete (%s), so the corpus "
                   "was never shown this mutant and said nothing about this rule" % how})
        tally["unproved"] += 1
        return
    attribution_how = None
    if not raised and "expected_mover" in mut:
        attributed, attribution_how = _expected_mover_attribution(mut, baseline, out)
        if moved and not attributed:
            tally["results"].append({
                "group": group, "label": mut["label"], "verdict": "survived",
                "scope": scope, "moved": len(moved), "how": attribution_how})
            tally["survived"] += 1
            return
    if raised or moved:
        how = (", ".join(sorted(set(raised.values()))) if raised
               else attribution_how if attribution_how is not None
               else "%d vector(s) moved" % len(moved))
        tally["results"].append({"group": group, "label": mut["label"], "verdict": "killed",
                                 "scope": scope, "moved": len(moved), "how": how})
        tally["killed"] += 1
    # A diagnostic-only move never overrides a declared exclusion.
    # `silent` says "the corpus claims this rule and its pinned
    # outcomes cannot see it". An out-of-scope mutant is not making
    # that claim, and an acknowledged hole has already made it and
    # recorded the fact, so reclassifying either one scored a rule
    # the author excluded and called a still-valid acknowledgement
    # stale. Outcome movement is unaffected: it kills above, and the
    # linger guard still retires an acknowledgement it kills.
    elif scope == "out_of_scope":
        tally["results"].append({"group": group, "label": mut["label"],
                                 "verdict": "unexercised", "scope": scope, "moved": 0,
                                 **_diagnostic_note(m, moved_diag),
                                 "how": "out of scope: %s%s"
                                        % (mut["reason"], _diagnostic_suffix(moved_diag))})
        tally["out_of_scope"] += 1
    elif label_identity(mut) in acknowledged:
        ack = acknowledged[label_identity(mut)]
        # A KNOWN HOLE is not a scope statement. The corpus does claim this
        # rule, the rule is genuinely unexercised, and that fact is recorded
        # against ONE digest rather than fixed today. It stays loud.
        tally["results"].append({"group": group, "label": mut["label"],
                                 "verdict": "known-hole", "scope": scope, "moved": 0,
                                 **_diagnostic_note(m, moved_diag),
                                 "how": "KNOWN HOLE against %s, recorded %s: %s%s"
                                        % (m["_corpus_digest"][:19], ack["recorded"],
                                           ack["reason"], _diagnostic_suffix(moved_diag))})
        tally["known_holes"] += 1
    elif moved_diag:
        tally["results"].append({"group": group, "label": mut["label"], "verdict": "silent",
                                 "scope": scope, "moved": 0,
                                 "moved_diagnostic": len(moved_diag),
                                 "how": "no vector's declared outcome distinguishes it, but "
                                        "%d vector(s) moved on the declared diagnostic "
                                        "channel. An implementation can delete this rule and "
                                        "still reproduce every pinned outcome; only a consumer "
                                        "comparing diagnostics would notice"
                                        % len(moved_diag)})
        tally["silent"] += 1
    else:
        tally["results"].append({"group": group, "label": mut["label"], "verdict": "survived",
                                 "scope": scope, "moved": 0,
                                 "how": "no vector distinguishes it. An implementation can "
                                        "delete this rule and still reproduce the digest"})
        tally["survived"] += 1




def _canonical_execution_profile(value, *, which: str) -> str:
    """Return the closed-set member, never the caller's object."""
    if not isinstance(value, str):
        raise ManifestError(
            "%s must be a string, got %s" % (which, type(value).__name__))
    for canonical in CLOSED_EXECUTION_PROFILES:
        if value == canonical:
            return canonical
    raise ManifestError("unknown %s %r" % (which, value))


def resolve_execution_profile(*, operator, manifest) -> str:
    """Closed operator-owned profile. Manifest may state only a minimum."""
    if type(manifest) is not dict:
        raise ManifestError(
            "manifest must be an object, got %s" % type(manifest).__name__)
    _require_manifest_profile_declaration(manifest)
    profile = _canonical_execution_profile(operator, which=OPERATOR_PROFILE_KEY)
    if MINIMUM_PROFILE_KEY not in manifest:
        return profile
    minimum = _canonical_execution_profile(
        manifest[MINIMUM_PROFILE_KEY], which=MINIMUM_PROFILE_KEY)
    if _PROFILE_STRENGTH[profile] < _PROFILE_STRENGTH[minimum]:
        raise ManifestError(
            "execution_profile %s is below minimum_execution_profile %s; "
            "downgrade is refused" % (profile, minimum))
    return profile


def _require_contained_execution(*, profile, runner, execution_backend) -> None:
    if profile in _CONTAINED_PROFILES:
        if runner == "module":
            raise ManifestError(
                "execution_profile %s cannot be used with runner module" % profile)
        if (execution_backend is None
                or execution_backend is _default_execution_backend):
            raise ManifestError(
                "execution_profile %s requires an explicitly supplied "
                "contained backend; contained-to-local fallback is refused"
                % profile)
        # The engine never tells a backend which profile it resolved, so the
        # backend declares the one it was built for and the two are compared
        # here, before the first call. Otherwise a backend built for another
        # profile would run and record that profile under this one.
        declared = getattr(execution_backend, BACKEND_PROFILE_ATTRIBUTE, _UNDECLARED)
        if declared is _UNDECLARED:
            if profile in _DECLARATION_REQUIRED_PROFILES:
                raise ManifestError(
                    "%s requires a backend that declares its profile; "
                    "the supplied backend declares none, so it is not called"
                    % profile)
        elif declared != profile:
            shown = (declared if isinstance(declared, str)
                     else type(declared).__name__)
            raise ManifestError(
                "execution_profile %s was resolved but the supplied backend "
                "declares %r; a backend built for another profile is not called"
                % (profile, shown))
    if profile not in _EXECUTABLE_PROFILES:
        raise ManifestError(
            "execution_profile %s is recognised but not yet executable: its "
            "PREPARE, Docker argv and envelope consumers do not select by "
            "this profile, so no backend is called" % profile)


def _run_process(m: dict, manifest_path: Path, *, execution_backend=None,
                 mutation_order=None, separate_build_phase=True,
                 execution_profile, control_observer=None) -> dict:
    """Mutate declared sources, rebuild, and run the corpus against the binary."""
    if type(separate_build_phase) is not bool:
        raise ManifestError("separate_build_phase must be a bool")
    if control_observer is not None and not callable(control_observer):
        raise ManifestError("control observer must be callable")
    profile = resolve_execution_profile(operator=execution_profile, manifest=m)
    _require_contained_execution(
        profile=profile, runner=m.get("runner"),
        execution_backend=execution_backend)
    # Validate caller-supplied identities before the lock, then resolve the
    # actual rows again from the session's detached trusted manifest below.
    ordered_declared_mutants(m["mutants"], mutation_order)
    all_vectors = load_vector_document(m)
    if m["runner"] == "batch" and m["group_key"] is None:
        m["group_key"] = "_g"
    backend = execution_backend or _default_execution_backend
    # Judged before the lock and before any call, like the profile declaration.
    if backend is not _default_execution_backend:
        _backend_accepts_step(backend)
    accumulator = _ProcessReportAccumulator()
    tally = accumulator.state

    # Manifest loading and execution can be separated by arbitrary caller work.
    # Re-resolve before touching git, capturing originals, or mutating a source.
    m["_source_paths"] = [
        _resolved_contained_source(path, m["_repo_root"])
        for path in m["_source_paths"]]

    # Pure corpus/manifest validation happens before acquiring a mutation lock.
    # A malformed vector must not leave a lock or captured source behind.
    groups_in_corpus = {_group_of(v, m) for v in all_vectors}
    tally["failures"].extend(structural_failures(m, groups_in_corpus))
    acknowledged = _acknowledged_holes(m)
    declared_controls = _declared_control_count(m)

    # Taken BEFORE the isolated copy: a tree observed outside the lock can
    # change before materialize. Isolation copies working-tree bytes, so dirty
    # declared sources are measured rather than refused; the lock still comes
    # first.
    original_root = m["_repo_root"]
    lock = _TreeLock(original_root)
    lock.__enter__()

    iso = IsolatedMutationTree(original_root)
    guard = None
    try:
        try:
            isolated = iso.materialize()
        except IsolationError as exc:
            raise ManifestError(str(exc)) from exc
        remapped = []
        root_res = original_root.resolve()
        for path in m["_source_paths"]:
            remapped.append(isolated / path.resolve().relative_to(root_res))
        missing = [str(p) for p in remapped if not p.is_file()]
        if missing:
            raise ManifestError("implementation source not found: %s" % missing)
        m["_repo_root"] = isolated
        m["_source_paths"] = remapped
        # Isolated tree has no .git; dirty working-tree bytes are allowed.
        guard = _SourceGuard(m["_source_paths"], repo_root=None)
    except BaseException:
        iso.cleanup()
        lock.__exit__()
        raise

    session = _ProcessMutationSession(
        m, backend, accumulator, acknowledged, declared_controls)
    m = session.manifest
    ordered_mutants = ordered_declared_mutants(m["mutants"], mutation_order)
    try:
        if separate_build_phase:
            prepared = session.execute(None, rebuild=True, step=_step("build"))
            if not prepared.built:
                raise ManifestError(
                    "the UNMUTATED tree does not build: %s" % prepared.detail)

        for group in sorted(m["mutants"]):
            vectors = [v for v in all_vectors if _group_of(v, m) == group]
            if not vectors:
                tally["failures"].append("%s: no vectors, so its mutants cannot be scored" % group)
                continue
            baseline = session.execute(
                vectors, rebuild=not separate_build_phase, record_selectors=True,
                step=_step("baseline", group))
            if not baseline.built:
                tally["failures"].append("%s: the UNMUTATED binary failed (%s) on %s"
                                         % (group, baseline.detail, ["<build>"]))
                continue
            if baseline.raised:
                kinds = sorted(set(baseline.raised.values()))
                extra = ""
                if kinds == ["unproved"]:
                    token = sanitize_unproved_reason(baseline.detail)
                    if token is not None:
                        extra = " [%s]" % token
                tally["failures"].append("%s: the UNMUTATED binary failed (%s)%s on %s"
                                         % (group, ", ".join(kinds), extra,
                                            sorted(baseline.raised)))
                continue
            session.baselines[group] = (
                vectors, baseline.outcomes, baseline.diagnostics)

        # An outcome member the implementation never emits contributes a constant
        # None to every comparison, so it discriminates nothing and every score
        # after it is over-generous by however much that member would have caught.
        # `doc.get(k)` made that silent. Absent on SOME vectors is legitimate --
        # `verdict` appears only when integrity passes, `claims` only when the
        # verdict is valid -- so the rule is "present at least once", not "always".
        for _selector in ("outcome_from", "diagnostic_from"):
            if m.get(_selector) is None:
                continue
            declared_keys = selector_members(m[_selector])
            seen = m.get("_selector_keys_seen", {}).get(_selector, set())
            never_seen = [k for k in declared_keys if k not in seen]
            if never_seen and session.baselines:
                tally["failures"].append(
                    "%s names %s, which the unmutated implementation never emits on any "
                    "vector. Those members compare None to None on every mutant, so they "
                    "discriminate nothing and this score is over-generous by whatever they "
                    "would have caught. Read the corpus's own declaration of its comparison "
                    "surface and match it." % (_selector, never_seen))

        prev_ordinary = None
        emitted_equivalents: set = set()
        entered_ordinary = False
        for group, mut in ordered_mutants:
            if not mut.get("control") and not entered_ordinary:
                if not _control_barrier_allows_ordinary(
                        tally["control_statuses"], declared_controls):
                    break
                entered_ordinary = True
            if entered_ordinary and prev_ordinary is not None and group != prev_ordinary:
                tally["equivalent"] += _append_group_equivalents(
                    tally["results"], m, prev_ordinary,
                    session.baselines, emitted_equivalents)
            if entered_ordinary:
                prev_ordinary = group
            if group not in session.baselines:
                continue
            if control_observer is None or not mut.get("control"):
                _run_mutation_step(session, group, mut)
            else:
                # Opt-in observation of the engine's own result, not a second
                # movement comparison. Detached events cannot mutate its tally.
                start = len(tally["results"])
                event = {"group": group, "id": mut.get("id"),
                         "polarity": _control_polarity(mut)}
                try:
                    _run_mutation_step(session, group, mut)
                except BaseException as primary:
                    try:
                        control_observer({**event, "state": "interrupted", "verdict": None})
                    except BaseException as observer_error:
                        primary.add_note("control observer failed: " + type(observer_error).__name__)
                    raise
                rows = tally["results"][start:]
                if (len(rows) != 1 or rows[0].get("group") != group
                        or rows[0].get("label") != mut["label"]
                        or rows[0].get("verdict") not in (
                            "control-killed", "control-unchanged", "control-SURVIVED",
                            "control-MOVED", "control-error")):
                    raise ManifestError("control observer has no exact engine result")
                control_observer({**event, "state": "evaluated", "verdict": rows[0]["verdict"]})
        if prev_ordinary is not None:
            tally["equivalent"] += _append_group_equivalents(
                tally["results"], m, prev_ordinary,
                session.baselines, emitted_equivalents)
        for group in sorted(m["mutants"]):
            tally["equivalent"] += _append_group_equivalents(
                tally["results"], m, group, session.baselines, emitted_equivalents)
    finally:
        try:
            if guard is not None:
                guard.restore()
                leaked = guard.verify_clean()
                if leaked:
                    tally["failures"].append("SOURCES NOT RESTORED: %s" % leaked)
        finally:
            iso.cleanup()
            lock.__exit__()

    final = accumulator.finalize(m, acknowledged, declared_controls)
    return _report_v0(
        manifest_path, m,
        killed=final["killed"], survived=final["survived"], silent=final["silent"],
        equivalent=final["equivalent"], out_of_scope=final["out_of_scope"],
        unproved=final["unproved"], known_holes=final["known_holes"],
        score=final["score"], results=final["results"], failures=final["failures"],
        control_status=final["control_status"],
        originals_unverified_against_head=guard.unverified)


def run(manifest_path: Path, *, execution_profile) -> dict:
    m = load_manifest(manifest_path)
    profile = resolve_execution_profile(operator=execution_profile, manifest=m)
    _require_contained_execution(
        profile=profile, runner=m["runner"], execution_backend=None)
    if m["runner"] in ("process", "batch"):
        return _run_process(
            m, manifest_path, execution_profile=execution_profile)
    source = m["_impl_path"].read_text(encoding="utf-8")
    all_vectors = load_vector_document(m)

    failures: list[str] = []
    groups_in_corpus = {_group_of(v, m) for v in all_vectors}

    failures.extend(structural_failures(m, groups_in_corpus))
    acknowledged = _acknowledged_holes(m)
    results, killed, survived, equivalent, out_of_scope = [], 0, 0, 0, 0
    control_statuses: list[str] = []
    declared_controls = _declared_control_count(m)
    # The module runner refuses `diagnostic_from` at load, so the silent class
    # cannot occur here. It is still reported: a consumer reading `.get("silent",
    # 0)` on a report that omits the key gets the false-measured answer the field
    # exists to prevent, and the same denominator rule has to hold on every path.
    silent = 0
    unproved = known_holes = 0
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        baselines = {}
        for group in sorted(m["mutants"]):
            vectors = [v for v in all_vectors if _group_of(v, m) == group]
            if not vectors:
                failures.append("%s: no vectors, so its mutants cannot be scored" % group)
                continue
            base = _module_outcomes(m, source, "base", vectors, tmp)
            if base.entrypoint_missing:
                raise ManifestError("implementation has no entrypoint %r" % m["entrypoint"])
            if base.load_error:
                raise ManifestError("the implementation does not load: %s" % base.load_error)
            # The unmutated run is what every other verdict in this group is
            # measured against, so its child dying is not a result about any
            # mutant. It invalidates the group and nulls the score.
            if base.abnormal:
                failures.append("%s: the UNMUTATED implementation could not be measured (%s)"
                                % (group, base.abnormal))
                continue
            if base.unsupported:
                failures.append(
                    "%s: the UNMUTATED implementation returned outcomes this runner cannot "
                    "transport on %s, so there is nothing for a mutant to be compared against"
                    % (group, base.unsupported))
                continue
            if base.raised:
                failures.append("%s: the UNMUTATED implementation raised on %s"
                                % (group, base.raised))
                continue
            baselines[group] = (vectors, base.outcomes)

        entered_ordinary = False
        for idx, (group, mut) in enumerate(ordered_declared_mutants(m["mutants"])):
            if not mut.get("control") and not entered_ordinary:
                if not _control_barrier_allows_ordinary(
                        control_statuses, declared_controls):
                    break
                entered_ordinary = True
            if group not in baselines:
                continue
            vectors, baseline = baselines[group]
            occurrences = source.count(mut["anchor"])
            if occurrences == 0:
                detail = (
                    "%s / %s: anchor not found in %s. The rule was renamed or removed and the "
                    "mutant is measuring nothing" % (group, mut["label"], m["implementation"]))
                failures.append(detail)
                if mut.get("control") and _control_polarity(mut) == "inert":
                    _record_control(
                        results, control_statuses, group, mut["label"], "declared",
                        polarity="inert", changed=False, moved=0, error=detail)
                continue
            if occurrences > 1:
                # Substituting the first of several is a coin flip about which rule is being
                # measured, and a mangled substitution is then scored as a kill.
                detail = (
                    "%s / %s: the anchor occurs %d times in %s, so the substitution would pick "
                    "one arbitrarily and any breakage would be scored as a kill. Make the "
                    "anchor unique" % (group, mut["label"], occurrences, m["implementation"]))
                failures.append(detail)
                if mut.get("control") and _control_polarity(mut) == "inert":
                    _record_control(
                        results, control_statuses, group, mut["label"], "declared",
                        polarity="inert", changed=False, moved=0, error=detail)
                continue
            scope = mut.get("scope", "declared")
            mutated = source.replace(mut["anchor"], mut["replacement"], 1)
            res = _module_outcomes(m, mutated,
                                   "%s_%d" % (group.replace("-", "_"), idx), vectors, tmp)
            if res.load_error or res.entrypoint_missing:
                # A mutant that never loaded was never shown to the corpus, so the
                # corpus said nothing about it. Counting that as a kill lets a typo
                # in the substitution print as "rule covered". The same argument
                # rules out treating a Rust build failure as a kill; measure a
                # load-bearing rule with a variant that RUNS, or declare it equivalent.
                detail = res.load_error or ("no entrypoint %r" % m["entrypoint"])
                if mut.get("control") and _control_polarity(mut) == "inert":
                    _record_control(
                        results, control_statuses, group, mut["label"], scope,
                        polarity="inert", changed=False, moved=0, error=detail)
                    failures.append(
                        "control %r ended abnormally (%s); that is not a kill and "
                        "this run has no adequacy score" % (mut["label"], detail))
                    continue
                results.append({"group": group, "label": mut["label"],
                                "verdict": "unproved", "scope": scope, "moved": 0,
                                "how": "the mutant does not load, so the corpus never "
                                       "saw it and said nothing about this rule: %s" % detail})
                unproved += 1
                continue
            if res.abnormal or res.unsupported:
                kind = res.abnormal or "unsupported-outcome"
                if mut.get("control"):
                    # The control is what proves the harness detects anything, so
                    # its child dying is not a kill. It leaves the run with no score.
                    _record_control(
                        results, control_statuses,
                        group, mut["label"], scope,
                        polarity=_control_polarity(mut), changed=False,
                        moved=0, error=kind)
                    failures.append(
                        "control %r ended abnormally (%s); that is not a kill and "
                        "this run has no adequacy score" % (mut["label"], kind))
                    continue
                if _child_failure_is_termination(kind):
                    # Observed termination: the unmutated run completed on these
                    # same vectors and this one did not.
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "killed", "scope": scope, "moved": 0,
                                    "how": kind})
                    killed += 1
                    continue
                results.append({
                    "group": group, "label": mut["label"], "verdict": "unproved",
                    "scope": scope, "moved": 0,
                    "how": "the measurement did not complete (%s), so the corpus was "
                           "never shown this mutant and said nothing about this rule" % kind})
                unproved += 1
                continue
            out = res.outcomes
            raised = list(res.raised)
            moved = [vid for vid, val in out.items() if baseline.get(vid) != val]
            if mut.get("control"):
                changed = bool(raised or moved)
                status = _record_control(
                    results, control_statuses,
                    group, mut["label"], scope, polarity=_control_polarity(mut),
                    changed=changed, moved=len(moved))
                if status == "survived":
                    failures.append(
                        "control %r survived: the harness cannot detect a change on this "
                        "path, so every other verdict in this run is meaningless"
                        % mut["label"])
                elif status == "moved":
                    failures.append(
                        "inert control %r moved: pinned outcomes changed under a declared "
                        "inert transformation, so this run has no adequacy score"
                        % mut["label"])
                continue
            if raised:
                results.append({"group": group, "label": mut["label"], "verdict": "killed",
                                "scope": scope, "how": "raises on %d vector(s)" % len(raised),
                                "moved": len(moved), "raised": raised})
                killed += 1
            elif moved:
                results.append({"group": group, "label": mut["label"], "verdict": "killed",
                                "scope": scope, "how": "%d vector(s) moved" % len(moved),
                                "moved": len(moved)})
                killed += 1
            elif scope == "out_of_scope":
                # Declared by the manifest as a rule this corpus does not claim to cover.
                # Reported every run, never scored: adequacy is relative to declared scope,
                # and scoring a rule nobody claimed manufactures a hole that is not one.
                results.append({
                    "group": group, "label": mut["label"], "verdict": "unexercised",
                    "scope": scope, "moved": 0,
                    # Print the stated reason, exactly as a declared equivalent does.
                    # "each with a stated reason" without showing one is an assertion.
                    "how": "out of scope: %s" % mut["reason"]})
                out_of_scope += 1
            elif label_identity(mut) in acknowledged:
                ack = acknowledged[label_identity(mut)]
                results.append({"group": group, "label": mut["label"],
                                "verdict": "known-hole", "scope": scope, "moved": 0,
                                "how": "KNOWN HOLE against %s, recorded %s: %s"
                                       % (str(m["_corpus_digest"])[:19], ack["recorded"],
                                          ack["reason"])})
                known_holes += 1
            else:
                results.append({
                    "group": group, "label": mut["label"], "verdict": "survived",
                    "scope": scope, "moved": 0,
                    "how": "no vector distinguishes it. An implementation can delete this rule, "
                           "reproduce the pinned digest, and be indistinguishable from a "
                           "conforming one. The corpus needs a vector where this rule, and only "
                           "this rule, decides the outcome"})
                survived += 1

        for group in sorted(m["mutants"]):
            if group not in baselines:
                continue
            for eq in m["equivalent"].get(group, []):
                results.append({"group": group, "label": eq["label"], "verdict": "equivalent",
                                "how": eq["reason"], "moved": 0})
                equivalent += 1

        # Execution is control-first, but report.v0 retains declaration order.
        declared_order = [
            (group, label_identity(entry))
            for group in sorted(m["mutants"])
            for entry in m["mutants"][group] + m["equivalent"].get(group, [])
        ]
        order_index = {identity: idx for idx, identity in enumerate(declared_order)}
        results.sort(key=lambda row: order_index[(row["group"], label_identity(row))])

    linger = {label_identity(r): r["verdict"] for r in results
              if label_identity(r) in acknowledged and r["verdict"] != "known-hole"}
    if linger:
        failures.append("known_holes acknowledge rules that are no longer holes: %s. Remove "
                        "them; an acknowledgement pointing at nothing hides the next regression"
                        % sorted("%s (now %s)" % kv for kv in linger.items()))
    denom = _scored_denominator(killed, survived, silent)
    score = _score_or_none(None if denom == 0 else round(100.0 * killed / denom, 1),
                           results, failures)
    if unproved:
        # Not a soft warning: an unproved mutant means the measurement did not happen,
        # and a score computed over the rest reports more than the run established.
        failures.append("%d mutant(s) never ran, so this corpus was not measured against "
                        "them. Fix the substitution or declare them equivalent" % unproved)
    if survived:
        failures.append("%d mutant(s) survived; the required score is 100%% of non-equivalent "
                        "mutants" % survived)
    if denom == 0:
        failures.append(null_result_reading(known_holes, equivalent, out_of_scope))

    # `silent` is 0 here by construction: the module runner refuses
    # `diagnostic_from` at load, so the class cannot occur. It is projected
    # rather than omitted, so a consumer can tell zero from unmeasured.
    return _report_v0(
        manifest_path, m,
        killed=killed, survived=survived, silent=silent, equivalent=equivalent,
        out_of_scope=out_of_scope, unproved=unproved, known_holes=known_holes,
        score=score, results=results, failures=failures,
        control_status=_control_status(control_statuses, declared_controls))


def null_result_reading(known_holes, equivalent, out_of_scope):
    """What a denominator of zero actually licenses you to say.

    A null result feels like a fact about the corpus -- "this one cannot be
    measured" -- and reads like a finding. It is almost always a fact about the
    manifest instead, and reads like a mistake, which is why the wrong reading
    survives. The author of this tool published "not measurable" for a 14-vector
    corpus after declaring three rules for it, two of which mutated the wrong
    stage; the verifier had at least eight more that the vector names all but
    announce. The message says so rather than leaving the reader to make the
    same inference unaided.
    """
    if known_holes or equivalent or out_of_scope:
        return ("nothing was measured: every declared in-scope rule is either a known hole, "
                "declared equivalent or out of scope. There is no adequacy result here. "
                "A null result is a statement about the DECLARATION before it is one about "
                "the corpus: count the rules the implementation has, from the implementation "
                "rather than from this manifest, before concluding the corpus cannot be measured")
    return ("no non-equivalent mutants were scored, so no adequacy was measured. "
            "Declare the rules the implementation actually has; an empty declaration "
            "measures nothing and says nothing")


def _score_or_none(score, results: list, failures: list):
    """A run whose own harness or unmutated baseline failed has no score.

    One function because it was two. The process path nulled the score on a
    control-error or a failed UNMUTATED run; the module path did not, so the
    same abnormality was a refusal on one runner and a printed percentage on
    the other. That is the defect structural_failures already describes one
    level up, in the arithmetic instead of in the guards.
    """
    if (any(r.get("verdict") in ("control-error", "control-MOVED") for r in results)
            or any("UNMUTATED" in f for f in failures)):
        return None
    return score


def structural_failures(m: dict, groups_in_corpus: set) -> list:
    """Guards that hold for EVERY runner, in one place because they drifted apart.

    These were written twice, once per runner, and the copies diverged: the
    control requirement reached the process and batch paths and never reached the
    module path. A module corpus could therefore score without ever declaring the
    one mutant that proves the harness can detect anything -- which is exactly the
    condition the control exists to exclude, missing from the runner where it was
    cheapest to check. One of this tool's own five subject corpora was in that
    state, while the page publishing its score said every manifest must declare a
    control.

    A rule stated in two places is a rule that will eventually be enforced in one.
    """
    failures = []
    unmutated = sorted(groups_in_corpus - set(m["mutants"]))
    if unmutated:
        failures.append(
            "groups present in the corpus with no declared mutants: %s. Declare a mutant per "
            "rule, or this check covers less than its name claims" % unmutated)
    stale = sorted(set(m["mutants"]) - groups_in_corpus)
    if stale:
        failures.append("mutants declared for groups not in the corpus: %s" % stale)
    if not any(mut.get("control") and _control_polarity(mut) == "positive"
               for muts in m["mutants"].values() for mut in muts):
        failures.append("no control mutant declared. Without one, a run of all-survivors "
                        "cannot be told apart from a harness that detects nothing")
    acknowledged = _acknowledged_holes(m)
    orphaned = set(acknowledged) - {
        label_identity(mu) for ms in m["mutants"].values() for mu in ms
    }
    if orphaned:
        failures.append("known_holes name mutants that do not exist: %s" % sorted(orphaned))
    return failures



def _write_encoded(encoded: bytes) -> None:
    if hasattr(sys.stdout, "buffer"):
        sys.stdout.buffer.write(encoded)
    else:
        sys.stdout.write(encoded.decode("utf-8"))


def _render_inspect_v0(doc: dict) -> None:
    """Render only the validated static inspection document."""
    _require_inspect_v0(doc)
    manifest = doc["manifest"]
    declared = doc["declared"]
    print("Manifest: %s (%d bytes, %s)"
          % (manifest["schema"], manifest["bytes"], manifest["sha256"]))
    print("Runner: %s" % declared["runner"])
    print("Implementation: %s" % declared["implementation"])
    print("Vectors: %s" % declared["vectors"])
    print("Statically checked: %s" % ", ".join(doc["statically_checked"]))
    print("Runtime unchecked: %s" % ", ".join(doc["runtime_unchecked"]))
    print("Review judgment: %s" % ", ".join(doc["review_judgment"]))
    print("Inspected only: nothing executed or authorized; every runtime_unchecked item "
          "remains unchecked.")


def _inspect_cli(args) -> int:
    """Bounded static declaration path; never binds files or calls run()."""
    try:
        raw = read_bounded_regular_file(args.inspect)
        projected = inspect_manifest_declaration(raw)
        encoded = encode_inspect_v0(projected) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError, ValueError) as exc:
        print("could not inspect: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="inspect"), indent=2, sort_keys=True))
        return 2
    if args.json:
        assert encoded is not None
        _write_encoded(encoded)
    else:
        _render_inspect_v0(projected)
    return 0


def _survivors_cli(args, ap) -> int:
    """Early sibling path: read a report.v0 file. Never calls run()."""
    if args.manifest is None:
        ap.error("report is required")
    try:
        raw = read_bounded_regular_file(args.manifest)
        report = _parse_projection_json(raw)
        projected = survivor_findings(report, manifest=args.anchor_manifest)
        encoded = encode_survivors_v0(projected) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError, ValueError) as exc:
        print("could not project: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="project"), indent=2, sort_keys=True))
        return 2
    if args.json:
        assert encoded is not None
        _write_encoded(encoded)
        return 0
    print("%d survivor findings (%d survived, %d silent)"
          % (projected["finding_count"], projected["survived"], projected["silent"]))
    for finding in projected["findings"]:
        print("%-22s %-9s %s" % (finding["group"], finding["verdict"], finding["rule"]))
        print("    %s" % finding["obligation"])
        if "anchor_excerpt" in finding:
            print("    anchor: %s" % finding["anchor_excerpt"])
        elif finding.get("anchor_omitted"):
            print("    anchor omitted: %s" % finding["anchor_omitted"])
    return 0


def _kills_cli(args, ap) -> int:
    """Sibling path like --survivors: read a report.v0 file. Never calls run()."""
    if args.manifest is None:
        ap.error("report is required")
    try:
        raw = read_bounded_regular_file(args.manifest)
        report = _parse_projection_json(raw)
        projected = kill_evidence(report, report_sha256=_file_sha256(raw))
        encoded = encode_kills_v0(projected) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError, ValueError) as exc:
        print("could not project: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="project"), indent=2, sort_keys=True))
        return 2
    if args.json:
        assert encoded is not None
        _write_encoded(encoded)
        return 0
    print("%d kills in report %s (%d without a moved outcome, %d unclassified)"
          % (projected["killed"], projected["report_sha256"],
             projected["without_moved_outcome"], projected["unclassified"]))
    for kill in projected["kills"]:
        print("%-22s %-21s %s" % (kill["group"], kill["basis"], kill["rule"]))
        if kill["classes"]:
            print("    %s" % "+".join(kill["classes"]))
    return 0


def _render_rules_v0(projected: dict) -> None:
    """Render only values already supplied by the index; never recompute a tally."""
    inventory = projected["inventory"]
    if inventory is None:
        print("Rule inventory: absent (manifest.v0); no zero was measured.")
        return
    print("Rule inventory: %d declared, %d mutation-linked, %d excluded, %d linked mutants"
          % (inventory["rules_declared"], inventory["rules_mutation_linked"],
             inventory["rules_excluded"], inventory["linked_mutants"]))
    for group in inventory["groups"]:
        print("%-22s %d declared, %d mutation-linked, %d excluded, %d linked mutants"
              % (group["group"], group["rules_declared"],
                 group["rules_mutation_linked"], group["rules_excluded"],
                 group["linked_mutants"]))
    for rule in inventory["rules"]:
        detail = (", ".join(rule["mutants"])
                  if rule["disposition"] == "mutated" else rule["reason"])
        print("%-22s %-9s %s: %s"
              % (rule["group"], rule["disposition"], rule["id"], detail))


def _rules_cli(args) -> int:
    """Early sibling path: read report and manifest files. Never calls run()."""
    try:
        if args.manifest is None:
            raise ManifestError("report is required for --rules")
        if args.anchor_manifest is None:
            raise ManifestError("--manifest is required for --rules")
        report_raw = read_bounded_regular_file(args.manifest)
        manifest_raw = read_bounded_regular_file(args.anchor_manifest)
        report = _parse_projection_json(report_raw)
        projected = rule_inventory_projection(report, manifest_raw)
        encoded = encode_rules_v0(projected) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError, ValueError) as exc:
        print("could not project: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="project"), indent=2, sort_keys=True))
        return 2
    if args.json:
        assert encoded is not None
        _write_encoded(encoded)
    else:
        _render_rules_v0(projected)
    return 0


def _diff_text_value(value) -> str:
    """Quote values on one line; retain printable Unicode, escape controls."""
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True)
    return "".join(char if char.isprintable() else json.dumps(char)[1:-1]
                   for char in encoded)


def _render_diff_v0(projected: dict) -> None:
    """Render identity and row facts already projected; never infers a cause."""
    old_input = projected["old_input"]
    new_input = projected["new_input"]
    print("old_input: adequate=%s control_status=%s unproved=%s"
          % (str(old_input["adequate"]).lower(), old_input["control_status"],
             old_input["unproved"]))
    print("new_input: adequate=%s control_status=%s unproved=%s"
          % (str(new_input["adequate"]).lower(), new_input["control_status"],
             new_input["unproved"]))
    identity = projected["identity"]
    print("identity:")
    components = [(key, identity[key]) for key in ("manifest_sha256", "corpus_digest")]
    components.extend((key, identity["tool"][key]) for key in (
        "tool_version", "tool_commit", "tool_source_state", "tool_content_sha256"))
    for key, component in components:
        print("  %s: %s (old=%s new=%s)" % (
            key, component["status"], _diff_text_value(component["old"]),
            _diff_text_value(component["new"])))
    print("rows:")
    for row in projected["rows"]:
        print("%s presence=%s verdict_transition=%s acknowledgement_retired=%s changed_fields=%s"
              % (_diff_text_value(row["label"]), row["presence"], row["verdict_transition"],
                 str(row["acknowledgement_retired"]).lower(),
                 ",".join(row["changed_fields"])))
        for key in ["verdict"] + [key for key in row["changed_fields"] if key != "verdict"]:
            values = [(_diff_text_value(side[key]) if side is not None and key in side
                       else "<absent>") for side in (row["old"], row["new"])]
            print("  %s: %s -> %s" % (key, values[0], values[1]))
    counts = projected["counts"]
    print("counts: common=%d added=%d removed=%d verdict_changed=%d verdict_same=%d "
          "acknowledgement_retired=%d"
          % (counts["common"], counts["added"], counts["removed"],
             counts["verdict_changed"], counts["verdict_same"],
             counts["acknowledgement_retired"]))
    print("non_claims:")
    for claim in projected["non_claims"]:
        print("  %s" % claim)


def _diff_cli(args) -> int:
    """Early sibling path: read two report.v0 files. Never calls run()."""
    try:
        if args.manifest is not None:
            raise ManifestError("--diff does not take a positional manifest")
        old_raw = read_bounded_regular_file(args.diff[0])
        new_raw = read_bounded_regular_file(args.diff[1])
        old_report = _parse_projection_json(old_raw)
        new_report = _parse_projection_json(new_raw)
        projected = diff_reports(old_report, new_report)
        encoded = encode_diff_v0(projected) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError, ValueError) as exc:
        print("could not project: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="project"), indent=2, sort_keys=True))
        return 2
    if args.json:
        assert encoded is not None
        _write_encoded(encoded)
        return 0
    _render_diff_v0(projected)
    return 0


class _InvocationParser(argparse.ArgumentParser):
    """Keep explicit JSON refusal output available even before parsing succeeds."""

    def __init__(self, *, argv, **kwargs):
        super().__init__(**kwargs)
        # Only the literal option requests JSON on the parser-error path. After
        # `--` the same spelling is positional data, not an output-mode request.
        options = argv[:argv.index("--")] if "--" in argv else argv
        self._json_errors = "--json" in options

    def error(self, message):
        if self._json_errors:
            print(json.dumps(error_envelope(ValueError(message), operation="invoke"),
                             indent=2, sort_keys=True))
        super().error(message)

    def diff_report_path(self, value):
        # Some argparse versions consume a recognized option as a nargs=2
        # operand. Refuse the literal flag before Path normalizes ./--json,
        # which is still a valid way to name a report file.
        if self._json_errors and value == "--json":
            raise argparse.ArgumentTypeError("--json is an output option, not a report path")
        return Path(value)


def main() -> int:
    if len(sys.argv)>1 and sys.argv[1] in ('observe-prefix','observe-resume','observe-close'):
        from observation_session import observation_cli
        return observation_cli(sys.argv[1:])
    ap = _InvocationParser(argv=sys.argv[1:], description=__doc__.split("\n")[0])
    ap.add_argument("--version", action="store_true",
                    help="print tool version (and commit, if resolvable) and exit")
    ap.add_argument("manifest", type=Path, nargs="?")
    ap.add_argument("--json", action="store_true")
    projection = ap.add_mutually_exclusive_group()
    projection.add_argument("--survivors", action="store_true",
                            help="project survivors.v0 from an existing report.v0 file")
    projection.add_argument("--kills", action="store_true",
                            help="project kills.v0 from an existing report.v0 file")
    projection.add_argument("--rules", action="store_true",
                            help="project rules.v0 from an existing report.v0 and manifest")
    projection.add_argument("--diff", nargs=2, metavar=("OLD", "NEW"), type=ap.diff_report_path,
                            help="project diff.v0 from two existing report.v0 files")
    projection.add_argument("--inspect", type=Path,
                            help="inspect manifest declarations without binding or execution")
    ap.add_argument("--manifest", dest="anchor_manifest", type=Path,
                    help="digest-matched manifest for --rules or optional --survivors anchors")
    args = ap.parse_args()
    if args.inspect is not None:
        if args.version:
            exc = ManifestError("--inspect is mutually exclusive with --version")
            print("could not inspect: %s" % exc, file=sys.stderr)
            if args.json:
                print(json.dumps(error_envelope(exc, operation="inspect"), indent=2,
                                 sort_keys=True))
            return 2
        if args.manifest is not None or args.anchor_manifest is not None:
            exc = ManifestError("--inspect does not take a positional or --manifest input")
            print("could not inspect: %s" % exc, file=sys.stderr)
            if args.json:
                print(json.dumps(error_envelope(exc, operation="inspect"), indent=2,
                                 sort_keys=True))
            return 2
        return _inspect_cli(args)
    if args.version:
        print(format_tool_identity())
        return 0
    if args.anchor_manifest is not None and not (args.survivors or args.rules):
        exc = ManifestError("--manifest requires --survivors or --rules")
        print("could not measure: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="measure"), indent=2, sort_keys=True))
        return 2
    if args.survivors:
        return _survivors_cli(args, ap)
    if args.kills:
        return _kills_cli(args, ap)
    if args.rules:
        return _rules_cli(args)
    if args.diff is not None:
        return _diff_cli(args)
    if args.manifest is None:
        ap.error("manifest is required")
    try:
        rep = run(args.manifest, execution_profile="trusted-local")
        encoded = encode_report_v0(rep) if args.json else None
    except (ManifestError, OSError, json.JSONDecodeError, ReportEncodingError) as exc:
        print("could not measure: %s" % exc, file=sys.stderr)
        if args.json:
            print(json.dumps(error_envelope(exc, operation="measure"), indent=2, sort_keys=True))
        return 2

    if args.json:
        assert encoded is not None
        if hasattr(sys.stdout, "buffer"):
            sys.stdout.buffer.write(encoded)
        else:
            # StringIO and embedded callers may expose only a text stream.
            sys.stdout.write(encoded.decode("utf-8"))
    else:
        print(format_tool_identity(rep))
        for r in rep["mutants"]:
            print("%-22s %-9s %s" % (r["group"], r["verdict"], r["label"]))
            if r["verdict"] != "killed" or rep.get("runner") in ("process", "batch"):
                print("    %s" % r["how"])
        print()
        # Never a bare percentage. A score reported without its denominator and its
        # exclusions is a percentage target wearing a different coat: an author can
        # exclude almost everything and still print 100%.
        pct = ("no result" if rep["score_percent"] is None
               else "%.1f%%" % rep["score_percent"])
        # How many kills rest on something other than a moved declared outcome
        # (#244). Counted by the same classifier as --kills; what counts as
        # killed does not change.
        kill_counts = _kill_basis_counts(rep["mutants"], rep.get("runner"))
        pct += _kill_basis_text(kill_counts)
        print("%d of %d DECLARED in-scope rules killed (%s). %d declared equivalent, "
              "%d declared out of scope, %d unproved. %d rules declared in total."
              % (rep["killed"],
                 _scored_denominator(rep["killed"], rep["survived"], rep["silent"]), pct,
                 rep["equivalent"], rep["unexercised_out_of_scope"], rep["unproved"],
                 rep["declared_total"]))
        also_line = _kill_also_terminated_line(kill_counts)
        if also_line is not None:
            print(also_line)
        if rep["score_percent"] is not None:
            print("This is %.1f%% of what the AUTHOR DECLARED, not of the rules the "
                  "implementation has. A rule nobody declared is invisible to this check."
                  % rep["score_percent"])
        if rep["unexercised_out_of_scope"]:
            print("out of scope (%d, each with a stated reason): real gaps in what the corpus "
                  "covers, not holes in what it claims" % rep["unexercised_out_of_scope"])
        if rep["out_of_scope_ratio"] is not None and rep["out_of_scope_ratio"] > 1.0:
            print("NOTE: more rules are excluded than measured (ratio %.2f). The score is real "
                  "but it is a statement about a minority of the declared rules."
                  % rep["out_of_scope_ratio"])
        for f in rep["failures"]:
            print("FAIL: %s" % f)
        if rep.get("known_holes"):
            # Louder than the pass line. Not pinned to the corpus: pinned to a value
            # another tool has to keep honest, which is what the text below says.
            print("%d KNOWN HOLE(S) against the DECLARED digest %s. These are rules the "
                  "corpus DOES claim and does NOT exercise."
                  % (rep["known_holes"], rep.get("corpus_digest")))
            print("  The digest is a value READ FROM A FILE THE MANIFEST NAMES. It is not "
                  "recomputed from the vectors, so these acknowledgements expire only if that "
                  "file is kept honest.")
            if rep.get("acknowledged_digests", 0) > 1:
                print("  %d digests carry acknowledgements in this manifest. Entries for a "
                      "digest that is not the declared one are not in force, but pre-declaring "
                      "future digests is how this expiry gets bypassed."
                      % rep["acknowledged_digests"])
            if rep.get("hole_ratio") is not None and rep["hole_ratio"] > 1.0:
                print("  more rules are acknowledged as holes than are measured (ratio %.2f). "
                      "The score printed above is a statement about a minority of the "
                      "declared rules." % rep["hole_ratio"])
        if rep["adequate"]:
            if rep["out_of_scope_ratio"] is not None and rep["out_of_scope_ratio"] > 1.0:
                # The closing line is what gets quoted. It may not read as unqualified
                # success when most declared rules were excluded from the measurement.
                print("mutation-adequacy check passed for the DECLARED IN-SCOPE rules only "
                      "(%d of %d rules declared here were excluded from it)"
                      % (rep["unexercised_out_of_scope"], rep["declared_total"]))
            else:
                suffix = ""
                if rep.get("known_holes"):
                    suffix = (" for the rules still measured -- %d are acknowledged holes"
                              % rep["known_holes"])
                print("mutation-adequacy check passed: every non-equivalent mutant is killed"
                      + suffix)

    return 0 if rep["adequate"] else 1


if __name__ == "__main__":
    sys.exit(main())
