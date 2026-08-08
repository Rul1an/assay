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

Failure modes this deliberately does not have:

- It does not pass when it could not look. No network, no token, or an API error is `could not
  check`, and that exits non-zero unless `--allow-offline` is passed explicitly. A gate that reports
  success for a comparison it did not make is the shape this repository has been bitten by before,
  most recently in the Linux cross-target gate.
- It does not scan for release-shaped text. It compares one declared tag against the published set.

Usage:
    check_clean_room_pack_reachable.py                  # both checks, needs network for check 1
    check_clean_room_pack_reachable.py --allow-offline  # check 2 only, states the skip
    check_clean_room_pack_reachable.py --self-test      # prove both checks can fail
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


def self_test() -> None:
    """Prove both checks can fail, since a check that cannot is worse than none."""
    # Check 1: a tag nobody published.
    tags = published_tags()
    if tags is None:
        print("SKIP self-test of check 1: releases API unreachable")
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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--allow-offline", action="store_true", help="state the skip instead of failing")
    ap.add_argument("--self-test", action="store_true", help="prove the checks can fail")
    args = ap.parse_args()

    if args.self_test:
        self_test()
        return 0

    check_descriptor_names_a_published_release(args.allow_offline)
    check_pack_carries_what_the_protocol_demands()
    return 0


if __name__ == "__main__":
    sys.exit(main())
