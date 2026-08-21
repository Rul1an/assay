#!/usr/bin/env python3
"""Contract tests for the adequacy drift lane's two helpers.

    python3 scripts/ci/test_adequacy_lane.py

A demonstrated red is not a pinned red: these exist so the lane's refusals stay refusals. Each test
names the way the lane could otherwise go green having measured nothing.
"""

from __future__ import annotations

import io
import json
import contextlib
import tempfile
import unittest
from pathlib import Path

import adequacy_lane_assert as la
import adequacy_lane_plan as lp

TOOL_PIN = "a" * 40
OTHER_PIN = "b" * 40

REPO_ROOT = Path(__file__).resolve().parents[2]


def manifest(**overrides) -> dict:
    base = {
        "schema": "corpus-adequacy.manifest.v0",
        "tool_pin": {"commit": TOOL_PIN},
        "vectors": "vectors.json",
        "mutants": {},
    }
    base.update(overrides)
    return base


class FixtureTree:
    """A throwaway repository with an adequacy directory, plus a parent for siblings."""

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        # Resolved: on macOS the temp dir is /var/... which is a symlink to /private/var/...,
        # and the plan resolves declared paths. An unresolved root would make every declared
        # source look like it escapes the repository.
        parent = Path(self._tmp.name).resolve()
        self.root = parent / "subject"
        (self.root / lp.ADEQUACY_DIR).mkdir(parents=True)
        (self.root / "crates" / "thing" / "src").mkdir(parents=True)
        (self.root / "crates" / "thing" / "src" / "lib.rs").write_text("", encoding="utf-8")
        (self.root / "crates" / "other").mkdir(parents=True)
        (self.root / "crates" / "other" / "lib.rs").write_text("", encoding="utf-8")
        (self.root / lp.ADEQUACY_DIR / "vectors.json").write_text("[]", encoding="utf-8")

    def write(self, name: str, data: dict) -> None:
        path = self.root / lp.ADEQUACY_DIR / f"{name}.manifest.json"
        path.write_text(json.dumps(data), encoding="utf-8")

    def plan(self, changed: list[str], *, full: bool = False) -> dict:
        corpora = lp.load(self.root)
        lp.pin_groups(corpora)  # every fixture manifest must be pinned, same as the real tree
        pins = lp.resolve_sibling_pins(corpora)
        return lp.plan(corpora, changed, full=full, pinned_siblings=set(pins))

    def close(self) -> None:
        self._tmp.cleanup()


class RelevanceDerivation(unittest.TestCase):
    """Relevance comes from what the manifest declares, not from a guessed path glob."""

    def setUp(self) -> None:
        self.tree = FixtureTree()
        self.addCleanup(self.tree.close)
        self.tree.write(
            "heavy",
            manifest(runner="process", implementation_sources=["../../crates/thing/src/lib.rs"]),
        )
        self.tree.write("cheap", manifest(implementation="../../crates/other/lib.rs"))

    def test_declared_source_selects_only_its_own_corpus(self) -> None:
        result = self.tree.plan(["crates/thing/src/lib.rs"])
        self.assertEqual(result["relevant"], ["heavy"])
        self.assertTrue(result["heavy_relevant"])

    def test_unrelated_change_selects_nothing(self) -> None:
        # The whole point of the gate: a change no corpus declares must not cost fifteen minutes.
        result = self.tree.plan(["crates/unrelated/src/main.rs", "README.md"])
        self.assertEqual(result["relevant"], [])
        self.assertFalse(result["heavy_relevant"])

    def test_manifest_change_selects_its_corpus(self) -> None:
        result = self.tree.plan([f"{lp.ADEQUACY_DIR}/cheap.manifest.json"])
        # `cheap` declares the pin like every manifest, so a pin move selects everything. That is
        # the intended reading: the instrument changing invalidates every number on the page.
        self.assertEqual(result["relevant"], ["cheap", "heavy"])

    def test_projection_changes_do_not_remeasure_unchanged_corpora(self) -> None:
        # Required CI renders and checks these paths directly. Re-running every producer would
        # measure unchanged implementations and conflate publication drift with corpus drift.
        for document in (
            "conformance/INDEX.md",
            "conformance/INDEX.md.in",
            "conformance/privileged-mcp-action-v0/ERRATA.md",
            "conformance/privileged-mcp-action-v0/ERRATA.md.in",
            "conformance/adequacy/project_published_numbers.py",
            "conformance/adequacy/check_published_numbers.py",
        ):
            with self.subTest(document=document):
                self.assertEqual(self.tree.plan([document])["relevant"], [])

    def test_measurement_driver_change_still_forces_a_full_measurement(self) -> None:
        self.assertEqual(
            sorted(self.tree.plan(["conformance/adequacy/measure_all.py"])["relevant"]),
            ["cheap", "heavy"],
        )

    def test_a_declared_source_missing_from_the_tree_fails_open(self) -> None:
        self.tree.write("gone", manifest(implementation="../../crates/thing/src/vanished.rs"))
        result = self.tree.plan(["README.md"])
        self.assertIn("gone", result["relevant"])
        self.assertIn("failing open", result["detail"]["gone"])


