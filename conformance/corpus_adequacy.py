#!/usr/bin/env python3
"""Mutation adequacy for a published conformance corpus, driven by a manifest.

Standard library only. No Assay import, no pip install, no network.

    python3 conformance/corpus_adequacy.py <manifest.json>
    python3 conformance/corpus_adequacy.py <manifest.json> --json

WHY A CORPUS NEEDS THIS AND A TEST SUITE DOES NOT
-------------------------------------------------
Outcome coverage is not rule coverage. A corpus can reach every declared
outcome while some rule never decides anything, because another rule reaches
the same outcome first on every vector it would have caught. Mutation adequacy
is the criterion that finds that, and it outperforms structural criteria such
as line and branch coverage for exactly this question.

The bar here is higher than the usual one. In ordinary mutation testing the
artifact under test is a test suite, a surviving mutant is a gap in confidence,
and a score near 80% is a working target. Here the artifact under test is a
*published corpus whose digest is the contract*. A surviving mutant means an
implementer can delete that rule, reproduce the pinned digest, and be
indistinguishable from a conforming implementation. That is not a confidence
gap, it is a hole in the contract. So the required score is 100% of
non-equivalent mutants.

WHAT THIS TOOL CAN AND CANNOT GENERALIZE, STATED PLAINLY
---------------------------------------------------------
It cannot infer a corpus's rules from arbitrary source. That would be a static
analysis project, and a tool that guessed would report a score it had not
earned. What is portable is the *method*, so a corpus that wants to be measured
declares its own mutants in a manifest:

  - one mutant per DECLARED RULE, not per line. A surviving mutant then names
    the rule an implementation could omit, instead of naming a line number.
    Generic operators would produce mostly-equivalent noise over rules this
    small.
  - ordinal axes are PERMUTED, not deleted. Deletion is the wrong operator for
    a ladder: the ordering lives in a table a comparison reads, not in a branch
    a mutant can cut. Declare flatten-to-top, flatten-to-bottom and invert.
  - equivalent mutants are DECLARED WITH A REASON, never inferred. Deciding
    mutant equivalence is undecidable in general, so a tool that claimed to
    detect it would be lying.
  - a crash is a KILL, reported separately, because "raises without this rule"
    says more about the rule than "returns something else".

WHAT THE PERCENTAGE IS A PERCENTAGE OF
---------------------------------------
100% here means 100% of the rules THE AUTHOR DECLARED. It does not mean 100% of
the rules the implementation has. A rule nobody declared is invisible to this
check, and there is no honest mechanical fix for that inside a manifest-driven
design: the manifest is written by the same hand as the corpus.

That is why the report never prints a bare percentage. It prints the numerator,
the denominator, the count declared equivalent, the count declared out of scope,
and the ratio between excluded and measured. A score reported without those is a
percentage target wearing a different coat, because an author can exclude almost
everything and still print 100%.

For the same reason an out_of_scope mutant must carry a stated reason. It leaves
the denominator exactly as a declared-equivalent one does, so it carries the same
obligation.

SELF-COVERAGE, APPLIED TO THIS CHECK
-------------------------------------
A group present in the corpus that declares no mutant is the same defect one
level up: the check would silently cover less than its name claims. That is a
hard failure here, not a warning.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import subprocess
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_run import (  # noqa: E402
    OUTPUT_CAP_BYTES, _OutputTooLarge, _run_capped,
)

SCHEMA = "assay.corpus_adequacy.manifest.v0"


class ManifestError(Exception):
    """The manifest does not describe a measurable corpus."""


def _req(obj: dict, key: str, where: str):
    if key not in obj:
        raise ManifestError("%s: missing required key %r" % (where, key))
    return obj[key]


def load_manifest(path: Path) -> dict:
    m = json.loads(path.read_text(encoding="utf-8"))
    if m.get("schema") != SCHEMA:
        raise ManifestError("schema must be %r, got %r" % (SCHEMA, m.get("schema")))
    base = path.parent
    _req(m, "vectors", "manifest")
    if m.get("runner", "module") == "module":
        _req(m, "implementation", "manifest")
    m["_impl_path"] = (base / m["implementation"]).resolve() if m.get("implementation") else None
    m["_vectors_path"] = (base / m["vectors"]).resolve()
    m.setdefault("entrypoint", "evaluate")
    # group_key is OPTIONAL. A corpus with no axis column is one group; forcing it to invent
    # an axis would be the tool bending the corpus to fit itself.
    m.setdefault("group_key", None)
    m.setdefault("id_key", "vector_id")
    m.setdefault("inputs_key", "inputs")
    m.setdefault("vectors_key", "vectors")
    # Which vector fields are passed positionally to the entrypoint. Signatures differ between
    # corpora and a fixed arity would exclude every corpus that did not guess the same one.
    m.setdefault("entrypoint_args",
                 [k for k in (m["group_key"], m["inputs_key"]) if k is not None])
    m.setdefault("default_group", "all")
    m.setdefault("known_holes", {})
    m["_corpus_digest"] = None
    if m["known_holes"]:
        for key in ("corpus_digest_file", "corpus_digest_key"):
            _req(m, key, "manifest (known_holes declared)")
        dp = (base / m["corpus_digest_file"]).resolve()
        if not dp.is_file():
            raise ManifestError("corpus_digest_file not found: %s" % dp)
        m["_corpus_digest"] = json.loads(dp.read_text(encoding="utf-8"))[m["corpus_digest_key"]]
        for digest, entries in m["known_holes"].items():
            for i, e in enumerate(entries):
                for key in ("label", "reason", "recorded"):
                    _req(e, key, "known_holes[%s][%d]" % (digest, i))
                if not str(e["reason"]).strip():
                    raise ManifestError("known_holes[%s][%d] %r: a hole needs a stated reason"
                                        % (digest, i, e["label"]))
    m.setdefault("runner", "module")
    if m["runner"] not in ("module", "process"):
        raise ManifestError("runner must be module or process, got %r" % m["runner"])
    if m["runner"] == "process":
        for key in ("build", "entrypoint_command", "outcome_from"):
            _req(m, key, "manifest (runner=process)")
        srcs = m.get("implementation_sources") or [m["implementation"]]
        m["_source_paths"] = [(base / x).resolve() for x in srcs]
        for sp in m["_source_paths"]:
            if not sp.is_file():
                raise ManifestError("implementation source not found: %s" % sp)
        m.setdefault("repo_root", ".")
        m["_repo_root"] = (base / m["repo_root"]).resolve()
        m.setdefault("vector_path_key", "path")
        m.setdefault("build_timeout", 1800)
        m.setdefault("vector_timeout", 120)
    m.setdefault("mutants", {})
    m.setdefault("equivalent", {})
    if not m["mutants"]:
        raise ManifestError("manifest declares no mutants; there is nothing to measure")
    for group, entries in m["mutants"].items():
        for i, e in enumerate(entries):
            for key in ("label", "anchor", "replacement"):
                _req(e, key, "mutants[%s][%d]" % (group, i))
            e.setdefault("scope", "declared")
            e.setdefault("control", False)
            if e["control"] and e["scope"] != "declared":
                raise ManifestError("mutants[%s][%d] %r: a control cannot be out_of_scope"
                                    % (group, i, e["label"]))
            if e["scope"] not in ("declared", "out_of_scope"):
                raise ManifestError("mutants[%s][%d] %r: scope must be declared or out_of_scope"
                                    % (group, i, e["label"]))
            # An out-of-scope mutant leaves the denominator exactly as an equivalent one does,
            # so it carries the same obligation: a stated reason, never a bare exclusion.
            if e["scope"] == "out_of_scope" and not str(e.get("reason", "")).strip():
                raise ManifestError(
                    "mutants[%s][%d] %r: an out_of_scope mutant needs a stated reason. It leaves "
                    "the denominator like an equivalent one, so it carries the same obligation"
                    % (group, i, e["label"]))
            if not e["anchor"]:
                raise ManifestError(
                    "mutants[%s][%d] %r: the anchor is empty. An empty anchor matches everywhere, "
                    "corrupts the source and is then counted as a kill" % (group, i, e["label"]))
            if e["anchor"] == e["replacement"]:
                raise ManifestError(
                    "mutants[%s][%d] %r: anchor and replacement are identical, so it mutates nothing"
                    % (group, i, e["label"]))
    for group, entries in m["equivalent"].items():
        for i, e in enumerate(entries):
            for key in ("label", "reason"):
                _req(e, key, "equivalent[%s][%d]" % (group, i))
            if not str(e["reason"]).strip():
                raise ManifestError(
                    "equivalent[%s][%d] %r: an equivalence needs a stated reason, never a bare claim"
                    % (group, i, e["label"]))
    return m


def _load_module(source: str, tag: str, tmp: Path):
    path = tmp / ("impl_%s.py" % tag)
    path.write_text(source, encoding="utf-8")
    spec = importlib.util.spec_from_file_location("adequacy_%s" % tag, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _acknowledged_holes(m: dict) -> dict:
    """Holes acknowledged against the corpus digest that is actually present.

    Pinned deliberately. An acknowledgement is a statement about ONE corpus, so a
    corpus that moves loses every acknowledgement and the holes reappear as
    survivors. Without that, this becomes the escape hatch out_of_scope nearly was.
    """
    if not m.get("_corpus_digest"):
        return {}
    return {e["label"]: e for e in m["known_holes"].get(m["_corpus_digest"], [])}


def _group_of(v: dict, m: dict) -> str:
    return v[m["group_key"]] if m["group_key"] else m["default_group"]


def _outcomes(fn, vectors: list[dict], m: dict) -> tuple[dict, list[str]]:
    """(outcome per vector id, ids that raised). A raise is a behaviour change."""
    outcomes, raised = {}, []
    for v in vectors:
        vid = v[m["id_key"]]
        try:
            outcomes[vid] = fn(*[v[k] for k in m["entrypoint_args"]])
        except Exception:  # noqa: BLE001 - any raise is the signal, not an error
            raised.append(vid)
    return outcomes, raised



# ---------------------------------------------------------------------------
# process runner: a compiled implementation behind a command line
# ---------------------------------------------------------------------------


class _SourceGuard:
    """Restores every mutated source file, including on crash.

    The adapter edits files in the working tree. A restore that only happens on
    the happy path leaks a mutant into a commit, so the originals are captured
    up front and rewritten in a finally block.
    """

    def __init__(self, paths: list[Path]) -> None:
        self.original = {p: p.read_bytes() for p in paths}

    def restore(self) -> None:
        for path, data in self.original.items():
            if path.read_bytes() != data:
                path.write_bytes(data)

    def verify_clean(self) -> list[str]:
        return [str(p) for p, d in self.original.items() if p.read_bytes() != d]


def _tree_is_dirty(repo_root: Path, paths: list[Path]) -> list[str]:
    """Declared sources with uncommitted changes. Mutating those loses work."""
    try:
        out = subprocess.run(["git", "-C", str(repo_root), "status", "--porcelain", "--"]
                             + [str(p) for p in paths],
                             capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return []          # no git: nothing to protect, and nothing to claim
    return [ln[3:] for ln in out.stdout.splitlines() if ln.strip()]


def _build(m: dict) -> tuple[bool, str]:
    try:
        p = _run_capped(list(m["build"]), m["_repo_root"], timeout=m["build_timeout"])
    except _OutputTooLarge:
        return False, "build output exceeded the ceiling"
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, "build could not complete: %r" % (exc,)
    if p.returncode != 0:
        tail = [ln for ln in (p.stdout + p.stderr).splitlines()
                if "error" in ln.lower()][-3:]
        return False, " | ".join(tail)[:300] or "build exited %d" % p.returncode
    return True, "built"


def _process_outcomes(m: dict, vectors: list[dict]) -> tuple[dict, list[str]]:
    """Run the built command once per vector. A vector that cannot be read is a raise."""
    outcomes, raised = {}, []
    for v in vectors:
        vid = v[m["id_key"]]
        cmd = [str(x).replace("{vector}", str((m["_repo_root"] / v[m["vector_path_key"]]).resolve()))
               for x in m["entrypoint_command"]]
        try:
            p = _run_capped(cmd, m["_repo_root"], timeout=m["vector_timeout"])
            doc = json.loads(p.stdout)
            keys = m["outcome_from"]
            # A single key can collapse every rejection onto one value and lose all
            # discrimination, so a list is allowed and compared as a tuple.
            outcomes[vid] = (tuple(doc.get(k) for k in keys) if isinstance(keys, list)
                             else doc.get(keys))
        except Exception:  # noqa: BLE001 - unreadable output is a behaviour change
            raised.append(vid)
    return outcomes, raised



def _run_process(m: dict, manifest_path: Path) -> dict:
    """Mutate declared sources, rebuild, and run the corpus against the binary."""
    doc = json.loads(m["_vectors_path"].read_text(encoding="utf-8"))
    all_vectors = doc[m["vectors_key"]] if isinstance(doc, dict) else doc
    failures: list[str] = []

    dirty = _tree_is_dirty(m["_repo_root"], m["_source_paths"])
    if dirty:
        raise ManifestError(
            "declared sources have uncommitted changes: %s. This adapter edits them in "
            "place, so it refuses to run rather than risk losing that work" % dirty)

    groups_in_corpus = {_group_of(v, m) for v in all_vectors}
    unmutated = sorted(groups_in_corpus - set(m["mutants"]))
    if unmutated:
        failures.append("groups present in the corpus with no declared mutants: %s" % unmutated)
    stale = sorted(set(m["mutants"]) - groups_in_corpus)
    if stale:
        failures.append("mutants declared for groups not in the corpus: %s" % stale)

    if not any(mut.get("control") for muts in m["mutants"].values() for mut in muts):
        failures.append("no control mutant declared. Without one, a run of all-survivors "
                        "cannot be told apart from a harness that detects nothing")
    acknowledged = _acknowledged_holes(m)
    stale = set(acknowledged) - {mu["label"] for ms in m["mutants"].values() for mu in ms}
    if stale:
        failures.append("known_holes name mutants that do not exist: %s" % sorted(stale))
    results, killed, survived, equivalent, out_of_scope, unproved = [], 0, 0, 0, 0, 0
    known_holes = 0
    guard = _SourceGuard(m["_source_paths"])
    try:
        ok, detail = _build(m)
        if not ok:
            raise ManifestError("the UNMUTATED tree does not build: %s" % detail)

        baselines = {}
        for group in sorted(m["mutants"]):
            vectors = [v for v in all_vectors if _group_of(v, m) == group]
            if not vectors:
                failures.append("%s: no vectors, so its mutants cannot be scored" % group)
                continue
            base, raised = _process_outcomes(m, vectors)
            if raised:
                failures.append("%s: the UNMUTATED binary failed on %s" % (group, raised))
                continue
            baselines[group] = (vectors, base)

        for group in sorted(m["mutants"]):
            if group not in baselines:
                continue
            vectors, baseline = baselines[group]
            for mut in m["mutants"][group]:
                scope = mut.get("scope", "declared")
                hits = [(sp, sp.read_text(encoding="utf-8").count(mut["anchor"]))
                        for sp in m["_source_paths"]]
                total = sum(n for _, n in hits)
                if total == 0:
                    failures.append("%s / %s: anchor not found in any declared source"
                                    % (group, mut["label"]))
                    continue
                if total > 1:
                    failures.append(
                        "%s / %s: the anchor occurs %d times across the declared sources, so "
                        "the substitution would pick one arbitrarily. Make it unique"
                        % (group, mut["label"], total))
                    continue

                target = next(sp for sp, n in hits if n == 1)
                original = target.read_text(encoding="utf-8")
                target.write_text(original.replace(mut["anchor"], mut["replacement"], 1),
                                  encoding="utf-8")
                try:
                    built, detail = _build(m)
                    if not built:
                        # Cursor's ruling on the design: rustc exit 1 yields no verdict, so
                        # the corpus never saw this mutant. Counting it killed would let a
                        # typo in the substitution print as "rule covered". Measure a
                        # load-bearing arm with a variant that COMPILES, or declare it
                        # equivalent.
                        results.append({"group": group, "label": mut["label"],
                                        "verdict": "unproved", "scope": scope, "moved": 0,
                                        "how": "the mutant does not build, so the corpus was "
                                               "never run against it: %s" % detail})
                        unproved += 1
                        continue
                    out, raised = _process_outcomes(m, vectors)
                finally:
                    target.write_text(original, encoding="utf-8")

                moved = [vid for vid, val in out.items() if baseline.get(vid) != val]
                if mut.get("control"):
                    # A control exists to prove the harness can detect ANYTHING. It is
                    # never scored: counting it would inflate the very number it exists
                    # to make trustworthy. A control that survives means the run says
                    # nothing at all about the corpus.
                    ok = bool(raised or moved)
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "control-killed" if ok else "control-SURVIVED",
                                    "scope": scope, "moved": len(moved),
                                    "how": ("harness detects a change on this path"
                                            if ok else "THE HARNESS DETECTS NOTHING")})
                    if not ok:
                        failures.append(
                            "control %r survived: the harness cannot detect a change on this "
                            "path, so every other verdict in this run is meaningless"
                            % mut["label"])
                    continue
                if raised or moved:
                    results.append({"group": group, "label": mut["label"], "verdict": "killed",
                                    "scope": scope, "moved": len(moved),
                                    "how": ("raises on %d vector(s)" % len(raised)) if raised
                                           else "%d vector(s) moved" % len(moved)})
                    killed += 1
                elif scope == "out_of_scope":
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "unexercised", "scope": scope, "moved": 0,
                                    "how": "out of scope: %s" % mut["reason"]})
                    out_of_scope += 1
                elif mut["label"] in acknowledged:
                    ack = acknowledged[mut["label"]]
                    # A KNOWN HOLE is not a scope statement. The corpus does claim this
                    # rule, the rule is genuinely unexercised, and that fact is recorded
                    # against ONE digest rather than fixed today. It stays loud.
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "known-hole", "scope": scope, "moved": 0,
                                    "how": "KNOWN HOLE against %s, recorded %s: %s"
                                           % (m["_corpus_digest"][:19], ack["recorded"],
                                              ack["reason"])})
                    known_holes += 1
                else:
                    results.append({"group": group, "label": mut["label"], "verdict": "survived",
                                    "scope": scope, "moved": 0,
                                    "how": "no vector distinguishes it. An implementation can "
                                           "delete this rule and still reproduce the digest"})
                    survived += 1

            for eq in m["equivalent"].get(group, []):
                results.append({"group": group, "label": eq["label"], "verdict": "equivalent",
                                "how": eq["reason"], "moved": 0})
                equivalent += 1
    finally:
        guard.restore()
        leaked = guard.verify_clean()
        if leaked:
            failures.append("SOURCES NOT RESTORED: %s" % leaked)
        _build(m)   # leave the tree with a binary built from the real source

    fixed = [r["label"] for r in results
             if r["verdict"] == "killed" and r["label"] in acknowledged]
    if fixed:
        failures.append("known_holes still acknowledge rules the corpus now exercises: %s. "
                        "Remove them; a stale acknowledgement hides a real regression later"
                        % sorted(fixed))
    denom = killed + survived
    # No denominator means no measurement. Printing 100% over zero is the same
    # defect as excluding everything and printing 100%.
    score = None if denom == 0 else round(100.0 * killed / denom, 1)
    if unproved:
        failures.append("%d mutant(s) never ran, so this corpus was not measured against them"
                        % unproved)
    if survived:
        failures.append("%d mutant(s) survived; the required score is 100%% of non-equivalent "
                        "mutants" % survived)
    if denom == 0:
        failures.append(
            "nothing was measured: every declared in-scope rule is either a known hole, "
            "declared equivalent or out of scope. There is no adequacy result here"
            if (known_holes or equivalent or out_of_scope)
            else "no non-equivalent mutants were scored, so no adequacy was measured")

    return {"schema": "assay.corpus_adequacy.report.v0", "manifest": str(manifest_path),
            "runner": "process", "killed": killed, "survived": survived,
            "known_holes": known_holes, "corpus_digest": m.get("_corpus_digest"),
            "equivalent": equivalent, "unexercised_out_of_scope": out_of_scope,
            "unproved": unproved,
            "declared_total": (killed + survived + equivalent + out_of_scope + unproved
                               + known_holes),
            "out_of_scope_ratio": (None if denom == 0 else round(out_of_scope / denom, 2)),
            "score_percent": score,
            "score_means": ("percent of author-declared in-scope rules killed; NOT percent of "
                            "the rules the implementation actually has"),
            "mutants": results, "failures": failures, "adequate": not failures}


def run(manifest_path: Path) -> dict:
    m = load_manifest(manifest_path)
    if m["runner"] == "process":
        return _run_process(m, manifest_path)
    source = m["_impl_path"].read_text(encoding="utf-8")
    doc = json.loads(m["_vectors_path"].read_text(encoding="utf-8"))
    all_vectors = doc[m["vectors_key"]] if isinstance(doc, dict) else doc

    failures: list[str] = []
    groups_in_corpus = {_group_of(v, m) for v in all_vectors}

    # Self-coverage: the reach of this check must not narrow silently.
    unmutated = sorted(groups_in_corpus - set(m["mutants"]))
    if unmutated:
        failures.append(
            "groups present in the corpus with no declared mutants: %s. Declare a mutant per "
            "rule, or this check covers less than its name claims" % unmutated)
    stale = sorted(set(m["mutants"]) - groups_in_corpus)
    if stale:
        failures.append("mutants declared for groups not in the corpus: %s" % stale)

    acknowledged = _acknowledged_holes(m)
    stale = set(acknowledged) - {mu["label"] for ms in m["mutants"].values() for mu in ms}
    if stale:
        failures.append("known_holes name mutants that do not exist: %s" % sorted(stale))
    results, killed, survived, equivalent, out_of_scope = [], 0, 0, 0, 0
    unproved = known_holes = 0
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        base_mod = _load_module(source, "base", tmp)
        base_fn = getattr(base_mod, m["entrypoint"], None)
        if base_fn is None:
            raise ManifestError("implementation has no entrypoint %r" % m["entrypoint"])

        for group in sorted(m["mutants"]):
            vectors = [v for v in all_vectors if _group_of(v, m) == group]
            if not vectors:
                failures.append("%s: no vectors, so its mutants cannot be scored" % group)
                continue
            baseline, base_raised = _outcomes(base_fn, vectors, m)
            if base_raised:
                failures.append("%s: the UNMUTATED implementation raised on %s"
                                % (group, base_raised))
                continue

            for idx, mut in enumerate(m["mutants"][group]):
                occurrences = source.count(mut["anchor"])
                if occurrences == 0:
                    failures.append(
                        "%s / %s: anchor not found in %s. The rule was renamed or removed and the "
                        "mutant is measuring nothing" % (group, mut["label"], m["implementation"]))
                    continue
                if occurrences > 1:
                    # Substituting the first of several is a coin flip about which rule is being
                    # measured, and a mangled substitution is then scored as a kill.
                    failures.append(
                        "%s / %s: the anchor occurs %d times in %s, so the substitution would pick "
                        "one arbitrarily and any breakage would be scored as a kill. Make the "
                        "anchor unique" % (group, mut["label"], occurrences, m["implementation"]))
                    continue
                scope = mut.get("scope", "declared")
                mutated = source.replace(mut["anchor"], mut["replacement"], 1)
                try:
                    mod = _load_module(mutated, "%s_%d" % (group.replace("-", "_"), idx), tmp)
                    fn = getattr(mod, m["entrypoint"])
                except Exception as exc:  # noqa: BLE001
                    # A mutant that never loaded was never shown to the corpus, so the
                    # corpus said nothing about it. Counting that as a kill lets a typo
                    # in the substitution print as "rule covered". The same argument
                    # rules out treating a Rust build failure as a kill; measure a
                    # load-bearing rule with a variant that RUNS, or declare it equivalent.
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "unproved", "scope": scope, "moved": 0,
                                    "how": "the mutant does not load, so the corpus never "
                                           "saw it and said nothing about this rule: %r" % (exc,)})
                    unproved += 1
                    continue
                out, raised = _outcomes(fn, vectors, m)
                moved = [vid for vid, val in out.items() if baseline.get(vid) != val]
                if mut.get("control"):
                    ok = bool(raised or moved)
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "control-killed" if ok else "control-SURVIVED",
                                    "scope": scope, "moved": len(moved),
                                    "how": ("harness detects a change on this path"
                                            if ok else "THE HARNESS DETECTS NOTHING")})
                    if not ok:
                        failures.append(
                            "control %r survived: the harness cannot detect a change on this "
                            "path, so every other verdict in this run is meaningless"
                            % mut["label"])
                    continue
                if raised:
                    results.append({"group": group, "label": mut["label"], "verdict": "killed",
                                    "scope": scope, "how": "raises on %d vector(s)" % len(raised),
                                    "moved": len(moved), "raised": raised})
                    killed += 1
                elif moved:
                    results.append({"group": group, "label": mut["label"], "verdict": "killed",
                                    "scope": scope, "how": "%d vector(s) moved" % len(moved),
                                    "moved": len(moved)})
                    killed += 1
                elif scope == "out_of_scope":
                    # Declared by the manifest as a rule this corpus does not claim to cover.
                    # Reported every run, never scored: adequacy is relative to declared scope,
                    # and scoring a rule nobody claimed manufactures a hole that is not one.
                    results.append({
                        "group": group, "label": mut["label"], "verdict": "unexercised",
                        "scope": scope, "moved": 0,
                        # Print the stated reason, exactly as a declared equivalent does.
                        # "each with a stated reason" without showing one is an assertion.
                        "how": "out of scope: %s" % mut["reason"]})
                    out_of_scope += 1
                elif mut["label"] in acknowledged:
                    ack = acknowledged[mut["label"]]
                    results.append({"group": group, "label": mut["label"],
                                    "verdict": "known-hole", "scope": scope, "moved": 0,
                                    "how": "KNOWN HOLE against %s, recorded %s: %s"
                                           % (str(m["_corpus_digest"])[:19], ack["recorded"],
                                              ack["reason"])})
                    known_holes += 1
                else:
                    results.append({
                        "group": group, "label": mut["label"], "verdict": "survived",
                        "scope": scope, "moved": 0,
                        "how": "no vector distinguishes it. An implementation can delete this rule, "
                               "reproduce the pinned digest, and be indistinguishable from a "
                               "conforming one. The corpus needs a vector where this rule, and only "
                               "this rule, decides the outcome"})
                    survived += 1

            for eq in m["equivalent"].get(group, []):
                results.append({"group": group, "label": eq["label"], "verdict": "equivalent",
                                "how": eq["reason"], "moved": 0})
                equivalent += 1

    fixed = [r["label"] for r in results
             if r["verdict"] == "killed" and r["label"] in acknowledged]
    if fixed:
        failures.append("known_holes still acknowledge rules the corpus now exercises: %s. "
                        "Remove them; a stale acknowledgement hides a real regression later"
                        % sorted(fixed))
    denom = killed + survived
    score = None if denom == 0 else round(100.0 * killed / denom, 1)
    if unproved:
        # Not a soft warning: an unproved mutant means the measurement did not happen,
        # and a score computed over the rest reports more than the run established.
        failures.append("%d mutant(s) never ran, so this corpus was not measured against "
                        "them. Fix the substitution or declare them equivalent" % unproved)
    if survived:
        failures.append("%d mutant(s) survived; the required score is 100%% of non-equivalent "
                        "mutants" % survived)
    if denom == 0:
        failures.append(
            "nothing was measured: every declared in-scope rule is either a known hole, "
            "declared equivalent or out of scope. There is no adequacy result here"
            if (known_holes or equivalent or out_of_scope)
            else "no non-equivalent mutants were scored, so no adequacy was measured")

    return {"schema": "assay.corpus_adequacy.report.v0", "manifest": str(manifest_path),
            "killed": killed, "survived": survived, "equivalent": equivalent,
            "known_holes": known_holes, "corpus_digest": m.get("_corpus_digest"),
            "unexercised_out_of_scope": out_of_scope, "unproved": unproved,
            "declared_total": (killed + survived + equivalent + out_of_scope + unproved
                               + known_holes),
            "out_of_scope_ratio": (None if (killed + survived) == 0
                                   else round(out_of_scope / (killed + survived), 2)),
            "score_means": ("percent of author-declared in-scope rules killed; NOT percent of the "
                            "rules the implementation actually has"),
            "score_percent": score, "mutants": results, "failures": failures,
            "adequate": not failures}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("manifest", type=Path)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    try:
        rep = run(args.manifest)
    except (ManifestError, OSError, json.JSONDecodeError) as exc:
        print("could not measure: %s" % exc, file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(rep, indent=2, sort_keys=True))
    else:
        for r in rep["mutants"]:
            print("%-22s %-9s %s" % (r["group"], r["verdict"], r["label"]))
            if r["verdict"] != "killed":
                print("    %s" % r["how"])
        print()
        # Never a bare percentage. A score reported without its denominator and its
        # exclusions is a percentage target wearing a different coat: an author can
        # exclude almost everything and still print 100%.
        pct = ("no result" if rep["score_percent"] is None
               else "%.1f%%" % rep["score_percent"])
        print("%d of %d DECLARED in-scope rules killed (%s). %d declared equivalent, "
              "%d declared out of scope, %d unproved. %d rules declared in total."
              % (rep["killed"], rep["killed"] + rep["survived"], pct,
                 rep["equivalent"], rep["unexercised_out_of_scope"], rep["unproved"],
                 rep["declared_total"]))
        if rep["score_percent"] is not None:
            print("This is %.1f%% of what the AUTHOR DECLARED, not of the rules the "
                  "implementation has. A rule nobody declared is invisible to this check."
                  % rep["score_percent"])
        if rep["unexercised_out_of_scope"]:
            print("out of scope (%d, each with a stated reason): real gaps in what the corpus "
                  "covers, not holes in what it claims" % rep["unexercised_out_of_scope"])
        if rep["out_of_scope_ratio"] is not None and rep["out_of_scope_ratio"] > 1.0:
            print("NOTE: more rules are excluded than measured (ratio %.2f). The score is real "
                  "but it is a statement about a minority of the declared rules."
                  % rep["out_of_scope_ratio"])
        for f in rep["failures"]:
            print("FAIL: %s" % f)
        if rep.get("known_holes"):
            # Louder than the pass line, and pinned. An acknowledgement that outlives
            # the corpus it was made about is worse than no acknowledgement.
            print("%d KNOWN HOLE(S) recorded against %s. These are rules the corpus DOES "
                  "claim and does NOT exercise. The acknowledgement is pinned to that digest "
                  "and expires the moment the corpus changes."
                  % (rep["known_holes"], rep.get("corpus_digest")))
        if rep["adequate"]:
            if rep["out_of_scope_ratio"] is not None and rep["out_of_scope_ratio"] > 1.0:
                # The closing line is what gets quoted. It may not read as unqualified
                # success when most declared rules were excluded from the measurement.
                print("mutation-adequacy check passed for the DECLARED IN-SCOPE rules only "
                      "(%d of %d rules declared here were excluded from it)"
                      % (rep["unexercised_out_of_scope"], rep["declared_total"]))
            else:
                suffix = (" -- but see the known holes above" if rep.get("known_holes") else "")
                print("mutation-adequacy check passed: every non-equivalent mutant is killed"
                      + suffix)

    return 0 if rep["adequate"] else 1


if __name__ == "__main__":
    sys.exit(main())
