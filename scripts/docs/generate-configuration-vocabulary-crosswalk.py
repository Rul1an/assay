#!/usr/bin/env python3
"""Generate the configuration-vocabulary crosswalk from the committed record corpus.

Several record schemas in this workspace carry a digest or version pinning *what was in force*
when a tool decision was made, under different field names. This emits a map of them.

The split matters and the output states it:

* **Discovered** — which schemas exist, which configuration-ish keys they carry, and how often those
  keys are populated. Read from the tree on every run, so a new schema cannot go unnoticed.
* **Curated** — what each field is a statement *about*, and how fields relate. That cannot be
  derived from data; it is read from the producing code by a human and recorded in SUBJECTS below.

A discovered schema with no curated entry is emitted as **semantics not stated**, which is a finding
rather than a silent omission. That is the whole point: a hand-maintained list goes stale quietly.

Usage: generate-configuration-vocabulary-crosswalk.py [--repo-root .] [--out docs/architecture/...]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

# Keys that plausibly pin what was in force. Deliberately broad: the point is discovery, and a
# false positive is visible in the output while a false negative is not.
CONFIGISH = re.compile(r"(digest|policy|manifest|version|schema|source_class|identity)", re.I)
# Keys describing the record's own format rather than the agent's configuration.
NOT_CONFIG = {"schema", "schema_version", "specversion", "digest_alg", "canonicalization"}

# CURATED. Each subject is read from the producing code, never inferred from the field name;
# inferring from names is the error this document exists to prevent.
SUBJECTS: dict[str, dict[str, str]] = {
    "assay.tool_decision_surface.v0": {
        "_field": "server.declared_manifest_digest",
        "_subject": "The MCP server's declared tool manifest — what the server advertised it could "
        "do. The server compares against it and can report `declared_manifest_digest_mismatch`.",
    },
    "assay.tool_decision_truth.v0": {
        "_field": "declared_policy_digest",
        "_subject": "The declared constraint set the decision was taken under: "
        "`McpPolicy::declared_constraint_digest_experimental`, binding tool name, args schema, "
        "identity, classes, approval, scope and redaction. Decision identity is the pair "
        "`(observed_input_digest, declared_policy_digest)`.",
    },
    "assay.tool_decision_truth.otel_projection.v0": {
        "_field": "spans[].attributes.assay.tdt.declared_policy_digest",
        "_subject": "The same fact as `assay.tool_decision_truth.v0`, carried as OpenTelemetry span "
        "attributes.",
    },
    "assay.tool_decision_truth.vectors.v0": {
        "_field": "policies.<name>.version",
        "_subject": "A named policy variant a vector exercises. A version label, not a digest over "
        "content: comparable for identity between records sharing a naming scheme, not "
        "recomputable from bytes.",
    },
}

# CURATED. Relations carry a direction. A relation valid one way is not assumed to hold in reverse.
RELATIONS = [
    ("`assay.tool_decision_truth.v0` → `…otel_projection.v0`", "projection", "one way only",
     "The span attributes carry the same fact. Reconstructing the carrier from the projection is "
     "**not** claimed: a projection may drop fields."),
    ("`assay.tool_decision_surface.v0` ↔ `assay.tool_decision_truth.v0`", "different subjects",
     "no derivation either way",
     "A server tool manifest is what the server advertised; a declared constraint set is the rule "
     "the decision was measured against. Both answer \"what was in force\", about different things."),
    ("`assay.tool_decision_truth.vectors.v0` ↔ any digest field", "different kind of statement",
     "no derivation",
     "A version names a variant; a digest commits to content."),
]

# CURATED. Declared in a type, instantiated nowhere. Regenerated warning if that ever changes.
UNINSTANTIATED = {
    "type": "PayloadToolDecision (assay-evidence)",
    "fields": "policy_digest, policy_snapshot_digest (+_alg/_canonicalization/_schema), "
    "tool_definition_digest (+_alg/_canonicalization/_schema/_source), args_schema_hash",
    "note": "All optional, no doc comments, and populated by no committed fixture. Semantics are "
    "stated nowhere and there is no instance to read them from, so **no relation to any field "
    "above can be asserted**. `policy_digest` reads like a shorter `declared_policy_digest`; that "
    "resemblance is a name, not evidence.",
}


def run(root: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(root), *args],
                          capture_output=True, text=True, check=True).stdout


def collect(node, prefix: str, out: dict[str, list]) -> None:
    if isinstance(node, dict):
        for k, v in node.items():
            path = f"{prefix}.{k}" if prefix else k
            if CONFIGISH.search(k) and k not in NOT_CONFIG:
                out.setdefault(path, []).append(v)
            collect(v, path, out)
    elif isinstance(node, list):
        for v in node:
            collect(v, f"{prefix}[]", out)


def discover(root: Path) -> dict[str, dict]:
    found: dict[str, dict] = {}
    for rel in run(root, "ls-tree", "-r", "--name-only", "HEAD").splitlines():
        if not rel.endswith((".json", ".ndjson")):
            continue
        blob = run(root, "show", f"HEAD:{rel}")
        if "tool_decision" not in blob and "ToolDecision" not in blob:
            continue
        for raw in ([blob] if rel.endswith(".json") else [x for x in blob.splitlines() if x.strip()]):
            try:
                doc = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if not isinstance(doc, dict):
                continue
            schema = doc.get("schema", "<no schema field>")
            entry = found.setdefault(schema, {"files": set(), "keys": {}})
            entry["files"].add(rel)
            collect(doc, "", entry["keys"])
    return found


def render(root: Path, found: dict[str, dict]) -> str:
    lines: list[str] = []
    add = lines.append

    add("# Configuration vocabulary crosswalk")
    add("")
    add("**Generated** by `scripts/docs/generate-configuration-vocabulary-crosswalk.py`. Do not")
    add("hand-edit: re-run it instead, or the map goes stale silently, which is the failure it")
    add("exists to prevent.")
    add("")
    add("Derived from the committed record corpus by that script. It deliberately records **no**")
    add("commit stamp: this file is regenerated and committed by the docs workflow, so a stamp would")
    add("name the commit before its own, making the file permanently stale against `--check`.")
    add("Freshness is enforced by re-running, not by a date.")
    add("")
    add("Several record schemas here carry a digest or version pinning *what was in force* when a")
    add("tool decision was made, under different field names. Nothing else says how they relate, so")
    add("a reader who meets one of them can reasonably assume the others mean the same thing. They")
    add("do not.")
    add("")
    add("**No claim in this codebase depends on configuration** — the claim gate knows")
    add("`PositiveExistence`, `ExhaustiveSet` and `BoundedNegative`, all about observation coverage.")
    add("This is a legibility map, not a correctness mechanism, and it justifies no code change.")
    add("")
    add("Field subjects below are read from the producing code, never inferred from the field name.")
    add("Inferring from names is exactly the error this page prevents.")
    add("")
    add("## Schemas found in the corpus")
    add("")
    add("| schema | documents | configuration key | populated | what it is a statement about |")
    add("|---|---|---|---|---|")
    for schema in sorted(found):
        entry = found[schema]
        curated = SUBJECTS.get(schema)
        keys = {k: v for k, v in entry["keys"].items() if "declared" in k or "polic" in k.lower()}
        keyname = curated["_field"] if curated else (", ".join(sorted(keys)[:2]) or "—")
        populated = "—"
        for k, vals in entry["keys"].items():
            if curated and curated["_field"].split(".")[-1] in k:
                populated = f"{sum(1 for v in vals if v is not None)}/{len(vals)}"
                break
        subject = curated["_subject"] if curated else (
            "**Semantics not stated.** This schema carries configuration-ish keys and has no curated "
            "entry in the generator. Read the producing code and add one, or record why it does not "
            "belong here.")
        add(f"| `{schema}` | {len(entry['files'])} | `{keyname}` | {populated} | {subject} |")
    add("")
    add("## How they relate")
    add("")
    add("Stated as relations with a **direction**, not as equality. Only one pair below earns")
    add("\"equivalent\", and only one way.")
    add("")
    add("| pair | relation | direction | note |")
    add("|---|---|---|---|")
    for pair, rel, direction, note in RELATIONS:
        add(f"| {pair} | {rel} | {direction} | {note} |")
    add("")
    add("## Declared in a type, instantiated nowhere")
    add("")
    add(f"**{UNINSTANTIATED['type']}** — `{UNINSTANTIATED['fields']}`")
    add("")
    add(UNINSTANTIATED["note"])
    add("")
    add("## Rules this map follows")
    add("")
    add("- **Not stated is a finding, not a gap.** A schema with no curated subject is emitted as")
    add("  such rather than omitted.")
    add("- **A mapping is itself a claim.** Saying two fields are the same fact needs a stated")
    add("  relation and a direction.")
    add("- **No new vocabulary.** This map references what exists and mints nothing.")
    # Exactly one trailing newline: the repository's end-of-file hook enforces it, and a generator
    # whose output the hook rewrites would drift against the generated-docs-match check every run.
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=".", type=Path)
    ap.add_argument("--out", default="docs/architecture/CONFIGURATION-VOCABULARY-CROSSWALK.md")
    ap.add_argument("--check", action="store_true",
                    help="fail if the committed doc differs from freshly generated output")
    args = ap.parse_args()

    root = args.repo_root.resolve()
    text = render(root, discover(root))
    out = root / args.out
    if args.check:
        current = out.read_text() if out.exists() else ""
        if current != text:
            print(f"{args.out} is stale; re-run this generator")
            return 1
        print(f"{args.out} is current")
        return 0
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
