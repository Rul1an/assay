#!/usr/bin/env python3
"""Behavioural tests for the closing-keyword landing guard (#2880).

The two incident fixtures reproduce what actually happened on this repository. GitHub parses
closing keywords only in PR descriptions and commit messages, never in file contents, so this
file is safe as it stands. The keyword is still assembled from parts and the issue numbers are
fake, so the fixture text cannot be lifted verbatim into a PR body or commit message, where it
would close whatever it names.
"""
import importlib.util
import pathlib
import unittest

MODULE_PATH = pathlib.Path(__file__).with_name("closing_keywords.py")
SPEC = importlib.util.spec_from_file_location("closing_keywords", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

REPO = "Rul1an/assay"
KW = "clo" + "se"          # assembled so this file carries no live keyword phrase
KWS = KW + "s"


def problems(title="", body="", commits=()):
    return MODULE.closing_problems(REPO, title, body, list(commits))


class IncidentFixtures(unittest.TestCase):
    def test_negated_keyword_in_body_is_refused(self):
        # PR #2805: the body said it did not close the trace-v7 issue, and GitHub closed it,
        # because GitHub has no notion of negation and reads the keyword and number alone.
        body = f"Storage coherence only. Does not {KW} #9001; the carrier stays open."
        found = problems(body=body)
        self.assertTrue(found, "a negated keyword next to an issue number must block landing")
        self.assertTrue(any("#9001" in p and "negat" in p for p in found), found)

    def test_commit_keyword_the_body_retracted_is_refused(self):
        # PR #2532: the author edited the body to say the issue stays open, but the squash
        # commit still carried the keyword from the original commit body, and the issue closed.
        body = "Addresses the inventory half of #9002. The M18 residual is untouched, so it stays open."
        commit = f"test(cli): compare parsed publisher sets\n\n{KWS.capitalize()} #9002"
        found = problems(body=body, commits=[commit])
        self.assertTrue(found, "a commit that closes what the body does not declare must block")
        self.assertTrue(any("#9002" in p and "commit" in p for p in found), found)


class Controls(unittest.TestCase):
    def test_no_keywords_is_clean(self):
        self.assertEqual(problems(title="ci: tidy", body="Refs #10. Nothing closes.",
                                  commits=["ci: tidy\n\nRefs #10"]), [])

    def test_declared_close_is_clean(self):
        self.assertEqual(problems(body=f"{KWS.capitalize()} #11"), [])

    def test_commit_close_echoed_by_body_is_clean(self):
        self.assertEqual(problems(body=f"{KWS.capitalize()} #12",
                                  commits=[f"fix: thing\n\n{KWS.capitalize()} #12"]), [])

    def test_negation_in_a_previous_sentence_does_not_leak(self):
        body = f"This does not change behaviour. {KWS.capitalize()} #13."
        self.assertEqual(problems(body=body), [])

    def test_refs_is_not_a_closing_keyword(self):
        self.assertEqual(problems(body="Does not close anything; refs #14 only."), [])


class Grammar(unittest.TestCase):
    """Every form GitHub documents must be recognised, or the guard has a blind spot."""

    def test_every_documented_keyword_is_recognised(self):
        for word in ("close", "closes", "closed", "fix", "fixes", "fixed",
                     "resolve", "resolves", "resolved"):
            with self.subTest(word=word):
                found = problems(commits=[f"x\n\n{word} #20"])
                self.assertTrue(found, f"{word!r} closes an issue and was not seen")

    def test_colon_and_uppercase_forms(self):
        self.assertTrue(problems(commits=["x\n\nCLOSES: #21"]))
        self.assertTrue(problems(commits=["x\n\nFixes:#22"]))

    def test_same_repo_long_form_and_url_normalise_to_one_issue(self):
        body = f"{KWS.capitalize()} #23"
        for ref in ("Rul1an/assay#23", "https://github.com/Rul1an/assay/issues/23"):
            with self.subTest(ref=ref):
                self.assertEqual(problems(body=body, commits=[f"x\n\nfixes {ref}"]), [])

    def test_cross_repo_close_must_also_be_declared(self):
        found = problems(commits=["x\n\nfixes octo/other#24"])
        self.assertTrue(any("octo/other#24" in p for p in found), found)

    def test_title_lands_in_the_merge_commit(self):
        found = problems(title="Fix #25 flake in the runner")
        self.assertTrue(any("#25" in p and "title" in p for p in found), found)

    def test_word_boundary_prevents_false_matches(self):
        # "disclose", "prefix", "unresolved" contain keywords as substrings, not as words.
        # This must sit in a commit message: a false match in the body only adds to the
        # declared set and reports nothing, so a body-only test cannot see the boundary go.
        for text in ("disclose #26", "prefix #27", "unresolved #28"):
            with self.subTest(text=text):
                self.assertEqual(problems(commits=[f"docs: x\n\n{text}"]), [])

    def test_offending_line_is_reported(self):
        body = f"Line one.\nIt does not {KW} #29 at all.\nLine three."
        found = problems(body=body)
        self.assertTrue(any(f"does not {KW} #29" in p for p in found), found)


if __name__ == "__main__":
    unittest.main()
