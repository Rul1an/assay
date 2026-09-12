#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import tempfile
import unittest
import io
import subprocess
from unittest.mock import patch


MODULE_PATH = pathlib.Path(__file__).with_name("pr_landing_readiness.py")
SPEC = importlib.util.spec_from_file_location("pr_landing_readiness", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class EveryTestInThisFileActuallyRuns(unittest.TestCase):
    """`unittest.main()` collects what is defined when it runs, so it has to be last.

    It sat mid-file while the #2958 classes were appended below it, and the invocation the
    README documents (`python3 scripts/review/test_pr_landing_readiness.py`) then ran 43 of
    60 tests and still printed OK. This class is first on purpose: a guard written below the
    entrypoint disappears along with what it was meant to report.
    """

    def test_nothing_is_defined_after_the_entrypoint(self):
        lines = pathlib.Path(__file__).read_text().splitlines()
        entry = [n for n, line in enumerate(lines) if line.startswith("if __name__")]
        self.assertEqual(len(entry), 1, "one entrypoint")
        after = [line for line in lines[entry[0]:] if line.startswith(("class ", "def "))]
        self.assertEqual(after, [], "these are invisible to a direct run")



class VerdictTests(unittest.TestCase):
    def test_exact_ready(self):
        self.assertEqual(MODULE.verdict("## Verdict\n\n**READY**"), "READY")

    def test_exact_blocked_wins(self):
        self.assertEqual(MODULE.verdict("READY\n\nBLOCKED"), "BLOCKED")

    def test_not_ready_is_not_ready(self):
        self.assertIsNone(MODULE.verdict("Verdict: NOT READY"))

    def test_quota_comment_is_not_a_review(self):
        body = "Review rate limited; unable to review.\nREADY"
        self.assertIsNone(MODULE.verdict(body))

    def test_quota_words_do_not_erase_a_blocked_verdict(self):
        self.assertEqual(MODULE.verdict("BLOCKED\nA quota limit affected a secondary check."),
                         "BLOCKED")

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
            "carry": None,
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

    def machine_body(self, verdict, instance):
        record = {
            "schema": "assay.review-record.v0",
            "head_sha": "b" * 40,
            "builder": {"agent": "ruley", "instance": "writer"},
            "review_completed": True,
            "verdict": verdict,
            "reviewer": {
                "agent": "claude",
                "instance": instance,
                "github_login": "owner",
            },
            "independence": {
                "did_not_build": True,
                "did_not_author_governing_spec": True,
            },
            "findings": [],
            "no_findings": True,
        }
        return "<!-- assay-review-record -->\n```json\n" + json.dumps(record) + "\n```"

    def test_invalid_identity_does_not_erase_a_blocked_record(self):
        body = self.machine_body("BLOCKED", "session 42")
        rows = MODULE.review_candidates({
            "reviews": [],
            "comments": [{"author": {"login": "owner"}, "body": body}],
        }, "b" * 40)

        self.assertEqual(rows[0]["verdict"], "BLOCKED")
        self.assertTrue(rows[0]["current_head"])
        self.assertIsNone(rows[0]["reviewer_identity"])

    def test_invalid_machine_ready_cannot_fall_back_to_prose(self):
        body = self.machine_body("READY", "session 42") + "\nREADY\n" + "b" * 40
        rows = MODULE.review_candidates({
            "reviews": [],
            "comments": [{"author": {"login": "owner"}, "body": body}],
        }, "b" * 40)

        self.assertEqual(rows[0]["verdict"], "BLOCKED")
        self.assertTrue(rows[0]["current_head"])

    def test_current_machine_carrier_with_invalid_contract_blocks(self):
        current = "b" * 40
        cases = (
            self.machine_body("READY", "reviewer").replace('"builder": {"agent": "ruley", "instance": "writer"}, ', ""),
            self.machine_body("blocked", "reviewer"),
            "<!-- assay-review-record -->\n```json\n{\"head_sha\": \"" + current + "\",\n```",
        )
        for body in cases:
            with self.subTest(body=body):
                rows = MODULE.review_candidates({
                    "headRefName": "codex/review-fix",
                    "reviews": [],
                    "comments": [{"author": {"login": "owner"}, "body": body}],
                }, current)
                self.assertEqual(rows[0]["verdict"], "BLOCKED")
                self.assertTrue(rows[0]["current_head"])

    def test_machine_blocked_is_not_erased_by_unavailable_words(self):
        body = self.machine_body("BLOCKED", "reviewer").replace(
            "\n```", "\n```\nquota limit prevented a second check")
        rows = MODULE.review_candidates({
            "headRefName": "codex/review-fix",
            "reviews": [],
            "comments": [{"author": {"login": "owner"}, "body": body}],
        }, "b" * 40)
        self.assertEqual(rows[0]["verdict"], "BLOCKED")

    def test_review_state_is_part_of_the_verdict(self):
        head = "b" * 40
        pr = {
            "reviews": [
                {"state": "DISMISSED", "author": {"login": "old"},
                 "commit": {"oid": head}, "body": f"READY\n{head}"},
                {"state": "CHANGES_REQUESTED", "author": {"login": "blocker"},
                 "commit": {"oid": head}, "body": "Please repair this."},
            ],
            "comments": [],
        }
        rows = MODULE.review_candidates(pr, head)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["verdict"], "BLOCKED")
        self.assertEqual(rows[0]["record_author"], "blocker")

    def test_markdown_atom_escapes_control_and_delimiter_characters(self):
        rendered = MODULE.markdown_atom("CI`\n- blockers:\n  - none")
        self.assertNotIn("\n", rendered)
        self.assertNotIn("`", rendered)
        self.assertIn("\\n", rendered)