class OutOfScope(unittest.TestCase):
    """A corpus whose subject is in an unpinned sibling is declared out of scope, never 'skipped'."""

    def setUp(self) -> None:
        self.tree = FixtureTree()
        self.addCleanup(self.tree.close)
        self.tree.write("local", manifest(implementation="../../crates/other/lib.rs"))
        self.tree.write("remote", manifest(implementation="../../../elsewhere/ref.py"))

    def test_unpinned_sibling_is_never_selected_even_by_a_global_trigger(self) -> None:
        for changed, full in (
            (["conformance/INDEX.md"], False),
            ([], True),
        ):
            with self.subTest(full=full):
                result = self.tree.plan(changed, full=full)
                self.assertNotIn("remote", result["relevant"])
                self.assertEqual(result["out_of_scope"], ["remote"])
                self.assertIn("OUT OF SCOPE", result["detail"]["remote"])

    def test_a_declared_pin_brings_it_into_the_scheduled_run(self) -> None:
        self.tree.write(
            "remote",
            manifest(
                implementation="../../../elsewhere/ref.py",
                pins={"elsewhere": {"repository": "owner/elsewhere", "commit": OTHER_PIN}},
            ),
        )
        result = self.tree.plan([], full=True)
        self.assertIn("remote", result["relevant"])
        self.assertEqual(result["out_of_scope"], [])


class InstrumentPin(unittest.TestCase):
    """The instrument fails closed: measuring with a floating ref is the bug the lane catches."""

    def setUp(self) -> None:
        self.tree = FixtureTree()
        self.addCleanup(self.tree.close)

    def test_missing_pin_refuses(self) -> None:
        self.tree.write("one", manifest(tool_pin=None))
        with self.assertRaises(lp.PinError):
            lp.pin_groups(lp.load(self.tree.root))

    def test_non_commit_pin_refuses(self) -> None:
        self.tree.write("one", manifest(tool_pin={"commit": "main"}))
        with self.assertRaises(lp.PinError):
            lp.pin_groups(lp.load(self.tree.root))

    def test_manifests_may_pin_different_commits(self) -> None:
        # Not a conflict. A corpus whose number was transcribed from a document measured at an
        # older tool commit keeps that commit; collapsing the two would make the lane re-measure
        # with an instrument the manifest does not pin, and the drift check would then fail on a
        # difference the lane itself introduced.
        self.tree.write("one", manifest())
        self.tree.write("two", manifest(tool_pin={"commit": OTHER_PIN}))
        groups = lp.pin_groups(lp.load(self.tree.root))
        self.assertEqual(groups, {TOOL_PIN: ["one"], OTHER_PIN: ["two"]})

    def test_conflicting_sibling_pins_refuse(self) -> None:
        self.tree.write(
            "one",
            manifest(pins={"e": {"repository": "a/e", "commit": OTHER_PIN}}),
        )
        self.tree.write(
            "two",
            manifest(pins={"e": {"repository": "z/e", "commit": OTHER_PIN}}),
        )
        with self.assertRaises(lp.PinError):
            lp.resolve_sibling_pins(lp.load(self.tree.root))


def results(**rows) -> dict:
    return {"schema": "assay.conformance.adequacy.results.v0", "corpora": list(rows.values())}


def row(name: str, *, kind: str = "measured", commit: str = TOOL_PIN, control: str = "killed"):
    return {
        "corpus": name,
        "killed": 6,
        "survived": 4,
        "score_percent": 60.0,
        "control": control,
        "tool_commit": commit,
        "provenance": {"kind": kind},
    }


