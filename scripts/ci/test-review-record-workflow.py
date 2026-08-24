#!/usr/bin/env python3
"""Mutation oracle for the review-record workflow contract."""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CHECKER = REPO / "scripts/ci/check-review-record-workflow.py"

spec = importlib.util.spec_from_file_location("review_record_workflow", CHECKER)
if spec is None or spec.loader is None:
    raise RuntimeError(f"cannot load {CHECKER}")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise AssertionError(f"mutation anchor count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1)


class ReviewRecordWorkflowMutations(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = contract.WORKFLOW.read_text(encoding="utf-8")
        cls.ci = contract.CI_WORKFLOW.read_text(encoding="utf-8")
        cls.host = contract.HOST_WORKFLOW.read_text(encoding="utf-8")
        cls.assert_clean(cls.workflow, cls.ci, cls.host)

    @classmethod
    def assert_clean(cls, workflow: str, ci: str, host: str) -> None:
        problems = contract.contract_problems(workflow, ci, host)
        if problems:
            raise AssertionError("; ".join(problems))

    def assert_mutation_bites(
        self, *, workflow: str | None = None, ci: str | None = None,
        host: str | None = None,
    ) -> None:
        problems = contract.contract_problems(
            workflow if workflow is not None else self.workflow,
            ci if ci is not None else self.ci,
            host if host is not None else self.host,
        )
        self.assertTrue(problems, "mutation survived")

    def test_noop_control_stays_green(self) -> None:
        self.assert_clean(self.workflow + "\n# no-op control\n", self.ci, self.host)

    def test_trigger_is_required(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "  pull_request:\n", "  workflow_dispatch:\n"))

    def test_path_filter_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow,
            "    types: [opened, reopened, synchronize, ready_for_review]\n",
            "    types: [opened, reopened, synchronize, ready_for_review]\n"
            "    paths: ['scripts/**']\n"))

    def test_ready_for_review_is_required(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, ", ready_for_review]", "]"))

    def test_job_name_is_pinned(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    name: review-record-check\n", "    name: review-record\n"))

    def test_job_if_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n    if: always()\n"))

    def test_job_needs_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n    needs: setup\n"))

    def test_job_matrix_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    runs-on: ubuntu-latest\n",
            "    runs-on: ubuntu-latest\n    strategy:\n      matrix: {python: ['3.12']}\n"))

    def test_continue_on_error_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    timeout-minutes: 5\n",
            "    timeout-minutes: 5\n    continue-on-error: true\n"))

    def test_checkout_ref_is_pinned_to_base(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "ref: ${{ github.event.pull_request.base.sha }}", "ref: main"))

    def test_default_checkout_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow,
            "        with:\n          ref: ${{ github.event.pull_request.base.sha }}\n"
            "          persist-credentials: false\n", ""))

    def test_persist_credentials_is_required(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "          persist-credentials: false\n", ""))

    def test_base_witness_is_required(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow,
            '          test "$(git rev-parse HEAD)" = '
            '"${{ github.event.pull_request.base.sha }}"\n',
            "          git rev-parse HEAD\n"))

    def test_nothing_executes_before_checkout(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "      - name: Check out trusted base\n",
            "      - name: Untrusted prelude\n        run: echo unsafe\n"
            "      - name: Check out trusted base\n"))

    def test_repository_secret_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "GITHUB_TOKEN: ${{ github.token }}",
            "GITHUB_TOKEN: ${{ secrets.REVIEW_TOKEN }}"))

    def test_pr_argument_is_required(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, ' --pr "$PR_NUMBER"', " --self-test"))

    def test_shell_success_fallback_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, ' --pr "$PR_NUMBER"\n', ' --pr "$PR_NUMBER" || true\n'))

    def test_unconditional_success_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, ' --pr "$PR_NUMBER"\n',
            ' --pr "$PR_NUMBER"\n          exit 0\n'))

    def test_set_plus_e_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow,
            "          set -euo pipefail\n"
            '          python3 scripts/ci/assay_review_record_check.py --pr "$PR_NUMBER"\n',
            "          set +e\n"
            '          python3 scripts/ci/assay_review_record_check.py --pr "$PR_NUMBER"\n'))

    def test_host_required_root_is_pinned(self) -> None:
        self.assert_mutation_bites(host=replace_once(
            self.host, "      - name: Verify review-record workflow contract\n",
            "      - name: Review-record contract removed\n"))

    def test_ci_required_root_is_pinned(self) -> None:
        self.assert_mutation_bites(ci=replace_once(
            self.ci, "      - name: Verify review-record workflow contract\n",
            "      - name: Review-record contract removed\n"))

    def test_checkout_action_sha_is_pinned(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, contract.CHECKOUT_ACTION, "actions/checkout@main"))

    def test_extra_job_is_forbidden(self) -> None:
        self.assert_mutation_bites(workflow=self.workflow + (
            "\n  extra:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - run: echo extra\n"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
