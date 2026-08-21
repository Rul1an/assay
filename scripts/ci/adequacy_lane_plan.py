#!/usr/bin/env python3
"""Decide which mutation-adequacy corpora a changed-file set can affect, and pin the instrument.

`conformance/INDEX.md` and `conformance/privileged-mcp-action-v0/ERRATA.md` publish measured
adequacy numbers. They were produced by hand, nothing re-derives them, and ERRATA.md is pinned to a
corpus *digest* — which does not move when the implementation moves. So the first change to a
declared implementation source makes a published number silently wrong and no existing guard fires.

Cost is what makes this a gating problem rather than a "just run it" problem. Four corpora are
module or batch runners and finish in seconds. `privileged-mcp-action-v0` is a process runner: about
32 `cargo build -p assay-cli` invocations, roughly fifteen minutes. On every pull request that is
unacceptable; never is how the numbers rot.

This applies the lesson `scripts/ci/perf_bench_relevance.py` already records for benchmarks: a path
pattern like `^crates/` matched all 21 crates and alerted on changes outside the benchmark's
compilation unit, so relevance is derived from the artifact's own declaration instead of from a glob
someone guessed. Here the declaration is the manifest: `implementation`,
`implementation_sources`, `vectors`, `corpus_digest_file`, and the manifest file itself.

`implementation` is named FIRST and deliberately. A reader who saw only
`implementation_sources` in this list concluded that `mcp-jsonrpc-id` and
`rge-bench` -- which declare `implementation` and no `implementation_sources` --
could never be selected, so an empty list would read as "never changed" and they
would freeze on first commit. That reading is wrong about the code and was right
about this sentence, which listed one field of the two that `_declared_paths`
actually reads. A hand-written path list would be a second
statement of what the manifest already says, and the two would drift.

Two different failure directions, two different postures:

* **Relevance fails open.** If the manifests cannot be read, or a declared source does not exist, or
  anything else goes wrong, every corpus is reported relevant. A gate that silently stops measuring
  is worse than one that measures too much.
* **The instrument pin fails closed.** The tool lives at a sibling checkout
  (`github.com/corpus-adequacy/corpus-adequacy`) and is deliberately not vendored. Measuring with a
  floating ref means the numbers are produced by an instrument that can change under them, which is
  the same class of bug this lane exists to catch. A manifest with no `tool_pin.commit`, or one
  that is not a 40-hex commit, exits non-zero rather than measuring. Manifests pinning *different*
  commits is not an error but a fact: a number transcribed from a document measured at an older
  commit keeps that commit, so the corpora are grouped by pin and measured one group per checkout.

Usage:
    git diff --name-only BASE...HEAD | scripts/ci/adequacy_lane_plan.py
    scripts/ci/adequacy_lane_plan.py --full        # schedule / dispatch: measure everything

Writes `key=value` lines to stdout, plus `#`-prefixed commentary. The caller routes them; this
script never opens a path taken from the environment (an uncontrolled-path sink, CodeQL
py/path-injection). `--repo-root` is an explicit argv path so the self-tests can point it at a
fixture tree.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ADEQUACY_DIR = "conformance/adequacy"

# Changed paths outside any single manifest's declaration that still change what the lane measures
# or what it is measured against. The two documents are here because they ARE the published numbers:
# editing a number without re-deriving it is precisely the rot this lane exists to prevent, so an
# edit to either forces a full measurement rather than trusting the edit.
GLOBAL_TRIGGERS = (
    ".github/workflows/adequacy-drift.yml",
    "scripts/ci/adequacy_lane_plan.py",
    "scripts/ci/adequacy_lane_assert.py",
    "scripts/ci/test_adequacy_cleanup.py",
    "conformance/adequacy/measure_all.py",
    "conformance/adequacy/check_published_numbers.py",
    "conformance/INDEX.md",
    "conformance/privileged-mcp-action-v0/ERRATA.md",
)

TOOL_REPOSITORY = "corpus-adequacy/corpus-adequacy"

# One spelling, matching `conformance/adequacy/check_published_numbers.py`, which requires
# `tool_pin.commit` on every manifest and compares it against the `tool_commit` each measured row
# records. A second accepted spelling here would be a second definition of where the pin lives.
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")

# The directory name the tool is expected to occupy beside this checkout. `INDEX.md` and
# `ERRATA.md` both document the reproduction as `python3 ../corpus-adequacy/corpus_adequacy.py`,
# so the sibling directory name is part of the published instructions, not an implementation choice.
TOOL_DIR = "corpus-adequacy"


class PinError(Exception):
    """The instrument pin could not be resolved. Fail closed."""


def _rel(path: Path, root: Path) -> str | None:
    """Repo-relative POSIX path, or None when `path` escapes the repository."""
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return None


def _sibling(path: Path, root: Path) -> str | None:
    """First path component of `path` under the repository's parent, or None when it is not there."""
    try:
        return path.relative_to(root.parent).parts[0]
    except (ValueError, IndexError):
        return None


