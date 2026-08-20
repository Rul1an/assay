#!/usr/bin/env python3
"""Project measured adequacy numbers into the documents that publish them.

    python3 conformance/adequacy/publish_numbers.py            # regenerate the documents
    python3 conformance/adequacy/publish_numbers.py --check     # regenerate and byte-compare
    python3 conformance/adequacy/publish_numbers.py --list      # every renderable field

`conformance/INDEX.md` and `conformance/privileged-mcp-action-v0/ERRATA.md` are
GENERATED from `INDEX.md.in` and `ERRATA.md.in` beside them. The narrative in
those templates is hand-written and stays hand-written. The measured cells are
not writable by hand at all: they are tokens, and this file renders them from
`results.json`.

WHY PROJECTION RATHER THAN A CHECKER OVER PROSE

The thing this replaces matched published sentences against the measurement. It
worked, and it answered the wrong question. A checker over authored prose can
only tell you that a registered cell agrees with the JSON; the sentence beside it
still says whatever its author typed, and last week's figure read as a
present-tense fact is exactly what survives such a check. The failure is
authorship. So authorship is what changed: a measured number is no longer
something an author can write.

THE RULE THIS ENFORCES. A number this revision did not measure must not appear as
a number. Every token for a corpus whose measurement is older than the code it
describes renders `[not re-measured at this revision]` instead of a figure -- in
running prose, in headings, in the reproduce command. The expensive corpus either
runs, or the page says out loud that it did not.

WHAT PROJECTION CANNOT DO ON ITS OWN, AND WHAT COVERS IT

Regenerate-and-compare has exactly one failure mode: someone edited the output. It
is blind to a number-shaped claim that was never a token, because a freshly
invented figure in the narrative renders byte-identically. So the sweep survives
from the old checker and changes subject: `unprojected_findings` reads the
TEMPLATE's authored prose, with every token removed, and refuses any `N of M` or
`NN.N%` that is not declared in `unprojected_numbers.json` with a reason.
Projection stops a measured cell from being edited; the sweep stops a new one from
being written.

TOKEN SYNTAX

    {{measured:<scope>.<field>}}                  digits
    {{measured:<scope>.<field>|words}}            English words: twenty-seven
    {{measured:<scope>.<field>|Words}}            capitalised: Twenty-seven
    {{measured:<scope>.<field>|1f}}               one decimal: 24.0
    {{measured:<scope>.<field>|g}}                shortest exact: 60
    {{measured:<scope>.<field>|pct1}}             one decimal, per cent: 24.0%
    {{measured:<scope>.<field>|pctg}}             shortest exact, per cent: 60%
    {{measured:<scope>.<field>|short}}            first 9 characters of a commit

`<scope>` is a corpus id from results.json, `@all` for the aggregate row, or
`@this` for facts about the template being rendered -- how many tokens it carries
against how many words of authored prose. That ratio is the only thing separating
a derivation from a heredoc with holes in it, so the page publishes its own.
The word form and the digit form read the SAME field, so they cannot disagree;
there is no second place to type the value.

A field an adjustment touched renders with a visible `(judged)` after it. That is
not decoration. A declared adjustment is a human judgement entering through a
field shaped like a measurement, and a mechanism that rendered it the way it
renders `killed` would not merely fail to catch an error in it -- it would lend it
the tool's authority. `six` and `five (judged)` have to look different on the page.

An unknown scope, an unknown field, a null field, a form the field's type cannot
take, or a `{{` that does not parse -- each is a hard error that stops
generation. Rendering an empty string for a mistyped token would put a silent
hole where a measurement was supposed to be, which is the defect one level up.

WHAT THIS STILL DOES NOT DO, NAMED RATHER THAN IMPLIED

  * It owns tokens, not sentences. A token can name the wrong field: `killed`
    where the sentence means `survived` renders a number that is true of the
    measurement and false in the sentence.
  * The sweep knows two shapes. A claim written entirely in English words -- "the
    corpus isolates six" -- can be added by hand and nothing here notices. The
    tokens cover the word-numbers already published; they cannot cover ones
    invented later.
  * Staleness is measured over each row's declared `depends_on`. A rule can move
    in a file no manifest declares, and nothing here notices.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import NamedTuple

REPO = Path(__file__).resolve().parents[2]

STALE = "[not re-measured at this revision]"
JUDGED = " (judged)"

# Chosen so it cannot occur in this prose by accident and cannot be produced by an
# editor reflowing a paragraph: a doubled brace, a mandatory `measured:` keyword,
# and a scope-qualified field. Neither document contains `{{` anywhere else, the
# keyword says what the value IS (so a reader of the template knows it is not
# theirs to edit), and anything matching `{{` that does not parse is refused.
TOKEN = re.compile(r"\{\{measured:([^{}|\s]+)(?:\|([A-Za-z0-9]+))?\}\}")
BRACES = re.compile(r"\{\{")

# The shapes a published adequacy number takes in these documents. Deliberately
# narrow: broadening it to every integer would make the exemption list a
# transcription of the prose and nobody would maintain it.
SWEEP = re.compile(r"\b\d+ of \d+\b|\b\d+(?:\.\d+)?%")

BANNER = ("<!-- GENERATED FROM {source} -- DO NOT EDIT THIS FILE.\n"
          "     Measured numbers are projected from conformance/adequacy/results.json by\n"
          "     conformance/adequacy/publish_numbers.py. Edit the template, then run\n"
          "     `python3 conformance/adequacy/publish_numbers.py`. -->\n")

_UNITS = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
          "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
          "seventeen", "eighteen", "nineteen"]
_TENS = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"]

# Adjustments declare an effect from a closed set. An unrecognised one is refused
# rather than ignored: a declaration the generator silently drops is a stated
# judgement that stopped applying without anyone being told.
EFFECTS = {"moves_killed_to_not_transmitted", "declared_count"}

# The fields a `moves_killed_to_not_transmitted` adjustment reaches. Named once.
MOVED_FIELDS = ("third_party_killed", "third_party_not_killed", "third_party_percent")


class Field(NamedTuple):
    """A renderable value, and whether a human judgement is inside it."""

    value: object
    judged: bool = False


class TemplateError(Exception):
    """A token that cannot be rendered. Generation stops; nothing is written."""


def pairs(repo: Path = REPO) -> list[tuple[Path, Path]]:
    """(template, generated) for every published document, in one place.

    `check_published_numbers.py` reads this list rather than keeping its own, so a
    third generated document is guarded the moment it is added here.
    """
    return [
        (repo / "conformance/INDEX.md.in", repo / "conformance/INDEX.md"),
        (repo / "conformance/privileged-mcp-action-v0/ERRATA.md.in",
         repo / "conformance/privileged-mcp-action-v0/ERRATA.md"),
    ]


def to_words(n: int) -> str:
    if not isinstance(n, int) or isinstance(n, bool) or n < 0 or n > 999:
        raise TemplateError("no word form for %r; the word form exists for counts, 0 to 999" % n)
    if n < 20:
        return _UNITS[n]
    if n < 100:
        return _TENS[n // 10] + ("-" + _UNITS[n % 10] if n % 10 else "")
    return _UNITS[n // 100] + " hundred" + (" and " + to_words(n % 100) if n % 100 else "")


def load_results(repo: Path) -> dict[str, dict]:
    path = repo / "conformance/adequacy/results.json"
    if not path.is_file():
        raise TemplateError("%s does not exist, so there is nothing to project" % path)
    doc = json.loads(path.read_text(encoding="utf-8"))
    return {r["corpus"]: r for r in doc.get("corpora", [])}


def load_adjustments(repo: Path) -> dict[str, dict[str, dict]]:
    """Declared editorial adjustments, keyed by scope then name."""
    path = repo / "conformance/adequacy/adjustments.json"
    if not path.is_file():
        return {}
    out: dict[str, dict[str, dict]] = {}
    for entry in json.loads(path.read_text(encoding="utf-8")).get("adjustments", []):
        for key in ("scope", "name", "effect", "value", "_why"):
            if entry.get(key) in (None, "") or not str(entry.get(key)).strip():
                raise TemplateError(
                    "an adjustment needs scope, name, effect, value and _why; %r has no %s. A "
                    "declared constant with no stated reason is a number smuggled past the "
                    "measurement" % (entry.get("name") or entry, key))
        if entry["effect"] not in EFFECTS:
            raise TemplateError("adjustment %r declares effect %r, which this generator does not "
                                "implement. Known: %s"
                                % (entry["name"], entry["effect"], ", ".join(sorted(EFFECTS))))
        out.setdefault(entry["scope"], {})[entry["name"]] = entry
    return out


def load_unprojected(repo: Path) -> dict[str, list[dict]]:
    path = repo / "conformance/adequacy/unprojected_numbers.json"
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8")).get("documents", {})


def stale_corpora(rows: dict[str, dict], repo: Path = REPO) -> dict[str, str]:
    """Corpora whose measurement is older than something it depends on.

    The question is NOT "was this re-run at HEAD", which would mark almost every
    row stale and train people to ignore the answer. It is whether anything the
    row declared it DEPENDS ON has moved since it was taken.

    `check_published_numbers.py` imports this rather than repeating it. Two
    implementations of "is this row still current" would drift, and the copy that
    drifts is the one that stops noticing.
    """
    out: dict[str, str] = {}
    for name, row in sorted(rows.items()):
        taken = row.get("measured_at") or {}
        if not taken.get("commit"):
            out[name] = "records no measured_at commit, so nothing can say whether its number " \
                        "is still current for this code"
            continue
        moved = subprocess.run(
            ["git", "-C", str(repo), "diff", "--name-only", taken["commit"], "HEAD", "--",
             *taken.get("depends_on", [])],
            capture_output=True, text=True)
        if moved.returncode != 0:
            continue          # shallow clone or unknown commit: not a claim either way
        changed = sorted(ln for ln in moved.stdout.splitlines() if ln.strip())
        if changed:
            out[name] = ("was measured at %s and %s changed since, so the published number "
                         "describes code this revision no longer has. Re-run measure_all.py "
                         "--only %s" % (taken["commit"][:9], ", ".join(changed[:3]), name))
    return out


def corpus_fields(row: dict, adjust: dict[str, dict]) -> dict[str, Field]:
    """Every field a token may name for one corpus.

    The derived ones are computed here and only here. `third_party_*` are the
    figures ERRATA.md publishes for a READER: what a stranger reproducing the
    fourteen vectors can and cannot be distinguished on. They differ from the
    tool's own numerator because of a declared adjustment, never because of a
    constant in this file -- the magnitude and the justification both live in
    adjustments.json, and this function only knows how a declared effect composes.
    Any field the adjustment reached is marked judged, and renders saying so.
    """
    fields: dict[str, Field] = {
        k: Field(row[k]) for k in
        ("killed", "survived", "equivalent", "out_of_scope", "known_holes", "declared_total",
         "unproved", "score_percent", "tool_commit", "tool_version", "control", "runner",
         "adequate", "corpus")
        if k in row
    }
    killed, survived, holes = row["killed"], row["survived"], row["known_holes"]
    fields["in_scope"] = Field(killed + survived)
    fields["in_scope_with_holes"] = Field(killed + survived + holes)
    if killed + survived:
        fields["out_of_scope_ratio"] = Field(row["out_of_scope"] / (killed + survived))

    moved = sum(a["value"] for a in adjust.values()
                if a["effect"] == "moves_killed_to_not_transmitted")
    fields["third_party_killed"] = Field(killed - moved, bool(moved))
    fields["third_party_not_killed"] = Field(
        (killed + survived + holes) - (killed - moved), bool(moved))
    if killed + survived + holes:
        fields["third_party_percent"] = Field(
            round(100.0 * (killed - moved) / (killed + survived + holes), 1), bool(moved))

    repos = (row.get("subject") or {}).get("repos") or []
    if len(repos) == 1 and repos[0].get("commit"):
        fields["subject_commit"] = Field(repos[0]["commit"])

    for name, entry in adjust.items():
        fields[name] = Field(entry["value"], True)
    return fields


def aggregate_fields(rows: dict[str, dict], adjust: dict[str, dict]) -> dict[str, Field]:
    values = list(rows.values())
    fields: dict[str, Field] = {
        "corpora_total": Field(len(values)),
        "measured": Field(sum(1 for r in values if r["score_percent"] is not None)),
        "control_only": Field(sum(1 for r in values if r["score_percent"] is None
                                  and r["control"] == "killed")),
        "transcribed": Field(sum(1 for r in values if r["provenance"]["kind"] == "transcribed")),
    }
    for name, entry in adjust.items():
        fields[name] = Field(entry["value"], True)
    return fields


def build_context(repo: Path = REPO) -> dict[str, dict[str, Field]]:
    rows = load_results(repo)
    adjustments = load_adjustments(repo)
    for scope in adjustments:
        if scope != "@all" and scope not in rows:
            raise TemplateError("adjustments.json declares scope %r, which results.json does not "
                                "measure" % scope)
    ctx = {name: corpus_fields(row, adjustments.get(name, {})) for name, row in rows.items()}
    ctx["@all"] = aggregate_fields(rows, adjustments.get("@all", {}))
    return ctx


def format_value(scope: str, field: str, value: object, form: str | None) -> str:
    where = "{{measured:%s.%s%s}}" % (scope, field, "|" + form if form else "")
    if value is None:
        raise TemplateError("%s: %s is null for %s, so there is no figure to publish"
                            % (where, field, scope))
    if form is None:
        if isinstance(value, (bool, str, int)):
            return str(value)
        raise TemplateError("%s: %s is a %s; a bare token renders only integers and strings. "
                            "Say |1f, |g, |pct1 or |pctg so the rendering is a decision and not "
                            "a default" % (where, field, type(value).__name__))
    if form in ("words", "Words"):
        if not isinstance(value, int) or isinstance(value, bool):
            raise TemplateError("%s: the word form exists for whole counts; %s is %r"
                                % (where, field, value))
        word = to_words(value)
        return word[0].upper() + word[1:] if form == "Words" else word
    if form in ("1f", "g", "pct1", "pctg"):
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise TemplateError("%s: |%s needs a number, %s is %r" % (where, form, field, value))
        if form in ("1f", "pct1"):
            text = "%.1f" % value
        else:
            text = ("%f" % value).rstrip("0").rstrip(".")
        return text + "%" if form.startswith("pct") else text
    if form == "short":
        if not isinstance(value, str):
            raise TemplateError("%s: |short abbreviates a commit, %s is %r" % (where, field, value))
        return value[:9]
    raise TemplateError("%s: unknown render form %r. Known: words, Words, 1f, g, pct1, pctg, short"
                        % (where, form))


def render(text: str, ctx: dict[str, dict[str, Field]], stale: dict[str, str],
           used: set[tuple[str, str]] | None = None) -> str:
    """Substitute every token. Any failure raises rather than rendering a hole."""

    def one(match: re.Match) -> str:
        target, form = match.group(1), match.group(2)
        scope, _, field = target.rpartition(".")
        if not scope or not field:
            raise TemplateError("%s: a token must name a scope and a field, as "
                                "{{measured:<corpus>.<field>}}" % match.group(0))
        if scope not in ctx:
            raise TemplateError("%s: names corpus %r, which results.json does not measure. "
                                "Known: %s" % (match.group(0), scope, ", ".join(sorted(ctx))))
        if field not in ctx[scope]:
            raise TemplateError("%s: %r has no field %r. Known: %s"
                                % (match.group(0), scope, field, ", ".join(sorted(ctx[scope]))))
        if used is not None:
            used.add((scope, field))
        if scope in stale:
            return STALE
        cell = ctx[scope][field]
        return format_value(scope, field, cell.value, form) + (JUDGED if cell.judged else "")

    out = TOKEN.sub(one, text)
    left = BRACES.search(out)
    if left:
        raise TemplateError("unparsed %r at offset %d. A token that does not parse is refused "
                            "rather than left in the page: ...%s..."
                            % (out[left.start():left.start() + 60], left.start(),
                               out[max(0, left.start() - 40):left.start() + 60]))
    return out


def density_fields(text: str) -> dict[str, Field]:
    """How much of one template is derived, as fields that template can publish.

    Not circular. The count is taken over the template as written, including the
    tokens that publish the count, so it has one value and regeneration is
    idempotent. It is a projection for the same reason everything else here is:
    a hand-typed ratio in the document that argues against hand-typed numbers
    would rot the first time anyone edited a paragraph, and rot flatteringly,
    because prose is what gets added.
    """
    tokens = TOKEN.findall(text)
    words = len(TOKEN.sub(" ", text).split())
    return {
        "tokens": Field(len(tokens)),
        "distinct_fields": Field(len(set(tokens))),
        "prose_words": Field(words),
        "tokens_per_1000_words": Field(round(1000.0 * len(tokens) / words, 1) if words else 0.0),
        "words_per_token": Field(round(words / len(tokens), 0) if tokens else 0.0),
    }


def render_all(repo: Path = REPO) -> dict[Path, str]:
    """Every generated document as it should be on disk, keyed by output path."""
    ctx = build_context(repo)
    stale = stale_corpora(load_results(repo), repo)
    used: set[tuple[str, str]] = set()
    out: dict[Path, str] = {}
    for source, target in pairs(repo):
        if not source.is_file():
            raise TemplateError("%s does not exist, so %s has no source to be generated from"
                                % (source, target))
        text = source.read_text(encoding="utf-8")
        local = dict(ctx, **{"@this": density_fields(text)})
        out[target] = BANNER.format(source=source.name) + render(text, local, stale, used)
    _check_adjustments_are_consumed(repo, used)
    return out


def _check_adjustments_are_consumed(repo: Path, used: set[tuple[str, str]]) -> None:
    """A declared adjustment that no rendered token consumes is refused.

    The rule the bindings block had, kept because it was the right rule: an
    exemption pointing at nothing is where the next unchecked number hides. A
    `moves_killed_to_not_transmitted` adjustment is consumed through the fields it
    moves, so those count as its use.
    """
    for scope, entries in sorted(load_adjustments(repo).items()):
        for name, entry in sorted(entries.items()):
            names = {name} | (set(MOVED_FIELDS)
                              if entry["effect"] == "moves_killed_to_not_transmitted" else set())
            if not any((scope, f) in used for f in names):
                raise TemplateError(
                    "adjustments.json declares %r for %s and no published token uses it. Remove "
                    "it or publish it; a stated judgement nothing renders is a number waiting to "
                    "be typed by hand instead" % (name, scope))


def unprojected_findings(repo: Path = REPO) -> list[str]:
    """Sweep each TEMPLATE's authored prose for a number nothing derives.

    This is the half regenerate-and-compare cannot do. Comparing a generated file
    with its own regeneration detects one thing: that someone edited the output. A
    number-shaped claim written straight into the narrative was never a token, so
    it regenerates byte-identically and reads to a stranger exactly like a
    measurement. The sweep is what forces a NEW figure to be justified rather than
    merely consistent.

    Subject is the template with every token removed, so what is swept is exactly
    what a human wrote.
    """
    findings: list[str] = []
    declared = load_unprojected(repo)
    for source, _ in pairs(repo):
        rel = source.relative_to(repo).as_posix()
        if not source.is_file():
            continue
        if rel not in declared:
            findings.append("%s has no entry in conformance/adequacy/unprojected_numbers.json. A "
                            "template that is not swept can publish any figure it likes; an empty "
                            "list is the declaration that it publishes none" % rel)
            continue
        swept = TOKEN.sub(" ", source.read_text(encoding="utf-8"))
        for entry in declared[rel]:
            token, reason = entry.get("token"), entry.get("reason")
            if not token or not str(reason or "").strip():
                findings.append("%s: an unprojected-number entry needs a token and a reason" % rel)
                continue
            if token not in swept:
                findings.append("%s: unprojected_numbers.json declares %r, which no longer "
                                "appears. Remove it; an exemption pointing at nothing is where "
                                "the next unchecked number hides" % (rel, token))
            swept = swept.replace(token, " ")
        for leftover in SWEEP.finditer(swept):
            at = " ".join(swept[max(0, leftover.start() - 70):leftover.end() + 70].split())
            findings.append("%s: %r is written by hand and is neither a projection token nor "
                            "declared in unprojected_numbers.json. Regenerate-and-compare cannot "
                            "see a number that was never a token. Context: ...%s..."
                            % (rel, leftover.group(0), at))
    return findings


def differences(repo: Path = REPO) -> list[str]:
    """Findings for `check_published_numbers.py`: regenerate and compare byte for byte."""
    try:
        rendered = render_all(repo)
    except TemplateError as exc:
        return ["the published documents cannot be generated: %s" % exc]
    findings = []
    for target, want in sorted(rendered.items()):
        rel = target.relative_to(repo)
        if not target.is_file():
            findings.append("%s is generated from %s.in and does not exist. Run "
                            "conformance/adequacy/publish_numbers.py" % (rel, rel))
            continue
        have = target.read_text(encoding="utf-8")
        if have == want:
            continue
        findings.append("%s is not what %s.in projects from results.json. %s. A measured number "
                        "in a generated document is not writable by hand: edit the template, or "
                        "re-run measure_all.py, then run publish_numbers.py"
                        % (rel, rel, _first_difference(have, want)))
    return findings


def _first_difference(have: str, want: str) -> str:
    """Where they part, so the finding points at a line instead of a file."""
    a, b = have.splitlines(), want.splitlines()
    for i, (x, y) in enumerate(zip(a, b), start=1):
        if x != y:
            return "First difference at line %d: on disk %r, projected %r" % (i, x[:120], y[:120])
    return "The files agree for %d lines and then one of them ends (%d on disk, %d projected)" % (
        min(len(a), len(b)), len(a), len(b))


def token_density(repo: Path = REPO) -> dict[str, dict[str, int | float]]:
    """How much of each template is actually derived, so the answer is not assumed.

    A template with three tokens in nine hundred lines is a heredoc with holes in
    it: a generator whose subject is mostly prose guarantees mostly nothing, and
    that is a thing to measure and publish rather than to hope about.
    """
    out: dict[str, dict[str, int | float]] = {}
    for source, _ in pairs(repo):
        if not source.is_file():
            continue
        out[source.relative_to(repo).as_posix()] = {
            k: v.value for k, v in density_fields(source.read_text(encoding="utf-8")).items()}
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true",
                    help="regenerate in memory, compare byte for byte, sweep the templates")
    ap.add_argument("--list", action="store_true", help="every renderable scope and field")
    ap.add_argument("--density", action="store_true",
                    help="how much of each template is derived rather than authored")
    args = ap.parse_args(argv)

    if args.list:
        for scope, fields in sorted(build_context().items()):
            print(scope)
            for field, cell in sorted(fields.items()):
                print("    %-24s %r%s" % (field, cell.value, "  (judged)" if cell.judged else ""))
        return 0

    if args.density:
        print(json.dumps(token_density(), indent=2, sort_keys=True))
        return 0

    if args.check:
        findings = differences() + unprojected_findings()
        for f in findings:
            print("  - %s\n" % f)
        if not findings:
            print("generated documents match what results.json projects")
        return 1 if findings else 0

    try:
        rendered = render_all()
    except TemplateError as exc:
        print("nothing written: %s" % exc, file=sys.stderr)
        return 1
    for target, text in sorted(rendered.items()):
        changed = not target.is_file() or target.read_text(encoding="utf-8") != text
        target.write_text(text, encoding="utf-8")
        print("%s %s" % ("wrote  " if changed else "same   ", target.relative_to(REPO)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
