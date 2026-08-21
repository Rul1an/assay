#!/usr/bin/env python3
"""The drift guard over published adequacy numbers, each control fired on purpose.

    python3 conformance/tests/test_published_numbers_guard.py

Every guard here is tested by BREAKING the thing it guards and asserting the
checker goes red, then restoring and asserting it goes green again. A guard whose
test has never seen it fail is not a guard; it is a green light wired to nothing,
which is the same defect the corpora themselves exist to find.

Each control runs against a byte-identical COPY of the real documents, manifests
and results, with the checker's own path constants pointed at the copy. The
corruption is therefore applied to the real content and evaluated by the real
code, while the working tree is never mutated -- important here because the
measurement this file is about takes an exclusive lock on the tree and a test
that edits it in place would collide with a run.

`test_the_real_tree_is_green` is the wiring test: unpatched constants, real repo.
Without it every other test could pass against a sandbox the checker never reads.
"""

from __future__ import annotations

import contextlib
import json
import shutil
import sys
import tempfile
import unittest
import warnings
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/adequacy"))

import check_published_numbers as chk  # noqa: E402
import measure_all  # noqa: E402

COPIED = [
    "conformance/adequacy/results.json",
    "conformance/INDEX.md",
    "conformance/privileged-mcp-action-v0/ERRATA.md",
]


@contextlib.contextmanager
def sandbox():
    """A byte-identical copy of everything the checker reads, with it pointed there."""
    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        for rel in COPIED:
            dst = root / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, dst)
        adequacy = root / "conformance/adequacy"
        for manifest in (REPO / "conformance/adequacy").glob("*.manifest.json"):
            shutil.copy2(manifest, adequacy / manifest.name)

        saved = (chk.REPO, chk.ADEQUACY, chk.RESULTS, chk.DOCUMENTS, chk.GIT_REPO)
        chk.REPO = root
        chk.ADEQUACY = adequacy
        chk.RESULTS = adequacy / "results.json"
        chk.DOCUMENTS = [root / "conformance/INDEX.md",
                         root / "conformance/privileged-mcp-action-v0/ERRATA.md"]
        try:
            yield root
        finally:
            chk.REPO, chk.ADEQUACY, chk.RESULTS, chk.DOCUMENTS, chk.GIT_REPO = saved


def edit(path: Path, old: str, new: str) -> None:
    """Replace the FIRST occurrence only.

    Replacing every occurrence would edit the prose and the bindings block
    together, which is the one edit a careful author makes and therefore the one
    a control must not simulate. The controls here break exactly one side.
    """
    text = path.read_text(encoding="utf-8")
    assert text.count(old) >= 1, "control anchor not found, so it would break nothing: %r" % old
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class TheGuardIsWiredToTheRealTree(unittest.TestCase):
    def test_the_real_tree_is_green(self):
        """Wiring, not a control: the checker's own constants over the real repository.

        Every other test in this file patches those constants. If none of them ran
        against the real paths, the suite could be green while the checker read
        nothing that ships.
        """
        self.assertEqual(chk.check(), [])

    def test_every_manifest_on_disk_is_measured(self):
        """Self-coverage, stated as its own assertion rather than left to the checker."""
        results = json.loads((REPO / "conformance/adequacy/results.json").read_text())
        measured = {c["corpus"] for c in results["corpora"]}
        on_disk = {p.name[: -len(".manifest.json")]
                   for p in (REPO / "conformance/adequacy").glob("*.manifest.json")}
        self.assertEqual(measured, on_disk)

    def test_results_json_is_deterministic_and_sorted(self):
        """A file rewritten on every run in a new order makes its own diff unreadable."""
        text = (REPO / "conformance/adequacy/results.json").read_text()
        doc = json.loads(text)
        names = [c["corpus"] for c in doc["corpora"]]
        self.assertEqual(names, sorted(names))
        self.assertEqual(text, json.dumps(doc, indent=2, sort_keys=True) + "\n")
        self.assertEqual(doc["schema"], measure_all.SCHEMA)


