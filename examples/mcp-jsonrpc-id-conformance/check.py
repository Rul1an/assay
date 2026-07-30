#!/usr/bin/env python3
"""Check whether an MCP error response can satisfy both MCP and JSON-RPC 2.0 at once.

MCP 2026-07-28 states that all messages MUST follow JSON-RPC 2.0. JSON-RPC 2.0 states that a
Response object's `id` is REQUIRED, and that it MUST be Null when the id could not be read from
the request. MCP's own schema makes `id` optional on an error response and gives it a type that
excludes null. Those two rules describe the same situation and cannot both be satisfied.

This script does not validate JSON Schema in general. It asserts two narrow structural facts about
the published MCP schema and then classifies each vector against both rule sets. Standard library
only, no network, no dependencies: the whole point is that a reader can rerun it without trusting
this repository.

The exit code is the verdict:

    0  the divergence is present, exactly as documented here
    1  a vector did not classify as its filename claims (this file is wrong, or the vectors are)
    2  the divergence is GONE upstream, so this finding is resolved and this example is stale
    3  the schema could not be read or does not match the pinned digest

Usage:
    python3 check.py --schema path/to/schema.json
"""

import argparse
import hashlib
import json
import pathlib
import sys

# Pinned upstream input. Both files were read at this commit and the digests recomputed from the
# bytes served for it, not from a branch tip that can move underneath the claim.
UPSTREAM_REPO = "modelcontextprotocol/modelcontextprotocol"
UPSTREAM_COMMIT = "271ecc9accafdd9b83a3c869fa67c22953b2af80"
SCHEMA_PATH = "schema/2026-07-28/schema.json"
SCHEMA_SHA256 = "ef70b61f99b6d2e5e3b46863822eab08dff6a45bedc7a08914e0e5b133f40203"

OK, VECTOR_MISMATCH, RESOLVED_UPSTREAM, INPUT_UNUSABLE = 0, 1, 2, 3


def load_schema(path: pathlib.Path):
    """Read the pinned schema, refusing anything whose bytes are not the ones this was written against."""
    try:
        raw = path.read_bytes()
    except OSError as exc:
        print(f"[input] cannot read {path}: {exc}")
        return None
    digest = hashlib.sha256(raw).hexdigest()
    if digest != SCHEMA_SHA256:
        # Not a failure of the finding. A different revision is simply a different subject, and
        # silently checking it would report a result about bytes nobody pinned.
        print(f"[input] schema digest {digest}")
        print(f"[input] expected      {SCHEMA_SHA256}")
        print(f"[input] this checks {UPSTREAM_COMMIT[:12]} only; rerun the finding against a newer revision deliberately")
        return None
    return json.loads(raw)


def read_mcp_rules(schema):
    """Extract the two facts the finding rests on, straight from the published schema."""
    defs = schema.get("$defs") or schema.get("definitions") or {}
    err = defs.get("JSONRPCErrorResponse", {})
    request_id = defs.get("RequestId", {})
    id_required = "id" in (err.get("required") or [])
    id_types = request_id.get("type")
    id_types = id_types if isinstance(id_types, list) else [id_types]
    null_allowed = "null" in id_types
    return id_required, null_allowed, id_types


def classify(message, id_required_by_mcp, null_allowed_by_mcp):
    """Say, for one message, which of the two rule sets it satisfies.

    JSON-RPC 2.0 on a Response object: `id` is REQUIRED, and MUST be Null when the request's id
    could not be read. So an absent id is non-conforming, and a null id is the prescribed form for
    exactly this case.
    """
    has_id = "id" in message
    id_is_null = message.get("id", ...) is None

    jsonrpc_ok = has_id  # REQUIRED, present in any form
    mcp_ok = (has_id or not id_required_by_mcp) and not (id_is_null and not null_allowed_by_mcp)
    return jsonrpc_ok, mcp_ok


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema", required=True, type=pathlib.Path)
    parser.add_argument("--vectors", type=pathlib.Path, default=pathlib.Path(__file__).parent / "vectors")
    args = parser.parse_args()

    schema = load_schema(args.schema)
    if schema is None:
        return INPUT_UNUSABLE

    id_required, null_allowed, id_types = read_mcp_rules(schema)
    print(f"MCP JSONRPCErrorResponse requires 'id' : {id_required}")
    print(f"MCP RequestId permits null             : {null_allowed}  (type: {id_types})")
    print()

    if id_required or null_allowed:
        # Either change closes the gap: requiring id, or permitting null, would let the
        # JSON-RPC prescribed form through.
        print("RESOLVED: the schema no longer forces the divergence. This example is stale.")
        return RESOLVED_UPSTREAM

    expected = {
        "n1-jsonrpc-conforming-error-response.json": (True, False),
        "n2-mcp-conforming-error-response.json": (False, True),
        "p1-both-conforming-error-response.json": (True, True),
    }

    failures = 0
    for name in sorted(expected):
        message = json.loads((args.vectors / name).read_text())
        got = classify(message, id_required, null_allowed)
        want = expected[name]
        mark = "ok " if got == want else "BAD"
        if got != want:
            failures += 1
        print(f"[{mark}] {name}")
        print(f"        json-rpc 2.0 conforming: {got[0]}   mcp 2026-07-28 conforming: {got[1]}")

    print()
    if failures:
        print(f"{failures} vector(s) did not classify as claimed")
        return VECTOR_MISMATCH

    print("For a request whose id could not be read, the form JSON-RPC 2.0 prescribes is rejected")
    print("by the MCP schema, and the form the MCP schema permits is rejected by JSON-RPC 2.0.")
    print("No message satisfies both.")
    return OK


if __name__ == "__main__":
    sys.exit(main())