class SupersedeTests(unittest.TestCase):
    """The helper applies the checker's same-head supersede rule, not a second reading of it."""

    HEAD = "b" * 40

    def record(self, **over):
        row = {
            "schema": "assay.review-record.v0",
            "head_sha": self.HEAD,
            "builder": {"agent": "ruley", "instance": "writer"},
            "reviewer": {"agent": "claude", "instance": "reviewer", "github_login": "owner"},
            "review_completed": True,
            "verdict": "READY",
            "findings": [],
            "no_findings": True,
            "independence": {"did_not_build": True, "did_not_author_governing_spec": True},
        }
        row.update(over)
        return row

    def comment(self, record, cid, minute, login="owner"):
        return {
            "author": {"login": login},
            "url": f"https://github.com/example/repo/pull/7#issuecomment-{cid}",
            "createdAt": f"2026-09-10T17:{minute:02d}:00Z",
            "body": "<!-- assay-review-record -->\n```json\n" + json.dumps(record) + "\n```",
        }

    def rows(self, *comments):
        # review_candidates() turns every refusal into a BLOCKED row, so anything it raises is a
        # crash; fail the test on it by name instead of letting unittest report an ERROR.
        try:
            return MODULE.review_candidates({"reviews": [], "comments": list(comments)}, self.HEAD)
        except Exception as exc:  # noqa: BLE001 - reported as this test's failure
            self.fail(f"review_candidates raised {type(exc).__name__}: {exc}")

    def test_superseded_malformed_record_is_not_a_current_blocker(self):
        bad = self.record(findings=[{"claim": 1, "status": "holds"}], no_findings=False)
        rows = self.rows(self.comment(bad, 101, 11), self.comment(self.record(supersedes=101), 102, 19))
        self.assertEqual([(row["verdict"], row["source"]) for row in rows], [("READY", "machine-comment")])

    def test_control_without_supersedes_the_malformed_record_still_blocks(self):
        bad = self.record(findings=[{"claim": 1, "status": "holds"}], no_findings=False)
        rows = self.rows(self.comment(bad, 101, 11), self.comment(self.record(), 102, 19))
        self.assertIn(("BLOCKED", "invalid-machine-comment"), [(row["verdict"], row["source"]) for row in rows])

    def test_refused_supersede_blocks_and_keeps_its_target(self):
        other = {"agent": "codex", "instance": "reviewer-2", "github_login": "owner"}
        rows = self.rows(
            self.comment(self.record(verdict="BLOCKED"), 101, 11),
            self.comment(self.record(reviewer=other, supersedes=101), 102, 19),
        )
        self.assertEqual([(row["verdict"], row["source"]) for row in rows],
                         [("BLOCKED", "machine-comment"), ("BLOCKED", "invalid-machine-comment")])

    def test_supersede_pointing_at_a_newer_comment_blocks(self):
        rows = self.rows(self.comment(self.record(supersedes=102), 101, 11),
                         self.comment(self.record(verdict="BLOCKED"), 102, 19))
        self.assertEqual([row["verdict"] for row in rows], ["BLOCKED", "BLOCKED"])

    def test_superseding_comment_without_an_addressable_id_blocks(self):
        bad = self.record(findings=[{"claim": 1}], no_findings=False)
        unaddressed = self.comment(self.record(supersedes=101), 102, 19)
        del unaddressed["url"]
        rows = self.rows(self.comment(bad, 101, 11), unaddressed)
        self.assertEqual([row["verdict"] for row in rows], ["BLOCKED", "BLOCKED"])