def _declared_paths(manifest: dict) -> list[str]:
    """Every filesystem path a manifest declares, as written (manifest-dir relative)."""
    out: list[str] = []
    for key in ("implementation", "vectors", "corpus_digest_file"):
        value = manifest.get(key)
        if isinstance(value, str):
            out.append(value)
    sources = manifest.get("implementation_sources")
    if isinstance(sources, list):
        out.extend(s for s in sources if isinstance(s, str))
    return out


def _extract_pin(manifest: dict) -> str | None:
    """The instrument commit this manifest declares, if any: `tool_pin.commit`."""
    pin = manifest.get("tool_pin")
    if isinstance(pin, dict):
        commit = pin.get("commit")
        if isinstance(commit, str) and COMMIT_RE.match(commit.strip()):
            return commit.strip()
    return None


class Corpus:
    """One manifest, read for what it declares about itself."""

    def __init__(self, path: Path, root: Path) -> None:
        self.manifest_path = _rel(path, root) or path.as_posix()
        self.name = path.name[: -len(".manifest.json")]
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise ValueError(f"{self.manifest_path}: manifest is not a JSON object")
        self.data = data

        # A process runner rebuilds the subject binary per mutant; that is the fifteen-minute
        # corpus. Cost class is read off `runner` rather than off a name, so a second process
        # corpus is classified correctly the day it lands instead of being quietly treated as cheap.
        self.heavy = data.get("runner") == "process"

        self.triggers: set[str] = {self.manifest_path}
        self.siblings: set[str] = set()
        self.missing: list[str] = []
        base = path.parent
        for declared in _declared_paths(data):
            resolved = (base / declared).resolve()
            inside = _rel(resolved, root)
            if inside is not None:
                self.triggers.add(inside)
                if not resolved.exists():
                    self.missing.append(inside)
                continue
            sibling = _sibling(resolved, root)
            if sibling is None:
                # A declared path that is neither in the repository nor beside it cannot be
                # attributed, so it cannot be watched. Fail open on this corpus.
                self.missing.append(declared)
            else:
                self.siblings.add(sibling)

        self.declares_pin = _extract_pin(data) is not None

        # A corpus whose declared sources live beside this repository rather than inside it cannot
        # be relevance-gated from this repository's diff: the change that would invalidate its
        # number happens in a repository this diff cannot see. Saying so is the point — the
        # alternative is a corpus that looks gated and is in fact unwatched.
        self.external = bool(self.siblings)

    def sibling_pins(self) -> dict[str, str]:
        """Non-instrument sibling directory -> `owner/repo@commit`, as this manifest declares it.

        The instrument is deliberately absent: it is pinned per corpus (see `pin_groups`), because
        a number transcribed from a document measured at an older commit keeps that commit. One
        map holding both would have to pretend there is a single tool commit, and there is not.
        """
        pins: dict[str, str] = {}
        declared = self.data.get("pins")
        if isinstance(declared, dict):
            for directory, spec in declared.items():
                if not isinstance(spec, dict):
                    continue
                repository = spec.get("repository")
                commit = spec.get("commit") or spec.get("sha")
                if isinstance(repository, str) and isinstance(commit, str) and COMMIT_RE.match(commit):
                    pins[directory] = f"{repository}@{commit}"
        return pins


def load(root: Path) -> list[Corpus]:
    directory = root / ADEQUACY_DIR
    return [Corpus(p, root) for p in sorted(directory.glob("*.manifest.json"))]


def pin_groups(corpora: list[Corpus]) -> dict[str, list[str]]:
    """instrument commit -> the corpora whose published numbers were measured with it.

    NOT one global pin. `conformance/adequacy/check_published_numbers.py` requires each row to
    record the commit **its own manifest** declares, and the manifests legitimately disagree: a row
    transcribed from a document measured at an older tool commit keeps that commit, while corpora
    re-measured since carry the newer one. Collapsing them to one value would make the lane
    re-measure a corpus with an instrument its manifest does not pin, and the drift check would
    then fail on a difference the lane itself introduced.

    A manifest with no pin is refused rather than defaulted: measuring with a floating ref means
    the numbers are produced by an instrument that can change under them, which is the same class
    of bug this lane exists to catch.
    """
    groups: dict[str, list[str]] = {}
    unpinned: list[str] = []
    for corpus in corpora:
        pin = _extract_pin(corpus.data)
        if pin is None:
            unpinned.append(corpus.name)
        else:
            groups.setdefault(pin, []).append(corpus.name)
    if unpinned:
        raise PinError(
            f"{', '.join(sorted(unpinned))}: no `tool_pin.commit` declared. Refusing to measure "
            "with a floating ref."
        )
    return {commit: sorted(names) for commit, names in sorted(groups.items())}


