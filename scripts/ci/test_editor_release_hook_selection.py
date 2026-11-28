#!/usr/bin/env python3
"""The pre-push selection of the editor/release consumer battery.

Required CI runs the battery directly on every change; this pins only when a push may skip it
locally. The rule's owner is the battery itself (`--select-inputs`), so these tests run the real
rule against the real `.pre-commit-config.yaml` and stub only the battery's full run.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CONSUMER_ID = "editor-release-hook-precommit-consumer"
BATTERY = "scripts/ci/test-editor-release-hook-precommit-consumer.sh"
SELECTOR = "scripts/ci/run-editor-release-hook-precommit-consumer-if-affected.sh"
LIB = "scripts/ci/lib/pushed-range-changes.sh"
GIT_ENV_LIB = "scripts/ci/lib/clear-git-repository-env.sh"
CONFIG = ".pre-commit-config.yaml"
REAL_BATTERY = "scripts/ci/.real-editor-release-battery.sh"
# The stub answers the rule question with the real battery and records a full run instead of
# doing one: only the push decision is under test here.
STUB_BATTERY = (
    "#!/usr/bin/env bash\n"
    'if [ "${1:-}" = --select-inputs ]; then exec bash "$(dirname "$0")/.real-editor-release-battery.sh" "$@"; fi\n'
    'printf "battery %s\\n" "$*" >>"$CALLS"\n'
    'exit "${STUB_RC:-0}"\n'
)
OVERRIDES = ("ASSAY_BIN", "ASSAY_EDITOR_RECIPE", "ASSAY_RELEASE_TAG_FILE")


def configured_consumer() -> dict:
    """The consumer hook exactly as .pre-commit-config.yaml declares it (parsed, not matched)."""
    raw = subprocess.check_output(
        ["ruby", "-rjson", "-ryaml", "-e",
         "puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))"],
        input=(ROOT / CONFIG).read_bytes(),
    )
    hooks = [hook for repo in json.loads(raw)["repos"] if repo["repo"] == "local"
             for hook in repo["hooks"] if hook["id"] == CONSUMER_ID]
    assert len(hooks) == 1, hooks
    return hooks[0]


def scratch_env(**extra: str) -> dict[str, str]:
    """No inherited Git, pre-commit or override state may reach the scratch repository."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("GIT_", "PRE_COMMIT")) and key not in OVERRIDES}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_NAME="scratch", GIT_AUTHOR_EMAIL="scratch@example.invalid",
               GIT_COMMITTER_NAME="scratch", GIT_COMMITTER_EMAIL="scratch@example.invalid",
               PYTHONDONTWRITEBYTECODE="1")
    env.update(extra)
    return env


class ConsumerHookContract(unittest.TestCase):
    """New contract (red until the hook entry is the selector): not a behaviour proof."""

    def test_the_hook_runs_the_selector_at_push_and_shows_its_reason(self) -> None:
        hook = configured_consumer()
        self.assertEqual(shlex.split(hook["entry"]), ["bash", SELECTOR])
        self.assertIs(hook.get("always_run"), True)
        self.assertIs(hook.get("pass_filenames"), False)
        self.assertEqual(hook.get("stages"), ["pre-push"])
        self.assertIs(hook.get("verbose"), True, "the skip reason and the duration must be visible")
        self.assertNotIn("files", hook)