class GitHubCommandBoundaryTests(unittest.TestCase):
    def test_repository_and_branch_inputs_are_validated(self):
        self.assertEqual(MODULE.parse_repo("example/repo"), ("example", "repo"))
        self.assertEqual(MODULE.encoded_branch("codex/review-fix"), "codex%2Freview-fix")
        for repo in ("owner/repo/extra", "-owner/repo", "owner/repo\n--help"):
            with self.subTest(repo=repo), self.assertRaises(SystemExit):
                MODULE.parse_repo(repo)
        for branch in ("../main", "refs//heads", "topic@{1}", "topic\n--help"):
            with self.subTest(branch=branch), self.assertRaises(SystemExit):
                MODULE.encoded_branch(branch)

    def test_run_json_refuses_unknown_command_shape_before_execution(self):
        MODULE.validate_gh_command([
            "gh", "api", "repos/example/repo/rules/branches/codex%2Freview-fix",
        ])
        commands = (
            ["python3", "-c", "print('not gh')"],
            ["not-gh", "api", "graphql", "-f", f"query={MODULE.REQUIRED_CONTEXT_QUERY}",
             "-f", "owner=example", "-f", "name=repo", "-f", "ref=refs/heads/main"],
            ["gh", "api", "repos/../x"],
            ["gh", "api", "repos/onlyone"],
            ["gh", "api", "repos/example/repo/branches/main", "--method", "DELETE"],
            ["gh", "api", "graphql", "-f", "query=mutation{deleteRepository}"],
            ["gh", "pr", "view", "30", "--repo", "example/repo", "--json", "url", "--web"],
            ["gh", "pr", "checks", "30", "--repo", "example/repo", "--required", "--json", MODULE.PR_CHECK_FIELDS, "--watch"],
        )
        for command in commands:
            with self.subTest(command=command), patch.object(MODULE.subprocess, "run") as run:
                with self.assertRaises(SystemExit):
                    MODULE.run_json(command)
                run.assert_not_called()

    def test_run_json_timeout_and_output_ceiling_fail_closed(self):
        command = [
            "gh", "api", "graphql", "-f", f"query={MODULE.REQUIRED_CONTEXT_QUERY}",
            "-f", "owner=example", "-f", "name=repo", "-f", "ref=refs/heads/main",
        ]
        def times_out(*args, **kwargs):
            self.assertEqual(kwargs.get("timeout"), MODULE.COMMAND_TIMEOUT_SECONDS)
            raise subprocess.TimeoutExpired("gh", 30)

        with patch.object(MODULE.subprocess, "run", side_effect=times_out):
            with self.assertRaisesRegex(SystemExit, "timed out"):
                MODULE.run_json(command)

        def oversized(*args, **kwargs):
            kwargs["stdout"].write(b"x" * (MODULE.MAX_JSON_BYTES + 1))
            return subprocess.CompletedProcess(args[0], 0)

        with patch.object(MODULE.subprocess, "run", side_effect=oversized):
            with self.assertRaisesRegex(SystemExit, "byte limit"):
                MODULE.run_json(command)


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
                   review=True, blocked=False, state="OPEN", draft=False,
                   mergeable="MERGEABLE", body_current=True, number=30,
                   output_format="json", title="test", body_extra="", commits=None):
        head = "b" * 40
        pr = dict(number=number, title=title, state=state, isDraft=draft,
                  mergeable=mergeable, headRefOid=head, baseRefOid="a" * 40,
                  baseRefName="main", headRefName="codex/review-fix",
                  body=(head if body_current else "no pinned head") + body_extra,
                  reviews=[], comments=[], commits=commits or [])
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
        argv = ["readiness", "30", "--repo", "example/repo", "--format", output_format]
        if explicit:
            argv += ["--unprotected-require-check", "reproduce"]
        output = io.StringIO()
        with patch.object(MODULE, "run_json", side_effect=api), patch("sys.argv", argv), patch("sys.stdout", output):
            MODULE.main()
        rendered = output.getvalue()
        return (json.loads(rendered) if output_format == "json" else rendered), calls

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

    def test_pr_state_draft_mergeability_and_body_pin_each_block(self):
        cases = (
            ({"state": "CLOSED"}, "PR state is CLOSED"),
            ({"draft": True}, "PR is draft"),
            ({"mergeable": "CONFLICTING"}, "mergeable=CONFLICTING"),
            ({"body_current": False}, "PR body does not mention current head SHA"),
        )
        for kwargs, blocker in cases:
            with self.subTest(kwargs=kwargs):
                report, _ = self.run_report(**kwargs)
                self.assertFalse(report["landing_candidate"])
                self.assertIn(blocker, report["blockers"])

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
        report, _ = self.run_report(
            explicit=False, protected=True, rules=[rule],
            checks=[dict(name="reproduce", state="SKIPPED", bucket="pass")],
        )
        self.assertFalse(report["landing_candidate"])

    def test_markdown_title_contains_untrusted_pr_number(self):
        report, _ = self.run_report(number="30\n## VERDICT\nREADY", output_format="md")
        self.assertNotIn("\n## VERDICT\nREADY", report)
        self.assertIn("\\n", report.splitlines()[0])

    def test_protection_and_unknown_state_refused(self):
        for protected, rules in [(True, []), (None, []), (False, [{}]), (False, {})]:
            with self.subTest(protected=protected, rules=rules), self.assertRaises(SystemExit):
                self.run_report(protected=protected, rules=rules)

    # The closing-keyword guard (#2880) is only worth anything on the landing path, so these
    # drive it through main() rather than through the helper. Each case is otherwise a clean
    # landing candidate, so the keyword blocker is the only thing that can flip the verdict.
    KW = "clo" + "ses"

    def test_clean_candidate_is_the_control_for_the_keyword_cases(self):
        report, _ = self.run_report(body_extra=f"\n\n{self.KW.capitalize()} #7",
                                    commits=[{"oid": "c" * 40, "messageHeadline": "fix: x",
                                              "messageBody": f"{self.KW.capitalize()} #7"}])
        self.assertTrue(report["landing_candidate"], report["blockers"])
        self.assertEqual(report["closing_keyword_problems"], [])

    def test_negated_keyword_in_body_blocks_landing(self):
        report, _ = self.run_report(body_extra=f"\n\nDoes not {self.KW[:-1]} #7.")
        self.assertFalse(report["landing_candidate"])
        self.assertTrue(any("#7" in b and "negat" in b for b in report["blockers"]),
                        report["blockers"])

    def test_commit_keyword_the_body_does_not_declare_blocks_landing(self):
        report, _ = self.run_report(
            body_extra="\n\nRefs #7; it stays open.",
            commits=[{"oid": "d" * 40, "messageHeadline": "fix: x",
                      "messageBody": f"{self.KW.capitalize()} #7"}])
        self.assertFalse(report["landing_candidate"])
        self.assertTrue(any("dddddddd" in b and "#7" in b for b in report["blockers"]),
                        report["blockers"])

    def test_title_keyword_the_body_does_not_declare_blocks_landing(self):
        report, _ = self.run_report(title="Fix #7 in the runner")
        self.assertFalse(report["landing_candidate"])
        self.assertTrue(any("title" in b and "#7" in b for b in report["blockers"]),
                        report["blockers"])

    def test_commits_are_requested_from_github(self):
        _, calls = self.run_report()
        view = next(c for c in calls if c[1:3] == ["pr", "view"])
        self.assertIn("commits", view[-1].split(","))

    def test_missing_or_non_success_checks_refused(self):
        for state, bucket in [("FAILURE", "fail"), ("PENDING", "pending"), ("SKIPPED", "pass"), ("NEUTRAL", "pass"), ("SUCCESS", "unknown")]:
            with self.subTest(state=state, bucket=bucket):
                report, _ = self.run_report(checks=[dict(name="reproduce", state=state, bucket=bucket)])
                self.assertFalse(report["landing_candidate"])
        report, _ = self.run_report(checks=[])
        self.assertFalse(report["landing_candidate"])