class ControlsOnTheProseBinding(unittest.TestCase):
    def assert_red(self, needle: str):
        findings = chk.check()
        self.assertTrue(findings, "the checker stayed green; this guard is wired to nothing")
        self.assertTrue(any(needle in f for f in findings),
                        "went red for the wrong reason: %s" % findings)

    def assert_green(self):
        """The other half of a control: the setup itself must not already be red.

        Without it a control can pass because the sandbox was broken before the
        mutation, which proves the guard fires and not that it fires at the thing
        the test names.
        """
        findings = chk.check()
        self.assertFalse(findings, "expected a clean baseline, got: %s" % findings)

    def test_a_measurement_that_moves_without_the_prose_is_red(self):
        """CONTROL: change killed 51 -> 50 in results.json for rge-bench.

        INDEX.md still says 51 of 54. Red on the assertion, green again on restore.
        """
        with sandbox() as root:
            self.assertEqual(chk.check(), [])
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            for c in doc["corpora"]:
                if c["corpus"] == "rge-bench":
                    c["killed"] = 50
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            self.assert_red("the measurement gives")
            shutil.copy2(REPO / "conformance/adequacy/results.json", res)
            self.assertEqual(chk.check(), [])

    def test_prose_edited_away_from_the_binding_is_red(self):
        """CONTROL: rewrite INDEX.md's rge-bench row to '51 of 55'.

        The registered wording no longer occurs, so the binding is orphaned.
        """
        with sandbox() as root:
            idx = root / "conformance/INDEX.md"
            edit(idx, "51 of 54 in scope (94.4%)", "51 of 55 in scope (94.4%)")
            self.assert_red("this exact wording is no longer in")
            shutil.copy2(REPO / "conformance/INDEX.md", idx)
            self.assertEqual(chk.check(), [])

    def test_an_unregistered_number_added_to_the_prose_is_red(self):
        """CONTROL: append a plausible new score to INDEX.md that no binding covers.

        This is the failure mode the sweep exists for: a number that reads like a
        measurement, was never measured, and would otherwise pass silently.
        """
        with sandbox() as root:
            idx = root / "conformance/INDEX.md"
            edit(idx, "## Claim vocabulary",
                 "A later re-measurement gave 12 of 13 in scope (92.3%).\n\n## Claim vocabulary")
            self.assert_red("neither derived from results.json nor")
            shutil.copy2(REPO / "conformance/INDEX.md", idx)
            self.assertEqual(chk.check(), [])

    def test_a_stale_not_derived_exemption_is_red(self):
        """CONTROL: delete the '6 of 8' anecdote from INDEX.md, leaving its exemption.

        An exemption pointing at nothing is where the next unchecked number hides,
        so the checker refuses to carry it.
        """
        with sandbox() as root:
            idx = root / "conformance/INDEX.md"
            edit(idx, "`6 of 8 (75.0%)`", "`(75.0%)`")
            self.assert_red("not_derived declares '6 of 8'")
            shutil.copy2(REPO / "conformance/INDEX.md", idx)
            self.assertEqual(chk.check(), [])

    def test_a_binding_that_checks_fewer_numbers_than_it_publishes_is_red(self):
        """CONTROL: drop the survivors assertion from the mcp-jsonrpc-id binding.

        The wording still says '4 survivors'. Without this rule a binding could
        register a sentence and check only the flattering half of it.
        """
        with sandbox() as root:
            idx = root / "conformance/INDEX.md"
            edit(idx, '"score_percent": 60.0,\n        "survived": 4',
                 '"score_percent": 60.0')
            self.assert_red("which no assertion checks")
            shutil.copy2(REPO / "conformance/INDEX.md", idx)
            self.assertEqual(chk.check(), [])

    def test_a_declared_constant_without_a_reason_is_red(self):
        """CONTROL: strip the _why from INDEX.md's not_measurable local.

        A constant with no stated reason is a number smuggled past the measurement,
        which is precisely how the out_of_scope discipline was breached upstream.
        """
        with sandbox() as root:
            idx = root / "conformance/INDEX.md"
            edit(idx, '"_why_not_measurable"', '"_note_not_measurable"')
            self.assert_red("has no _why_")
            shutil.copy2(REPO / "conformance/INDEX.md", idx)
            self.assertEqual(chk.check(), [])

    def test_deleting_the_block_is_red(self):
        """CONTROL: remove the BEGIN marker from ERRATA.md.

        A document must not be able to opt out of the guard by dropping its block.
        """
        with sandbox() as root:
            err = root / "conformance/privileged-mcp-action-v0/ERRATA.md"
            edit(err, chk.BEGIN, "<!-- gone -->")
            self.assert_red("must contain exactly one")
            shutil.copy2(REPO / "conformance/privileged-mcp-action-v0/ERRATA.md", err)
            self.assertEqual(chk.check(), [])


