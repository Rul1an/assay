#!/usr/bin/env python3
"""Fail when a published adequacy number stops agreeing with the measurement.

    python3 conformance/adequacy/check_published_numbers.py
    python3 conformance/adequacy/check_published_numbers.py --json

`conformance/INDEX.md` and `conformance/privileged-mcp-action-v0/ERRATA.md` publish
measured mutation-adequacy numbers. Until this checker existed those numbers were
produced once, by hand, and nothing re-derived them: a rule deleted from a declared
implementation source changes what a corpus can transmit, changes no digest, and
would have left both documents silently wrong. A published claim nobody re-derives
is the exact defect this body of work exists to criticise, so the numbers were held
to a lower standard than the ones they replaced.

WHAT IS CHECKED

1. Self-coverage. Every `*.manifest.json` on disk has a row in results.json and
   every row has a manifest. A checker that silently skips a corpus is the same
   failure one level up, so a new manifest with no measurement is red, not absent.
2. Tool pin. Every manifest declares `tool_pin.commit`, and every row was measured
   with the commit its manifest declares.
3. Transcription. A row that was not re-derived by the tool must name the document
   it was transcribed from and quote it verbatim, and that quote must still occur
   in that document byte-for-byte (whitespace-normalised).
4. Prose. Every registered claim's numbers must equal the measurement, and the
   claim's exact wording must still occur in the document.
5. Sweep. No number of adequacy shape may appear in a checked document without
   being either derived from results.json or declared `not_derived` with a reason.

HOW PROSE IS TIED TO JSON, AND WHAT THAT DOES NOT PROTECT

A regex over free prose is fragile in both directions: it misses "twenty-two" and
it fires on "8 of 31 RFC 8785 vectors". So each document carries a *bindings
block* this checker owns, delimited by `<!-- BEGIN CHECKED NUMBERS -->`. Each
binding is a triple:

    text     the exact wording published in the document
    asserts  expressions over the measured row, with their expected values
    (locals) declared constants, each with a stated reason

and three separate obligations hold it in place:

  * every `asserts` expression is evaluated against results.json and must match,
    so editing the JSON without editing the prose is red;
  * every number appearing in `text` -- digits or English words -- must be among
    the asserted values, so a binding cannot carry a number it never checked;
  * `text` must still occur in the document, so editing the prose without editing
    the binding is red.

To change a published number you must therefore move all three together, and
moving all three means re-running the measurement.

THE GAP, NAMED RATHER THAN IMPLIED. This makes registered numbers un-rottable; it
does not make the documents' *prose* true. Three specific holes:

  * The sweep in check 5 only knows two shapes, `N of M` and `NN.N%`. A claim
    written entirely in English words -- "the corpus isolates six" -- can be added,
    or silently changed, without this checker noticing. The bindings cover the
    word-numbers that were already published; they cannot cover ones invented
    later.
  * Only the documents in DOCUMENTS are read. A third file publishing adequacy
    numbers is unguarded until someone adds it here.
  * A transcribed row (check 3) is pinned to a quote, not to a measurement. It
    goes stale the moment the implementation it describes moves, and nothing here
    can tell. That is the same declared-versus-observed gap one level up, and the
    only repair is running `measure_all.py` for that corpus.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ADEQUACY = REPO / "conformance/adequacy"
RESULTS = ADEQUACY / "results.json"

# Documents that MUST carry a bindings block. Discovery is not used for these two:
# a document that could opt out by deleting its block would be guarded only while
# someone remembered to keep it guarded.
DOCUMENTS = [
    REPO / "conformance/INDEX.md",
    REPO / "conformance/privileged-mcp-action-v0/ERRATA.md",
]

BEGIN = "<!-- BEGIN CHECKED NUMBERS -->"
END = "<!-- END CHECKED NUMBERS -->"

# The shapes a published adequacy number takes in these documents. Deliberately
# narrow: broadening it to every integer would make the not_derived list a
# transcription of the prose and nobody would maintain it.
SWEEP = re.compile(r"\b\d+ of \d+\b|\b\d+(?:\.\d+)?%")

_UNITS = {"zero": 0, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6,
          "seven": 7, "eight": 8, "nine": 9, "ten": 10, "eleven": 11, "twelve": 12,
          "thirteen": 13, "fourteen": 14, "fifteen": 15, "sixteen": 16, "seventeen": 17,
          "eighteen": 18, "nineteen": 19}
_TENS = {"twenty": 20, "thirty": 30, "forty": 40, "fifty": 50, "sixty": 60,
         "seventy": 70, "eighty": 80, "ninety": 90}
_WORDS: dict[str, int] = dict(_UNITS)
for _t, _tv in _TENS.items():
    _WORDS[_t] = _tv
    for _u, _uv in _UNITS.items():
        if _uv:
            _WORDS["%s-%s" % (_t, _u)] = _tv + _uv
# Longest first so "twenty-seven" is not read as "twenty".
_WORD_RE = re.compile(r"\b(%s)\b" % "|".join(sorted(_WORDS, key=len, reverse=True)),
                      re.IGNORECASE)

# An expression is arithmetic over the measured row. Anything else is refused
# rather than evaluated: this file runs in CI over repository content.
_EXPR_OK = re.compile(r"^[A-Za-z0-9_+\-*/(). ,]+$")


def normalise(text: str) -> str:
    """Collapse whitespace so a re-wrapped paragraph is not a false failure.

    Line wrapping is not a number change, and a checker that goes red when an
    editor reflows a paragraph gets disabled rather than fixed.
    """
    return " ".join(text.split())


def numbers_in(text: str) -> list[float]:
    found = [float(x) for x in re.findall(r"\d+(?:\.\d+)?", text)]
    found += [float(_WORDS[m.group(1).lower()]) for m in _WORD_RE.finditer(text)]
    return found


def evaluate(expr: str, namespace: dict) -> float:
    if not _EXPR_OK.match(expr):
        raise ValueError("expression contains characters this checker will not evaluate: %r"
                         % expr)
    return eval(expr, {"__builtins__": {}}, dict(namespace, round=round))  # noqa: S307


def strip_block(text: str) -> str:
    """Everything outside the bindings block.

    Found by a positive control: the block quotes the prose it binds, so a check
    that searched the whole document could be satisfied by the binding's own copy
    of the sentence. Editing the prose then left the guard green because the guard
    was reading itself.
    """
    if BEGIN in text and END in text:
        start, stop = text.index(BEGIN), text.index(END) + len(END)
        if stop > start:
            return text[:start] + text[stop:]
    return text


def load_bindings(doc: Path) -> tuple[dict, str]:
    """Return (bindings, document text with the block region removed)."""
    text = doc.read_text(encoding="utf-8")
    if text.count(BEGIN) != 1 or text.count(END) != 1:
        raise ValueError("%s must contain exactly one %s ... %s block"
                         % (doc.name, BEGIN, END))
    start = text.index(BEGIN)
    stop = text.index(END) + len(END)
    if stop < start:
        raise ValueError("%s: END marker precedes BEGIN" % doc.name)
    block = text[start:stop]
    fence = re.search(r"```json\n(.*?)\n```", block, re.DOTALL)
    if not fence:
        raise ValueError("%s: the checked-numbers block carries no ```json fence" % doc.name)
    return json.loads(fence.group(1)), text[:start] + text[stop:]


def aggregate_namespace(rows: list[dict]) -> dict:
    return {
        "corpora_total": len(rows),
        "measured": sum(1 for r in rows if r["score_percent"] is not None),
        "control_only": sum(1 for r in rows
                            if r["score_percent"] is None and r["control"] == "killed"),
        "transcribed": sum(1 for r in rows if r["provenance"]["kind"] == "transcribed"),
    }


def check() -> list[str]:
    findings: list[str] = []

    if not RESULTS.is_file():
        return ["%s does not exist. Nothing re-derives the published numbers; run "
                "conformance/adequacy/measure_all.py" % RESULTS.relative_to(REPO)]
    doc = json.loads(RESULTS.read_text(encoding="utf-8"))
    rows = doc.get("corpora", [])
    by_corpus = {r["corpus"]: r for r in rows}

    # ---- 1. self-coverage -------------------------------------------------
    on_disk = {p.name[: -len(".manifest.json")]: p
               for p in sorted(ADEQUACY.glob("*.manifest.json"))}
    for missing in sorted(set(on_disk) - set(by_corpus)):
        findings.append("no measurement for %s. results.json covers %d of %d manifests; a "
                        "checker that skips a corpus is the failure it is checking for"
                        % (on_disk[missing].relative_to(REPO), len(by_corpus), len(on_disk)))
    for orphan in sorted(set(by_corpus) - set(on_disk)):
        findings.append("results.json carries %r, which has no manifest on disk. A result "
                        "outliving its manifest is a number about nothing" % orphan)

    # ---- 1b. subject provenance for out-of-tree corpora --------------------
    # A number about a repository we do not own says nothing checkable unless it
    # names which state of that repository it describes. Two were published that
    # way: rge-bench and observed-effect-v0, scored with no commit anywhere in
    # the prose or the results. Freshness is not the requirement and cannot be --
    # those repositories move without this diff seeing it -- so the requirement
    # is that the published claim carries the commit it is permanently true of.
    published = "\n".join(strip_block(d.read_text(encoding="utf-8"))
                          for d in DOCUMENTS if d.is_file())
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

    # ---- 2. tool pin ------------------------------------------------------
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

    # ---- 3. transcription -------------------------------------------------
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
        source_text = strip_block(source.read_text(encoding="utf-8"))
        if normalise(quote) not in normalise(source_text):
            findings.append("%s is transcribed from %s, but that document no longer contains "
                            "the quoted measurement. Either it was edited, in which case the "
                            "transcription is void, or re-run measure_all.py --only %s"
                            % (row["corpus"], src, row["corpus"]))
        # The source document also pins the tool commit in PROSE, which is where it
        # drifts: the manifest can be re-pinned and the sentence left behind.
        if row.get("tool_commit") and row["tool_commit"] not in source_text:
            findings.append("%s names corpus-adequacy %s, which %s no longer mentions. The "
                            "document tells a reproducer which tool to use; a transcription "
                            "measured with a different one is not the run it describes"
                            % (row["corpus"], row["tool_commit"], src))

    # ---- 3b. the prose tool pin, whatever the provenance --------------------
    # This lived inside the transcription branch, so the moment a row stopped
    # being transcribed the rule went quiet. It caught a real staleness within
    # the hour: privileged-mcp-action-v0 was re-pinned to a newer tool and
    # ERRATA.md kept handing a reproducer the old commit. The document tells
    # someone which instrument to check out; whether we derived the row or
    # quoted it changes nothing about that sentence needing to be true.
    for row in rows:
        commit = row.get("tool_commit")
        if not commit:
            continue
        for path in DOCUMENTS:
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8")
            if row["corpus"] not in text and path.name != "ERRATA.md":
                continue
            # Not same-line: the sentence wraps, and a same-line regex silently
            # matched nothing, which is a guard that cannot fail. Any full commit
            # hash in a document that talks about the tool is treated as the pin.
            named = re.findall(r"\b([0-9a-f]{40})\b", text) if "corpus-adequacy" in text else []
            if named and not any(commit.startswith(c) or c.startswith(commit[:9])
                                 for c in named):
                findings.append(
                    "%s names corpus-adequacy %s in prose but %s was measured with %s. The "
                    "document hands a reproducer an instrument; a number measured with a "
                    "different one is not the run it describes"
                    % (path.relative_to(REPO), named[0][:9], row["corpus"], commit[:9]))

    # ---- 4 and 5. prose ---------------------------------------------------
    for path in DOCUMENTS:
        rel = path.relative_to(REPO)
        if not path.is_file():
            findings.append("%s does not exist" % rel)
            continue
        try:
            bindings, body = load_bindings(path)
        except ValueError as exc:
            findings.append(str(exc))
            continue
        except json.JSONDecodeError as exc:
            findings.append("%s: the checked-numbers block is not valid JSON: %s" % (rel, exc))
            continue

        haystack = normalise(body)
        for claim in bindings.get("claims", []):
            label = "%s: %r" % (rel, claim.get("text", "")[:60])
            text = claim.get("text")
            asserts = claim.get("asserts")
            if not text or not isinstance(asserts, dict) or not asserts:
                findings.append("%s: a claim needs a text and at least one assertion" % label)
                continue

            corpus = claim.get("corpus")
            if corpus == "*":
                namespace = aggregate_namespace(rows)
            elif corpus in by_corpus:
                namespace = {k: v for k, v in by_corpus[corpus].items()
                             if isinstance(v, (int, float)) and not isinstance(v, bool)}
            else:
                findings.append("%s: names corpus %r, which results.json does not measure"
                                % (label, corpus))
                continue
            for key, local in (claim.get("locals") or {}).items():
                if key.startswith("_"):
                    continue
                if not str((claim.get("locals") or {}).get("_why_" + key, "")).strip():
                    findings.append("%s: local %r has no _why_%s. A declared constant with no "
                                    "stated reason is a number smuggled past the measurement"
                                    % (label, key, key))
                namespace[key] = local

            expected_values: list[float] = []
            for expr, expected in asserts.items():
                try:
                    actual = evaluate(expr, namespace)
                except Exception as exc:  # noqa: BLE001
                    findings.append("%s: cannot evaluate %r: %s" % (label, expr, exc))
                    continue
                expected_values.append(float(expected))
                if abs(float(actual) - float(expected)) > 1e-9:
                    findings.append("%s: publishes %s = %s, the measurement gives %s. "
                                    "Re-run measure_all.py or correct the document"
                                    % (label, expr, expected, actual))

            for n in numbers_in(text):
                if not any(abs(n - v) <= 1e-9 for v in expected_values):
                    findings.append("%s: the wording carries %g, which no assertion checks. "
                                    "Every number in a published claim must be checked against "
                                    "the measurement" % (label, n))

            if normalise(text) not in haystack:
                findings.append("%s: this exact wording is no longer in %s. The prose was "
                                "edited away from the binding; update both together"
                                % (label, rel))

        # ---- 5. sweep -----------------------------------------------------
        swept = haystack
        for claim in bindings.get("claims", []):
            swept = swept.replace(normalise(claim.get("text", "") or "\0"), " ")
        for entry in bindings.get("not_derived", []):
            token, reason = entry.get("token"), entry.get("reason")
            if not token or not str(reason or "").strip():
                findings.append("%s: a not_derived entry needs a token and a reason" % rel)
                continue
            if token not in swept:
                findings.append("%s: not_derived declares %r, which no longer appears. Remove "
                                "it; an exemption pointing at nothing is where the next "
                                "unchecked number hides" % (rel, token))
            swept = swept.replace(token, " ")
        for leftover in SWEEP.finditer(swept):
            at = swept[max(0, leftover.start() - 70): leftover.end() + 70]
            findings.append("%s: %r is published but neither derived from results.json nor "
                            "declared not_derived. Context: ...%s..."
                            % (rel, leftover.group(0), at))

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
