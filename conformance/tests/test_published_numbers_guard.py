#!/usr/bin/env python3
"""The guard over published adequacy numbers, each control fired on purpose.

    python3 conformance/tests/test_published_numbers_guard.py

`conformance/INDEX.md` and `conformance/privileged-mcp-action-v0/ERRATA.md` are
generated from `.in` templates: the narrative is hand-written, every measured cell
is a token projected from `results.json`, and `check_published_numbers.py`
regenerates both and compares them byte for byte.

Every guard here is tested by BREAKING the thing it guards and asserting the
checker or the generator goes red for the RIGHT reason, then restoring and
asserting it goes green again. A guard whose test has never seen it fail is not a
guard; it is a green light wired to nothing, which is the same defect the corpora
themselves exist to find.

Each control runs against a byte-identical COPY of the real documents, templates,
manifests and declarations, with the checker's path constant pointed at the copy.
The corruption is therefore applied to the real content and evaluated by the real
code, while the working tree is never mutated -- important here because the
measurement this file is about takes an exclusive lock on the tree and a test that
edited it in place would collide with a run.

`test_the_real_tree_is_green` is the wiring test: unpatched constant, real repo.
Without it every other test could pass against a sandbox the checker never reads.
"""

from __future__ import annotations

import contextlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/adequacy"))

import check_published_numbers as chk  # noqa: E402
import measure_all  # noqa: E402
import publish_numbers as pub  # noqa: E402

COPIED = [
    "conformance/adequacy/results.json",
    "conformance/adequacy/adjustments.json",
    "conformance/adequacy/unprojected_numbers.json",
    "conformance/INDEX.md",
    "conformance/INDEX.md.in",
    "conformance/privileged-mcp-action-v0/ERRATA.md",
    "conformance/privileged-mcp-action-v0/ERRATA.md.in",
]

IDX_IN = "conformance/INDEX.md.in"
IDX = "conformance/INDEX.md"
ERR_IN = "conformance/privileged-mcp-action-v0/ERRATA.md.in"
ERR = "conformance/privileged-mcp-action-v0/ERRATA.md"


@contextlib.contextmanager
def sandbox():
    """A byte-identical copy of everything the checker reads, with it pointed there.

    The copy is a real git repository containing one commit, because the staleness
    rule asks git whether a row's declared dependencies moved. Pointed at a
    directory that is not a checkout, `git diff` fails, the rule declines to make
    a claim either way, and every staleness control would pass for the wrong
    reason -- a control that cannot fail.
    """
    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        for rel in COPIED:
            dst = root / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / rel, dst)
        adequacy = root / "conformance/adequacy"
        for manifest in (REPO / "conformance/adequacy").glob("*.manifest.json"):
            shutil.copy2(manifest, adequacy / manifest.name)

        git = lambda *a: subprocess.run(["git", "-C", str(root), *a],  # noqa: E731
                                        capture_output=True, text=True, check=True)
        git("init", "-q")
        git("config", "user.email", "t@example.invalid")
        git("config", "user.name", "control")
        git("add", "-A")
        git("commit", "-qm", "sandbox")
        head = git("rev-parse", "HEAD").stdout.strip()

        # Every row is re-pinned to the sandbox's own single commit. Its real
        # measured_at commit does not exist here, `git diff` would fail, and the
        # staleness rule would go quiet in exactly the harness meant to exercise it.
        res = adequacy / "results.json"
        doc = json.loads(res.read_text())
        for row in doc["corpora"]:
            row.setdefault("measured_at", {})["commit"] = head
        res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
        git("add", "-A")
        git("commit", "-qm", "re-pin")

        saved = chk.REPO
        chk.REPO = root
        try:
            yield root
        finally:
            chk.REPO = saved


def edit(path: Path, old: str, new: str) -> None:
    """Replace the FIRST occurrence only, and refuse an anchor that is not there."""
    text = path.read_text(encoding="utf-8")
    assert text.count(old) >= 1, "control anchor not found, so it would break nothing: %r" % old
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