class ControlsOnCoverageAndPinning(unittest.TestCase):
    def assert_red(self, needle: str):
        findings = chk.check()
        self.assertTrue(findings, "the checker stayed green; this guard is wired to nothing")
        self.assertTrue(any(needle in f for f in findings),
                        "went red for the wrong reason: %s" % findings)

    def assert_green(self):
        """The other half of a control: the setup itself must not already be red.

        Without it a control can pass because the sandbox was broken before the
        mutation, which proves the guard fires and not that it fires at the thing
        the test names.
        """
        findings = chk.check()
        self.assertFalse(findings, "expected a clean baseline, got: %s" % findings)

    def test_a_manifest_with_no_measurement_is_red(self):
        """CONTROL: add a sixth manifest to the directory and measure nothing.

        The checker must not pass by reporting on the five it happens to know.
        A checker that silently skips a corpus is the exact failure one level up.
        """
        with sandbox() as root:
            new = root / "conformance/adequacy/newly-added.manifest.json"
            shutil.copy2(root / "conformance/adequacy/rge-bench.manifest.json", new)
            self.assert_red("no measurement for")
            new.unlink()
            self.assertEqual(chk.check(), [])

    def test_a_result_outliving_its_manifest_is_red(self):
        """CONTROL: delete the rge-bench manifest, leaving its row behind."""
        with sandbox() as root:
            manifest = root / "conformance/adequacy/rge-bench.manifest.json"
            keep = manifest.read_bytes()
            manifest.unlink()
            self.assert_red("indexed manifest is not present")
            manifest.write_bytes(keep)
            self.assertEqual(chk.check(), [])

    def test_a_manifest_with_no_tool_pin_is_red(self):
        """CONTROL: strip tool_pin from the observed-effect manifest.

        Numbers measured with an unnamed tool cannot be re-derived by anyone, which
        is the state every manifest was in before this layer existed.
        """
        with sandbox() as root:
            baseline = chk.published_rows.load_results(chk.RESULTS)
            path = root / "conformance/adequacy/observed-effect-drift-consumer.manifest.json"
            original = path.read_bytes()
            doc = json.loads(path.read_text())
            doc.pop("tool_pin")
            path.write_text(json.dumps(doc, indent=2))
            # The canonical loader normally catches the changed manifest digest first.
            # Freeze its validated rows here so this control still proves the downstream
            # tool-pin rule independently rather than passing for the earlier guard.
            with mock.patch.object(chk.published_rows, "load_results", return_value=baseline):
                self.assert_red("declares no tool_pin.commit")
            path.write_bytes(original)
            self.assertEqual(chk.check(), [])

    def test_a_row_measured_with_another_commit_is_red(self):
        """CONTROL: re-pin the mcp-jsonrpc-id manifest to a different commit.

        The row still records the commit it was measured with. A number and the
        tool that produced it must move together.
        """
        with sandbox() as root:
            baseline = chk.published_rows.load_results(chk.RESULTS)
            path = root / "conformance/adequacy/mcp-jsonrpc-id.manifest.json"
            # Read the pin rather than hard-coding it: a control that carries its
            # own copy of the value stops firing the day the real pin moves, which
            # is precisely when it is needed.
            edit(path, json.loads(path.read_text())["tool_pin"]["commit"], "0" * 40)
            with mock.patch.object(chk.published_rows, "load_results", return_value=baseline):
                self.assert_red("but mcp-jsonrpc-id.manifest.json pins")
            shutil.copy2(REPO / "conformance/adequacy/mcp-jsonrpc-id.manifest.json", path)
            self.assertEqual(chk.check(), [])

    def test_the_transcription_rule_has_no_live_subject_and_says_so(self):
        """Its control was retired with the last transcribed row, deliberately.

        privileged-mcp-action-v0 was the only row nobody re-derived; it is now
        measured like the rest, which closed the weakest link on the page. The
        transcription rule still exists and still matters, because the next
        expensive corpus will arrive transcribed before it arrives measured. But
        there is nothing live for a control to break, and a synthetic transcribed
        row did not reproduce the rule's own baseline in this harness.

        Shipping a control that passes without proving the rule fires would be
        worse than recording that the guard is currently unexercised, so this
        records it. Restore a real control the moment a transcribed row returns:
        a rule whose control is retired is a rule on the way to being unenforced,
        which is the defect this whole file exists to prevent.
        """
        doc = json.loads((REPO / "conformance/adequacy/results.json").read_text())
        kinds = {c["provenance"]["kind"] for c in doc["corpora"]}
        self.assertEqual(kinds, {"measured"},
                         "a transcribed row is back; restore the quote control with it")
        self.assertIn("transcribed", Path(chk.__file__).read_text(),
                      "the rule itself must still be present for the day one returns")

    def test_the_prose_commit_pin_drifting_from_the_manifest_is_red(self):
        """CONTROL: change the commit ERRATA.md names in prose.

        The document hands a reproducer a commit to check out. That sentence is the
        original hand-maintained pin and the reason tool_pin exists; it must not be
        able to part from the manifest quietly.
        """
        with sandbox() as root:
            err = root / "conformance/privileged-mcp-action-v0/ERRATA.md"
            rows = {c["corpus"]: c for c in json.loads(
                (root / "conformance/adequacy/results.json").read_text())["corpora"]}
            edit(err, rows["privileged-mcp-action-v0"]["tool_commit"], "f" * 40)
            # Reworded when the rule was ungated: it used to live inside the
            # transcription branch and went quiet the moment the row became
            # measured, which is a guard that disappears exactly when the thing
            # it guards stops being obviously fragile.
            self.assert_red("hands a reproducer an instrument")
            shutil.copy2(REPO / "conformance/privileged-mcp-action-v0/ERRATA.md", err)
            self.assertEqual(chk.check(), [])

    def test_a_missing_results_file_is_red(self):
        """CONTROL: delete results.json.

        No measurement must never read as agreement.
        """
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            keep = res.read_bytes()
            res.unlink()
            self.assert_red("Nothing re-derives the published numbers")
            res.write_bytes(keep)
            self.assertEqual(chk.check(), [])