class PushSelection(unittest.TestCase):
    """Behaviour: run the consumer hook's CONFIGURED entry, whatever it is today.

    On a base whose entry is still the battery itself, the stub battery is the entry, so a push of
    an unrelated `.rs` change starts it: `test_an_unrelated_rust_change_skips` is the real
    reproduction of the unneeded run. Once the entry is the selector, the same harness runs that.
    Each full-run check asserts the behaviour (the battery started) before the selector's reason
    line, which is new API.
    """

    UNRELATED = "crates/assay-core/src/lib.rs"

    def setUp(self) -> None:
        self.entry = shlex.split(configured_consumer()["entry"])
        self.assertEqual(self.entry[0], "bash", self.entry)
        self.assertTrue(self.entry[1].startswith("scripts/ci/"), self.entry)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tree = Path(temporary.name) / "repo"
        self.calls = Path(temporary.name) / "calls"
        self.tmp = Path(temporary.name) / "tmp"  # the entry's own TMPDIR, checked after every push
        self.tmp.mkdir()
        self.env = scratch_env(CALLS=str(self.calls), TMPDIR=str(self.tmp))
        # Whatever the configured entry needs, as far as it exists on this tree: a selector that
        # sources a missing lib fails here exactly as it would in the checkout.
        for path in {self.entry[1], LIB, GIT_ENV_LIB, CONFIG} - {BATTERY}:
            if (ROOT / path).exists():
                self.write(path, (ROOT / path).read_text())
        self.write(REAL_BATTERY, (ROOT / BATTERY).read_text())
        self.write(BATTERY, STUB_BATTERY)
        for path in (self.UNRELATED, "docs/x.md", "README.md", "SECURITY.md",
                     "docs/guides/editor-mcp-recipe.md", "Cargo.lock"):
            self.write(path, path + "\n")
        self.git("init", "-q", "-b", "main")
        self.base = self.commit("base")

    def write(self, path: str, text: str) -> None:
        target = self.tree / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def git(self, *args: str) -> str:
        return subprocess.run(["git", *args], cwd=self.tree, env=self.env, check=True,
                              capture_output=True, text=True).stdout.strip()

    def commit(self, message: str) -> str:
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", message)
        return self.git("rev-parse", "HEAD")

    def push(self, from_ref: str | None = None, to_ref: str | None = None, **extra: str | None):
        if self.calls.exists():
            self.calls.unlink()
        env = dict(self.env, PRE_COMMIT_FROM_REF=from_ref or self.base,
                   PRE_COMMIT_TO_REF=to_ref or self.git("rev-parse", "HEAD"))
        env.update(extra)
        env = {key: value for key, value in env.items() if value is not None}
        result = subprocess.run(self.entry, cwd=self.tree, env=env, capture_output=True,
                                text=True, timeout=60)
        self.assertEqual(sorted(self.tmp.iterdir()), [], "the entry left scratch files behind")
        return result.returncode, result.stdout + result.stderr, self.calls.exists()

    def assert_skipped(self, result) -> None:
        rc, output, called = result
        self.assertFalse(called, "BEHAVIOUR: the battery started for a push that changes none of "
                                 "its inputs:\n" + output)
        self.assertEqual(rc, 0, output)
        self.assertIn("Required CI still runs it.", output, "selector skip line missing (new API)")
        self.assertNotIn("running in full", output)

    def assert_full(self, result, reason: str) -> None:
        rc, output, called = result
        self.assertTrue(called, "BEHAVIOUR: the battery did not start:\n" + output)
        self.assertEqual(rc, 0, output)
        self.assertIn("running in full", output, "selector reason line missing (new API)")
        self.assertIn(reason, output, "selector reason does not name the cause (new API)")
        self.assertNotIn("skipped;", output)

    # --- what the rule skips -------------------------------------------------------------------

    def test_an_unrelated_rust_change_skips(self) -> None:
        self.write(self.UNRELATED, "// changed\n")
        self.commit("rust")
        self.assert_skipped(self.push())

    def test_an_unrelated_doc_skips(self) -> None:
        self.write("docs/x.md", "changed\n")
        self.commit("doc")
        self.assert_skipped(self.push())

    def test_an_empty_range_on_a_clean_head_skips(self) -> None:
        self.assert_skipped(self.push())

    # --- what the rule runs for ----------------------------------------------------------------

    def test_each_input_prefix_and_the_config_run(self) -> None:
        for path in ("scripts/ci/new-check.sh", ".github/dependabot.yml", "conformance/x.py", CONFIG):
            with self.subTest(path=path):
                before = self.git("rev-parse", "HEAD")
                self.write(path, (self.tree / path).read_text() + "# changed\n"
                           if (self.tree / path).exists() else "x\n")
                self.commit(path)
                self.assert_full(self.push(from_ref=before), path)

    def test_every_cargo_lock_runs(self) -> None:
        for path in ("Cargo.lock", "fuzz/Cargo.lock"):
            with self.subTest(path=path):
                before = self.git("rev-parse", "HEAD")
                self.write(path, "lock " + path + "\n")
                self.commit(path)
                self.assert_full(self.push(from_ref=before), path)

    def test_a_gitattributes_file_runs(self) -> None:
        # The battery snapshots with `git archive HEAD`, which follows .gitattributes.
        for path in (".gitattributes", "docs/.gitattributes"):
            with self.subTest(path=path):
                before = self.git("rev-parse", "HEAD")
                self.write(path, "*.md export-ignore\n")
                self.commit(path)
                self.assert_full(self.push(from_ref=before), path)

    def test_paths_the_protected_hooks_select_run(self) -> None:
        # README.md is in release-surface's files regex, the recipe in editor-mcp-recipe-truth's.
        for path in ("README.md", "docs/guides/editor-mcp-recipe.md"):
            with self.subTest(path=path):
                before = self.git("rev-parse", "HEAD")
                self.write(path, "changed " + path + "\n")
                self.commit(path)
                self.assert_full(self.push(from_ref=before), path)

    def test_a_path_only_the_editor_regex_selects_runs(self) -> None:
        # In today's config every editor-selected path is also release-selected or under a prefix,
        # so only a config where the editor regex alone selects a path shows that it is read.
        text = (self.tree / CONFIG).read_text()
        block = text.index("- id: editor-mcp-recipe-truth")
        line_start = text.index("files: ", block)
        line_end = text.index("\n", line_start)
        tail = "|\\.pre-commit-config\\.yaml)$"
        line = text[line_start:line_end]
        self.assertTrue(line.endswith(tail), line)
        widened = line[: -len(tail)] + "|docs/editor-only\\.md" + tail
        self.write(CONFIG, text[:line_start] + widened + text[line_end:])
        self.write("docs/editor-only.md", "x\n")
        self.base = self.commit("editor regex selects one more path")
        self.write("docs/editor-only.md", "changed\n")
        self.commit("editor-only change")
        self.assert_full(self.push(), "docs/editor-only.md")

    def test_a_deleted_input_runs(self) -> None:
        (self.tree / "SECURITY.md").unlink()
        self.commit("delete")
        self.assert_full(self.push(), "SECURITY.md")

    def test_an_input_renamed_out_of_the_rule_runs(self) -> None:
        self.git("mv", "README.md", "docs/archive-readme.md")
        self.commit("rename")
        self.assert_full(self.push(), "README.md")

    def test_a_set_override_runs_in_full(self) -> None:
        # Each of these changes what the battery reads, whatever the push changed.
        self.write(self.UNRELATED, "// changed\n")
        self.commit("rust")
        for name in OVERRIDES:
            with self.subTest(name=name):
                self.assert_full(self.push(**{name: "/x"}), name)

    # --- the shared preconditions --------------------------------------------------------------

    def test_a_missing_range_runs(self) -> None:
        env_free = {"PRE_COMMIT_FROM_REF": None, "PRE_COMMIT_TO_REF": None}
        rc, output, called = self.push(**env_free)
        self.assert_full((rc, output, called), "no pushed range")

    def test_a_pushed_commit_that_is_not_head_runs(self) -> None:
        self.write(self.UNRELATED, "// one\n")
        first = self.commit("one")
        self.write(self.UNRELATED, "// two\n")
        self.commit("two")
        self.assert_full(self.push(to_ref=first), "is not the checked-out HEAD")

    def test_a_tracked_change_on_top_of_head_runs(self) -> None:
        self.write(self.UNRELATED, "// dirty\n")
        self.assert_full(self.push(), "tracked files differ from HEAD")

    def test_a_non_fast_forward_push_runs(self) -> None:
        self.write(self.UNRELATED, "// side\n")
        side = self.commit("side")
        self.git("reset", "-q", "--hard", self.base)
        self.write("docs/x.md", "main\n")
        self.commit("main")
        self.assert_full(self.push(from_ref=side), "not a fast-forward")

    def test_an_unknown_from_ref_runs(self) -> None:
        self.assert_full(self.push(from_ref="0" * 40), "")

    # --- the rule's owner ----------------------------------------------------------------------

    def test_an_unreadable_rule_runs_in_full(self) -> None:
        # A config without the release-surface hook gives the battery no rule to answer with.
        text = (self.tree / CONFIG).read_text()
        self.assertIn("- id: release-surface", text)
        self.write(CONFIG, text.replace("- id: release-surface", "- id: release-surface-renamed", 1))
        self.base = self.commit("config without the rule")
        self.write(self.UNRELATED, "// changed\n")
        self.commit("rust")
        self.assert_full(self.push(), "input rule")

    def test_the_rule_is_answered_without_pre_commit_on_path(self) -> None:
        # Required CI's test job has PyYAML but no pre-commit: the read-only rule question must not
        # need pre-commit, or every push there would run in full.
        farm = self.tree.parent / "farm"
        farm.mkdir()
        for tool in ("bash", "sh", "env", "git", "mktemp", "tr", "wc", "head", "dirname", "basename",
                     "cat", "sed", "rm", "mkdir", "ls", "grep", "sort", "python3"):
            found = shutil.which(tool)
            self.assertIsNotNone(found, f"the farm needs {tool}")
            (farm / tool).symlink_to(found)
        self.assertIsNone(shutil.which("pre-commit", path=str(farm)), "pre-commit leaked into the farm")
        self.assertEqual(subprocess.run([str(farm / "python3"), "-c", "import yaml"]).returncode, 0,
                         "the farm's python3 needs PyYAML, as the CI step installs it")
        self.write(self.UNRELATED, "// changed\n")
        self.commit("rust")
        self.assert_skipped(self.push(PATH=str(farm)))

    def test_a_failed_second_allocation_leaves_no_scratch_file(self) -> None:
        # The entry allocates two scratch files. If the second allocation fails, the first must
        # still be removed: push() fails on any file the entry leaves in its TMPDIR.
        real_mktemp = shutil.which("mktemp")
        self.assertIsNotNone(real_mktemp)
        stub_bin = self.tree.parent / "stub-bin"
        stub_bin.mkdir()
        count = self.tree.parent / "mktemp-count"
        (stub_bin / "mktemp").write_text(
            "#!/usr/bin/env bash\n"
            f'n=$(( $(cat "{count}" 2>/dev/null || echo 0) + 1 ))\n'
            f'echo "$n" > "{count}"\n'
            'if [ "$n" -ge 2 ]; then echo "stub mktemp: out of space" >&2; exit 1; fi\n'
            f'exec "{real_mktemp}" "$@"\n')
        (stub_bin / "mktemp").chmod(0o755)
        self.write(self.UNRELATED, "// changed\n")
        self.commit("rust")
        rc, output, called = self.push(PATH=f"{stub_bin}:{self.env['PATH']}")
        self.assertGreaterEqual(int(count.read_text()), 2, "the second allocation was never reached:\n" + output)
        self.assertFalse(called, output)

    def test_a_battery_failure_propagates(self) -> None:
        self.write("scripts/ci/new-check.sh", "x\n")
        self.commit("input")
        rc, output, called = self.push(STUB_RC="3")
        self.assertTrue(called, output)
        self.assertEqual(rc, 3, output)


class SelectorInterface(unittest.TestCase):
    """New API (red while the selector does not exist): not a behaviour proof."""

    def test_refuses_arguments_before_running_anything(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary)
            for path in (SELECTOR, LIB):
                (tree / path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / path, tree / path)
            (tree / BATTERY).write_text(STUB_BATTERY)
            calls = tree / "calls"
            result = subprocess.run(["bash", str(tree / SELECTOR), "--all"], cwd=tree,
                                    env=scratch_env(CALLS=str(calls)), capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertFalse(calls.exists())


if __name__ == "__main__":
    unittest.main()