class Fires(unittest.TestCase):
    """assert_red / assert_green, shared by every control class below."""

    def assert_red(self, needle: str):
        findings = chk.check()
        self.assertTrue(findings, "the checker stayed green; this guard is wired to nothing")
        self.assertTrue(any(needle in f for f in findings),
                        "went red for the wrong reason: %s" % findings)

    def assert_green(self):
        findings = chk.check()
        self.assertFalse(findings, "expected a clean baseline, got: %s" % findings)


class TheGuardIsWiredToTheRealTree(unittest.TestCase):
    def test_the_real_tree_is_green(self):
        """Wiring, not a control: the checker's own constant over the real repository."""
        self.assertEqual(chk.check(), [])

    def test_the_sandbox_itself_is_green(self):
        """The other half of every control below: the copy must start clean.

        Without it a control can pass because the sandbox was broken before the
        mutation, which proves the guard fires and not that it fires at the thing
        the test names.
        """
        with sandbox():
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

    def test_the_denominator_sources_are_declared_where_the_row_can_see_them(self):
        """The v0 score's denominator rests on an audit of the profile, not only on code.

        A normative rule added to `v0.md` changes what the corpus ought to be
        scored against, mutates nothing, and would move nothing here unless the
        profile is a declared dependency. Asserted on the manifest AND on the
        function that consumes it, because either alone is a declaration nobody
        reads.
        """
        manifest = json.loads((REPO / "conformance/adequacy"
                               / "privileged-mcp-action-v0.manifest.json").read_text())
        declared = set(manifest["declaration_sources"])
        for expected in ("../../docs/profiles/privileged-mcp-action/v0.md",
                         "../privileged-mcp-action-v0/MANIFEST.json",
                         "../privileged-mcp-action-v0/gen_vectors.py",
                         "../privileged-mcp-action-v0/vectors"):
            self.assertIn(expected, declared)
        derived = measure_all.measured_at(
            manifest, REPO / "conformance/adequacy/privileged-mcp-action-v0.manifest.json")
        self.assertIn("docs/profiles/privileged-mcp-action/v0.md",
                      derived["measured_at"]["depends_on"])
        self.assertIn("conformance/privileged-mcp-action-v0/vectors",
                      derived["measured_at"]["depends_on"])


class ControlsOnProjection(Fires):
    def test_a_hand_edited_generated_file_is_red(self):
        """CONTROL: change a projected number in INDEX.md and leave the template alone.

        The single failure mode regenerate-and-compare exists for.
        """
        with sandbox() as root:
            self.assert_green()
            edit(root / IDX, "51 of 54 in scope", "52 of 54 in scope")
            self.assert_red("is not what conformance/INDEX.md.in projects")
            shutil.copy2(REPO / IDX, root / IDX)
            self.assert_green()

    def test_a_hand_edited_narrative_is_red_too(self):
        """CONTROL: reword a sentence in the generated file rather than the template.

        Projection owns the whole document, not only its cells: prose edited in the
        output is prose that will vanish on the next regeneration.
        """
        with sandbox() as root:
            edit(root / IDX, "Agreement\n  with yourself is not evidence.",
                 "Agreement\n  with yourself is fine, actually.")
            self.assert_red("is not what conformance/INDEX.md.in projects")
            shutil.copy2(REPO / IDX, root / IDX)
            self.assert_green()

    def test_a_measurement_that_moves_without_a_regeneration_is_red(self):
        """CONTROL: change killed 51 -> 50 for rge-bench and do not regenerate.

        The old checker's headline case, now answered by the same comparison.
        """
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            for c in doc["corpora"]:
                if c["corpus"] == "rge-bench":
                    c["killed"] = 50
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            self.assert_red("is not what conformance/INDEX.md.in projects")

    def test_a_measurement_that_moves_WITH_a_regeneration_is_green_and_moves_the_prose(self):
        """The positive half: the numbers follow the JSON without anyone typing them.

        A comparison that can only ever go red proves the guard fires, not that the
        mechanism works. This one asserts the English WORD form moved as well as
        the digits, which is the requirement the whole scheme turns on.
        """
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            for c in doc["corpora"]:
                if c["corpus"] == "privileged-mcp-action-v0":
                    c["killed"], c["survived"] = 7, 18
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            for target, text in pub.render_all(root).items():
                target.write_text(text, encoding="utf-8")
            self.assert_green()
            index = (root / IDX).read_text()
            self.assertIn("7 of 25 in scope", index)
            self.assertIn("The corpus isolates seven:", index)
            self.assertNotIn("The corpus isolates six:", index)

    def test_the_word_form_and_the_digit_form_cannot_disagree(self):
        """Both read one field, so there is no second place for a number to rot.

        Rendered over every integer field of every corpus rather than the two the
        documents happen to use: the property is of the renderer, and a test that
        only covered today's usage would stop covering it the day a template
        changed.
        """
        ctx = pub.build_context(REPO)
        checked = 0
        for scope, fields in ctx.items():
            for field, cell in fields.items():
                if not isinstance(cell.value, int) or isinstance(cell.value, bool):
                    continue
                digits = pub.render("{{measured:%s.%s}}" % (scope, field), ctx, {})
                words = pub.render("{{measured:%s.%s|words}}" % (scope, field), ctx, {})
                caps = pub.render("{{measured:%s.%s|Words}}" % (scope, field), ctx, {})
                bare = digits.replace(pub.JUDGED, "")
                self.assertEqual(pub.to_words(int(bare)), words.replace(pub.JUDGED, ""))
                self.assertEqual(words.capitalize(), caps.capitalize())
                checked += 1
        self.assertGreater(checked, 20, "the sweep over fields selected almost nothing")