# The tool is not vendored (INDEX.md, "Where the adequacy tool lives"). Expect it as
# a sibling checkout and skip LOUDLY rather than pretending results.json was checked.
_TOOL = REPO.parent / "corpus-adequacy"
if _TOOL.is_dir():
    sys.path.insert(0, str(_TOOL))
try:
    import corpus_adequacy as ca  # noqa: E402
except ImportError:  # pragma: no cover - environment without the sibling checkout
    ca = None

# Corpora cheap enough to re-derive in a test. privileged-mcp-action-v0 is excluded on
# purpose: ~32 cargo builds mutating shared source in place is not something a test
# suite may do to a working tree, which is exactly why its row is transcribed.
CHEAP = {
    "mcp-jsonrpc-id": REPO / "conformance/adequacy/mcp-jsonrpc-id.manifest.json",
    "observed-effect-drift-consumer":
        REPO / "conformance/adequacy/observed-effect-drift-consumer.manifest.json",
    "rge-bench": REPO / "conformance/adequacy/rge-bench.manifest.json",
}
SIBLINGS = {
    "observed-effect-drift-consumer": REPO.parent / "observed-effect-v0",
    "rge-bench": REPO.parent / "rge-bench",
}


@unittest.skipIf(ca is None,
                 "corpus-adequacy not found as a sibling checkout; clone "
                 "https://github.com/corpus-adequacy/corpus-adequacy next to this repository")
