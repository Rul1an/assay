#!/usr/bin/env python3
"""Mutation oracle for the release runbook / release.yml contract."""
from __future__ import annotations

import importlib.util
import tempfile
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

    def test_docs_omit_release_tag_on_local_lsm_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "scripts/verify_lsm_docker.sh --release-tag vX.Y.Z",
            "scripts/verify_lsm_docker.sh",
        )
        problems = contract.contract_problems(self.workflow, mutated)
        self.assertTrue(problems, "mutation survived")
        self.assertTrue(
            any("--release-tag" in problem for problem in problems),
            problems,
        )

    def test_docs_move_lsm_phrase_outside_item_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "Not a stable-release requirement. ",
            "",
        ) + "\nNot a stable-release requirement.\n"
        self.assertIn("not a stable-release requirement", mutated.lower())
        self.assert_mutation_bites(docs=mutated)
        item = contract._optional_lsm_item(mutated)
        self.assertNotIn("not a stable-release requirement", item.lower())

    def test_pypi_publisher_expectation_matches_release_job(self) -> None:
        self.assertEqual(
            contract._pypi_publisher_problems(self.workflow, self.docs), []
        )

    def test_pypi_environment_drift_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "    environment: pypi\n",
            "    environment: release\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_pypi_publish_action_removed_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "        uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33 # v1.14.2\n",
            "        run: python3 -m twine upload dist/*\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_pypi_publish_action_in_a_comment_does_not_count(self) -> None:
        mutated = replace_once(
            self.workflow,
            "        uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33 # v1.14.2\n",
            "        # uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33\n"
            "        run: python3 -m twine upload dist/*\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_pypi_publish_action_mutable_ref_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33",
            "pypa/gh-action-pypi-publish@main",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_pypi_publish_action_in_disabled_step_does_not_count(self) -> None:
        mutated = replace_once(
            self.workflow,
            "      - name: Publish to PyPI\n"
            "        uses: pypa/gh-action-pypi-publish@",
            "      - name: Publish to PyPI\n"
            "        if: ${{ false }}\n"
            "        uses: pypa/gh-action-pypi-publish@",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_pypi_publish_action_with_spaced_if_key_does_not_count(self) -> None:
        mutated = replace_once(
            self.workflow,
            "      - name: Publish to PyPI\n"
            "        uses: pypa/gh-action-pypi-publish@",
            "      - name: Publish to PyPI\n"
            "        if : ${{ false }}\n"
            "        uses: pypa/gh-action-pypi-publish@",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_nested_uses_does_not_count_as_step_action(self) -> None:
        mutated = replace_once(
            self.workflow,
            "        uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33 # v1.14.2\n",
            "        uses: actions/checkout@0ad4b8fadaa221de15dcec353f45205ec38ea70b\n"
            "        env:\n"
            "          uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_runbook_allows_no_legacy_publisher(self) -> None:
        item = contract._checklist_item(self.docs, "PyPI Trusted Publisher")
        self.assertIn("exactly one", item)
        self.assertIn("`Rul1an/assay`", item)
        self.assertIn("`release.yml`", item)
        self.assertIn("`pypi`", item)
        self.assertIn("`publish.yml`", item)
        self.assertIn("Remove", item)

    def test_runbook_that_keeps_publish_yml_bites_despite_remove_words(self) -> None:
        mutated = replace_once(
            self.docs,
            "  Remove every other publisher, including the legacy `publish.yml` identity. An empty environment\n",
            "  Keep the legacy `publish.yml` publisher. Remove only unrelated publishers. An empty environment\n",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_runbook_that_denies_exactly_one_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "require exactly one\n  GitHub publisher",
            "do not require exactly one\n  GitHub publisher",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_runbook_that_accepts_empty_environment_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "An empty environment\n  is broader authority and does not match this contract.",
            "An empty environment\n  is broader authority and matches this contract.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_hidden_runbook_contract_does_not_count(self) -> None:
        item = contract._checklist_item(self.docs, "PyPI Trusted Publisher")
        opposite = item.replace(
            "require exactly one\n  GitHub publisher",
            "do not require exactly one\n  GitHub publisher",
        ).replace(
            "Remove every other publisher, including the legacy `publish.yml` identity.",
            "Keep the legacy `publish.yml` publisher.",
        ).replace(
            "does not match this contract",
            "matches this contract",
        )
        mutated = self.docs.replace(item, f"{opposite}\n  <!-- {item} -->")
        self.assert_mutation_bites(docs=mutated)

    def test_pypi_identity_outside_checklist_item_does_not_count(self) -> None:
        item = contract._checklist_item(self.docs, "PyPI Trusted Publisher")
        mutated = self.docs.replace("`Rul1an/assay`", "`wrong/repository`", 1)
        mutated += "\n`Rul1an/assay` remains the expected repository.\n"
        self.assertNotEqual(item, contract._checklist_item(mutated, "PyPI Trusted Publisher"))
        self.assert_mutation_bites(docs=mutated)

    def test_crates_publisher_expectation_matches_release_job(self) -> None:
        self.assertEqual(
            contract._crates_publisher_problems(self.workflow, self.docs), []
        )

    def test_crates_empty_environment_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "An unset environment is broader authority and does not match this contract.",
            "An unset environment is broader authority and matches this contract.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_crates_wrong_environment_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "workflow `release.yml`, environment `crates`.",
            "workflow `release.yml`, environment `pypi`.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_crates_wrong_workflow_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "repository `Rul1an/assay`, workflow `release.yml`, environment `crates`.",
            "repository `Rul1an/assay`, workflow `publish.yml`, environment `crates`.",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_crates_legacy_row_bites(self) -> None:
        mutated = replace_once(
            self.docs,
            "  Remove every other publisher, including any publisher whose environment is unset.\n",
            "  Keep publishers whose environment is unset. Remove only unrelated publishers.\n",
        )
        self.assert_mutation_bites(docs=mutated)

    def test_crates_workflow_environment_drift_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "    environment: crates\n",
            "    environment: release\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_crates_auth_action_removed_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n",
            "        run: echo skipped\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_crates_auth_action_disabled_step_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "      - name: Authenticate with crates.io\n"
            "        id: auth\n"
            "        uses: rust-lang/crates-io-auth-action@",
            "      - name: Authenticate with crates.io\n"
            "        if: ${{ false }}\n"
            "        id: auth\n"
            "        uses: rust-lang/crates-io-auth-action@",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_crates_auth_action_mutable_ref_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18",
            "rust-lang/crates-io-auth-action@main",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_crates_auth_action_duplicated_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n",
            "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n"
            "      - name: Authenticate with crates.io again\n"
            "        uses: rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5\n",
        )
        self.assert_mutation_bites(workflow=mutated)

    def test_crates_secret_token_source_bites(self) -> None:
        mutated = replace_once(
            self.workflow,
            "          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}\n",
            "          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n",
        )
        problems = contract.contract_problems(mutated, self.docs)
        self.assertTrue(problems, "mutation survived")
        self.assertIn(contract._CRATES_SECRET_TOKEN_SOURCE_MESSAGE, problems)

    def _append_publisher_sentence(self, title: str, sentence: str) -> str:
        item = contract._checklist_item(self.docs, title)
        return replace_once(
            self.docs,
            item,
            item.rstrip("\n") + f"\n  {sentence}\n",
        )

    def _assert_unpinned_environment_bites(self, *, title: str, sentence: str) -> None:
        mutated = self._append_publisher_sentence(title, sentence)
        problems = contract.contract_problems(self.workflow, mutated)
        self.assertTrue(problems, "mutation survived")
        self.assertTrue(
            any(
                "unpinned" in problem.lower() and "environment" in problem.lower()
                for problem in problems
            ),
            problems,
        )

    def test_pypi_appended_optional_environment_contradiction_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._PYPI_ITEM_TITLE,
            sentence="The environment field is optional and may be left unset.",
        )

    def test_crates_appended_optional_environment_contradiction_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._CRATES_ITEM_TITLE,
            sentence="The environment field is optional and may be left unset.",
        )

    def test_pypi_codex_omitted_when_unavailable_paraphrase_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._PYPI_ITEM_TITLE,
            sentence="The environment value can be omitted when unavailable.",
        )

    def test_crates_codex_omitted_when_unavailable_paraphrase_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._CRATES_ITEM_TITLE,
            sentence="The environment value can be omitted when unavailable.",
        )

    def test_pypi_blank_environment_accepted_paraphrase_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._PYPI_ITEM_TITLE,
            sentence="Publication proceeds when the environment is not configured.",
        )

    def test_crates_blank_environment_accepted_paraphrase_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._CRATES_ITEM_TITLE,
            sentence="Publication proceeds when the environment is not configured.",
        )

    def test_pypi_publisher_scope_must_be_pypi_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._PYPI_ITEM_TITLE,
            sentence="Publisher scope must be `pypi`.",
        )

    def test_crates_publisher_scope_must_be_crates_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._CRATES_ITEM_TITLE,
            sentence="Publisher scope must be `crates`.",
        )

    def test_pypi_publisher_scope_must_be_pypi_crates_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._PYPI_ITEM_TITLE,
            sentence="Publisher scope must be `pypi`/`crates`.",
        )

    def test_crates_publisher_scope_must_be_pypi_crates_bites(self) -> None:
        self._assert_unpinned_environment_bites(
            title=contract._CRATES_ITEM_TITLE,
            sentence="Publisher scope must be `pypi`/`crates`.",
        )

    def test_adding_or_removing_a_crates_inventory_line_stays_green(self) -> None:
        added = replace_once(
            self.docs,
            "  - `assay-cli`\n",
            "  - `assay-cli`\n  - `assay-newcrate`\n",
        )
        self.assert_clean(self.workflow, added)
        removed = replace_once(
            self.docs,
            "  - `assay-sim`\n",
            "",
        )
        self.assert_clean(self.workflow, removed)
        item = contract._checklist_item(added, contract._CRATES_ITEM_TITLE)
        self.assertNotIn("environment", item.split("assay-newcrate")[1].lower())
        self.assertNotIn("`pypi`", item.split("assay-newcrate")[1])
        self.assertNotIn("`crates`", item.split("assay-newcrate")[1])

    def test_removing_environment_allowlist_lets_paraphrases_survive(self) -> None:
        source = Path(contract.__file__).read_text(encoding="utf-8")
        needle = (
            "    problems.extend(\n"
            "        _unpinned_environment_sentence_problems(\n"
            "            item, required_sentences, label=label, environment=environment\n"
            "        )\n"
            "    )\n"
        )
        self.assertEqual(source.count(needle), 1)
        paraphrases = (
            (contract._PYPI_ITEM_TITLE, "The environment value can be omitted when unavailable."),
            (contract._CRATES_ITEM_TITLE, "The environment value can be omitted when unavailable."),
            (contract._PYPI_ITEM_TITLE, "Publication proceeds when the environment is not configured."),
            (contract._CRATES_ITEM_TITLE, "Publication proceeds when the environment is not configured."),
        )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "check-release-runbook-truth.py"
            path.write_text(source.replace(needle, "", 1), encoding="utf-8")
            spec = importlib.util.spec_from_file_location(
                "disabled_release_runbook_truth", path
            )
            if spec is None or spec.loader is None:
                raise AssertionError("disabled checker is not loadable")
            disabled = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(disabled)
            for title, sentence in paraphrases:
                mutated = self._append_publisher_sentence(title, sentence)
                problems = disabled.contract_problems(self.workflow, mutated)
                with self.assertRaisesRegex(AssertionError, "mutation survived"):
                    self.assertTrue(problems, "mutation survived")
                live = contract.contract_problems(self.workflow, mutated)
                self.assertTrue(live, "live allowlist must still bite")

    def test_removing_value_keyed_trigger_lets_value_probes_survive(self) -> None:
        source = Path(contract.__file__).read_text(encoding="utf-8")
        needle = '    return bool(environment) and f"`{environment}`" in text\n'
        self.assertEqual(source.count(needle), 1)
        probes = (
            (contract._PYPI_ITEM_TITLE, "Publisher scope must be `pypi`."),
            (contract._CRATES_ITEM_TITLE, "Publisher scope must be `crates`."),
            (contract._PYPI_ITEM_TITLE, "Publisher scope must be `pypi`/`crates`."),
            (contract._CRATES_ITEM_TITLE, "Publisher scope must be `pypi`/`crates`."),
        )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "check-release-runbook-truth.py"
            path.write_text(source.replace(needle, "    return False\n", 1), encoding="utf-8")
            spec = importlib.util.spec_from_file_location(
                "value_trigger_disabled_release_runbook_truth", path
            )
            if spec is None or spec.loader is None:
                raise AssertionError("disabled checker is not loadable")
            disabled = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(disabled)
            for title, sentence in probes:
                mutated = self._append_publisher_sentence(title, sentence)
                problems = disabled.contract_problems(self.workflow, mutated)
                with self.assertRaisesRegex(AssertionError, "mutation survived"):
                    self.assertTrue(problems, "mutation survived")
                live = contract.contract_problems(self.workflow, mutated)
                self.assertTrue(live, "live allowlist must still bite")

    def test_crates_credentials_lead_in_is_complete(self) -> None:
        item = contract._checklist_item(self.docs, contract._CRATES_ITEM_TITLE)
        normalized = " ".join(item.split())
        self.assertIn(
            "No credentials. Apply this on every current crates.io crate:",
            normalized,
        )

    def test_crates_lead_in_fragment_bites(self) -> None:
        item = contract._checklist_item(self.docs, contract._CRATES_ITEM_TITLE)
        if "Apply this on every current crates.io crate:" in item:
            mutated_item = item.replace(
                "Apply this on every current crates.io crate:",
                "on every current crates.io crate:",
                1,
            )
        else:
            mutated_item = item
        mutated = replace_once(self.docs, item, mutated_item)
        problems = contract.contract_problems(self.workflow, mutated)
        self.assertTrue(problems, "mutation survived")
        self.assertTrue(
            any("Apply this" in problem for problem in problems),
            problems,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