class ControlsOnTokens(unittest.TestCase):
    """Generation-time hard errors. A bad token must stop the run, not render blank."""

    def render(self, template: str) -> str:
        ctx = pub.build_context(REPO)
        return pub.render(template, ctx, {})

    def test_an_unknown_corpus_is_a_hard_error(self):
        """CONTROL: name a corpus results.json does not measure."""
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("{{measured:no-such-corpus.killed}}")
        self.assertIn("which results.json does not measure", str(caught.exception))

    def test_an_unknown_field_is_a_hard_error(self):
        """CONTROL: name a field that does not exist on a corpus that does."""
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("{{measured:rge-bench.almost_killed}}")
        self.assertIn("has no field", str(caught.exception))

    def test_neither_renders_an_empty_string(self):
        """The failure this refuses: a mistyped token silently deleting a number.

        Asserted as its own case because "raises" and "does not quietly render
        nothing" are different properties and only the second one matters here.
        """
        for bad in ("{{measured:no-such-corpus.killed}}", "{{measured:rge-bench.nope}}"):
            with self.assertRaises(pub.TemplateError):
                out = self.render("before " + bad + " after")
                self.fail("rendered %r instead of failing" % out)

    def test_a_null_field_is_a_hard_error(self):
        """CONTROL: rfc8785 has no score. A corpus with no number publishes none."""
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("{{measured:rfc8785-canonicalization.score_percent|1f}}")
        self.assertIn("is null", str(caught.exception))

    def test_a_word_form_of_a_percentage_is_a_hard_error(self):
        """CONTROL: ask for words where the value is not a whole count."""
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("{{measured:rge-bench.score_percent|words}}")
        self.assertIn("word form exists for whole counts", str(caught.exception))

    def test_an_unparsed_brace_is_a_hard_error(self):
        """CONTROL: two typos, one caught by the grammar and one only by the guard.

        `{{measured rge-bench.killed}}` does not match the token pattern at all,
        so nothing substitutes it and it would render verbatim into a published
        document, where it reads as a redaction. That is the case the leftover-
        brace guard exists for. The comma form does match the pattern and fails
        one step later, on the scope-and-field rule; both must stop the run.
        """
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("scored {{measured rge-bench.killed}} of them")
        self.assertIn("unparsed", str(caught.exception))

        with self.assertRaises(pub.TemplateError) as caught:
            self.render("scored {{measured:rge-bench,killed}} of them")
        self.assertIn("must name a scope and a field", str(caught.exception))

    def test_a_float_with_no_declared_form_is_a_hard_error(self):
        """CONTROL: 24.0 and 24 are both defensible renderings, so neither is a default."""
        with self.assertRaises(pub.TemplateError) as caught:
            self.render("{{measured:rge-bench.score_percent}}")
        self.assertIn("rendering is a decision and not a default", str(caught.exception))


