#!/usr/bin/env python3
"""Generate the configuration-vocabulary crosswalk from the record corpus in the tree.

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
import os
import re
import subprocess
from pathlib import Path

# Keys that plausibly pin what was in force. Deliberately broad: the point is discovery, and a
# false positive is visible in the output while a false negative is not.
CONFIGISH = re.compile(r"(digest|policy|manifest|version|schema|source_class|identity)", re.I)
# Keys describing the record's own format rather than the agent's configuration.
# Matched on the key's FINAL SEGMENT, so `data.external_schema` is excluded the same way a
# top-level `schema` is. Whole-key equality missed every nested occurrence.
NOT_CONFIG = {
    "schema", "schema_version", "specversion", "digest_alg", "canonicalization",
    "external_schema", "assayproducerversion", "producerversion",
}

# CURATED. Each subject is read from the producing code, never inferred from the field name;
# inferring from names is the error this document exists to prevent.
SUBJECTS: dict[str, dict[str, str]] = {
    "assay.tool_decision_surface.v0": {
        "_field": "server.declared_manifest_digest",
        "_subject": "The **declared, baselined** tool manifest. `docs/reference/mcp-manifest-drift.md` "
        "defines *observed* as the latest fully observed `tools/list` — what the server advertised — "
        "and *declared* as the baseline it is compared against, so this names the baseline side. The "
        "related finding `declared_manifest_digest_mismatch` is a self-consistency check on that side "
        "alone (`recompute(declared.tools) != declared.manifest_digest`), belongs to the "
        "manifest-drift records rather than to this schema, and is emitted today only by a "
        "test-local reference verifier.",
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
     "A baselined tool manifest is the surface a server was expected to expose; a declared "
     "constraint set is the rule the decision was measured against. Both answer \"what was in "
     "force\", about different things."),
    ("`assay.tool_decision_truth.vectors.v0` ↔ any digest field", "different kind of statement",
     "no derivation",
     "A version names a variant; a digest commits to content."),
    ("`policy_digest` → `policy_snapshot_digest`", "self-describing projection",
     "same value, stated as a MUST",
     "PLAN-P56A (Status: Implemented): `policy_snapshot_digest` is the self-describing projection "
     "of the existing `policy_digest`, and in supported decision paths both MUST represent the "
     "same digest value while the compatibility field remains present."),
    ("`policy_digest` → `declared_policy_digest`", "projection, whole to part", "one way only",
     "The doc comment on `McpPolicy::declared_constraint_digest_experimental` states it: unlike "
     "`policy_digest`, which is the whole policy, this projects to the declared-constraint surface "
     "only. **The containment runs whole-to-part**, which is the opposite of what the shorter name "
     "suggests — an earlier version of this page guessed from the names and inverted it."),
    ("`protectmcp:decision`'s `policy_digest` ↔ this workspace's `policy_digest`", "same name, "
     "different producer", "no stated relation",
     "The signed receipts under `tests/fixtures/interop/` are a third-party record format carrying "
     "a field of the same name. Nothing states that the two are computed the same way, so nothing "
     "here asserts they are comparable."),
]

# CURATED. Fields declared on a type that no JSON fixture populates. An earlier version of this
# page read that absence as "semantics stated nowhere, and no instance to read them from", and both
# halves were false. The correction is the most useful thing on the page, so it is stated rather
# than quietly dropped.
PAYLOAD_FIELDS = {
    "type": "PayloadToolDecision (assay-evidence)",
    # Spelled out rather than abbreviated. A `(+_alg/_canonicalization/_schema)` shorthand reads as
    # `policy_snapshot_digest_canonicalization`, which does not exist: only `_alg` keeps the `digest`
    # infix. Inventing field names by pattern is the error this page is about.
    "fields": "args_schema_hash, policy_digest, policy_snapshot_digest, policy_snapshot_digest_alg, "
    "policy_snapshot_canonicalization, policy_snapshot_schema, tool_definition_digest, "
    "tool_definition_digest_alg, tool_definition_canonicalization, tool_definition_schema, "
    "tool_definition_source",
    "note": [
        "No `.json` or `.ndjson` fixture in the tree populates these, which is why they do not "
        "appear in the table above. That absence says nothing about their meaning.",
        "**Instances exist.** `crates/assay-evidence/tests/verify_strict_test.rs` builds the "
        "payload with these fields populated and runs it through `verify_single_event`; "
        "`crates/assay-evidence/src/types/tests.rs` deserializes the same cluster.",
        "**The semantics are stated**, in prose, outside the corpus this generator reads: "
        "`docs/architecture/PLAN-P56A-POLICY-SNAPSHOT-DIGEST-VISIBILITY-2026q2.md` (Status: "
        "Implemented) for the `policy_snapshot_*` cluster, `PLAN-P56B-TOOL-DEFINITION-DIGEST-"
        "BINDING-2026q2.md` for `tool_definition_*`. Both are Status: Implemented and carry "
        "per-field MUSTs. `args_schema_hash` is weaker: the only prose on it is one row of "
        "`docs/architecture/evidence-metrics-mapping.md` saying how a metric consumes it, not what "
        "is hashed or under what canonicalization. That one **is** close to unstated, and saying so "
        "is the point — the three citations do not carry equal weight and the page should not "
        "imply they do.",
        "The lesson is this page's own rule turned on itself. Searching for populated JSON "
        "fixtures and finding none is evidence about **fixtures**, not about semantics. Reading "
        "that absence as a gap is the same mistake as reading a field name as a meaning.",
    ],
}


# Directories that are not this tree's own content: build output, dependency caches, and any
# directory carrying its own .git, which is a nested checkout whose files are copies.
PRUNE = {".git", "target", "node_modules", ".venv"}


def collect(node, prefix: str, out: dict[str, list]) -> None:
    if isinstance(node, dict):
        for k, v in node.items():
            path = f"{prefix}.{k}" if prefix else k
            if CONFIGISH.search(k) and k.split(".")[-1] not in NOT_CONFIG:
                out.setdefault(path, []).append(v)
            collect(v, path, out)
    elif isinstance(node, list):
        for v in node:
            collect(v, f"{prefix}[]", out)


def tracked_paths(root: Path) -> "set[str] | None":
    """Tracked paths, or None when `root` is not a worktree root and everything should be read.

    The drift gate seeds its scratch copy from `git ls-files` and deletes the `.git` directory,
    "so a stray target/ or an untracked scratch file cannot change what the generators see". A
    generator that reads the worktree indiscriminately does not honour that: an untracked JSON
    scratch file adds a row locally that the gate can never reproduce, and the developer gets a
    drift failure that re-running cannot clear. Reading the tracked set makes the two agree.

    None is the strict direction and is what the gate itself needs -- its scratch tree has no
    `.git`, and there everything present is exactly the tracked set already.
    """
    try:
        top = subprocess.run(["git", "-C", str(root), "rev-parse", "--show-toplevel"],
                             capture_output=True, check=True).stdout
        if os.path.realpath(top.decode().strip()) != os.path.realpath(root):
            return None
        out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                             capture_output=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return {n.decode("utf-8", "surrogateescape") for n in out.split(b"\0") if n} or None


def discover(root: Path) -> dict[str, dict]:
    """Read the corpus from the worktree, not from `git ls-tree HEAD`.

    Reading HEAD would be the tighter provenance, and it is the wrong choice here. The repository's
    generated-docs drift gate rebuilds the tree into a scratch directory with no `.git` and runs
    every registered generator there, so a generator that shells out to git cannot be registered at
    all -- it raises instead of regenerating, and the gate reports "could not check". Reading the
    worktree is also what makes the check mean anything on a pull request: HEAD lags the change
    being reviewed, so a commit that adds a schema would regenerate against the corpus as it was
    before that commit.

    The walk is sorted at every level so the output does not depend on directory order.
    """
    found: dict[str, dict] = {}
    outside: dict[str, set] = {}
    tracked = tracked_paths(root)
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(
            d for d in dirnames if d not in PRUNE and not (Path(dirpath) / d / ".git").exists()
        )
        for name in sorted(filenames):
            if not name.endswith((".json", ".ndjson")):
                continue
            path = Path(dirpath) / name
            rel = path.relative_to(root).as_posix()
            if tracked is not None and rel not in tracked:
                continue
            try:
                blob = path.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            # No lexical pre-filter. An earlier version skipped any file whose bytes lacked the
            # word "decision", which ran BEFORE the out-of-scope accounting below and so hid
            # records from the very count that exists to show what was excluded -- including
            # `assay.declared_mcp_manifest.v0`, the declared baseline that the first row of this
            # page is defined against. A denominator computed after a silent filter is not a
            # denominator.
            for raw in ([blob] if name.endswith(".json") else
                        [x for x in blob.splitlines() if x.strip()]):
                try:
                    doc = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                if not isinstance(doc, dict):
                    continue
                keys: dict[str, list] = {}
                collect(doc, "", keys)
                label = schema_label(doc)
                # A record that does not name its own type is not a vocabulary; grouping the
                # nameless together would invent one.
                in_scope = bool(label) and (
                    has_key(doc, "decision")
                    or "decision" in label.lower()
                    or any("decision" in key.lower() for key in keys)
                )
                if keys and label and not in_scope:
                    outside.setdefault(label, set()).add(rel)
                if not keys or not in_scope:
                    # Scope, stated rather than guessed: a record is in scope when it carries a
                    # configuration key AND is about a decision -- by its own type name, by a
                    # `decision` key, or by a configuration key that names one. Three narrower
                    # rules failed in turn: matching "tool_decision" missed a whole vocabulary,
                    # matching "decision" anywhere in the bytes swept in JSON Schema documents
                    # whose `type` is the word "object", and requiring a literal `decision` key
                    # dropped the four truth schemas, which name it `decision_identity`.
                    continue
                entry = found.setdefault(label, {"files": set(), "documents": 0, "keys": {}})
                entry["files"].add(rel)
                entry["documents"] += 1
                for key, values in keys.items():
                    entry["keys"].setdefault(key, []).extend(values)
    return found, outside


def has_key(node, name: str) -> bool:
    """Whether a key of this name appears anywhere in the record."""
    if isinstance(node, dict):
        return name in node or any(has_key(v, name) for v in node.values())
    if isinstance(node, list):
        return any(has_key(v, name) for v in node)
    return False


def declared_schemas(node, depth: int = 0, out: "list[tuple[int, str]] | None" = None):
    """Every `schema` / `external_schema` value in the record, with its depth."""
    out = [] if out is None else out
    if isinstance(node, dict):
        for key, value in node.items():
            if key in ("schema", "external_schema") and isinstance(value, str):
                if "." in value or ":" in value:
                    out.append((depth, value))
            declared_schemas(value, depth + 1, out)
    elif isinstance(node, list):
        for value in node:
            declared_schemas(value, depth + 1, out)
    return out


def schema_label(doc: dict) -> str:
    """What this record calls itself, preferring its own schema over its envelope's type.

    Not every record announces itself under a top-level `schema`. Signed receipts carry `type`
    inside a `payload`, and reading only `schema` made a whole vocabulary invisible -- one with its
    own `policy_digest`.

    A record's declared schema outranks the envelope's `type`. A CloudEvent wrapping an observation
    carries a placeholder `type` and the real schema inside `data`, and preferring the envelope
    listed one vocabulary twice under two names, the second of them literally called
    `example.placeholder.*`. Two rows for one thing is the confusion this page exists to remove.

    `type` is accepted only when it reads as a namespaced identifier. A bare word is JSON Schema's
    own vocabulary -- `"type": "object"` -- and treating that as a record type invents a schema.
    """
    declared = declared_schemas(doc)
    if declared:
        return min(declared)[1]
    for holder in (doc, doc.get("payload") if isinstance(doc.get("payload"), dict) else {}):
        value = holder.get("type")
        if isinstance(value, str) and (":" in value or "." in value):
            return value
    return ""


def populated_ratio(entry: dict, field: str) -> str:
    """How many occurrences of the curated field carry a value, over how many occur at all.

    Matched on the final path segment by EQUALITY, never by substring. A substring match binds to
    the wrong key silently: `declared_manifest_digest` is a prefix of `declared_manifest_digest_
    mismatch`, and reporting one field's count beside another field's name is the exact error this
    page exists to prevent, committed by the page itself.

    Summed over every matching key rather than stopping at the first. One document can carry the
    field many times -- four named policy variants, two spans -- and stopping early silently
    discards the rest. The count is occurrences, not documents, and the column says so.
    """
    tail = field.split(".")[-1]
    total = filled = 0
    for key, values in entry["keys"].items():
        if key.split(".")[-1] != tail:
            continue
        total += len(values)
        filled += sum(1 for v in values if v is not None)
    return f"{filled}/{total}" if total else "—"


def key_list(keys, limit: int = 4) -> str:
    keys = sorted(keys)
    shown = ", ".join(f"`{k}`" for k in keys[:limit])
    if len(keys) > limit:
        shown += f", and {len(keys) - limit} more"
    return shown or "—"


def render(found: dict[str, dict], outside: dict[str, set]) -> str:
    lines: list[str] = []
    add = lines.append

    add("# Configuration vocabulary crosswalk")
    add("")
    add("**Generated** by `scripts/docs/generate-configuration-vocabulary-crosswalk.py`. Do not")
    add("hand-edit: re-run it instead, or the map goes stale silently, which is the failure it")
    add("exists to prevent.")
    add("")
    add("Derived from the record corpus in the tree by that script. It deliberately records **no**")
    add("commit stamp: this file is regenerated and committed by the docs workflow, so a stamp would")
    add("name the commit before its own, making the file permanently stale against `--check`.")
    add("Freshness is enforced by re-running, not by a date.")
    add("")
    add("Several record schemas here carry a digest or version pinning *what was in force* when a")
    add("tool decision was made, under different field names. Nothing else says how they relate, so")
    add("a reader who meets one of them can reasonably assume the others mean the same thing. They")
    add("do not.")
    add("")
    add("**The claim gate does not take configuration as an input.** Its claim kinds are")
    add("`PositiveExistence`, `ExhaustiveSet` and `BoundedNegative`, and all three are about")
    add("observation coverage.")
    add("")
    add("That is a statement about the gate, not about the codebase, and an earlier version of this")
    add("page generalised it into \"no claim in this codebase depends on configuration\". That is")
    add("false. ADR-043 conditions an enforcement statement on configuration — *a capability that")
    add("cannot bind `declared_policy_digest` makes no enforcement statement in evidence* — and the")
    add("decision identity in the table below is the pair `(observed_input_digest,")
    add("declared_policy_digest)`, which takes configuration as an input by construction. The false")
    add("generalisation mattered because it was what licensed the next sentence.")
    add("")
    add("This page is a legibility map rather than a mechanism: it adds no check and changes no")
    add("behaviour. That is a claim about **this page**, and nothing follows from it about what else")
    add("in the tree depends on configuration.")
    add("")
    add("Field subjects below are read from the producing code, never inferred from the field name.")
    add("Inferring from names is exactly the error this page prevents.")
    add("")
    mapped = sorted(k for k in found if k in SUBJECTS)
    unmapped = sorted(k for k in found if k not in SUBJECTS)

    add("## The mapped vocabularies")
    add("")
    add("`populated` counts **occurrences, not documents**: one record can carry the field several")
    add("times, and each is counted. It is matched on the field's final path segment by equality,")
    add("never by substring, because `declared_manifest_digest` is a prefix of")
    add("`declared_manifest_digest_mismatch` and a loose match reports one field's count beside")
    add("another field's name.")
    add("")
    add("| schema | documents | curated key | populated | other keys it carries | what it is a statement about |")
    add("|---|---|---|---|---|---|")
    for schema in mapped:
        entry, curated = found[schema], SUBJECTS[schema]
        tail = curated["_field"].split(".")[-1]
        others = [k for k in entry["keys"] if k.split(".")[-1] != tail]
        add(f"| `{schema}` | {entry['documents']} | `{curated['_field']}` | "
            f"{populated_ratio(entry, curated['_field'])} | {key_list(others)} | "
            f"{curated['_subject']} |")
    add("")
    add("The **other keys** column exists because curating one field must not delete the rest from")
    add("view. Without it, moving a row into this table would turn every one of its other")
    add("configuration keys from a stated finding into a silent gap, which inverts this page's own")
    add("first rule.")
    add("")
    add("## Carrying configuration, semantics not stated")
    add("")
    add("These records reached the same scope test and carry keys the generator's filter reads as")
    add("configuration-ish, and nobody has written down what those keys are a statement about. They")
    add("are listed rather than omitted: **not stated is a finding, not a gap.**")
    add("")
    add("The filter is deliberately broad, so expect false positives here — a `policy_decisions`")
    add("count is not a configuration basis. That direction is the intended one: a false positive is")
    add("visible in this table, while a false negative is a vocabulary nobody ever learns about.")
    add("Adding a curated subject moves a row up into the table above, and deciding a row does not")
    add("belong is equally good, once the reason is written down somewhere.")
    add("")
    add("No relation is asserted for anything here. A shared field name is not evidence.")
    add("")
    add("| schema | documents | configuration keys it carries |")
    add("|---|---|---|")
    for schema in unmapped:
        entry = found[schema]
        add(f"| `{schema}` | {entry['documents']} | {key_list(entry['keys'])} |")
    add("")
    add("## Outside this page's scope")
    add("")
    add("The scope rule, stated here rather than left in the generator: a record is in scope when")
    add("it carries a configuration-ish key **and** is about a decision — by naming one in its own")
    add("type, by carrying a `decision` key, or by a configuration key that names one.")
    add("")
    add(f"**{len(outside)} further record types** carry configuration-ish keys and fall outside it.")
    add("They are counted here so the denominator is visible: \"a new schema cannot go unnoticed\"")
    add("is only true inside a declared scope, and an undeclared one hides its own misses.")
    add("")
    add("| schema | files |")
    add("|---|---|")
    for schema in sorted(outside):
        add(f"| `{schema}` | {len(outside[schema])} |")
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
    add("## Declared in a type, populated by no fixture")
    add("")
    add(f"**{PAYLOAD_FIELDS['type']}** — `{PAYLOAD_FIELDS['fields']}`")
    add("")
    for paragraph in PAYLOAD_FIELDS["note"]:
        add(paragraph)
        add("")
    lines.pop()
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
    text = render(*discover(root))
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
