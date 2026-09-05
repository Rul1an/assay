#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import unittest
import io
from unittest.mock import patch


MODULE_PATH = pathlib.Path(__file__).with_name("pr_landing_readiness.py")
SPEC = importlib.util.spec_from_file_location("pr_landing_readiness", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class VerdictTests(unittest.TestCase):
    def test_exact_ready(self):
        self.assertEqual(MODULE.verdict("## Verdict\n\n**READY**"), "READY")

    def test_exact_blocked_wins(self):
        self.assertEqual(MODULE.verdict("READY\n\nBLOCKED"), "BLOCKED")

    def test_not_ready_is_not_ready(self):
        self.assertIsNone(MODULE.verdict("Verdict: NOT READY"))

    def test_quota_comment_is_not_a_review(self):
        body = "Review rate limited; unable to review. Mark READY when quota resets."
        self.assertIsNone(MODULE.verdict(body))

    def test_ready_in_prose_is_not_a_verdict(self):
        self.assertIsNone(MODULE.verdict("The branch may be ready after CI."))


class CandidateBindingTests(unittest.TestCase):
    def test_stale_and_current_sha_are_distinguished(self):
        current = "b" * 40
        stale = "a" * 40
        pr = {
            "reviews": [],
            "comments": [
                {"author": {"login": "old"}, "body": f"READY\n{stale}"},
                {"author": {"login": "current"}, "body": f"READY\n{current}"},
            ],
        }
        rows = MODULE.review_candidates(pr, current)
        self.assertFalse(rows[0]["current_head"])
        self.assertTrue(rows[1]["current_head"])

    def test_canonical_machine_record_is_a_ready_candidate(self):
        current = "b" * 40
        record = {
            "schema": "assay.review-record.v0",
            "head_sha": current,
            "builder": {"agent": "ruley", "instance": "writer"},
            "reviewer": {
                "agent": "claude",
                "instance": "reviewer",
                "github_login": "Rul1an",
            },
            "review_completed": True,
            "verdict": "READY",
            "findings": [],
            "no_findings": True,
            "independence": {
                "did_not_build": True,
                "did_not_author_governing_spec": True,
            },
        }
        pr = {
            "reviews": [],
            "comments": [{
                "author": {"login": "Rul1an"},
                "body": "<!-- assay-review-record -->\n```json\n"
                + json.dumps(record)
                + "\n```",
            }],
        }

        self.assertEqual(MODULE.review_candidates(pr, current), [{
            "record_author": "Rul1an",
            "reviewer_identity": "claude/reviewer",
            "verdict": "READY",
            "bound_sha": current,
            "current_head": True,
            "source": "machine-comment",
        }])

    def test_reviewer_identity_cannot_inject_human_read_output(self):
        current = "b" * 40
        record = {
            "schema": "assay.review-record.v0",
            "head_sha": current,
            "review_completed": True,
            "verdict": "READY",
            "reviewer": {
                "agent": "claude",
                "instance": "x`; **INDEPENDENCE VERIFIED**\n- blockers:\n  - none",
                "github_login": "owner",
            },
            "independence": {
                "did_not_build": True,
                "did_not_author_governing_spec": True,
            },
        }
        body = "<!-- assay-review-record -->\n```json\n" + json.dumps(record) + "\n```"
        self.assertIsNone(MODULE.machine_review_candidate(body, "owner"))


class RequiredContextTests(unittest.TestCase):
    def test_missing_required_context_is_reported(self):
        reported = [
            {"name": "host-capability-check", "bucket": "pass"},
            {"name": "lane-check/proof", "bucket": "pass"},
        ]
        expected = ["CI", "host-capability-check", "lane-check/proof"]

        self.assertEqual(MODULE.missing_required_contexts(reported, expected), ["CI"])


class RulesetPolicyTests(unittest.TestCase):
    def policy(self, classic=None, pages=None):
        response = {"data": {"repository": {"ref": {"branchProtectionRule": classic}}}}
        with patch.object(MODULE, "run_json", side_effect=[response, pages]) as api:
            result = MODULE.required_contexts("example/repo", "main")
        self.assertIn("--paginate", api.call_args_list[1].args[0])
        self.assertIn("--slurp", api.call_args_list[1].args[0])
        return result

    def rule(self, name):
        return {"type": "required_status_checks", "parameters": {
            "required_status_checks": [{"context": name, "integration_id": 15368}]}}

    def test_ruleset_only_and_multiple_pages(self):
        self.assertEqual(self.policy(pages=[[self.rule("linux")], [self.rule("windows")]]),
                         ["linux", "windows"])

    def test_union_never_drops_classic_requirements(self):
        self.assertEqual(self.policy({"requiredStatusCheckContexts": ["CI", "linux"]},
                                     [[self.rule("linux")]]), ["CI", "linux"])

    def test_classic_only(self):
        self.assertEqual(self.policy({"requiredStatusCheckContexts": ["CI"]}, [[]]), ["CI"])

    def test_malformed_or_empty_policy_refuses(self):
        for pages in [None, {}, [None], [[{}]], [[{"type": "required_status_checks"}]],
                      [[self.rule("")]], [[]]]:
            with self.subTest(pages=pages), self.assertRaises(SystemExit):
                self.policy(pages=pages)

    def test_api_failure_refuses(self):
        with patch.object(MODULE, "run_json", side_effect=SystemExit("403")):
            with self.assertRaisesRegex(SystemExit, "403"):
                MODULE.required_contexts("example/repo", "main")

    def test_missing_branch_is_not_absent_classic_rule(self):
        with patch.object(MODULE, "run_json", return_value={"data": {"repository": {"ref": None}}}):
            with self.assertRaises(SystemExit):
                MODULE.required_contexts("example/repo", "main")


class UnprotectedPolicyTests(unittest.TestCase):
    def run_report(self, *, explicit=True, protected=False, rules=None, checks=None,
                   review=True, blocked=False):
        head = "b" * 40
        pr = dict(number=30, title="test", state="OPEN", isDraft=False,
                  mergeable="MERGEABLE", headRefOid=head, baseRefOid="a" * 40,
                  baseRefName="main", body=head, reviews=[], comments=[])
        if review:
            pr["comments"] = [{"author": {"login": "reviewer"}, "body": f"READY\n{head}"}]
        if blocked:
            pr["comments"].append(
                {"author": {"login": "blocker"}, "body": f"BLOCKED\n{head}"})
        calls = []
        def api(args, **kwargs):
            calls.append(args)
            if args[1:3] == ["pr", "view"]:
                return pr
            if args[1:3] == ["pr", "checks"]:
                return checks if checks is not None else [dict(name="reproduce", state="SUCCESS", bucket="pass")]
            if args[-1].endswith("/protection/required_status_checks"):
                raise SystemExit("HTTP 404")
            if args[1:3] == ["api", "graphql"]:
                return {"data": {"repository": {"ref": {"branchProtectionRule": None}}}}
            if "--slurp" in args:
                return [[] if rules is None else rules]
            if "/rules/branches/" in args[-1]:
                return [] if rules is None else rules
            if args[-1].endswith("/branches/main"):
                return {"protected": protected}
            raise AssertionError(args)
        argv = ["readiness", "30", "--repo", "example/repo", "--format", "json"]
        if explicit:
            argv += ["--unprotected-require-check", "reproduce"]
        output = io.StringIO()
        with patch.object(MODULE, "run_json", side_effect=api), patch("sys.argv", argv), patch("sys.stdout", output):
            MODULE.main()
        return json.loads(output.getvalue()), calls

    def test_explicit_policy_requires_success_and_review(self):
        report, calls = self.run_report()
        self.assertTrue(report["landing_candidate"])
        self.assertTrue(any("/rules/branches/" in c[-1] for c in calls))
        self.assertFalse(any("--required" in c for c in calls))
        report, _ = self.run_report(review=False)
        self.assertFalse(report["landing_candidate"])

    def test_current_head_blocked_review_overrides_ready(self):
        report, _ = self.run_report(blocked=True)
        self.assertFalse(report["landing_candidate"])
        self.assertIn("current-head BLOCKED review exists", report["blockers"])

    def test_default_does_not_turn_absent_policy_into_unprotected(self):
        with self.assertRaisesRegex(SystemExit, "cannot establish required check policy"):
            self.run_report(explicit=False)

    def test_default_ruleset_policy_is_used_by_main(self):
        rule = {"type": "required_status_checks", "parameters": {
            "required_status_checks": [{"context": "reproduce"}]}}
        report, calls = self.run_report(explicit=False, protected=True, rules=[rule])
        self.assertTrue(report["landing_candidate"])
        self.assertEqual(report["check_policy"], "classic-and-active-rulesets")
        report, _ = self.run_report(explicit=False, protected=True, rules=[rule], checks=[])
        self.assertFalse(report["landing_candidate"])

    def test_protection_and_unknown_state_refused(self):
        for protected, rules in [(True, []), (None, []), (False, [{}]), (False, {})]:
            with self.subTest(protected=protected, rules=rules), self.assertRaises(SystemExit):
                self.run_report(protected=protected, rules=rules)

    def test_missing_or_non_success_checks_refused(self):
        for state, bucket in [("FAILURE", "fail"), ("PENDING", "pending"), ("SKIPPED", "pass"), ("NEUTRAL", "pass"), ("SUCCESS", "unknown")]:
            with self.subTest(state=state, bucket=bucket):
                report, _ = self.run_report(checks=[dict(name="reproduce", state=state, bucket=bucket)])
                self.assertFalse(report["landing_candidate"])
        report, _ = self.run_report(checks=[])
        self.assertFalse(report["landing_candidate"])


if __name__ == "__main__":
    unittest.main()