class ControlsOnStaleness(Fires):
    def test_a_stale_corpus_renders_the_phrase_and_not_a_number(self):
        """CONTROL: move a file rge-bench declared it depends on, and regenerate.

        The rule the objection asked for: a number this revision did not measure
        must not appear as a number. Asserted on the rendered page, in running
        prose, and asserted NEGATIVELY too -- the digits must be gone, not merely
        accompanied by a warning somewhere else.
        """
        with sandbox() as root:
            self.assert_green()
            manifest = root / "conformance/adequacy/rge-bench.manifest.json"
            manifest.write_text(manifest.read_text().replace(
                '"schema"', '"_touched": "control", "schema"', 1))
            subprocess.run(["git", "-C", str(root), "commit", "-qam", "move a declared source"],
                           check=True, capture_output=True)

            self.assertIn("rge-bench", pub.stale_corpora(pub.load_results(root), root))
            index = pub.render_all(root)[root / IDX]
            self.assertIn("**%s of %s in scope" % (pub.STALE, pub.STALE), index)
            self.assertNotIn("51 of 54 in scope", index)
            self.assertNotIn("94.4%", index)
            # ...and the check is red until someone regenerates, then red because
            # the row is stale. Both readings are the same instruction: re-measure.
            self.assert_red("changed since, so the published number describes code")

    def test_the_staleness_rule_has_exactly_one_implementation(self):
        """The checker's finding and the renderer's refusal must never disagree.

        Two answers to "is this row current" would drift, and the copy that drifts
        is the one that stops noticing. Asserted by identity rather than by
        comparing outputs, because equal outputs today prove nothing about
        tomorrow.
        """
        self.assertIs(chk.publish_numbers.stale_corpora, pub.stale_corpora)
        # The checker asks git nothing. It cannot: it no longer has the import.
        # A structural assertion rather than a comparison of outputs, because
        # equal outputs today prove nothing about tomorrow.
        self.assertNotIn("import subprocess", Path(chk.__file__).read_text())


class ControlsOnTheSweep(Fires):
    def test_a_new_hand_written_number_in_the_template_is_red(self):
        """CONTROL: add a plausible score to the template's narrative.

        This is the case regenerate-and-compare structurally cannot see: the
        figure was never a token, so the document regenerates byte-identically
        and reads as a measurement. Without the sweep this control is green.
        """
        with sandbox() as root:
            edit(root / IDX_IN, "## Claim vocabulary",
                 "A later re-measurement gave 12 of 13 in scope (92.3%).\n\n## Claim vocabulary")
            for target, text in pub.render_all(root).items():
                target.write_text(text, encoding="utf-8")
            # The byte comparison is satisfied. Only the sweep is not.
            self.assertEqual(pub.differences(root), [])
            self.assert_red("neither a projection token nor declared")
            shutil.copy2(REPO / IDX_IN, root / IDX_IN)
            shutil.copy2(REPO / IDX, root / IDX)
            self.assert_green()

    def test_a_stale_exemption_is_red(self):
        """CONTROL: delete the '6 of 8' anecdote, leaving its exemption behind.

        An exemption pointing at nothing is where the next unchecked number hides.
        """
        with sandbox() as root:
            path = root / IDX_IN
            # Every occurrence: an exemption covers the whole template, so leaving
            # one behind would leave the exemption legitimately in use and the
            # control would be testing nothing.
            self.assertIn("6 of 8", path.read_text())
            path.write_text(path.read_text().replace("6 of 8", "the contaminated run"))
            for target, text in pub.render_all(root).items():
                target.write_text(text, encoding="utf-8")
            self.assert_red("declares '6 of 8', which no longer appears")

    def test_an_exemption_without_a_reason_is_red(self):
        """CONTROL: strip the reason from a declared exemption.

        A number exempted with no stated reason is a number smuggled past the
        measurement, which is how the discipline was breached upstream.
        """
        with sandbox() as root:
            path = root / "conformance/adequacy/unprojected_numbers.json"
            doc = json.loads(path.read_text())
            doc["documents"][IDX_IN][0]["reason"] = "   "
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("needs a token and a reason")

    def test_a_template_with_no_sweep_entry_is_red(self):
        """CONTROL: remove a template's entry entirely.

        A document must not be able to opt out of the sweep by having no list; an
        empty list is a declaration, a missing one is a silence.
        """
        with sandbox() as root:
            path = root / "conformance/adequacy/unprojected_numbers.json"
            doc = json.loads(path.read_text())
            doc["documents"].pop(ERR_IN)
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("has no entry in conformance/adequacy/unprojected_numbers.json")


