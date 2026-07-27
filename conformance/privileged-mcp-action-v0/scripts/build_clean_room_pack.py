#!/usr/bin/env python3
"""Build a deterministic, inputs-only clean-room conformance pack."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re

from bounded_process import ProcessLimitError, run_bounded
from pack_format import (
    clean_room_descriptor,
    clean_room_spec,
    deterministic_tar_gz,
    opaque_case_id,
    opaque_run_id,
    ordered_vectors,
    rewrite_bundle_stream_identity,
)


PACK_ROOT = "privileged-mcp-action-v0"
PACK_SCHEMA = "assay.privileged_mcp_action.clean_room_pack.v0"
PROFILE = "privileged-mcp-action/v0"
CORPUS_PATH = "conformance/privileged-mcp-action-v0"
SPEC_PATH = "docs/profiles/privileged-mcp-action/v0.md"
DESCRIPTOR_PATH = f"{CORPUS_PATH}/descriptor.json"
MANIFEST_PATH = f"{CORPUS_PATH}/MANIFEST.json"
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
MAX_GIT_BLOB_BYTES = 32 * 1024 * 1024
MAX_GIT_ERROR_BYTES = 64 * 1024

README = """\
# Privileged MCP Action v0 clean-room pack

This pack contains the specification, machine-readable descriptor, and fourteen
opaque evidence-bundle cases. It deliberately omits expected outcomes, semantic
case names, the vector generator, and Assay's implementation.

Implement the profile from `spec.md` and `descriptor.json` before running a
scorer. Your implementation should accept one bundle path and emit exactly one
JSON report in the report shape defined by the profile. The scorer and canonical
expectations are post-implementation reconciliation surfaces, not authorship
inputs.

`cases.json` binds each opaque case to its bytes and records the declared source
commit, source-corpus digest, and rendered-set digest. It does not contain expected
results. Verify the release attestation separately before relying on pack provenance.
"""


def sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def git_bytes(repo_root: Path, commit: str, path: str) -> bytes:
    result = run_bounded(
        ["git", "show", f"{commit}:{path}"],
        cwd=repo_root,
        timeout_seconds=30,
        stdout_limit=MAX_GIT_BLOB_BYTES,
        stderr_limit=MAX_GIT_ERROR_BYTES,
    )
    if result.returncode != 0:
        raise ValueError(
            f"cannot read {path} at {commit}: "
            f"{result.stderr.decode('utf-8', errors='replace').strip()}"
        )
    return result.stdout


def build_pack(repo_root: Path, source_commit: str) -> bytes:
    if not FULL_SHA.fullmatch(source_commit):
        raise ValueError("--source-commit must be a full lowercase 40-hex Git commit")

    spec = clean_room_spec(git_bytes(repo_root, source_commit, SPEC_PATH))
    descriptor = clean_room_descriptor(
        git_bytes(repo_root, source_commit, DESCRIPTOR_PATH)
    )
    manifest_bytes = git_bytes(repo_root, source_commit, MANIFEST_PATH)
    manifest = json.loads(manifest_bytes)

    cases = []
    seen_source_digests: set[str] = set()
    packed_bundle_by_digest: dict[str, bytes] = {}
    for index, vector in enumerate(ordered_vectors(manifest["vectors"]), start=1):
        bundle = git_bytes(
            repo_root,
            source_commit,
            f"{CORPUS_PATH}/{vector['file']}",
        )
        digest = sha256(bundle)
        if digest != vector["sha256"]:
            raise ValueError(f"source vector digest mismatch for {vector['file']}")
        if digest in seen_source_digests:
            raise ValueError(f"duplicate source vector bytes: {digest}")
        seen_source_digests.add(digest)
        case_id = opaque_case_id(index)
        bundle = rewrite_bundle_stream_identity(
            bundle,
            opaque_run_id(index),
        )
        digest = sha256(bundle)
        if digest in packed_bundle_by_digest:
            raise ValueError(f"duplicate rendered case bytes: {digest}")
        packed_bundle_by_digest[digest] = bundle
        cases.append(
            {
                "id": case_id,
                "file": f"cases/{case_id}.bundle.tar.gz",
                "sha256": digest,
            }
        )

    case_index = {
        "schema": PACK_SCHEMA,
        "profile": PROFILE,
        "declared_source_commit": source_commit,
        "source_corpus_digest": manifest["corpus_digest"],
        "rendered_set_digest": sha256(
            "".join(case["sha256"] + "\n" for case in cases).encode()
        ),
        "case_count": len(cases),
        "cases": cases,
    }
    files = {
        "README.md": README.encode(),
        "cases.json": (
            json.dumps(case_index, indent=2, sort_keys=True) + "\n"
        ).encode(),
        "descriptor.json": descriptor,
        "spec.md": spec,
    }
    files.update(
        {
            case["file"]: packed_bundle_by_digest[case["sha256"]]
            for case in cases
        }
    )
    return deterministic_tar_gz(
        {f"{PACK_ROOT}/{name}": data for name, data in files.items()}
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        pack = build_pack(args.repo_root.resolve(), args.source_commit)
    except (
        OSError,
        EOFError,
        ProcessLimitError,
        RecursionError,
        ValueError,
        KeyError,
        json.JSONDecodeError,
    ) as error:
        print(f"clean-room pack build failed: {error}", file=__import__("sys").stderr)
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(pack)
    print(f"wrote {args.output} ({sha256(pack)})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