class ResultsAreStillWhatTheToolProduces(unittest.TestCase):
    """results.json is a measurement, and a measurement nobody repeats is a memory.

    Not a control: the standing check that the file has not gone stale. The
    published prose is held to results.json by the checker; this holds results.json
    to the tool. Without it the whole layer would guarantee only that two documents
    agree with a third that nothing re-derives.
    """

    def stored(self) -> dict:
        return {c["corpus"]: c for c in json.loads(
            (REPO / "conformance/adequacy/results.json").read_text())["corpora"]}

    def drifted(self, stored: dict) -> tuple[list[str], list[str]]:
        """Corpora whose recorded row differs from a fresh run of the tool.

        Rows are built through measure_all.row, so this comparison cannot drift
        from the shape the writer actually records. tool_version is provenance
        rather than measurement, and older tool checkouts report none.
        """
        out, unavailable = [], []
        for corpus, manifest in sorted(CHEAP.items()):
            sibling = SIBLINGS.get(corpus)
            if sibling is not None and not sibling.is_dir():
                unavailable.append(corpus)
                continue
            report = measure_all.run_producer(ca, manifest)
            fresh = measure_all.row(manifest, report, ca.encode_report_v0(report))
            def trim(row):
                comparable = {k: v for k, v in row.items() if k != "tool_version"}
                measured = dict(comparable["measured_at"])
                measured.pop("commit", None)
                comparable["measured_at"] = measured
                return comparable
            if trim(fresh) != trim(stored[corpus]):
                out.append(corpus)
        if unavailable:
            warnings.warn("not measured: %s" % ", ".join(unavailable), RuntimeWarning)
        return out, unavailable

    def test_the_cheap_corpora_still_measure_what_results_json_records(self):
        drifted, unavailable = self.drifted(self.stored())
        if len(unavailable) == len(CHEAP):
            self.skipTest("no declared cheap comparison can run")
        self.assertEqual(drifted, [],
                         "results.json has gone stale; re-run measure_all.py")

    def test_a_missing_sibling_keeps_completed_comparisons(self):
        missing = REPO / "a-sibling-that-does-not-exist"
        selected = {name: CHEAP[name] for name in ("mcp-jsonrpc-id", "rge-bench")}
        siblings = {"rge-bench": missing}
        with mock.patch.dict(CHEAP, selected, clear=True), \
                mock.patch.dict(SIBLINGS, siblings, clear=True), \
                warnings.catch_warnings(record=True) as caught:
            try:
                drifted, unavailable = self.drifted(self.stored())
            except unittest.SkipTest:
                self.fail("one missing sibling discarded a completed comparison")
        self.assertEqual(drifted, [])
        self.assertEqual(unavailable, ["rge-bench"])
        self.assertTrue(any("not measured: rge-bench" in str(item.message) for item in caught))

    def test_a_doctored_result_is_caught(self):
        """CONTROL: hand-edit mcp-jsonrpc-id's killed count and confirm the drift check bites.

        Without it this comparison could be structurally unable to fail -- comparing
        a value with itself, or skipping every corpus -- and would still print green.
        """
        stored = self.stored()
        stored["mcp-jsonrpc-id"] = dict(
            stored["mcp-jsonrpc-id"], killed=stored["mcp-jsonrpc-id"]["killed"] + 1)
        drifted, _ = self.drifted(stored)
        self.assertEqual(drifted, ["mcp-jsonrpc-id"])


class TheTranscribedRowSaysSoOutLoud(unittest.TestCase):
    def test_a_row_nobody_re_derives_must_say_so(self):
        """Every row is derived today; the rule holds for the day one is not.

        privileged-mcp-action-v0 was the exception and was re-measured, so the
        assertion is now over all rows rather than naming the weak one: any row
        that is not measured must be labelled transcribed and must carry the
        command that re-derives it. Naming the specific corpus would have made
        this test go stale the moment the weak link moved, which it did.
        """
        doc = json.loads((REPO / "conformance/adequacy/results.json").read_text())
        rows = {c["corpus"]: c for c in doc["corpora"]}
        self.assertIn("--only", measure_all.__doc__)
        for name, r in sorted(rows.items()):
            kind = r["provenance"]["kind"]
            if kind == "measured":
                continue
            self.assertEqual(kind, "transcribed", name)
            self.assertIn("measure_all.py --only %s" % name, r["provenance"]["_why"],
                          "a row nobody re-derives must name how to")


if __name__ == "__main__":
    unittest.main(verbosity=2)