class ControlsOnDeclaredJudgements(Fires):
    def test_a_judged_figure_does_not_render_like_a_measurement(self):
        """The laundering the marking exists to prevent, asserted on the page.

        `six` is the tool's. `five` is the tool's minus a human judgement. If both
        render bare, this mechanism does not merely fail to check the judgement --
        it lends it the tool's authority.
        """
        errata = (REPO / ERR).read_text()
        self.assertIn("five (judged)", errata)
        self.assertIn("18.5% (judged)", errata)
        self.assertNotIn("The denominator here is **twenty-seven (judged)**", errata)
        self.assertIn("The denominator here is **twenty-seven**", errata)
        index = (REPO / IDX).read_text()
        self.assertIn("one (judged) not measurable", index)
        self.assertIn("**Four measured, one control-only,", index)

    def test_an_unmarked_judgement_would_be_caught(self):
        """CONTROL: render a judged field with the marker suppressed.

        Without this, `JUDGED` could be set to the empty string and every
        assertion above would still pass on a page that laundered the judgement.
        """
        saved = pub.JUDGED
        try:
            pub.JUDGED = ""
            plain = pub.render("{{measured:privileged-mcp-action-v0.third_party_killed|words}}",
                               pub.build_context(REPO), {})
            self.assertEqual(plain, "five")
        finally:
            pub.JUDGED = saved
        marked = pub.render("{{measured:privileged-mcp-action-v0.third_party_killed|words}}",
                            pub.build_context(REPO), {})
        self.assertEqual(marked, "five (judged)")
        self.assertNotEqual(plain, marked)

    def test_an_adjustment_without_a_reason_is_a_hard_error(self):
        """CONTROL: strip the _why from the declared adjustment."""
        with sandbox() as root:
            path = root / "conformance/adequacy/adjustments.json"
            doc = json.loads(path.read_text())
            doc["adjustments"][0].pop("_why")
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("no _why")

    def test_an_adjustment_nothing_publishes_is_a_hard_error(self):
        """CONTROL: declare an adjustment no token consumes."""
        with sandbox() as root:
            path = root / "conformance/adequacy/adjustments.json"
            doc = json.loads(path.read_text())
            doc["adjustments"].append({"scope": "rge-bench", "name": "invented",
                                       "effect": "declared_count", "value": 3,
                                       "_why": "a control"})
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("no published token uses it")

    def test_an_unknown_effect_is_a_hard_error(self):
        """CONTROL: an effect the generator does not implement must not be ignored.

        A declaration silently dropped is a stated judgement that stopped applying
        without anyone being told.
        """
        with sandbox() as root:
            path = root / "conformance/adequacy/adjustments.json"
            doc = json.loads(path.read_text())
            doc["adjustments"][0]["effect"] = "rounds_up_a_bit"
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("which this generator does not")

    def test_the_adjustment_actually_moves_the_published_figure(self):
        """Not a control: the arithmetic is in one place and this is that place.

        Reading the declared value rather than hard-coding 1, so the test does not
        carry its own copy of the number it is checking.
        """
        declared = pub.load_adjustments(REPO)["privileged-mcp-action-v0"]
        moved = declared["not_isolated_for_a_third_party"]["value"]
        f = pub.build_context(REPO)["privileged-mcp-action-v0"]
        self.assertEqual(f["third_party_killed"].value, f["killed"].value - moved)
        self.assertEqual(f["third_party_not_killed"].value,
                         f["in_scope_with_holes"].value - f["third_party_killed"].value)
        self.assertTrue(f["third_party_killed"].judged)
        self.assertFalse(f["killed"].judged)