def _git(root, *args, check=True):
    import os
    # `-C <root>` names the repository, and GIT_DIR and friends override it. The pre-commit hook
    # exports them, so an inherited one would run this fixture against the real checkout.
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
                        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE")}
    env.update({"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.invalid",
                "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.invalid"})
    done = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, env=env)
    if check and done.returncode != 0:
        raise AssertionError(f"git {args}: {done.stderr}")
    return done.stdout.strip()


def _carry_fixture(root):
    """A reviewed head plus the advances the carry rules have to tell apart."""
    rows = "".join(f"line {n}\n" for n in range(1, 21))
    (root / "shared.txt").write_text(rows)
    _git(root, "-c", "init.defaultBranch=main", "init", "-q")
    _git(root, "add", "-A")
    _git(root, "commit", "-qm", "base")
    base = _git(root, "rev-parse", "HEAD")
    _git(root, "checkout", "-q", "-b", "work")
    (root / "feature.txt").write_text("feature\n")
    (root / "shared.txt").write_text(rows.replace("line 1\n", "line 1 from the branch\n"))
    _git(root, "add", "-A")
    _git(root, "commit", "-qm", "reviewed work")
    reviewed = _git(root, "rev-parse", "HEAD")
    _git(root, "checkout", "-q", "-b", "up-clean", base)
    (root / "other.txt").write_text("other\n")
    _git(root, "add", "-A")
    _git(root, "commit", "-qm", "upstream adds an untouched file")
    _git(root, "checkout", "-q", "-b", "up-overlap", base)
    (root / "shared.txt").write_text(rows.replace("line 20\n", "line 20 from main\n"))
    _git(root, "add", "-A")
    _git(root, "commit", "-qm", "upstream edits a reviewed file")
    heads = {"reviewed": reviewed}
    for name, branch in (("clean", "up-clean"), ("overlap", "up-overlap")):
        _git(root, "checkout", "-q", "-b", f"live-{name}", "work")
        _git(root, "merge", "-q", "--no-ff", "-m", "Merge main into work", branch)
        heads[name] = _git(root, "rev-parse", "HEAD")
    _git(root, "checkout", "-q", "-b", "live-push", "work")
    (root / "feature.txt").write_text("feature again\n")
    _git(root, "add", "-A")
    _git(root, "commit", "-qm", "one more push")
    heads["push"] = _git(root, "rev-parse", "HEAD")
    return heads


