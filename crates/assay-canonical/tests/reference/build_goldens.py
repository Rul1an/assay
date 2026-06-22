#!/usr/bin/env python3
"""Build the golden vectors for the semantic-digest contract (deterministic). Run: python3 build_goldens.py

Each golden carries a record + a reordered variant, the digest under the contract (set-paths normalized),
and the digest of the reordered variant. Set classes must collapse to one digest under the contract; ordered
classes must not. `old_digest` is the digest with NO normalization (arrival order) to show where adoption
moves a digest (and therefore needs a profile bump + an old/new golden pair).
"""

from __future__ import annotations

import json
import os

import canonical as C

HERE = os.path.dirname(os.path.abspath(__file__))

_CASES = [
    {
        "id": "env_filtered_passed_keys_set",
        "schema": "assay-evidence/PayloadEnvFiltered",
        "class": "set",
        "set_paths": [["passed_keys"], ["dropped_keys"]],
        "record": {
            "passed_keys": ["PATH", "HOME", "PATH"],
            "dropped_keys": ["AWS_SECRET", "TOKEN"],
        },
        "reordered": {"passed_keys": ["HOME", "PATH"], "dropped_keys": ["TOKEN", "AWS_SECRET"]},
        "note": "Vec<String> env-key sets: order/dups carry no meaning -> normalize. Producer-unsorted "
        "here, so the digest moves on adoption -> profile bump + old/new golden.",
    },
    {
        "id": "tool_classes_set_producer_sorted",
        "schema": "assay.tool.decision/DecisionData",
        "class": "set",
        "set_paths": [["tool_classes"], ["matched_tool_classes"]],
        "record": {
            "tool_classes": ["fs.read", "net.connect"],
            "matched_tool_classes": ["net.connect"],
        },
        "reordered": {
            "tool_classes": ["net.connect", "fs.read"],
            "matched_tool_classes": ["net.connect"],
        },
        "note": "Vec<String> already producer-sorted -> normalization is a no-op for a conforming producer "
        "(no digest change); the contract formalizes and locks it.",
    },
    {
        "id": "policy_extends_ordered",
        "schema": "assay-evidence/PayloadPolicySuggested",
        "class": "ordered",
        "set_paths": [],
        "record": {"extends": ["base-allow", "tighten-net"]},
        "reordered": {"extends": ["tighten-net", "base-allow"]},
        "note": "policy extends precedence is order-significant -> NOT a set -> reordering MUST move the digest.",
    },
    {
        "id": "capability_surface_btreeset_already_canonical",
        "schema": "assay.runner.capability_surface.v0",
        "class": "already_canonical_set",
        "set_paths": [["mcp_tools"], ["network_endpoints"]],
        "record": {"mcp_tools": ["a", "b"], "network_endpoints": []},
        "reordered": {"mcp_tools": ["b", "a"], "network_endpoints": []},
        "note": "ILLUSTRATIVE: the contract treats this as a semantic set that is already canonical in the "
        "product TYPE (BTreeSet<String> -> serialized sorted + unique). This is NOT a product-serialization "
        "parity proof: a real BTreeSet would never emit the reordered bytes, so the Python record here only "
        "demonstrates the contract. The product PR pins serialization with a real Rust golden.",
    },
]


def _golden(case):
    sp = case["set_paths"]
    new = C.normalize_sets(case["record"], sp)
    new_re = C.normalize_sets(case["reordered"], sp)
    return {
        "id": case["id"],
        "schema": case["schema"],
        "class": case["class"],
        "set_paths": sp,
        "note": case["note"],
        "record": case["record"],
        "reordered": case["reordered"],
        "old_digest": C.content_id(case["record"]),
        "new_digest": C.content_id(new) if new is not None else None,
        "reordered_new_digest": C.content_id(new_re) if new_re is not None else None,
    }


def build():
    goldens = [_golden(c) for c in _CASES]
    with open(os.path.join(HERE, "goldens.json"), "w") as f:
        json.dump(goldens, f, indent=2, sort_keys=True)
        f.write("\n")
    return goldens


if __name__ == "__main__":
    for g in build():
        moves = g["old_digest"] != g["new_digest"]
        collapses = g["reordered_new_digest"] == g["new_digest"]
        print(
            f"{g['id']:46s} {g['class']:24s} adoption-moves-digest={moves}  reorder-collapses={collapses}"
        )