class CoverageAssertion(unittest.TestCase):
    """Every way the lane could report a verdict it did not earn."""

    PINS = {"a": TOOL_PIN, "b": TOOL_PIN}

    def check(self, document, planned, every, out_of_scope=(), require_all=False):
        return la.check(
            document, self.PINS, planned, every, list(out_of_scope), require_all=require_all
        )

    def test_a_carried_over_row_is_not_a_re_derivation(self) -> None:
        # `measure_all.py --only b` leaves a's row untouched and byte-identical. Claiming a was
        # measured must be refused, or relevance gating silently becomes "trust the old number".
        document = results(a=row("a", kind="transcribed"), b=row("b"))
        with self.assertRaises(la.AssertError) as caught:
            self.check(document, ["a", "b"], ["a", "b"])
        self.assertIn("not a re-derivation", str(caught.exception))

    def test_an_absent_row_is_refused(self) -> None:
        with self.assertRaises(la.AssertError):
            self.check(results(b=row("b")), ["a", "b"], ["a", "b"])

    def test_an_unpinned_instrument_is_refused(self) -> None:
        document = results(a=row("a", commit=f"{TOOL_PIN}-dirty"))
        with self.assertRaises(la.AssertError) as caught:
            self.check(document, ["a"], ["a"])
        self.assertIn("not the pinned one", str(caught.exception))

    def test_a_survived_control_voids_the_run(self) -> None:
        document = results(a=row("a", control="SURVIVED"))
        with self.assertRaises(la.AssertError) as caught:
            self.check(document, ["a"], ["a"])
        self.assertIn("voids", str(caught.exception))

    def test_an_empty_plan_names_every_corpus_as_unmeasured(self) -> None:
        # Relevance finding nothing is legitimate and common. What is not legitimate is passing in
        # silence, so the pass must still say, per corpus, that nothing was re-derived.
        lines = self.check(results(a=row("a"), b=row("b")), [], ["a", "b"])
        for name in ("a", "b"):
            self.assertTrue(any(f"NOT re-derived by this run: {name}" in line for line in lines))

    def test_knowing_no_corpora_at_all_is_refused(self) -> None:
        with self.assertRaises(la.AssertError):
            self.check(results(a=row("a")), [], [])

    def test_require_all_refuses_a_partial_scheduled_run(self) -> None:
        # The schedule is the only thing bounding staleness. A full run that quietly measured a
        # subset would remove that bound while still reporting success.
        with self.assertRaises(la.AssertError):
            self.check(results(a=row("a")), ["a"], ["a", "b"], require_all=True)

    def test_require_all_tolerates_declared_out_of_scope(self) -> None:
        lines = self.check(
            results(a=row("a")), ["a"], ["a", "b"], out_of_scope=["b"], require_all=True
        )
        self.assertTrue(any("OUT OF SCOPE: b" in line for line in lines))

    def test_unmeasured_corpora_are_named_in_words(self) -> None:
        lines = self.check(results(a=row("a"), b=row("b")), ["a"], ["a", "b"])
        self.assertTrue(any("NOT re-derived by this run: b" in line for line in lines))

    def test_an_empty_corpora_list_is_refused(self) -> None:
        with self.assertRaises(la.AssertError):
            self.check({"corpora": []}, ["a"], ["a"])


