#!/usr/bin/env python3
"""Fail when the published adequacy documents stop agreeing with the measurement.

    python3 conformance/adequacy/check_published_numbers.py
    python3 conformance/adequacy/check_published_numbers.py --json

`conformance/INDEX.md` and `conformance/privileged-mcp-action-v0/ERRATA.md` publish
measured mutation-adequacy numbers. They are now GENERATED from `INDEX.md.in` and
`ERRATA.md.in` by `publish_numbers.py`: the narrative is hand-written, and every
measured cell is a token projected from `results.json`.

WHAT CHANGED, AND WHY THE OLD MACHINERY IS GONE

This file used to hold prose to JSON with a bindings block: each published
sentence was registered with the expressions that produced its numbers, every
number in the wording had to be among the asserted values, and a regex swept the
rest of the document for unregistered `N of M` and `NN.N%` shapes.

The bindings were the wrong mechanism. A checker over authored prose can only say
that a registered cell agrees with the JSON. The sentence beside it still says
whatever its author typed, and a figure this revision did not measure can still be
published as a present-tense fact. The failure is authorship, so authorship is
what changed: a measured number is no longer something an author can write. The
bindings and their assertion evaluator existed to detect what projection makes
impossible, and two mechanisms for one rule is the drift this repository keeps
finding, so they are deleted rather than kept alongside.

THE SWEEP IS NOT IN THAT CATEGORY AND STAYS. Regenerate-and-compare has exactly
one failure mode: someone edited the output. It cannot see a number-shaped claim
that was never tokenised, because a freshly invented figure in the narrative
regenerates byte-identically. The sweep is the only mechanism here that forces a
NEW number to be justified rather than merely consistent. It changed subject --
from the rendered document to the TEMPLATE's authored prose -- and it kept its
rule: any `N of M` or `NN.N%` that is not a projection token and not declared
with a reason in `unprojected_numbers.json` is red.

WHAT IS CHECKED HERE

1. Self-coverage. Every `*.manifest.json` on disk has a row in results.json and
   every row has a manifest. A checker that silently skips a corpus is the same
   failure one level up, so a new manifest with no measurement is red, not absent.
2. Subject provenance. Every row says which state of the measured thing it
   describes, and a row measuring a repository we do not own names that
   repository's commit, is not measured against a dirty tree, and has that commit
   published where a reader can see it.
3. Staleness. A row whose declared `depends_on` moved since it was taken no longer
   describes this revision. Computed by `publish_numbers.stale_corpora`, which is
   the same function that makes such a row's tokens refuse to render a figure --
   one implementation, so the guard and the renderer cannot disagree about which
   rows are current.
4. Tool pin. Every manifest declares `tool_pin.commit`, every row was measured
   with the commit its manifest declares, and no published document hands a
   reproducer a different commit in hand-written prose.
5. Transcription. A row that was not re-derived by the tool must name the document
   it was transcribed from and quote it verbatim, and that quote must still occur
   in that document byte-for-byte (whitespace-normalised).
6. Projection. Each generated document is re-rendered from its template and
   compared BYTE FOR BYTE. A hand-edited number, a hand-edited narrative, a stale
   template, an unresolvable token: all one finding, and all fixed the same way.
7. Sweep. No number of adequacy shape may appear in a template's authored prose
   without being either a projection token or declared, with a reason, in
   `unprojected_numbers.json`.

THE GAP, NAMED RATHER THAN IMPLIED. Projection makes a measured cell unwritable.
It does not make the documents true.

  * The sweep knows two shapes, `N of M` and `NN.N%`. A claim written entirely in
    English words -- "the corpus isolates six" -- can be added by hand, and the
    tokens cover only the word-numbers already published.
  * Superseded scores, vector counts, the rfc8785 control's `8 of 31` and the
    hand-numbered list of twenty-two rules in ERRATA.md are hand-written on
    purpose, and nothing re-derives them.
  * A token can name the wrong field. `killed` where the sentence means `survived`
    renders a number that is true of the measurement and false in the sentence.
  * A `(judged)` figure is marked, not verified. Marking stops the mechanism from
    lending the tool's authority to a human judgement; it does not make the
    judgement right.
  * Only the documents in `publish_numbers.pairs()` are generated. A third file
    publishing adequacy numbers is unguarded until someone adds it there.
  * Staleness is judged over each row's declared `depends_on`. A rule can move in a
    file no manifest declares, and nothing here notices. That is the same
    declared-versus-observed gap one level up.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import publish_numbers  # noqa: E402

REPO = Path(__file__).resolve().parents[2]


def normalise(text: str) -> str:
    """Collapse whitespace so a re-wrapped paragraph is not a false failure.

    Line wrapping is not a number change, and a checker that goes red when an
    editor reflows a paragraph gets disabled rather than fixed.
    """
    return " ".join(text.split())


def check() -> list[str]:
    findings: list[str] = []

    adequacy = REPO / "conformance/adequacy"
    results = adequacy / "results.json"
    documents = [target for _, target in publish_numbers.pairs(REPO)]

    if not results.is_file():
        return ["%s does not exist. Nothing re-derives the published numbers; run "
                "conformance/adequacy/measure_all.py" % results.relative_to(REPO)]
    doc = json.loads(results.read_text(encoding="utf-8"))
    rows = doc.get("corpora", [])
    by_corpus = {r["corpus"]: r for r in rows}

    # ---- 1. self-coverage -------------------------------------------------
    on_disk = {p.name[: -len(".manifest.json")]: p
               for p in sorted(adequacy.glob("*.manifest.json"))}
    for missing in sorted(set(on_disk) - set(by_corpus)):
        findings.append("no measurement for %s. results.json covers %d of %d manifests; a "
                        "checker that skips a corpus is the failure it is checking for"
                        % (on_disk[missing].relative_to(REPO), len(by_corpus), len(on_disk)))
    for orphan in sorted(set(by_corpus) - set(on_disk)):
        findings.append("results.json carries %r, which has no manifest on disk. A result "
                        "outliving its manifest is a number about nothing" % orphan)

    # ---- 2. subject provenance for out-of-tree corpora ---------------------
    # A number about a repository we do not own says nothing checkable unless it
    # names which state of that repository it describes. Two were published that
    # way: rge-bench and observed-effect-v0, scored with no commit anywhere in
    # the prose or the results. Freshness is not the requirement and cannot be --
    # those repositories move without this diff seeing it -- so the requirement
    # is that the published claim carries the commit it is permanently true of.
    #
    # Projection now supplies that commit through a `subject_commit` token, which
    # is how the requirement is MET rather than a reason to drop it: delete the
    # token from a template and this rule is what notices.
    published = "\n".join(d.read_text(encoding="utf-8") for d in documents if d.is_file())
    for name, row in sorted(by_corpus.items()):
        subject = row.get("subject")
        if not subject:
            findings.append("%s records no subject. Every row must say which state of the "
                            "measured thing it describes, in-tree or otherwise" % name)
            continue
        if subject.get("kind") != "out_of_tree":
            continue
        for repo in subject.get("repos", []):
            commit = repo.get("commit")
            if not commit:
                findings.append("%s measures %s, which is not a git checkout, so this number "
                                "names no state at all" % (name, repo.get("path", "?")))
                continue
            if repo.get("dirty"):
                findings.append("%s was measured against %s with tracked modifications, so the "
                                "number describes a working tree nobody else has"
                                % (name, repo.get("repository") or repo.get("path")))
            if commit[:9] not in published:
                findings.append(
                    "%s is measured against %s at %s and no published document names that "
                    "commit. Provenance recorded only in results.json is provenance the reader "
                    "does not get" % (name, repo.get("repository") or "an external repository",
                                      commit[:9]))

    # ---- 3. is the measurement still current for the code -------------------
    # The lane re-derives a corpus only when its declared sources move. Every
    # other revision compares the documents against an older results.json and
    # goes green, which is accurate about the comparison and silent about the
    # measurement. A figure taken last week, read as a fact about today, is the
    # thing this file exists to stop.
    #
    # The question asked is not "was this re-run at HEAD", which would mark almost
    # every row stale and train people to ignore the finding. It is whether
    # anything the row DEPENDS ON has moved since it was taken. Delegated to the
    # renderer so that "which rows are stale" has exactly one answer: a row this
    # reports is a row whose tokens refuse to print a figure.
    for name, why in sorted(publish_numbers.stale_corpora(by_corpus, REPO).items()):
        findings.append("%s %s" % (name, why))

    # ---- 4. tool pin ------------------------------------------------------
    for name, path in sorted(on_disk.items()):
        manifest = json.loads(path.read_text(encoding="utf-8"))
        pin = (manifest.get("tool_pin") or {}).get("commit")
        if not pin:
            findings.append("%s declares no tool_pin.commit. Numbers measured with an unnamed "
                            "tool cannot be re-derived by anyone" % path.relative_to(REPO))
            continue
        row = by_corpus.get(name)
        if row and row.get("tool_commit") != pin:
            findings.append("%s was measured with corpus-adequacy %s but %s pins %s. Re-measure "
                            "or re-pin; a number and its tool must move together"
                            % (name, row.get("tool_commit"), path.name, pin))

    # ---- 5. transcription -------------------------------------------------
    for row in rows:
        prov = row.get("provenance") or {}
        if prov.get("kind") == "measured":
            continue
        if prov.get("kind") != "transcribed":
            findings.append("%s: provenance.kind must be 'measured' or 'transcribed', got %r"
                            % (row["corpus"], prov.get("kind")))
            continue
        src, quote = prov.get("from"), prov.get("quote")
        if not src or not quote:
            findings.append("%s is transcribed but names no source document and quote"
                            % row["corpus"])
            continue
        source = REPO / src
        if not source.is_file():
            findings.append("%s is transcribed from %s, which does not exist"
                            % (row["corpus"], src))
            continue
        if normalise(quote) not in normalise(source.read_text(encoding="utf-8")):
            findings.append("%s is transcribed from %s, but that document no longer contains "
                            "the quoted measurement. Either it was edited, in which case the "
                            "transcription is void, or re-run measure_all.py --only %s"
                            % (row["corpus"], src, row["corpus"]))

    # ---- 4b. the tool pin as a published document spells it -----------------
    # The tool commit ERRATA.md hands a reproducer is a projected token, so it
    # cannot part from the row by an author's edit. This rule stays because a
    # template may still hard-code a hash instead of using the token, and that is
    # exactly the pin that drifted before: the manifest was re-pinned to a newer
    # tool and the sentence kept handing out the old commit. Any full hash in a
    # document that talks about the tool is treated as the pin.
    for row in rows:
        commit = row.get("tool_commit")
        if not commit:
            continue
        for path in documents:
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8")
            if row["corpus"] not in text and path.name != "ERRATA.md":
                continue
            named = re.findall(r"\b([0-9a-f]{40})\b", text) if "corpus-adequacy" in text else []
            if named and not any(commit.startswith(c) or c.startswith(commit[:9])
                                 for c in named):
                findings.append(
                    "%s names corpus-adequacy %s in prose but %s was measured with %s. The "
                    "document hands a reproducer an instrument; a number measured with a "
                    "different one is not the run it describes"
                    % (path.relative_to(REPO), named[0][:9], row["corpus"], commit[:9]))

    # ---- 6. projection ----------------------------------------------------
    # The whole prose-side guarantee, in one comparison. Re-render each document
    # from its template and require the bytes on disk to be exactly that.
    findings.extend(publish_numbers.differences(REPO))

    # ---- 7. the sweep, over the templates ----------------------------------
    # Kept from the old checker and pointed somewhere new. Regenerate-and-compare
    # detects one thing: that someone edited the output. It is blind to a
    # number-shaped claim that was never a token, because a freshly invented
    # figure in the narrative regenerates byte-identically and reads to a
    # stranger exactly like a measurement. Projection stops a measured cell from
    # being edited; this is what stops a new one from being written.
    findings.extend(publish_numbers.unprojected_findings(REPO))

    return findings


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--json", action="store_true", help="machine-readable findings")
    args = ap.parse_args(argv)

    findings = check()
    if args.json:
        print(json.dumps({"schema": "assay.conformance.adequacy.check.v0",
                          "findings": findings, "ok": not findings}, indent=2))
    elif findings:
        print("published adequacy numbers disagree with the measurement:\n")
        for f in findings:
            print("  - %s\n" % f)
    else:
        print("published adequacy numbers agree with conformance/adequacy/results.json")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
