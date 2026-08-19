#!/usr/bin/env python3
"""Assert that the adequacy lane actually re-derived what it planned to re-derive.

`conformance/adequacy/check_published_numbers.py` answers "do the published numbers agree with
results.json". It cannot answer "was results.json produced by running anything", and it is not
supposed to: `measure_all.py --only X` deliberately **carries over** every row it did not select,
unchanged. That is the right behaviour for a tool a human drives, and it is exactly the shape that
lets a relevance-gated CI lane go green having measured nothing — a carried-over row and a
freshly-measured one are byte-identical.

So this checker sits between the plan and the drift check and holds the run to its own plan:

* every corpus the plan selected must carry `provenance.kind == "measured"` — a row still marked
  transcribed, or carried from an earlier run's prose, is a corpus that was not re-derived;
* every selected row must record the `tool_commit` its manifest pins, so a measurement taken with a
  dirty or floating instrument (which `measure_all.py` records as `<sha>-dirty`) is refused;
* a `control` that SURVIVED voids its run: the tool's own rule is that every other verdict in a
  control-survived run is meaningless, so a score is not a result;
* every corpus the plan did **not** select is named, in words, as NOT re-derived by this run, with
  the reason the plan gave. Silence there is how "skipped" comes to read as "verified".

Usage:
    adequacy_lane_assert.py --results conformance/adequacy/results.json \
        --manifest-dir conformance/adequacy \
        --planned a,b --all a,b,c,d,e [--out-of-scope c,d] [--require-all]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


class AssertError(Exception):
    """The lane cannot show that it re-derived what it planned to."""


def rows_by_corpus(document: object) -> dict[str, dict]:
    """corpus id -> its row, from the `assay.conformance.adequacy.results.v0` shape."""
    if not isinstance(document, dict):
        raise AssertError("results file is not a JSON object")
    corpora = document.get("corpora")
    if not isinstance(corpora, list):
        raise AssertError("results file has no `corpora` list")
    rows: dict[str, dict] = {}
    for item in corpora:
        if not isinstance(item, dict):
            raise AssertError("`corpora` holds a non-object element")
        name = item.get("corpus")
        if not isinstance(name, str):
            raise AssertError("a `corpora` row has no string `corpus` id")
        rows[name] = item
    if not rows:
        raise AssertError("`corpora` is empty; nothing was measured and nothing can be asserted")
    return rows


def manifest_pins(manifest_dir: Path) -> dict[str, str]:
    """corpus id -> the instrument commit its manifest pins."""
    pins: dict[str, str] = {}
    for path in sorted(manifest_dir.glob("*.manifest.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        pin = data.get("tool_pin")
        commit = pin.get("commit") if isinstance(pin, dict) else None
        if isinstance(commit, str):
            pins[path.name[: -len(".manifest.json")]] = commit
    return pins


def check(
    document: object,
    pins: dict[str, str],
    planned: list[str],
    every: list[str],
    out_of_scope: list[str],
    *,
    require_all: bool,
) -> list[str]:
    """Report lines for a run that held to its plan; raise on the first thing it cannot show."""
    if require_all:
        owed = [n for n in every if n not in planned and n not in out_of_scope]
        if owed:
            raise AssertError(
                "this run was required to re-derive every in-scope corpus, but the plan omitted "
                f"{', '.join(owed)}"
            )

    if not every:
        raise AssertError(
            "no corpora are known at all. Either the manifests are gone or the plan failed to "
            "read them; a run that cannot name what it should measure must not report a verdict."
        )

    # An empty plan is a legitimate outcome and the common one: relevance found nothing that could
    # move a number. It is NOT the same as a run that measured nothing while something did change,
    # which is why every corpus is then listed by name below rather than passing in silence. What
    # licenses the pass is the relevance derivation, and it is stated in the plan step's summary.
    rows = rows_by_corpus(document)
    failures: list[str] = []
    lines: list[str] = []

    for name in planned:
        row = rows.get(name)
        if row is None:
            failures.append(f"{name}: planned for measurement but absent from results.json")
            continue

        provenance = row.get("provenance")
        kind = provenance.get("kind") if isinstance(provenance, dict) else None
        if kind != "measured":
            failures.append(
                f"{name}: planned for measurement but its row says provenance.kind={kind!r}. "
                "A carried-over or transcribed row is not a re-derivation."
            )
            continue

        expected = pins.get(name)
        recorded = row.get("tool_commit")
        if expected is None:
            failures.append(f"{name}: no `tool_pin.commit` in its manifest to check against")
        elif recorded != expected:
            failures.append(
                f"{name}: measured with tool_commit={recorded!r} but its manifest pins "
                f"{expected!r}. The instrument that produced this number is not the pinned one."
            )
            continue

        control = row.get("control")
        if control == "SURVIVED":
            failures.append(
                f"{name}: its control mutant SURVIVED, which voids every other verdict in the "
                "run. A score from a control-survived run is not a result."
            )
            continue
        if control == "none_declared":
            lines.append(
                f"re-derived: {name} (no control declared; a zero from this corpus cannot be "
                "distinguished from nothing having been measured)"
            )
        else:
            lines.append(f"re-derived: {name} (control {control})")

    for name in every:
        if name in planned:
            continue
        if name in out_of_scope:
            lines.append(
                f"OUT OF SCOPE: {name} — this lane cannot keep its number fresh; see the plan step"
            )
        else:
            lines.append(
                f"NOT re-derived by this run: {name} — relevance gating found no change to its "
                "declared inputs. Its row is carried over, and the scheduled full run is what "
                "bounds how long that can stay true."
            )

    unknown = sorted(set(rows) - set(every))
    if unknown:
        # Not a failure on its own: a new manifest can land before the plan is re-run. It is
        # reported because a silently-extra row is how a rename hides a corpus going dark.
        lines.append(f"results carry corpora the plan does not know: {', '.join(unknown)}")

    if failures:
        raise AssertError("; ".join(failures))
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", required=True, help="Path to results.json (explicit argv).")
    parser.add_argument("--manifest-dir", required=True, help="Directory holding the manifests.")
    parser.add_argument("--planned", default="", help="Comma-separated corpora this run selected.")
    parser.add_argument("--all", dest="every", default="", help="Comma-separated known corpora.")
    parser.add_argument("--out-of-scope", default="", help="Comma-separated unmeasurable corpora.")
    parser.add_argument(
        "--require-all",
        action="store_true",
        help="Refuse unless the plan covered every in-scope corpus (scheduled full run).",
    )
    args = parser.parse_args(argv)

    planned = [s for s in args.planned.split(",") if s]
    every = [s for s in args.every.split(",") if s] or list(planned)
    out_of_scope = [s for s in args.out_of_scope.split(",") if s]

    try:
        document = json.loads(Path(args.results).read_text(encoding="utf-8"))
        pins = manifest_pins(Path(args.manifest_dir))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        print(
            f"::error::adequacy inputs unreadable ({exc}); the lane cannot show that anything was "
            "re-derived, so it fails rather than passing",
            file=sys.stderr,
        )
        return 2

    try:
        lines = check(document, pins, planned, every, out_of_scope, require_all=args.require_all)
    except AssertError as exc:
        print(f"::error::adequacy coverage assertion failed: {exc}", file=sys.stderr)
        return 1

    for line in lines:
        print(line)
    in_scope = [n for n in every if n not in out_of_scope]
    print(
        f"adequacy coverage: {len(planned)} of {len(in_scope)} in-scope corpora re-derived this "
        f"run ({len(out_of_scope)} out of scope, {len(every)} declared)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
