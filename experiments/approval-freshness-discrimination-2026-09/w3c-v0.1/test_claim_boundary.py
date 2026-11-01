"""The report claims no reading of the checker boundary; the proposal is not consumed by it."""
import copy
import unittest

import check


class ClaimBoundary(unittest.TestCase):
    def setUp(self):
        self.report, self.prop = check.load()

    def test_package_passes_as_committed(self):
        self.assertEqual(check.all_problems(self.report, self.prop), [])

    def test_aggregate_rejects_invented_non_verdict_outcomes(self):
        for state in ("void", "inconclusive"):
            with self.subTest(state=state):
                report = copy.deepcopy(self.report)
                report["rollup"]["aggregate"][state] = 999
                self.assertIn("5.1: aggregate does not recount from the records",
                              check.all_problems(report, self.prop))

    def test_report_answers_nothing_and_cites_no_evidence_object(self):
        self.assertEqual(self.report["rollup"]["discriminating_power"]["answer"], "nothing")
        self.assertNotIn("evidence_objects", self.report)
        self.assertEqual(self.report["rollup"]["carried_vs_referenced"], {"carried": 0, "referenced": 0})

    def test_control_rejects_fail_claim_over_passing_log(self):
        prop = copy.deepcopy(self.prop)
        observed = prop["proposed_control_5_5_shape"][0]["observed"]
        observed.update(ref="../observations/baseline.txt",
                        sha256="e8ec316404c92bced962b8cb4a15f96ea455d6f5d039281a71c689741de36b40")
        self.assertTrue(any("observed state differs from the referenced log" in error
                            for error in check.all_problems(self.report, prop)))

    def test_proposal_cannot_be_promoted_into_the_report(self):
        r = copy.deepcopy(self.report)
        r["records"][3].update(other_verdict={"demonstrated": "ev-1"}, discrimination={"demonstrated": "ev-1"})
        self.assertTrue(any(e.startswith("package rule") for e in check.all_problems(r, self.prop)))

    def test_boundary_is_a_question_and_claims_no_reading(self):
        cb = self.report["checker_boundary"]
        self.assertTrue(cb["question"].endswith("?"))
        self.assertEqual(cb["claimed"], "none")


if __name__ == "__main__":
    unittest.main()
