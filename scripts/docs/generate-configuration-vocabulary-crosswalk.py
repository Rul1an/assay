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
import sys
from pathlib import Path
from urllib.parse import urlsplit

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
        "_field": "observed_tool_decisions[].server.declared_manifest_digest",
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
        "_subject": "The declared constraint set the decision was taken under, digested by "
        "`McpPolicy::declared_constraint_digest_experimental`. **What it covers is defined by "
        "`project_and_normalize_declared`** in `crates/assay-core/src/mcp/policy/mod.rs`; read it "
        "there rather than trusting a summary. Two things worth knowing before you do: it does "
        "**not** cover identity. The projection is an allowlist, and it copies exactly four things: "
        "`version`, `enforcement`, ten `tools.*` list keys, and `schemas`. `tool_pins`, which is the "
        "only tool identity in the policy, is **not** one of them and is therefore outside the "
        "digest. An earlier version of this row "
        "enumerated the surface from a module doc comment instead of from the projection, claimed "
        "identity was bound, and omitted both of those. Decision identity is a separate thing: the "
        "pair `(observed_input_digest, declared_policy_digest)`.",
    },
    "assay.tool_decision_truth.otel_projection.v0": {
        "_field": "spans[].attributes.assay.tdt.declared_policy_digest",
        "_subject": "The same fact as `assay.tool_decision_truth.v0`, carried as OpenTelemetry span "
        "attributes.",
    },
    "assay.tool_decision_truth.vectors.v0": {
        "_field": "carriers[].carrier.declared_policy_digest",
        "_subject": "The same declared constraint set as `assay.tool_decision_truth.v0`, carried per "
        "vector. An earlier version of this row pointed at `policies.<name>.version` instead and "
        "called it \"a named policy variant\". That was read off the key path rather than off the "
        "type: `McpPolicy::version` is the **policy document format** version, used at "
        "`crates/assay-core/src/mcp/policy/legacy.rs` to detect a v1 shape, and it is the constant "
        "`1` for all four variants in the fixture — so it names no variant and can compare nothing. "
        "The variant is named by the map key, not by the field.",
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
    ("`McpPolicy::version` ↔ any digest field here", "not a configuration basis at all",
     "no derivation",
     "`version` is the **policy document format** version — `crates/assay-core/src/mcp/policy/"
     "legacy.rs` reads it to detect a v1 shape — and it is the constant `1` across the fixture. An "
     "earlier row called it a name for a policy variant, which it is not: the variant is named by "
     "the map key. It pins no configuration and nothing here derives from it."),
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
        "Implemented) for the `policy_snapshot_*` cluster, "
        "`docs/architecture/PLAN-P56B-TOOL-DEFINITION-DIGEST-"
        "BINDING-2026q2.md` for `tool_definition_*`. Both are Status: Implemented and carry "
        "per-field MUSTs. `args_schema_hash` is weaker, and the weakness is specific rather than "
        "total: what it is **for** is stated — `docs/architecture/evidence-metrics-mapping.md` has "
        "a metric consume it — while **what is hashed and under which canonicalization** is not. "
        "An earlier version of this paragraph said that row was the only prose on the field, which "
        "was not checked and is not true. The three citations do not carry equal weight and the "
        "page should not imply they do.",
        "The lesson is this page's own rule turned on itself. Searching for populated JSON "
        "fixtures and finding none is evidence about **fixtures**, not about semantics. Reading "
        "that absence as a gap is the same mistake as reading a field name as a meaning.",
    ],
}


# Directories that are not this tree's own content: build output, dependency caches, and any
# directory carrying its own .git, which is a nested checkout whose files are copies.
#
# The .git prune only matters on the fallback path. When the tracked set is available a nested
# checkout's files are already excluded as untracked, and the drift gate's scratch tree has no
# nested checkouts at all. It is kept for the case the fallback exists for -- a tree read from
# somewhere that is not a worktree root -- and not because it guards the normal path.
PRUNE = {".git", "target", "node_modules", ".venv"}
# Ceiling applied before materialisation, per the repository's rule for hostile input.
MAX_RECORD_BYTES = 8 * 1024 * 1024


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


# Interrogating a tree must not inherit this process's git environment. GIT_DIR / GIT_INDEX_FILE
# point somewhere else entirely under pre-commit and inside hooks, and `git -C <tree> ls-files`
# would then answer about that other repository -- a wrong tracked set, arrived at in silence,
# which is the failure mode these guards exist to prevent.
GIT_ENV = (
    "GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "GIT_CEILING_DIRECTORIES",
)


