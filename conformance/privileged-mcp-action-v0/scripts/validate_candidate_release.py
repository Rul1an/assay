#!/usr/bin/env python3
"""Validate that a clean-room candidate release names the current corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


SCHEMA = "assay.privileged_mcp_action.candidate_release.v0"
TAG = re.compile(r"^privileged-mcp-action-v0-candidate\.[1-9][0-9]*$")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")


def manifest_corpus_digest(manifest: object) -> str:
    if not isinstance(manifest, dict) or not isinstance(manifest.get("vectors"), list):
        raise ValueError("manifest must contain a vectors list")
    digests = []
    for vector in manifest["vectors"]:
        if (
            not isinstance(vector, dict)
            or not isinstance(vector.get("sha256"), str)
            or not DIGEST.fullmatch(vector["sha256"])
        ):
            raise ValueError("manifest vector must contain a sha256 digest")
        digests.append(vector["sha256"] + "\n")
    return "sha256:" + hashlib.sha256("".join(digests).encode()).hexdigest()


def validate(candidate_path: Path, manifest_path: Path) -> dict[str, object]:
    candidate = json.loads(candidate_path.read_text())
    manifest = json.loads(manifest_path.read_text())
    if not isinstance(candidate, dict) or set(candidate) != {
        "schema",
        "tag",
        "case_count",
        "corpus_digest",
    }:
        raise ValueError("candidate release must contain exactly the registered fields")
    if candidate["schema"] != SCHEMA:
        raise ValueError("unexpected candidate release schema")
    if not isinstance(candidate["tag"], str) or not TAG.fullmatch(candidate["tag"]):
        raise ValueError("invalid candidate release tag")
    computed_digest = manifest_corpus_digest(manifest)
    if manifest.get("corpus_digest") != computed_digest:
        raise ValueError("manifest corpus digest does not match ordered vector digests")
    if (
        not isinstance(candidate["case_count"], int)
        or isinstance(candidate["case_count"], bool)
        or candidate["case_count"] != len(manifest["vectors"])
    ):
        raise ValueError("candidate case count does not match manifest")
    if (
        not isinstance(candidate["corpus_digest"], str)
        or not DIGEST.fullmatch(candidate["corpus_digest"])
        or candidate["corpus_digest"] != manifest["corpus_digest"]
    ):
        raise ValueError("candidate corpus digest does not match manifest")
    return candidate


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        candidate = validate(args.candidate, args.manifest)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"candidate release validation failed: {error}", file=sys.stderr)
        return 2
    if args.github_output is not None:
        with args.github_output.open("a") as output:
            print(f"tag={candidate['tag']}", file=output)
            print(f"case-count={candidate['case_count']}", file=output)
            print(f"corpus-digest={candidate['corpus_digest']}", file=output)
    print("candidate-release=valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
