#!/usr/bin/env python3
"""
Inject the tamper-evident manifest-digest binding event into a trace.json
produced under Arm C.

Background. In Arm C the child process (our workload) runs inside
`assay runner-spike`, which only writes the `.tar.gz` archive AFTER the
child exits. The workload therefore cannot compute the manifest digest
in-process; the archive does not exist at trace-flush time. This script
runs as a post-step after `assay runner-spike` returns, computes the
manifest digest from the now-finalized archive's exact `manifest.json`
bytes, and attaches an `assay.archive.created` event to the root
`assay.runner.measured_run` span in the trace.json.

Stdlib only. The digest format and event shape are identical to what
`workload/src/manifest-binding.ts` produces in the locally-runnable
Arm B dual-simulation; the two paths must stay byte-compatible because
they feed into the same `compare.py` matrix.

Usage:
    python3 bind-archive.py \\
        --trace runs/run-id/trace.json \\
        --archive runs/run-id/archive.tar.gz \\
        [--root-span-name assay.runner.measured_run]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tarfile
from pathlib import Path

DEFAULT_ROOT_SPAN_NAME = "assay.runner.measured_run"
MANIFEST_PATH = "manifest.json"
EVENT_NAME = "assay.archive.created"
SCHEMA = "assay.runner.archive_manifest.v0"


def manifest_bytes_from_archive(archive: Path) -> bytes:
    """
    Return the **exact** manifest.json bytes from the archive (.tar.gz or
    extracted directory). Never re-serialize the parsed JSON: the digest
    must be over the bytes as written by `assay runner-spike` so it lines
    up with what `compare.py` computes on the same archive.
    """
    if archive.is_dir():
        return (archive / MANIFEST_PATH).read_bytes()
    with tarfile.open(archive, "r:*") as tf:
        member = tf.extractfile(MANIFEST_PATH)
        if member is None:
            raise FileNotFoundError(
                f"{MANIFEST_PATH} not found in archive: {archive}"
            )
        return member.read()


def sha256_of(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def now_unix_nanos() -> str:
    import time

    return str(int(time.time_ns()))


def inject_binding_event(
    trace_doc: dict,
    *,
    root_span_name: str,
    manifest_digest: str,
    archive_path: str,
    manifest_bytes: int,
) -> bool:
    """Return True when the binding event was injected, False otherwise."""
    event = {
        "name": EVENT_NAME,
        "timeUnixNano": now_unix_nanos(),
        "attributes": [
            {"key": "assay.archive.schema", "value": {"stringValue": SCHEMA}},
            {
                "key": "assay.archive.manifest_digest",
                "value": {"stringValue": manifest_digest},
            },
            {
                "key": "assay.archive.path",
                "value": {"stringValue": archive_path},
            },
            {
                "key": "assay.archive.manifest_bytes",
                "value": {"intValue": str(manifest_bytes)},
            },
            {
                "key": "assay.archive.source",
                "value": {"stringValue": "post_hoc_bind"},
            },
        ],
    }

    for resource in trace_doc.get("resourceSpans", []):
        for scope in resource.get("scopeSpans", []):
            for span in scope.get("spans", []):
                if span.get("name") == root_span_name:
                    span.setdefault("events", []).append(event)
                    return True
    return False


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument(
        "--root-span-name",
        default=DEFAULT_ROOT_SPAN_NAME,
        help=(
            "Span name to attach the binding event to. Defaults to "
            f"`{DEFAULT_ROOT_SPAN_NAME}`, which matches workload.ts."
        ),
    )
    args = parser.parse_args(argv)

    if not args.trace.exists():
        sys.stderr.write(f"error: trace path not found: {args.trace}\n")
        return 2
    if not args.archive.exists():
        sys.stderr.write(f"error: archive path not found: {args.archive}\n")
        return 2

    manifest_bytes = manifest_bytes_from_archive(args.archive)
    digest = sha256_of(manifest_bytes)

    trace_doc = json.loads(args.trace.read_text(encoding="utf-8"))
    injected = inject_binding_event(
        trace_doc,
        root_span_name=args.root_span_name,
        manifest_digest=digest,
        archive_path=str(args.archive),
        manifest_bytes=len(manifest_bytes),
    )
    if not injected:
        sys.stderr.write(
            f"error: no span named `{args.root_span_name}` found in "
            f"{args.trace}; nothing injected\n"
        )
        return 3

    args.trace.write_text(
        json.dumps(trace_doc, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"bound {args.archive} ({len(manifest_bytes)} bytes, {digest}) "
        f"to {args.trace} on span `{args.root_span_name}`"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