def git_env() -> dict:
    """The environment with git's own configuration knobs removed.

    GIT_CONFIG_COUNT is the load-bearing one beyond the directory variables: without it git ignores
    the numbered GIT_CONFIG_KEY_n / GIT_CONFIG_VALUE_n pairs entirely, so dropping the count drops
    the injected configuration with it.
    """
    return {k: v for k, v in os.environ.items() if k not in GIT_ENV}


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
                             capture_output=True, check=True, env=git_env()).stdout
        if os.path.realpath(top.decode().strip()) != os.path.realpath(root):
            return None
        out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                             capture_output=True, check=True, env=git_env()).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    # An empty listing here is an answer, not a failure: git succeeded and said nothing is tracked,
    # so the corpus is empty. The checker treats the same input as a reason to fall back and scan
    # everything, because for a supply-chain check scanning more is the safe direction. For this
    # generator the safe direction is the opposite -- falling back would read untracked files and
    # produce a page the drift gate, which seeds from `git ls-files`, could never reproduce.
    return {n.decode("utf-8", "surrogateescape") for n in out.split(b"\0") if n}


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
    unlabelled: set = set()
    unlabelled_jsonschema: set = set()
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
            # Ceiling before materialisation. Refusing is the point: whether an oversize record is
            # in scope cannot be known without reading it, so skipping it would drop a candidate
            # from the corpus in silence -- the exact failure the out-of-scope section argues
            # against, committed by the code that renders it.
            if path.stat().st_size > MAX_RECORD_BYTES:
                raise SystemExit(
                    f"error: {rel} is {path.stat().st_size} bytes, over the "
                    f"{MAX_RECORD_BYTES}-byte record ceiling; refusing to read it, and refusing to "
                    f"drop it silently from the corpus"
                )
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
                if keys and not label:
                    unlabelled.add(rel)
                    if declares_meta_schema(doc.get("$schema")):
                        unlabelled_jsonschema.add(rel)
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
    return found, outside, unlabelled, unlabelled_jsonschema


def declares_meta_schema(value) -> bool:
    """Whether `$schema` points at the JSON Schema meta-schema, matched on the URL's host.

    A substring test accepts the host appearing anywhere in the URL, so
    `https://example.invalid/json-schema.org/x` would count as a meta-schema. The host is the only
    part that identifies who defined the vocabulary, so it is the part compared.
    """
    if not isinstance(value, str):
        return False
    host = (urlsplit(value).hostname or "").lower()
    return host == "json-schema.org" or host.endswith(".json-schema.org")


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
        shallowest = min(d for d, _ in declared)
        names = sorted({v for d, v in declared if d == shallowest})
        # A tie is two vocabularies declared at the same level, not one winner. Taking min() dropped
        # `assay.mcp_manifest_observed.v0` -- the record this page's own cited reference is titled
        # after -- purely because "d" sorts before "m". A rule that silently discards a co-equal
        # schema cannot also claim that a new schema cannot go unnoticed.
        return names[0] if len(names) == 1 else " + ".join(names)
    for holder in (doc, doc.get("payload") if isinstance(doc.get("payload"), dict) else {}):
        value = holder.get("type")
        if isinstance(value, str) and (":" in value or "." in value):
            return value
    return ""


def populated_ratio(entry: dict, field: str) -> str:
    """How many occurrences of the curated field carry a value, over how many occur at all.

    Matched on the WHOLE path, not on its final segment. Matching the tail merged keys that merely
    end alike: `carriers[].carrier.declared_policy_digest` was summed with
    `carriers[].carrier.decision_identity.declared_policy_digest` and reported as one number, which
    is a milder form of the error the column exists to avoid -- a count beside a name it does not
    belong to. Substring matching was worse still and came first, binding
    `declared_manifest_digest` to `declared_manifest_digest_mismatch`.

    The count is occurrences of that one path, not documents, and the column says so.
    """
    values = entry["keys"].get(field, [])
    if not values:
        return "—"
    return f"{sum(1 for v in values if v is not None)}/{len(values)}"


def key_list(keys, limit: int = 4) -> str:
    keys = sorted(keys)
    shown = ", ".join(f"`{k}`" for k in keys[:limit])
    if len(keys) > limit:
        shown += f", and {len(keys) - limit} more"
    return shown or "—"


