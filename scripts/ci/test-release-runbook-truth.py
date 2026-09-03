#!/usr/bin/env python3
"""Mutation oracle for the release runbook / release.yml contract."""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CHECKER = REPO / "scripts/ci/check-release-runbook-truth.py"

spec = importlib.util.spec_from_file_location("release_runbook_truth", CHECKER)
if spec is None or spec.loader is None:
    raise RuntimeError(f"cannot load {CHECKER}")
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise AssertionError(f"mutation anchor count for {old!r}: {text.count(old)}")
    return text.replace(old, new, 1)


class ReleaseRunbookTruthMutations(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = contract.WORKFLOW.read_text(encoding="utf-8")
        cls.docs = contract.DOCS.read_text(encoding="utf-8")
        cls.assert_clean(cls.workflow, cls.docs)

    @classmethod
    def assert_clean(cls, workflow: str, docs: str) -> None:
        problems = contract.contract_problems(workflow, docs)
        if problems:
            raise AssertionError("; ".join(problems))

    def assert_mutation_bites(
        self, *, workflow: str | None = None, docs: str | None = None,
    ) -> None:
        problems = contract.contract_problems(
            workflow if workflow is not None else self.workflow,
            docs if docs is not None else self.docs,
        )
        self.assertTrue(problems, "mutation survived")

    def test_current_tree_control_stays_green(self) -> None:
        self.assert_clean(self.workflow, self.docs)

    def test_rename_verify_lsm_input_bites(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "      verify_lsm:\n", "      verify_lsm_renamed:\n"))

    def test_remove_publish_crates_needs_release_bites(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    needs: release\n", "    needs: build\n"))

    def test_publish_crates_needs_list_without_release_bites(self) -> None:
        self.assert_mutation_bites(workflow=replace_once(
            self.workflow, "    needs: release\n", "    needs: [build]\n"))

    def test_publish_crates_needs_list_including_release_stays_green(self) -> None:
        self.assert_clean(
            replace_once(self.workflow, "    needs: release\n", "    needs: [release]\n"),
            self.docs,
        )

    def test_docs_reverse_order_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "The GitHub Release is created before crates publication because `publish-crates` needs `release`.",
            "crates publication before GitHub Release; omit the needs reason.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_docs_omit_needs_release_reason_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "because `publish-crates` needs `release`.",
            "because crates follow the GitHub Release.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_docs_reintroduce_lsm_smoke_test_bites(self) -> None:
        mutated = self.docs + (
            "\nManually dispatch the lsm-smoke-test workflow.\n"
        )
        problems = contract.contract_problems(self.workflow, mutated)
        self.assertTrue(problems, "mutation survived")
        self.assertTrue(
            any("lsm-smoke-test" in problem for problem in problems),
            problems,
        )
        self.assert_clean(self.workflow, self.docs)

    def test_noop_comment_on_workflow_stays_green(self) -> None:
        self.assert_clean(self.workflow + "\n# no-op control\n", self.docs)

    def test_noop_comment_on_docs_stays_green(self) -> None:
        self.assert_clean(self.workflow, self.docs + "\n<!-- no-op control -->\n")

    def test_checker_searches_docs_argument_not_dunder_file(self) -> None:
        source = Path(contract.__file__).read_text(encoding="utf-8")
        self.assertNotIn("Path(__file__).read_text()", source)
        self.assertNotIn("lsm-smoke-test", self.docs)
        problems = contract.contract_problems(
            self.workflow, self.docs + "\nlsm-smoke-test\n",
        )
        self.assertTrue(problems)


if __name__ == "__main__":
    unittest.main(verbosity=2)