class LiveTree(unittest.TestCase):
    """The rules above, applied to this repository's actual manifests."""

    def setUp(self) -> None:
        self.corpora = lp.load(REPO_ROOT)
        self.pins = set(lp.resolve_sibling_pins(self.corpora))

    def test_every_manifest_pins_an_instrument_commit(self) -> None:
        groups = lp.pin_groups(self.corpora)
        self.assertEqual(
            sorted(n for names in groups.values() for n in names),
            sorted(c.name for c in self.corpora),
        )
        for commit in groups:
            self.assertRegex(commit, lp.COMMIT_RE)

    def test_the_heavy_corpus_is_the_process_runner(self) -> None:
        heavy = [c.name for c in self.corpora if c.heavy]
        self.assertEqual(heavy, ["privileged-mcp-action-v0"])

    def test_touching_a_declared_source_selects_the_heavy_corpus(self) -> None:
        # Regression fixture. `denial_marker.rs` is one of the three files the manifest declares;
        # a change to it moves what the corpus can transmit and moves no digest.
        result = lp.plan(
            self.corpora,
            ["crates/assay-evidence/src/denial_marker.rs"],
            full=False,
            pinned_siblings=self.pins,
        )
        self.assertEqual(result["relevant"], ["privileged-mcp-action-v0"])

    def test_an_unrelated_crate_does_not_select_the_heavy_corpus(self) -> None:
        result = lp.plan(
            self.corpora,
            ["crates/assay-runner-schema/src/lib.rs"],
            full=False,
            pinned_siblings=self.pins,
        )
        self.assertFalse(result["heavy_relevant"])

    def test_cleanup_contract_change_forces_a_full_measurement(self) -> None:
        result = lp.plan(
            self.corpora,
            ["scripts/ci/test_adequacy_cleanup.py"],
            full=False,
            pinned_siblings=self.pins,
        )
        self.assertEqual(
            result["relevant"],
            sorted(c.name for c in self.corpora),
        )

    def test_emit_lines_are_routable_key_values(self) -> None:
        result = lp.plan(self.corpora, [], full=True, pinned_siblings=self.pins)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            lp.emit(
                result,
                [],
                groups=lp.pin_groups(self.corpora),
                sibling_pins={},
                notes=[],
            )
        keys = {
            line.split("=", 1)[0]
            for line in buffer.getvalue().splitlines()
            if not line.startswith("#")
        }
        # The workflow reads exactly these; a rename here silently empties a job input.
        self.assertEqual(
            keys,
            {
                "mode",
                "tool_repository",
                "tool_dir",
                "pin_groups",
                "all_corpora",
                "relevant_corpora",
                "heavy_corpora",
                "heavy_relevant",
                "external_corpora",
                "out_of_scope_corpora",
                "sibling_pins",
            },
        )



class CheckDriftProvenanceTransport(unittest.TestCase):
    """The check-drift job must fetch history or the exact commit, never a branch."""

    WORKFLOW = REPO_ROOT / ".github/workflows/adequacy-drift.yml"

    def _check_drift_job(self) -> str:
        text = self.WORKFLOW.read_text(encoding="utf-8")
        marker = "name: Adequacy drift gate"
        start = text.index(marker)
        return text[start:]

    def test_check_drift_checkout_is_not_shallow(self):
        """The gate's checkout used to be depth-1; a missing measured_at then skipped."""
        job = self._check_drift_job()
        checkout = job.split("uses: actions/checkout", 1)[1]
        # First checkout block in this job.
        self.assertIn("fetch-depth: 0", checkout.split("\n- ")[0])

    def test_check_drift_does_not_substitute_a_branch_for_the_commit(self):
        job = self._check_drift_job()
        self.assertNotIn("origin/main", job)
        self.assertNotIn("git fetch origin main", job)
        self.assertNotIn("refs/heads/main", job)

    def test_checker_step_has_no_continue_on_error(self):
        job = self._check_drift_job()
        self.assertIn("python3 conformance/adequacy/check_published_numbers.py", job)
        self.assertNotIn("continue-on-error:", job)

    def test_check_drift_does_not_fetch_commits_from_results_json(self):
        job = self._check_drift_job()
        self.assertNotIn("Fetch recorded measurement commits", job)
        self.assertNotIn("git fetch", job)


class MeasurementWorkspaceHygiene(unittest.TestCase):
    """Bounded isolation must see source, not a restored Cargo cache."""

    WORKFLOW = REPO_ROOT / ".github/workflows/adequacy-drift.yml"

    def test_restored_build_outputs_are_removed_before_measurement(self):
        workflow = self.WORKFLOW.read_text(encoding="utf-8")
        setup = workflow.index("uses: ./.github/actions/setup-rust")
        cleanup_test = workflow.index("python3 scripts/ci/test_adequacy_cleanup.py")
        cleanup_target = "CARGO_TARGET_DIR: ${{ github.workspace }}/target"
        target = workflow.index(cleanup_target)
        clean_command = "run: cargo clean\n"
        clean = workflow.index(clean_command)
        measure = workflow.index("python3 conformance/adequacy/measure_all.py")
        self.assertNotIn("cargo clean --workspace", workflow)
        self.assertLess(setup, cleanup_test)
        self.assertLess(cleanup_test, target)
        self.assertLess(target, clean)
        self.assertLess(setup, clean)
        self.assertLess(clean, measure)


if __name__ == "__main__":
    unittest.main(verbosity=2)