def render(found: dict[str, dict], outside: dict[str, set], unlabelled: set,
           unlabelled_jsonschema: set) -> str:
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
    add("A row labelled `A + B` is **one record declaring two or more schemas at the same depth**,")
    add("not a")
    add("schema called \"A + B\". Reporting both is deliberate: taking one by alphabetical order")
    add("silently dropped `assay.mcp_manifest_observed.v0`, a vocabulary this page cites a reference")
    add("document for. The joined string is a rendering of two declarations, not a new name.")
    add("")
    mapped = sorted(k for k in found if k in SUBJECTS)
    unmapped = sorted(k for k in found if k not in SUBJECTS)

    add("## The mapped vocabularies")
    add("")
    add("`populated` counts **occurrences of one whole path, not documents**: a record can carry")
    add("that path several times and each is counted, but a different path is never folded in. The")
    add("match is exact whole-path equality. Matching the final segment instead merged")
    add("`carriers[].carrier.declared_policy_digest` with")
    add("`carriers[].carrier.decision_identity.declared_policy_digest` and reported the sum under")
    add("one name; matching a substring was worse still, binding `declared_manifest_digest` to")
    add("`declared_manifest_digest_mismatch`. Both report one field's count beside another field's")
    add("name, which is the error this column exists to avoid.")
    add("")
    add("| schema | documents | curated key | populated | other keys it carries | what it is a statement about |")
    add("|---|---|---|---|---|---|")
    for schema in mapped:
        entry, curated = found[schema], SUBJECTS[schema]
        # Exclude the curated path itself and nothing else. Excluding by tail hid
        # `carriers[].carrier.decision_identity.declared_policy_digest`, a different key that merely
        # ends alike, from the column whose whole purpose is that curating one field must not delete
        # the others from view.
        others = [k for k in entry["keys"] if k != curated["_field"]]
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
    add("The filter is deliberately broad, so expect false positives here — `policy_decisions`")
    add("holds a list of decisions taken, which is not a configuration basis. That direction is the")
    add("intended one: a false positive is")
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
    # A composite label is several schemas. Comparing label strings let a schema appear in both
    # tables whenever it reached one side only inside an `A + B` row, which is exactly what the
    # sentence below promises does not happen.
    def parts(label: str) -> set:
        return {p.strip() for p in label.split(" + ")}

    in_scope_schemas = {s for label in found for s in parts(label)}
    both = sorted({s for label in outside for s in parts(label) if s in in_scope_schemas})
    outside = {k: v for k, v in outside.items() if not (parts(k) & in_scope_schemas)}
    add("## Outside this page's scope")
    add("")
    add("The scope rule, stated here rather than left in the generator. A record is in scope when")
    add("it **declares its own schema or a namespaced type**, **carries a configuration-ish key**,")
    add("and **is about a decision** — by naming one in its type, by carrying a `decision` key, or")
    add("by a configuration key that names one. All three are required; an earlier version of this")
    add("page stated only the last two, so the first excluded records silently.")
    add("")
    add(f"Scope is decided per document, while both tables above are keyed per schema. "
        f"**{len(both)} schema{'' if len(both) == 1 else 's'}** had documents on both sides and "
        f"{'is' if len(both) == 1 else 'are'} counted above rather than below, so nothing is listed "
        f"twice: " + (", ".join(f"`{b}`" for b in both) if both else "none"))
    add("")
    add(f"**{len(unlabelled)} further records** carry a configuration-ish key and declare no schema")
    add("and no namespaced type, so they fail the first conjunct and appear nowhere on this page.")
    add(f"{len(unlabelled_jsonschema)} of them declare a **meta-schema** under `$schema`, so they")
    add("are schemas rather than records. The rest are records this rule drops, including some that")
    add("name their own format under the same key — SARIF reports, CycloneDX BOMs — which is why")
    add("this counts the `$schema` **value** and not the presence of the key. An earlier version of")
    add("this line read the key name and called all of them schemas, which is the one inference this")
    add("page forbids, committed by the page against its own denominator.")
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
    add("Stated as relations with a **direction**, not as equality. Read the direction column: most")
    add("rows are one-way projections, and the one row asserting equal values says so — equality of")
    add("value is symmetric, which is a different claim from a projection being reversible.")
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


# Anchored on the repository's real top-level directories. An unanchored "contains a slash" rule
# also matched `tools/list`, which is an MCP method name, not a path.
CITATION = re.compile(
    r"`((?:crates|docs|scripts|tests|packaging|conformance|\.github)"
    r"(?:/[A-Za-z0-9_.\-]+)+/?)`"
)


DEFAULT_OUT = "docs/architecture/CONFIGURATION-VOCABULARY-CROSSWALK.md"
SYMBOL = re.compile(r"`([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)`")
SEARCHED = ("crates", "docs", "scripts")
SEARCHED_SUFFIXES = {".rs", ".md", ".py", ".sh", ".json", ".yml", ".yaml", ".toml"}


def curated_text() -> str:
    """Only the hand-written half of the page is checked for symbols.

    The other half is discovered from data, and a discovered JSON key is not a code symbol: keys
    like `descriptor_schema` live in fixtures under `tests/` and legitimately appear nowhere in
    `crates`, `docs` or `scripts`. Checking them reported a citation failure for a fact the
    generator had just read off disk. The curated half is where prose asserts what code does, which
    is the only thing this check is for.
    """
    parts = [v["_subject"] for v in SUBJECTS.values()]
    parts += [note for _, _, _, note in RELATIONS]
    parts += PAYLOAD_FIELDS["note"] + [PAYLOAD_FIELDS["fields"], PAYLOAD_FIELDS["type"]]
    return "\n".join(parts)


