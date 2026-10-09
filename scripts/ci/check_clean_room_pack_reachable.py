#!/usr/bin/env python3
"""The clean-room pack a reproducer can obtain must exist and must carry what the protocol demands.

Two checks over one seam, filed as #2153 after that seam was open from 7 August to 8 August 2026
with nothing watching it. During that window `candidate-release.json` named `candidate.4`, the pack
builder already produced the canonicalization vectors, and the only artifact anyone could download
was `candidate.3` without them. Correct code, correct descriptor, unpublished result, no failure.

**Check 1 — the descriptor names a release that exists.** A tag in `candidate-release.json` that has
no published release means the repository documents a pack nobody can fetch.

**Check 2 — the pack carries every path the protocol tells a reproducer to run.** The required paths
are read out of `CONFORMANCE-PROTOCOL.md` rather than listed here, because a hard-coded list is a
third place the requirement can live and the first one to go stale. If the protocol stops naming a
file, this check stops requiring it, which is the behaviour we want.

**Check 3 — the action the protocol pins scores the pack the protocol names.** The protocol pins the
composite action to a full commit, separately from the release tag, and the action runs the scorer
and reads `MANIFEST.json` from that commit, not from the release. From 8 August to 9 October 2026 the
pin was `16ea2b84`, a commit from before `candidate.4` added the two canonicalization members to the
pack, so its loader rejected every `candidate.4` pack with `pack contains surplus members` and never
invoked the candidate. The protocol said the pin "does not alter the invoked scoring path"; nothing
checked that sentence, and the first outside reproducer was the one who found it (#1840). This
check compares the scoring path at the pinned commit (the action, the scorer scripts and the
manifest, by git object id) with the same paths at the commit the declared release tag resolves to.

Failure modes this deliberately does not have:

- It does not pass when it could not look. No network, no token, or an API error is `could not
  check`, and that exits non-zero unless `--allow-offline` is passed explicitly. A gate that reports
  success for a comparison it did not make is the shape this repository has been bitten by before,
  most recently in the Linux cross-target gate.
- It does not scan for release-shaped text. It compares one declared tag against the published set.

Usage:
    check_clean_room_pack_reachable.py                  # all checks; check 1 needs network, check 3 history
    check_clean_room_pack_reachable.py --allow-offline  # states each skip instead of failing
    check_clean_room_pack_reachable.py --self-test      # prove every check can fail
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

REPO = "Rul1an/assay"
ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "conformance" / "privileged-mcp-action-v0"
DESCRIPTOR = CORPUS / "candidate-release.json"
PROTOCOL = CORPUS / "CONFORMANCE-PROTOCOL.md"
BUILDER = CORPUS / "scripts" / "build_clean_room_pack.py"

# The action reads every one of these from the commit it is pinned to, so together they are the
# invoked scoring path. Compared as whole trees: a file added to `scripts/` that the scorer does not
# import still fails the check, which errs towards a re-pin rather than towards a silent mismatch.
SCORING_PATHS = (
    ".github/actions/privileged-mcp-action-conformance",
    "conformance/privileged-mcp-action-v0/scripts",
    "conformance/privileged-mcp-action-v0/MANIFEST.json",
)
PINNED_ACTION = re.compile(
    r"uses:\s*Rul1an/assay/\.github/actions/privileged-mcp-action-conformance@([0-9a-f]+)"
)
# The pin that shipped the defect check 3 exists for. The self-test runs it as the negative control.
DEFECTIVE_PIN = "16ea2b84e472412e3e5c4d9dcabff61b7fac72f8"

# A backticked filename with an extension, optionally in a directory.
INSTRUCTED_PATH = re.compile(r"`([A-Za-z0-9_.\-]+(?:/[A-Za-z0-9_.\-]+)*\.[A-Za-z0-9]+)`")

# The protocol names files in both directions: some a reproducer MUST run or read, and some that
# disqualify the run if read before freezing. A regex over every backticked path cannot tell those
# apart, so extraction is scoped to the authorship-order steps and the disqualifying paragraph is
# removed first. Keying on the section rather than on a list of known filenames is what makes a
# newly instructed file required the day it is written.
DISQUALIFYING_MARKER = "Reading Assay's verifier"


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def declared_tag() -> str:
    data = json.loads(DESCRIPTOR.read_text(encoding="utf-8"))
    tag = data.get("tag")
    if not isinstance(tag, str) or not tag:
        fail(f"{DESCRIPTOR.relative_to(ROOT)} has no usable `tag`")
    return tag


def published_tags() -> list[str] | None:
    """Every published release tag, or None when the question could not be asked."""
    try:
        out = subprocess.run(
            ["gh", "api", f"repos/{REPO}/releases", "--paginate", "--jq", ".[].tag_name"],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    return [line.strip() for line in out.stdout.splitlines() if line.strip()]


def instructed_paths() -> set[str]:
    text = PROTOCOL.read_text(encoding="utf-8")
    try:
        section = text.split("## Authorship order", 1)[1].split("\n## ", 1)[0]
    except IndexError:
        fail(f"{PROTOCOL.relative_to(ROOT)} has no `## Authorship order` section to read")
    # Drop the paragraph that names what must NOT be read before freezing, so a disqualifying file
    # is never turned into a required one.
    section = section.split(DISQUALIFYING_MARKER, 1)[0]
    found = set(INSTRUCTED_PATH.findall(section))
    # Non-vacuous: the protocol has always named at least the canonicalization vectors. An empty set
    # would make check 2 pass by reading nothing, which is the failure this file exists to prevent.
    if not found:
        fail(
            f"{PROTOCOL.relative_to(ROOT)} named no reproducer-run paths; the extractor stopped "
            "matching rather than the protocol stopping requiring"
        )
    return found


def source_commit() -> str:
    out = subprocess.run(
        ["git", "rev-parse", "HEAD"], capture_output=True, text=True, cwd=ROOT, check=False
    )
    return out.stdout.strip() or "0" * 40


def built_pack_entries() -> set[str]:
    """Paths inside a pack built from the working tree, relative to its top directory."""
    with tempfile.TemporaryDirectory() as tmp:
        out = subprocess.run(
            [
                sys.executable,
                str(BUILDER),
                "--repo-root",
                str(ROOT),
                "--source-commit",
                source_commit(),
                "--output",
                str(Path(tmp) / "pack.tar.gz"),
            ],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        if out.returncode != 0:
            fail(f"the pack builder failed, so the pack could not be checked:\n{out.stderr}")
        tarballs = list(Path(tmp).rglob("*.tar.gz"))
        if not tarballs:
            fail("the pack builder produced no tarball")
        with tarfile.open(tarballs[0]) as tf:
            names = tf.getnames()
    entries = set()
    for name in names:
        parts = name.split("/", 1)
        if len(parts) == 2:
            entries.add(parts[1])
    return entries


def git_out(*args: str) -> str | None:
    out = subprocess.run(["git", *args], capture_output=True, text=True, cwd=ROOT, check=False)
    return out.stdout.strip() if out.returncode == 0 else None


def pinned_action_commit() -> str:
    pins = set(PINNED_ACTION.findall(PROTOCOL.read_text(encoding="utf-8")))
    if not pins:
        fail(f"{PROTOCOL.relative_to(ROOT)} pins no conformance action; nothing to compare")
    if len(pins) > 1:
        fail(f"{PROTOCOL.relative_to(ROOT)} pins the action to more than one commit: {sorted(pins)}")
    (pin,) = pins
    if not re.fullmatch(r"[0-9a-f]{40}", pin):
        fail(f"the action pin `{pin}` is not a full 40-character commit")
    return pin


def release_source_commit(tag: str) -> str | None:
    """The commit `tag` resolves to, from local tags first and the API second; None if neither."""
    local = git_out("rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}")
    if local:
        return local
    try:
        out = subprocess.run(
            ["gh", "api", f"repos/{REPO}/commits/{tag}", "--jq", ".sha"],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    sha = out.stdout.strip()
    return sha if out.returncode == 0 and re.fullmatch(r"[0-9a-f]{40}", sha) else None


def scoring_path_differences(pin: str, source: str) -> list[str] | None:
    """Scoring paths whose object differs between the two commits; None if either is not local."""
    for commit in (pin, source):
        if git_out("cat-file", "-e", f"{commit}^{{commit}}") is None:
            return None
    return [
        path
        for path in SCORING_PATHS
        if git_out("rev-parse", "--verify", "--quiet", f"{pin}:{path}")
        != git_out("rev-parse", "--verify", "--quiet", f"{source}:{path}")
    ]


def check_pinned_action_scores_the_released_pack(allow_offline: bool) -> None:
    tag = declared_tag()
    pin = pinned_action_commit()
    source = release_source_commit(tag)
    differences = None if source is None else scoring_path_differences(pin, source)
    if differences is None:
        if allow_offline:
            print(f"SKIP: could not resolve `{tag}` and the pin `{pin[:12]}` locally; not compared")
            return
        fail(
            f"could not resolve `{tag}` and the action pin `{pin[:12]}` to local commits, so this "
            "says nothing about whether the pinned action can score the release. Fetch full "
            "history and tags, or pass --allow-offline to state that deliberately."
        )
    if differences:
        fail(
            f"the protocol pins the action to `{pin[:12]}`, whose scoring path differs from the "
            f"`{tag}` source `{source[:12]}` at: {', '.join(differences)}. A reproducer following "
            "the protocol would score the release with code it was not built for. Re-pin the "
            "action to the release source."
        )
    print(f"ok: the action pinned at `{pin[:12]}` runs the scoring path `{tag}` was built from")


def check_descriptor_names_a_published_release(allow_offline: bool) -> None:
    tag = declared_tag()
    tags = published_tags()
    if tags is None:
        if allow_offline:
            print(f"SKIP: could not reach the releases API; `{tag}` not verified")
            return
        fail(
            "could not reach the releases API, so this says nothing about whether "
            f"`{tag}` is published. Pass --allow-offline to state that deliberately."
        )
    if tag not in tags:
        fail(
            f"`candidate-release.json` names `{tag}`, which has no published release. "
            "The repository describes a pack nobody can fetch. Dispatch "
            "`privileged-mcp-action-pack-release.yml`."
        )
    print(f"ok: `{tag}` is published")


def check_pack_carries_what_the_protocol_demands() -> None:
    required = instructed_paths()
    entries = built_pack_entries()
    missing = sorted(p for p in required if p not in entries)
    if missing:
        fail(
            "the protocol instructs a reproducer to run paths the built pack does not contain: "
            + ", ".join(missing)
        )
    print(f"ok: the pack carries all {len(required)} path(s) the protocol names")


def self_test(allow_offline: bool) -> None:
    """Prove both checks can fail, since a check that cannot is worse than none.

    A skip here is a failure unless it is asked for. The first CI wiring of this file set the token
    on the real check and not on this step, so the self-test reported `SKIP ... releases API
    unreachable` and exited zero two seconds before the real check reached the same API and passed.
    Green, and it had verified nothing about check 1. That is the exact shape this file exists to
    catch, and a self-test allowed to pass while skipping its own assertion is the worst place to
    have it.
    """
    # Check 1: a tag nobody published.
    tags = published_tags()
    if tags is None:
        if not allow_offline:
            fail(
                "the self-test could not reach the releases API, so it did not prove check 1 can "
                "fail. Provide GH_TOKEN, or pass --allow-offline to state the gap deliberately."
            )
        print("SKIP self-test of check 1: releases API unreachable (asked for)")
    else:
        assert "privileged-mcp-action-v0-candidate.99999" not in tags
        print("ok self-test: an unpublished tag is not in the published set")

    # Check 2: the extractor finds the file the incident was about, and a path the protocol does not
    # name is not required. Both directions, because over-requiring is as wrong as under-requiring.
    required = instructed_paths()
    assert any("rfc8785" in p for p in required), (
        "the protocol no longer names the canonicalization vectors; if that is deliberate, this "
        "assertion is the thing to update, and if it is not, the protocol regressed"
    )
    # Both directions. Over-requiring is as wrong as under-requiring: turning a disqualifying file
    # into a required one would demand the pack ship the very material the clean-room property
    # depends on withholding.
    for forbidden in ("gen_vectors.py", "MANIFEST.json"):
        assert forbidden not in required, (
            f"{forbidden} is named as disqualifying to read, and must never become a required "
            "pack entry"
        )
    assert "not/a/real-file.json" not in required
    print(
        f"ok self-test: extractor found {len(required)} instructed path(s) "
        f"({', '.join(sorted(required))}), and no disqualifying or phantom path"
    )

    # Check 3: the pin that shipped the defect must be reported as different from the release
    # source, and the release source must be reported as identical to itself. Running the real
    # historical pin is the point: a comparison that cannot tell those two apart is not a check.
    tag = declared_tag()
    source = release_source_commit(tag)
    defective = None if source is None else scoring_path_differences(DEFECTIVE_PIN, source)
    if defective is None:
        if not allow_offline:
            fail(
                f"the self-test could not resolve `{tag}` and `{DEFECTIVE_PIN[:12]}` locally, so "
                "it did not prove check 3 can fail. Fetch full history and tags, or pass "
                "--allow-offline to state the gap deliberately."
            )
        print("SKIP self-test of check 3: commits not available locally (asked for)")
        return
    assert "conformance/privileged-mcp-action-v0/scripts" in defective, (
        f"the pin that rejected every `{tag}` pack compares equal to its source; check 3 is blind"
    )
    assert scoring_path_differences(source, source) == [], "a commit differs from itself"
    print(
        f"ok self-test: the defective pin `{DEFECTIVE_PIN[:12]}` differs from the `{tag}` source "
        f"at {', '.join(defective)}, and the source matches itself"
    )


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--allow-offline", action="store_true", help="state the skip instead of failing")
    ap.add_argument("--self-test", action="store_true", help="prove the checks can fail")
    args = ap.parse_args()

    if args.self_test:
        self_test(args.allow_offline)
        return 0

    check_descriptor_names_a_published_release(args.allow_offline)
    check_pack_carries_what_the_protocol_demands()
    check_pinned_action_scores_the_released_pack(args.allow_offline)
    return 0


if __name__ == "__main__":
    sys.exit(main())