class ControlsOnCoverageAndPinning(Fires):
    def test_a_manifest_with_no_measurement_is_red(self):
        """CONTROL: add a sixth manifest to the directory and measure nothing."""
        with sandbox() as root:
            new = root / "conformance/adequacy/newly-added.manifest.json"
            shutil.copy2(root / "conformance/adequacy/rge-bench.manifest.json", new)
            self.assert_red("no measurement for")
            new.unlink()
            self.assert_green()

    def test_a_result_outliving_its_manifest_is_red(self):
        """CONTROL: delete the rge-bench manifest, leaving its row behind."""
        with sandbox() as root:
            manifest = root / "conformance/adequacy/rge-bench.manifest.json"
            keep = manifest.read_bytes()
            manifest.unlink()
            self.assert_red("has no manifest on disk")
            manifest.write_bytes(keep)
            self.assert_green()

    def test_a_manifest_with_no_tool_pin_is_red(self):
        """CONTROL: strip tool_pin from the observed-effect manifest."""
        with sandbox() as root:
            path = root / "conformance/adequacy/observed-effect-drift-consumer.manifest.json"
            doc = json.loads(path.read_text())
            saved = doc.pop("tool_pin")
            path.write_text(json.dumps(doc, indent=2))
            self.assert_red("declares no tool_pin.commit")
            doc["tool_pin"] = saved
            path.write_text(json.dumps(doc, indent=2))
            self.assert_green()

    def test_a_row_measured_with_another_commit_is_red(self):
        """CONTROL: re-pin the mcp-jsonrpc-id manifest to a different commit."""
        with sandbox() as root:
            path = root / "conformance/adequacy/mcp-jsonrpc-id.manifest.json"
            # Read the pin rather than hard-coding it: a control that carries its
            # own copy of the value stops firing the day the real pin moves.
            edit(path, json.loads(path.read_text())["tool_pin"]["commit"], "0" * 40)
            self.assert_red("but mcp-jsonrpc-id.manifest.json pins")

    def test_a_hand_written_tool_commit_in_a_template_is_red(self):
        """CONTROL: replace ERRATA's tool-commit TOKEN with a literal hash.

        The token makes the published pin unwritable. This rule is what remains
        useful: a template can still hard-code a hash, and that is the pin that
        drifted before -- the manifest was re-pinned to a newer tool and the
        sentence kept handing a reproducer the old commit.
        """
        with sandbox() as root:
            edit(root / ERR_IN, "{{measured:privileged-mcp-action-v0.tool_commit}}", "f" * 40)
            for target, text in pub.render_all(root).items():
                target.write_text(text, encoding="utf-8")
            self.assert_red("hands a reproducer an instrument")

    def test_an_out_of_tree_commit_no_document_names_is_red(self):
        """CONTROL: remove the subject-commit token from INDEX's rge-bench row.

        Provenance recorded only in results.json is provenance the reader does not
        get. Projection is how the requirement is now MET; deleting the token is
        what this rule still notices.
        """
        with sandbox() as root:
            edit(root / IDX_IN, "@ `{{measured:rge-bench.subject_commit|short}}`", "@ some commit")
            for target, text in pub.render_all(root).items():
                target.write_text(text, encoding="utf-8")
            self.assert_red("no published document names that commit")

    def test_a_dirty_out_of_tree_measurement_is_red(self):
        """CONTROL: mark the rge-bench checkout dirty in the recorded row."""
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            for c in doc["corpora"]:
                if c["corpus"] == "rge-bench":
                    c["subject"]["repos"][0]["dirty"] = True
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            self.assert_red("describes a working tree nobody else has")

    def test_a_missing_results_file_is_red(self):
        """CONTROL: delete results.json. No measurement must never read as agreement."""
        with sandbox() as root:
            (root / "conformance/adequacy/results.json").unlink()
            self.assert_red("Nothing re-derives the published numbers")

    def test_the_transcription_rule_has_no_live_subject_and_says_so(self):
        """Its control was retired with the last transcribed row, deliberately.

        privileged-mcp-action-v0 was the only row nobody re-derived; it is now
        measured like the rest. The transcription rule still exists and still
        matters, because the next expensive corpus will arrive transcribed before
        it arrives measured. But there is nothing live for a control to break, and
        shipping a control that passes without proving the rule fires would be
        worse than recording that the guard is currently unexercised.
        """
        doc = json.loads((REPO / "conformance/adequacy/results.json").read_text())
        kinds = {c["provenance"]["kind"] for c in doc["corpora"]}
        self.assertEqual(kinds, {"measured"},
                         "a transcribed row is back; restore the quote control with it")
        self.assertIn("transcribed", Path(chk.__file__).read_text(),
                      "the rule itself must still be present for the day one returns")


