#!/usr/bin/env python3
"""Re-derive every published mutation-adequacy number into results.json.

    python3 conformance/adequacy/measure_all.py                 # every corpus
    python3 conformance/adequacy/measure_all.py --only rge-bench --only rfc8785-canonicalization
    python3 conformance/adequacy/measure_all.py --list

results.json is the single measured truth behind the numbers INDEX.md and
ERRATA.md publish. `check_published_numbers.py` fails when the prose and this
file disagree, so the prose stops being a place a number can quietly rot.

WHY THIS SCRIPT EXISTS AT ALL. The numbers it writes were, until it landed,
produced by a human running the tool by hand one evening. Nothing re-derived
them, and ERRATA.md pins its scope to a corpus DIGEST, which does not move when
the *implementation* moves. So a rule deleted from `denial_marker.rs` tomorrow
changes what the corpus can transmit and changes no digest, no test and no
published number. That is a claim nobody re-derives, which is the exact defect
this whole body of work exists to criticise.

TWO OPERATIONAL WARNINGS, both learned the expensive way.

* `privileged-mcp-action-v0` is a `process` corpus: ~32 cargo builds, mutating
  shared source files in place. It is not something to run casually, and it is
  why `--only` exists. Rows for corpora not selected are carried over from the
  existing results.json untouched.
* NEVER `git commit` while a measurement runs. Pre-commit stashes unstaged
  changes, which removes the in-flight mutant and rebuilds the harness from
  unmutated source; every mutant then "survives". One 29-mutant run was voided
  that way. Check `pgrep -f corpus_adequacy` first.

The tool is NOT vendored here (see INDEX.md, "Where the adequacy tool lives").
Clone https://github.com/corpus-adequacy/corpus-adequacy as a sibling checkout.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import published_rows

REPO = Path(__file__).resolve().parents[2]
ADEQUACY = REPO / "conformance/adequacy"
RESULTS = ADEQUACY / "results.json"
TOOL = REPO.parent / "corpus-adequacy"

SCHEMA = "assay.conformance.adequacy.results.v0"


def load_tool():
    """Import the sibling checkout, or say plainly that nothing was measured."""
    if not TOOL.is_dir():
        raise SystemExit(
            "corpus-adequacy not found at %s.\n"
            "It is deliberately not vendored. Clone it as a sibling checkout:\n"
            "  git clone https://github.com/corpus-adequacy/corpus-adequacy %s" % (TOOL, TOOL))
    sys.path.insert(0, str(TOOL))
    import corpus_adequacy as ca  # noqa: E402
    return ca


def require_clean_tool() -> None:
    """Refuse to record a measurement taken with an uncommitted tool checkout.

    A commit id that does not describe the code that ran is worse than no pin: it
    is a pin that looks re-derivable and is not. Dirty and unresolved producer
    identities are not publishable rows.
    """
    dirty = bool(subprocess.run(["git", "-C", str(TOOL), "status", "--porcelain"],
                                capture_output=True, text=True, check=True).stdout.strip())
    if dirty:
        raise SystemExit(
            "the corpus-adequacy checkout at %s has uncommitted changes, so no commit id\n"
            "describes the code that would run. Commit or restore it before measuring."
            % TOOL)


def manifests() -> list[Path]:
    return sorted(ADEQUACY.glob("*.manifest.json"))


def corpus_id(manifest: Path) -> str:
    return manifest.name[: -len(".manifest.json")]


def rel(path: Path) -> str:
    return path.resolve().relative_to(REPO).as_posix()


def measured_at(declared: dict, manifest: Path) -> dict:
    """The revision of THIS repository the row was measured against.

    Without it a number is present tense forever. The lane re-derives a corpus
    only when its declared sources move, which is the right cost trade and leaves
    every other revision comparing the documents against an older results.json and
    going green. That green is accurate about the comparison and silent about the
    measurement being old, and last week's figure read as a fact about today is
    the thing this whole file exists to stop.

    Recording the commit lets `check_published_numbers.py` ask the only question
    that matters: has anything this row DEPENDS ON moved since it was taken. Not
    "was it re-run this revision", which would mark almost every row stale and
    train people to ignore it.
    """
    out = subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"],
                         capture_output=True, text=True)
    if out.returncode != 0:
        return {}
    return {"measured_at": {"commit": out.stdout.strip(),
                            "depends_on": published_rows.declared_dependencies(
                                manifest, REPO, declared)}}


def subject(manifest: Path, declared: dict) -> dict:
    """Which state of the measured thing this number describes.

    A corpus inside this repository is pinned by the commit carrying the number.
    A corpus in a SIBLING repository is not: `rge-bench` and `observed-effect-v0`
    are other people's repositories, they move without this diff seeing it, and a
    score published against them without naming a commit says nothing checkable.
    Two such numbers were published that way.

    Fresh is not the goal and cannot be. A measurement of rge-bench at c51c5af
    stays permanently true about rge-bench at c51c5af; what was missing is the
    second half of that sentence.

    The basis is the files actually measured, NOT `repo_root`. A first version
    used repo_root and recorded `rge-bench` as in-tree, because that manifest
    declares no repo_root at all and the default resolved inside this repository
    while the measured file sat three levels above it. Recording an external
    corpus as internal is the same silence this field exists to break, so it is
    derived from `implementation` and `implementation_sources` instead.
    """
    outside = {}
    for rel_path, src in published_rows.declared_external_paths(manifest, REPO, declared):
        root = subprocess.run(["git", "-C", str(src.parent), "rev-parse", "--show-toplevel"],
                              capture_output=True, text=True)
        key = root.stdout.strip() if root.returncode == 0 else str(src.parent)
        outside.setdefault(key, []).append(rel_path)
    if not outside:
        return {"subject": {"kind": "in_tree"}}

    def git(root, *args):
        out = subprocess.run(["git", "-C", root, *args], capture_output=True, text=True)
        return out.stdout.strip() if out.returncode == 0 else None

    repos = []
    for root in sorted(outside):
        commit = git(root, "rev-parse", "HEAD")
        if commit is None:
            repos.append({"path": root, "commit": None,
                          "_why": "not a git checkout, so this number names no state at all"})
            continue
        origin = git(root, "remote", "get-url", "origin") or ""
        tracked = [ln for ln in (git(root, "status", "--porcelain") or "").splitlines()
                   if ln[:2] != "??"]
        repos.append({
            "repository": origin.rsplit("github.com", 1)[-1].lstrip(":/").removesuffix(".git") or None,
            "commit": commit,
            "dirty": bool(tracked),
            "measured": sorted(outside[root]),
        })
    return {"subject": {"kind": "out_of_tree", "repos": repos}}


def row(manifest: Path, report: dict, encoded_report: bytes) -> dict:
    """Build one index row from the producer report and its exact wire bytes."""
    declared = json.loads(published_rows.read_regular_file(manifest).decode("utf-8"))
    measured = measured_at(declared, manifest).get("measured_at") or {}
    return published_rows.project_report(
        manifest,
        report,
        encoded_report,
        corpus=corpus_id(manifest),
        manifest=rel(manifest),
        measured_commit=measured.get("commit"),
        depends_on=measured.get("depends_on"),
        subject=subject(manifest, declared)["subject"],
    )


def read_existing() -> dict[str, dict]:
    if not RESULTS.is_file():
        return {}
    return published_rows.load_results(RESULTS).by_corpus()


def write(rows: dict[str, dict], reports: dict[str, str]) -> None:
    addressed = {row["report_sha256"] for row in rows.values()}
    doc = {"schema": SCHEMA,
           "row_contract": published_rows.ROW_CONTRACT,
           "_about": "Measured mutation adequacy for every corpus manifest in this "
                     "directory. Written by measure_all.py; asserted against the published "
                     "prose by check_published_numbers.py. Do not hand-edit a measured row.",
           "reports": {key: reports[key] for key in sorted(addressed)},
           "unmeasured": [],
           "corpora": [rows[k] for k in sorted(rows)]}
    RESULTS.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--only", action="append", default=[],
                    metavar="CORPUS",
                    help="measure only this corpus (repeatable). Rows for the others are "
                         "carried over from results.json unchanged.")
    ap.add_argument("--list", action="store_true", help="list corpus ids and exit")
    args = ap.parse_args(argv)

    found = manifests()
    if args.list:
        for m in found:
            print(corpus_id(m))
        return 0
    if not found:
        raise SystemExit("no manifests in %s" % ADEQUACY)

    selected = found
    if args.only:
        known = {corpus_id(m) for m in found}
        unknown = sorted(set(args.only) - known)
        if unknown:
            raise SystemExit("no such corpus: %s. Known: %s"
                             % (", ".join(unknown), ", ".join(sorted(known))))
        selected = [m for m in found if corpus_id(m) in set(args.only)]

    ca = load_tool()
    require_clean_tool()
    if RESULTS.is_file():
        loaded = published_rows.load_results(RESULTS)
        rows = loaded.by_corpus()
        reports = dict(loaded.document.get("reports") or {})
        if "reports" not in loaded.document and set(selected) != set(found):
            raise SystemExit("legacy results require one full remeasurement before --only")
    else:
        rows, reports = {}, {}

    for manifest in selected:
        print("measuring %s ..." % corpus_id(manifest), flush=True)
        report = ca.run(manifest)
        encoded_report = ca.encode_report_v0(report)
        rows[corpus_id(manifest)] = row(manifest, report, encoded_report)
        reports[rows[corpus_id(manifest)]["report_sha256"]] = encoded_report.decode("utf-8")
        r = rows[corpus_id(manifest)]
        print("  killed=%s survived=%s score=%s control=%s"
              % (r["killed"], r["survived"], r["score_percent"], r["control"]), flush=True)

    write(rows, reports)
    print("wrote %s (%d corpora)" % (rel(RESULTS), len(rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