def _record_pr(bound):
    record = {
        "schema": "assay.review-record.v0", "head_sha": bound,
        "builder": {"agent": "ruley", "instance": "writer"},
        "reviewer": {"agent": "claude", "instance": "reviewer", "github_login": "Rul1an"},
        "review_completed": True, "verdict": "READY", "findings": [], "no_findings": True,
        "independence": {"did_not_build": True, "did_not_author_governing_spec": True},
    }
    body = "<!-- assay-review-record -->\n```json\n" + json.dumps(record) + "\n```"
    return {"reviews": [], "headRefName": "ruley/2958-landing-derived-carry",
            "comments": [{"author": {"login": "Rul1an"}, "body": body}]}


class DerivedCarryBindsTheLandingGate(unittest.TestCase):
    """#2958: the landing gate answers through the CI gate's own derivation, not its own rule."""

    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        cls.root = pathlib.Path(cls._tmp.name)
        cls.heads = _carry_fixture(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls._tmp.cleanup()

    def _rows(self, head):
        rows = MODULE.review_candidates(_record_pr(self.heads["reviewed"]), head, git_root=self.root)
        self.assertEqual(len(rows), 1, f"the record produced no candidate row: {rows}")
        return rows

    def test_an_upstream_advance_carries_and_says_so(self):
        row = self._rows(self.heads["clean"])[0]
        self.assertTrue(row["current_head"])
        self.assertEqual(row["source"], "machine-comment-carry")
        self.assertIn("re-derived-by-checker", row["carry"])

    def test_an_advance_touching_a_reviewed_file_does_not_carry(self):
        row = self._rows(self.heads["overlap"])[0]
        self.assertFalse(row["current_head"])
        self.assertEqual(row["source"], "machine-comment")
        self.assertIsNotNone(row["carry"], "no derivation was attempted")
        self.assertIn("carry_touched_reviewed_file", row["carry"])

    def test_a_further_push_is_not_an_upstream_merge(self):
        row = self._rows(self.heads["push"])[0]
        self.assertFalse(row["current_head"])
        self.assertIsNotNone(row["carry"], "no derivation was attempted")
        self.assertIn("carry_not_upstream_merge", row["carry"])

    def test_a_head_sha_matching_nothing_is_refused(self):
        rows = MODULE.review_candidates(_record_pr("f" * 40), self.heads["clean"], git_root=self.root)
        self.assertFalse(rows[0]["current_head"])
        self.assertIsNotNone(rows[0]["carry"], "no derivation was attempted")
        self.assertIn("carry_objects_unavailable", rows[0]["carry"])

    def test_both_gates_answer_the_same_on_the_same_head(self):
        import assay_review_record_check as checker
        for name, expected in (("clean", True), ("overlap", False), ("push", False)):
            with self.subTest(head=name):
                head = self.heads[name]
                pr = _record_pr(self.heads["reviewed"])
                landing = any(r["current_head"] and r["verdict"] == "READY"
                              for r in MODULE.review_candidates(pr, head, git_root=self.root))
                try:
                    checker.evaluate(head, pr["headRefName"],
                                     [{"body": c["body"], "user": {"login": "Rul1an", "type": "User"},
                                       "created_at": "2026-09-12T00:00:00Z",
                                       "updated_at": "2026-09-12T00:00:00Z", "id": 1}
                                      for c in pr["comments"]], git_root=str(self.root))
                    ci = True
                except MODULE.GateError:
                    ci = False
                self.assertEqual(landing, ci, f"gates disagree on {name}")
                self.assertEqual(landing, expected)


class CarryWithoutACheckoutTests(unittest.TestCase):
    """The landing recipe allows a `git archive` extract, which has no `.git` to derive in."""

    def test_no_checkout_and_no_repo_refuses_rather_than_guessing(self):
        with tempfile.TemporaryDirectory() as bare:
            carried, note = MODULE.carried_to_head("a" * 40, "b" * 40, pathlib.Path(bare), {})
        self.assertFalse(carried)
        self.assertIn("carry_objects_unavailable", note)

    def test_no_checkout_with_a_repo_derives_in_a_temporary_clone(self):
        with tempfile.TemporaryDirectory() as bare:
            cache = {}
            root, note = MODULE._objects_root(pathlib.Path(bare), "Rul1an/assay", cache)
            self.assertNotEqual(root, bare, "an extract is not a repository to derive in")
            self.assertIn("temporary clone", note)
            self.assertEqual(
                MODULE.Git(root).run("remote", "get-url", "origin")[1],
                "https://github.com/Rul1an/assay.git")

    def test_a_real_checkout_is_used_as_is(self):
        with tempfile.TemporaryDirectory() as real:
            root = pathlib.Path(real)
            _git(root, "-c", "init.defaultBranch=main", "init", "-q")
            chosen, note = MODULE._objects_root(root, "Rul1an/assay", {})
        self.assertEqual(chosen, str(root))
        self.assertEqual(note, "")


class GateSetJudgementTests(unittest.TestCase):
    """#2958 F2: the derivation carries one record; the gate judges the whole comment set."""

    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        cls.root = pathlib.Path(cls._tmp.name)
        cls.heads = _carry_fixture(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls._tmp.cleanup()

    def _rows(self, gate, head=None):
        return MODULE.review_candidates(
            _record_pr(self.heads["reviewed"]), head or self.heads["clean"],
            git_root=self.root, gate=gate)

    def test_a_refusing_gate_stops_a_derivable_carry(self):
        for reason in ("bot_carrier: Bot", "edited_current: updated_at != created_at",
                       "ambiguous_current: 2", "supersede_refused: 7 is not older than 8"):
            with self.subTest(reason=reason):
                row = self._rows(lambda reason=reason: (False, reason))[0]
                self.assertFalse(row["current_head"], "the set refuses, so nothing carries")
                self.assertIn(reason, row["carry"])

    def test_a_passing_gate_leaves_the_derivation_in_charge(self):
        row = self._rows(lambda: (True, "review-record-check would pass"))[0]
        self.assertTrue(row["current_head"])
        self.assertEqual(row["source"], "machine-comment-carry")

    def test_no_gate_answer_keeps_the_derivation_alone(self):
        row = self._rows(None)[0]
        self.assertTrue(row["current_head"])

    def test_the_gate_is_asked_only_when_there_is_a_carry_to_judge(self):
        """The thunk costs an API call and can fail; nothing else may depend on it.

        A record on the live head needs no carry, and a derivation that already refused cannot
        be rescued by the set passing - so in both cases the answer is not read, and an outage
        reaching that endpoint must not decide a landing check it was never going to decide.
        """
        def gate():
            raise AssertionError("the gate was consulted without a carry to judge")

        on_head = self._rows(gate, head=self.heads["reviewed"])[0]
        self.assertTrue(on_head["current_head"])
        self.assertEqual(on_head["source"], "machine-comment")

        refused = self._rows(gate, head=self.heads["overlap"])[0]
        self.assertFalse(refused["current_head"])
        self.assertIn("carry_touched_reviewed_file", refused["carry"])


class MalformedHeadShaNeverReachesGit(unittest.TestCase):
    """#2958 F1: a record's head_sha is attacker-shaped text until it is 40 lowercase hex."""

    def test_option_shaped_head_sha_is_refused_before_any_subprocess(self):
        marker = pathlib.Path(tempfile.gettempdir()) / "landing-carry-injection-probe"
        if marker.exists():
            marker.unlink()
        for bad in (f"--upload-pack=touch {marker}", "-x", "HEAD", "A" * 40, "", None):
            with self.subTest(bad=bad):
                carried, note = MODULE.carried_to_head(bad, "b" * 40, pathlib.Path("."), {})
                self.assertFalse(carried)
                if bad:
                    self.assertIn("carry_malformed_sha", note)
        self.assertFalse(marker.exists(), "a git option in a head_sha executed")


class IdentityVerifierAcceptsACarry(unittest.TestCase):
    """#2958 F3: `safe_merge.sh` runs this after readiness, and it read the head literally.

    Readiness is what judges the comment set (it asks `review-record-check`'s own `evaluate`),
    so this check is not a second authorization of the carry: it re-derives the same conditions
    on the record it was handed, and must not refuse what the gate before it accepted.
    """

    @classmethod
    def setUpClass(cls):
        cls._tmp = tempfile.TemporaryDirectory()
        cls.root = pathlib.Path(cls._tmp.name)
        cls.heads = _carry_fixture(cls.root)
        spec = importlib.util.spec_from_file_location(
            "verify_review_identity", MODULE_PATH.with_name("verify_review_identity.py"))
        cls.identity = importlib.util.module_from_spec(spec)
        import sys
        sys.path.insert(0, str(MODULE_PATH.parent))
        spec.loader.exec_module(cls.identity)

    @classmethod
    def tearDownClass(cls):
        cls._tmp.cleanup()

    def _verify(self, bound, head):
        body = _record_pr(bound)["comments"][0]["body"]
        comment = {
            "html_url": "https://github.com/Rul1an/assay/pull/30#issuecomment-123",
            "issue_url": "https://api.github.com/repos/Rul1an/assay/issues/30",
            "user": {"login": "Rul1an"}, "body": body,
        }
        with patch.object(self.identity, "run_json", return_value=comment), \
                patch.object(self.identity, "REPO_ROOT", self.root), \
                patch("sys.stdout", new_callable=io.StringIO) as out:
            self.identity.verify(
                "Rul1an/assay", "30", head, "ruley/2958-landing-derived-carry", "Rul1an",
                "claude/reviewer", "https://github.com/Rul1an/assay/pull/30#issuecomment-123",
                "someone-else")
        return out.getvalue()

    def test_a_derived_carry_is_accepted_and_printed(self):
        printed = self._verify(self.heads["reviewed"], self.heads["clean"])
        self.assertIn("Carried to the live head", printed)
        self.assertIn("re-derived-by-checker", printed)

    def test_an_exact_head_record_still_passes_without_a_carry_line(self):
        printed = self._verify(self.heads["clean"], self.heads["clean"])
        self.assertNotIn("Carried to the live head", printed)

    def test_an_advance_touching_a_reviewed_file_is_refused(self):
        with self.assertRaises(ValueError):
            self._verify(self.heads["reviewed"], self.heads["overlap"])

    def test_a_further_push_is_refused(self):
        with self.assertRaises(ValueError):
            self._verify(self.heads["reviewed"], self.heads["push"])

if __name__ == "__main__":
    unittest.main()