def verify_symbols(text: str, root: Path, out: Path, tracked: "set[str] | None") -> list:
    """Symbols the page names in backticks must appear somewhere in the tree's own code.

    Paths were already checked; symbols were not, and the load-bearing claim on this page now hangs
    on one. The row for `declared_policy_digest` deliberately stops paraphrasing and points at a
    function instead, which is only an improvement while the function still carries that name.

    The page's own toolchain is excluded, and that exclusion is the whole check. Without it the
    citation validates itself: the name lives in SUBJECTS here, in the rendered page, and in the
    test that mutates this file, so renaming the real Rust function leaves every one of those three
    copies behind and the check green. Verified by renaming it and watching this pass.

    Matched on exact token boundaries. A substring match would accept a symbol that survives only as
    part of a longer, different name.

    Filesystem search rather than `git grep`: the drift gate runs generators in a scratch copy with
    no `.git`, and a check that quietly stops working there is worse than no check.
    """
    wanted = {c for c in SYMBOL.findall(text) if len(c) >= 8 and ("_" in c or "::" in c)}
    if not wanted:
        return []
    # The committed page is excluded by its canonical path, not by whatever --out happens to be.
    # Excluding only the destination left the in-tree copy readable, so a control run writing to
    # /tmp still found the symbol in the page that cites it, and the check passed while the real
    # declaration had been renamed away.
    excluded = {
        Path(__file__).resolve(),
        out.resolve(),
        (root / DEFAULT_OUT).resolve(),
        (root / "scripts/ci/test-crosswalk-generator.sh").resolve(),
    }
    needles = {c: re.compile(rf"\b{re.escape(c.split('::')[-1])}\b") for c in wanted}
    for base in SEARCHED:
        for path in sorted((root / base).rglob("*")):
            if not path.is_file() or path.suffix not in SEARCHED_SUFFIXES:
                continue
            if path.resolve() in excluded:
                continue
            # Same tracked-set rule the corpus walk uses. An untracked file carrying a symbol that
            # had been removed from its real declaration would satisfy the check locally and fail
            # in a clean checkout -- a green that does not survive being cloned.
            if tracked is not None and path.relative_to(root).as_posix() not in tracked:
                continue
            try:
                if path.stat().st_size > MAX_RECORD_BYTES:
                    continue  # (3) bounded before materialisation; too large to be a declaration
                blob = path.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            for name in [c for c, rx in needles.items() if rx.search(blob)]:
                needles.pop(name, None)
            if not needles:
                return []
    return sorted(needles)


def verify_citations(text: str, root: Path) -> list:
    """Every repo path the CURATED prose cites must exist. Returns the ones that do not.

    Curated prose only, for the same reason the symbol check is: the rendered page also carries
    keys discovered from the corpus, and a record whose key happens to read like a path -- a JSON
    key containing slashes -- would be validated as a citation and fail generation for a path no
    sentence ever claimed. Reproduced before this was narrowed.

    Three review rounds found this page asserting things about code that the code does not say:
    an enumeration copied from a doc comment instead of the projection, a relation inverted from a
    field name, a finding attributed to production code that only a test emits. Prose cannot be
    checked mechanically, but the pointers it hangs on can, and a citation that has rotted is the
    cheapest signal that the sentence around it has too. This does not verify that a citation
    supports what the sentence claims -- nothing here can -- so it is a floor, not a guarantee.
    """
    return [c for c in sorted(set(CITATION.findall(text))) if not (root / c.rstrip("/")).exists()]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=".", type=Path)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--check", action="store_true",
                    help="fail if the committed doc differs from freshly generated output")
    args = ap.parse_args()

    root = args.repo_root.resolve()
    out = root / args.out
    text = render(*discover(root))
    missing = verify_citations(curated_text(), root)
    if missing:
        for path in missing:
            print(f"error: cited path does not exist: {path}", file=sys.stderr)
        return 1
    unknown = verify_symbols(curated_text(), root, out, tracked_paths(root))
    if unknown:
        for name in unknown:
            print(f"error: cited symbol appears nowhere in the tree: {name}", file=sys.stderr)
        return 1
    if args.check:
        current = out.read_text(encoding="utf-8") if out.exists() else ""
        if current != text:
            print(f"{args.out} is stale; re-run this generator")
            return 1
        print(f"{args.out} is current")
        return 0
    out.parent.mkdir(parents=True, exist_ok=True)
    # encoding pinned: the page contains em dashes and arrows, and on a non-UTF-8 locale
    # an unpinned write_text raises mid-call and leaves the committed file truncated to 0 bytes.
    out.write_text(text, encoding="utf-8")
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