class WhatTheGuardsSubjectActuallyIs(unittest.TestCase):
    """How much of each template is derived, published rather than assumed.

    A generator whose subject is mostly prose guarantees mostly nothing --
    `docs/generated/module-map.mermaid` sits in this repository's drift list,
    passes on every commit and reads nothing from the tree. The ratio is the only
    thing that separates a derivation from a heredoc with holes in it, so it is
    measured here and reported rather than hoped about. The floor is deliberately
    low: this asserts the templates have not quietly stopped projecting, not that
    the ratio is good.
    """

    def test_the_ratio_is_recorded_and_has_not_collapsed(self):
        density = pub.token_density(REPO)
        self.assertEqual(set(density), {IDX_IN, ERR_IN})
        for name, d in sorted(density.items()):
            print("\n    %s: %d tokens over %d authored words (%s per 1000), %d distinct fields"
                  % (name, d["tokens"], d["prose_words"], d["tokens_per_1000_words"],
                     d["distinct_fields"]))
            self.assertGreaterEqual(d["tokens"], 20)
            self.assertGreaterEqual(d["distinct_fields"], 10)

    def test_every_published_document_is_generated(self):
        """No document may publish adequacy numbers outside the projection.

        The list is small and hand-kept; this asserts the two that exist are both
        generated, so removing one from `pairs()` is a failing test rather than a
        quietly unguarded file.
        """
        targets = {t.relative_to(REPO).as_posix() for _, t in pub.pairs(REPO)}
        self.assertEqual(targets, {IDX, ERR})
        for source, target in pub.pairs(REPO):
            self.assertTrue(source.is_file(), source)
            self.assertTrue(target.read_text().startswith("<!-- GENERATED FROM"), target)


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
# suite may do to a working tree.
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
    published documents are held to results.json by projection; this holds
    results.json to the tool. Without it the whole layer would guarantee only that
    two documents agree with a third that nothing re-derives.
    """

    def stored(self) -> dict:
        return {c["corpus"]: c for c in json.loads(
            (REPO / "conformance/adequacy/results.json").read_text())["corpora"]}

    @staticmethod
    def trim(row: dict) -> dict:
        """Everything a re-run must reproduce.

        `tool_version` is provenance rather than measurement, and older tool
        checkouts report none. `measured_at.commit` is WHEN, and a fresh run
        records today's HEAD, so comparing it would make every row differ the
        moment anyone commits. `measured_at.depends_on` stays compared on purpose:
        a dependency list that quietly shrinks is the same under-declaration this
        whole layer is about, one level down, and it is exactly what a trim of the
        entire `measured_at` block would hide.
        """
        out = {k: v for k, v in row.items() if k != "tool_version"}
        if isinstance(out.get("measured_at"), dict):
            out["measured_at"] = {k: v for k, v in out["measured_at"].items() if k != "commit"}
        return out

    def drifted(self, stored: dict) -> list[str]:
        out = []
        for corpus, manifest in sorted(CHEAP.items()):
            sibling = SIBLINGS.get(corpus)
            if sibling is not None and not sibling.is_dir():
                self.skipTest("%s not found as a sibling checkout" % sibling.name)
            fresh = measure_all.row(manifest, ca.run(manifest),
                                    {"tool_commit": stored[corpus]["tool_commit"]})
            if self.trim(fresh) != self.trim(stored[corpus]):
                out.append(corpus)
        return out

    def test_the_cheap_corpora_still_measure_what_results_json_records(self):
        self.assertEqual(self.drifted(self.stored()), [],
                         "results.json has gone stale; re-run measure_all.py")

    def test_a_doctored_result_is_caught(self):
        """CONTROL: hand-edit rge-bench's killed count and confirm the drift check bites."""
        stored = self.stored()
        stored["rge-bench"] = dict(stored["rge-bench"], killed=stored["rge-bench"]["killed"] + 1)
        self.assertEqual(self.drifted(stored), ["rge-bench"])


class TheTranscribedRowSaysSoOutLoud(unittest.TestCase):
    def test_a_row_nobody_re_derives_must_say_so(self):
        """Every row is derived today; the rule holds for the day one is not."""
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