def resolve_sibling_pins(corpora: list[Corpus]) -> dict[str, str]:
    """`sibling directory -> owner/repo@commit`, for every sibling checkout any manifest pins.

    Same rule as the instrument, applied to every checkout a measurement reads: a sibling at a
    floating ref can change under the number, so an unpinned sibling is never checked out. What
    happens to a corpus that needs one is a scope decision, made in `plan()`; this function only
    reports what is pinned.
    """
    merged: dict[str, set[str]] = {}
    for corpus in corpora:
        for directory, spec in corpus.sibling_pins().items():
            merged.setdefault(directory, set()).add(spec)

    conflicting = sorted(d for d, specs in merged.items() if len(specs) > 1)
    if conflicting:
        detail = "; ".join(f"{d}: {', '.join(sorted(merged[d]))}" for d in conflicting)
        raise PinError(f"manifests disagree on a sibling checkout pin: {detail}")

    return {d: next(iter(specs)) for d, specs in sorted(merged.items())}


def plan(
    corpora: list[Corpus],
    changed: list[str],
    *,
    full: bool,
    pinned_siblings: set[str],
) -> dict:
    """Which corpora this run must measure, and why.

    A corpus whose subject lives in a sibling repository that nothing pins is **out of this lane's
    scope**, not skipped. The distinction is the point: this repository cannot keep such a number
    fresh by any CI design, because the change that invalidates it happens in a repository this
    lane may not check out at a floating ref. Saying so, per corpus, with the exact declaration
    that would bring it in scope, is a scope statement. Leaving it in the planned set would make
    the lane red forever, which teaches people to ignore it — the same defect as a green that
    measured nothing, arrived at from the other side.
    """
    detail: dict[str, str] = {}

    def unpinned(corpus: Corpus) -> list[str]:
        return sorted(s for s in corpus.siblings if s not in pinned_siblings)

    out_of_scope = {c.name: unpinned(c) for c in corpora if unpinned(c)}
    for name, missing in out_of_scope.items():
        detail[name] = (
            "OUT OF SCOPE for this lane: its subject lives in sibling checkout(s) "
            f"{', '.join(missing)}, which no manifest pins. This lane will not clone at a floating "
            'ref. Declare `"pins": {"<dir>": {"repository": "owner/name", "commit": "<40-hex>"}}` '
            "in the manifest and it joins the scheduled run with no change here."
        )

    if full:
        relevant = [c.name for c in corpora if c.name not in out_of_scope]
        for name in relevant:
            detail[name] = "full run requested (schedule or dispatch)"
        reason = "full"
    else:
        changed_set = set(changed)
        global_hits = sorted(changed_set & set(GLOBAL_TRIGGERS))
        # A manifest that declares the instrument pin governs every corpus, not only its own: the
        # pin moving means every number on the page was produced by a different instrument.
        pin_hits = sorted(
            c.manifest_path for c in corpora if c.declares_pin and c.manifest_path in changed_set
        )
        relevant = []
        for corpus in corpora:
            hits = sorted(changed_set & corpus.triggers)
            if corpus.name in out_of_scope:
                continue  # detail already states why, and no trigger can make it measurable
            if global_hits:
                relevant.append(corpus.name)
                detail[corpus.name] = f"lane-wide change: {', '.join(global_hits)}"
            elif pin_hits:
                relevant.append(corpus.name)
                detail[corpus.name] = f"instrument pin may have moved: {', '.join(pin_hits)}"
            elif corpus.missing:
                relevant.append(corpus.name)
                detail[corpus.name] = (
                    "declared path could not be resolved in this tree "
                    f"({', '.join(corpus.missing)}); failing open"
                )
            elif hits:
                relevant.append(corpus.name)
                detail[corpus.name] = f"declared inputs changed: {', '.join(hits)}"
            elif corpus.external:
                detail[corpus.name] = (
                    "declared sources live outside this repository "
                    f"({', '.join(sorted(corpus.siblings))}); a change there is invisible to this "
                    "diff, so relevance computed here would be a false negative. Measured by the "
                    "scheduled full run, which checks those siblings out at their declared pins"
                )
            else:
                detail[corpus.name] = (
                    f"no changed path is declared by this corpus "
                    f"({len(corpus.triggers)} declared: {', '.join(sorted(corpus.triggers))})"
                )
        reason = "relevance"

    relevant_set = set(relevant)
    return {
        "mode": reason,
        "all": [c.name for c in corpora],
        "relevant": relevant,
        "heavy": [c.name for c in corpora if c.heavy],
        "heavy_relevant": any(c.heavy and c.name in relevant_set for c in corpora),
        "external": [c.name for c in corpora if c.external],
        "out_of_scope": sorted(out_of_scope),
        "siblings": sorted({s for c in corpora if c.name in relevant_set for s in c.siblings}),
        "detail": detail,
    }


def emit(
    result: dict,
    changed: list[str],
    *,
    groups: dict[str, list[str]],
    sibling_pins: dict[str, str],
    notes: list[str],
) -> None:
    """Write everything to stdout; the caller routes it.

    Machine-readable lines match `^[a-z_]+=`; everything else is commentary prefixed with `#`.
    """
    for note in notes:
        print(f"# {note}")
    print(f"# mode: {result['mode']}")
    for name in result["all"]:
        if name in result["relevant"]:
            mark = "MEASURE   "
        elif name in result["out_of_scope"]:
            mark = "OUT-OF-SCOPE"
        else:
            mark = "skip      "
        print(f"# {mark} {name}: {result['detail'].get(name, '')}")
    for path in changed:
        print(f"#   changed: {path}")
    print(f"mode={result['mode']}")
    print(f"tool_repository={TOOL_REPOSITORY}")
    print(f"tool_dir={TOOL_DIR}")
    # `<commit>=<corpus>,<corpus>` per group, space-separated, restricted to the selected corpora.
    # The measurement runs once per group with the tool checked out at that group's commit, because
    # a row must record the commit its own manifest pins.
    selected = set(result["relevant"])
    print(
        "pin_groups="
        + " ".join(
            f"{commit}={','.join(n for n in names if n in selected)}"
            for commit, names in groups.items()
            if any(n in selected for n in names)
        )
    )
    print(f"all_corpora={','.join(result['all'])}")
    print(f"relevant_corpora={','.join(result['relevant'])}")
    print(f"heavy_corpora={','.join(result['heavy'])}")
    print(f"heavy_relevant={str(result['heavy_relevant']).lower()}")
    print(f"external_corpora={','.join(result['external'])}")
    print(f"out_of_scope_corpora={','.join(result['out_of_scope'])}")
    # `<dir>=<owner/repo>@<commit>` per sibling, space-separated: the workflow reads it with a
    # `read` loop, so no value may contain a space.
    print("sibling_pins=" + " ".join(f"{d}={spec}" for d, spec in sorted(sibling_pins.items())))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--full",
        action="store_true",
        help="Measure every corpus regardless of the changed-file set (schedule, dispatch).",
    )
    parser.add_argument(
        "--repo-root",
        default=str(Path(__file__).resolve().parents[2]),
        help="Repository root. Explicit argv, never read from the environment.",
    )
    args = parser.parse_args(argv)
    root = Path(args.repo_root).resolve()

    changed = [] if args.full else [line.strip() for line in sys.stdin if line.strip()]

    notes: list[str] = []
    try:
        corpora = load(root)
        if not corpora:
            raise ValueError(f"no manifests found under {root / ADEQUACY_DIR}")
    except Exception as exc:  # noqa: BLE001 - relevance fails open, loudly
        print(
            f"::error::adequacy manifests unreadable ({exc}); relevance cannot be derived and the "
            "instrument pin cannot be resolved, so this lane refuses rather than measuring nothing"
        )
        return 2

    try:
        groups = pin_groups(corpora)
    except PinError as exc:
        print(f"::error::{exc}")
        return 2

    try:
        sibling_pins = resolve_sibling_pins(corpora)
    except PinError as exc:
        print(f"::error::{exc}")
        return 2

    try:
        result = plan(
            corpora, changed, full=args.full, pinned_siblings=set(sibling_pins)
        )
    except Exception as exc:  # noqa: BLE001 - fail open, loudly
        notes.append(f"relevance derivation failed ({exc}); treating every corpus as relevant")
        print(f"::warning::adequacy relevance failed ({exc}); measuring every corpus")
        result = {
            "mode": "fail-open",
            "all": [c.name for c in corpora],
            "relevant": [c.name for c in corpora],
            "heavy": [c.name for c in corpora if c.heavy],
            "heavy_relevant": any(c.heavy for c in corpora),
            "external": [c.name for c in corpora if c.external],
            "out_of_scope": [],
            "siblings": sorted({s for c in corpora for s in c.siblings}),
            "detail": {c.name: "fail-open" for c in corpora},
        }

    emit(result, changed, groups=groups, sibling_pins=sibling_pins, notes=notes)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
