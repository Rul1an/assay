#!/usr/bin/env python3
"""Mutation tests must never write the source tree they test (H9).

Builds throwaway source repositories, runs the production entry points in them with stub `cargo` and
`rustup`, and requires the source's bytes, modes, index entries and index file to be unchanged after
a normal run, a failing run and a SIGKILL. Also pins the scratch-copy rules of
scripts/ci/lib/mutation_scratch.py. No real cargo build runs here.
"""
from __future__ import annotations

import hashlib
import importlib.util
import os
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/lib/mutation_scratch.py"
LINUX_TEST = "scripts/ci/test-check-linux.sh"
GATE = "scripts/ci/check-linux.sh"
MONITOR = "crates/assay-monitor/src/lib.rs"
CLI_LINUX = "crates/assay-cli/src/linux.rs"
PLANT = "A_VALUE_THAT_DOES_NOT_EXIST"

STUB_CARGO = r"""#!/usr/bin/env bash
printf '%s\t%s\t%s\t%s\n' "$PWD" "${CARGO_TARGET_DIR:-}" "${CARGO_BUILD_BUILD_DIR:-}" "$*" >>"$STUB_LOG"
crate=""; quiet=0; previous=""
for arg in "$@"; do
  if [ "$previous" = -p ]; then crate="$arg"; fi
  if [ "$arg" = --quiet ]; then quiet=1; fi
  previous="$arg"
done
case "$crate" in
  assay-runner-linux) exit 0 ;;
  assay-discovered)
    echo 'crates/assay-discovered/src/lib.rs:2:28: error[E0425]: cannot find value NEW_LINUX_ERROR'
    exit 101 ;;
  assay-monitor)
    if [ "$quiet" = 1 ]; then
      if grep -q A_VALUE_THAT_DOES_NOT_EXIST crates/assay-monitor/src/lib.rs; then
        printf 'PLANTED\n' >>"$HOST_WITNESS"
        case "${STUB_RUST_HOST:-aarch64-apple-darwin}" in
          *-linux-*) echo 'error[E0425]: cannot find value A_VALUE_THAT_DOES_NOT_EXIST'; exit 101 ;;
        esac
      else printf 'ABSENT\n' >>"$HOST_WITNESS"; fi
      exit "${STUB_HOST_RC:-0}"
    fi
    if grep -q 'fixture build unavailable' crates/assay-monitor/build.rs; then
      echo 'error: failed to run custom build command for assay-monitor'; exit 101
    fi
    if grep -q A_VALUE_THAT_DOES_NOT_EXIST crates/assay-monitor/src/lib.rs; then
      echo 'crates/assay-monitor/src/lib.rs:9:13: error[E0425]: cannot find value A_VALUE_THAT_DOES_NOT_EXIST'
      exit 101
    fi
    exit 0 ;;
  *) echo "unexpected Cargo invocation: $*" >&2; exit 99 ;;
esac
"""
STUB_RUSTUP = r"""#!/usr/bin/env bash
if [ "$*" = 'toolchain list' ]; then
  if [ "${STUB_TOOLCHAIN_MISSING:-0}" != 1 ]; then echo "${RUSTUP_TOOLCHAIN}-aarch64-apple-darwin"; fi
  exit 0
fi
printf '%s\t%s\t%s\t%s\n' "$PWD" "${CARGO_TARGET_DIR:-}" "${CARGO_BUILD_BUILD_DIR:-}" "rustup $*" >>"$STUB_LOG"
if grep -q A_VALUE_THAT_DOES_NOT_EXIST crates/assay-monitor/src/lib.rs 2>/dev/null; then
  : >"$STUB_STARTED"
  if [ "${STUB_BLOCK:-0}" = 1 ]; then sleep 600; fi
fi
if [ "$*" = 'target list --installed' ]; then
  if [ "${STUB_TARGET_MISSING:-0}" != 1 ]; then echo x86_64-unknown-linux-gnu; fi
  exit 0
fi
exit 99
"""
STUB_RUSTC = r"""#!/usr/bin/env bash
[ "$*" = -vV ] || exit 99
printf 'host: %s\n' "${STUB_RUST_HOST:-aarch64-apple-darwin}"
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def clean_env(**extra: str) -> dict[str, str]:
    """No Git, pre-commit or cargo state from the outer run may reach a fixture."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("GIT_", "PRE_COMMIT")) and key not in ("CARGO_TARGET_DIR", "CARGO_BUILD_BUILD_DIR")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_NAME="fixture", GIT_AUTHOR_EMAIL="fixture@example.invalid",
               GIT_COMMITTER_NAME="fixture", GIT_COMMITTER_EMAIL="fixture@example.invalid")
    env.update(extra)
    return env


def git(repo: Path, *args: str, env: dict[str, str]) -> str:
    return subprocess.run(["git", "--no-optional-locks", "-C", str(repo), "-c", "core.hooksPath=/dev/null",
                           "-c", "commit.gpgsign=false", *args],
                          env=env, check=True, capture_output=True, text=True).stdout


def source_state(repo: Path, paths: list[str], env: dict[str, str]) -> dict[str, object]:
    """Everything a mutation test must leave alone: bytes, modes, index entries, index file, status."""
    return {
        "bytes": {p: sha256(repo / p) for p in paths},
        "modes": {p: stat.S_IMODE(os.lstat(repo / p).st_mode) for p in paths},
        "index": git(repo, "ls-files", "-s", "--", *paths, env=env),
        "index_file": sha256(repo / ".git/index"),
        "status": git(repo, "status", "--porcelain", "--untracked-files=all", env=env),
    }


def make_repo(base: Path, files: dict[str, tuple[bytes, int]], env: dict[str, str]) -> Path:
    repo = base / "source"
    for rel, (data, mode) in files.items():
        path = repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        os.chmod(path, mode)
    git(repo, "init", "-q", "-b", "main", env=env)
    git(repo, "add", "-A", env=env)
    git(repo, "commit", "-q", "-m", "fixture", env=env)
    return repo


def kill_when_started(argv: list[str], cwd: Path, env: dict[str, str], started: Path) -> tuple[bool, int, str]:
    """SIGKILL the run's own process group once `started` exists; never read a live pipe.

    The group is stopped and reaped before any output is read, so a run that never starts or never
    exits cannot block the test before its kill.
    """
    process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               start_new_session=True)
    reached = False
    try:
        deadline = time.monotonic() + 60
        while process.poll() is None and time.monotonic() < deadline:
            if started.exists():
                reached = True
                break
            time.sleep(0.05)
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=30)
        output = process.stdout.read().decode(errors="replace")
        process.stdout.close()
    return reached, process.returncode, output


def load_helper():
    spec = importlib.util.spec_from_file_location("mutation_scratch", HELPER)
    module = importlib.util.module_from_spec(spec)
    # Loading from the checkout must not write __pycache__ into it.
    previous, sys.dont_write_bytecode = sys.dont_write_bytecode, True
    try:
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return module


class LinuxGateTestIsolation(unittest.TestCase):
    """Runs the offline fixture self-test from a throwaway source repository, with stub tools."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.env = clean_env()
        cfg = b'#[cfg(target_os = "linux")]\nfn linux_only() {}\n'
        files = {
            LINUX_TEST: ((ROOT / LINUX_TEST).read_bytes(), 0o755),
            GATE: ((ROOT / GATE).read_bytes(), 0o755),
            MONITOR: (cfg, 0o640),
            CLI_LINUX: (cfg, 0o644),
            ".gitignore": (b"/target\n", 0o644),
        }
        for path in sorted((ROOT / "scripts/ci/lib").rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                files[str(path.relative_to(ROOT))] = (path.read_bytes(), stat.S_IMODE(path.stat().st_mode))
        for rel in ("rust-toolchain.toml", "scripts/precommit/run-with-timeout.py", "scripts/precommit/run-with-timeout.sh"):
            files[rel] = ((ROOT / rel).read_bytes(), 0o644)
        self.repo = make_repo(self.base, files, self.env)
        self.paths = [MONITOR, CLI_LINUX, LINUX_TEST, GATE]
        self.before = source_state(self.repo, self.paths, self.env)
        stubs = self.base / "stubs"
        stubs.mkdir()
        for name, body in (("cargo", STUB_CARGO), ("rustup", STUB_RUSTUP), ("rustc", STUB_RUSTC)):
            (stubs / name).write_text(body)
            (stubs / name).chmod(0o755)
        self.run_tmp = self.base / "run-tmp"
        self.run_tmp.mkdir()
        self.log = self.base / "cargo.log"
        self.started = self.base / "started"
        self.run_env = dict(self.env, PATH=f"{stubs}{os.pathsep}{os.environ['PATH']}", TMPDIR=str(self.run_tmp),
                            STUB_LOG=str(self.log), STUB_STARTED=str(self.started),
                            HOST_WITNESS=str(self.base / "host-witness"),
                            ASSAY_LINUX_TARGET_TIMEOUT="0",  # no timeout wrapper, so one process group
                            # The self-test must establish its own full/partial cases.
                            ASSAY_LINUX_CHECK_REQUIRE_FULL="0")

    def calls(self):
        return [line.split("\t") for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def run_linux_test(self, **extra: str):
        result = subprocess.run(["bash", LINUX_TEST], cwd=self.repo, env=dict(self.run_env, **extra),
                                capture_output=True, text=True, timeout=120)
        self.assertEqual(list(self.run_tmp.iterdir()), [], "ordinary exit left owned scratch behind")
        return result

    def assert_source_unchanged(self, context: str):
        self.assertEqual(source_state(self.repo, self.paths, self.env), self.before, context)

    def assert_built_in_scratch(self, output: str):
        calls = self.calls()
        self.assertTrue(calls, output)
        for cwd, target, build_dir, argv in calls:
            self.assertNotEqual(Path(cwd).resolve(), self.repo.resolve(), f"cargo ran in the source: {argv}\n{output}")
            expected_target = str((Path(cwd) / "target").resolve())
            self.assertEqual(str(Path(target).resolve()), expected_target, f"cargo target dir for {argv}\n{output}")
            # build.build-dir takes intermediate artifacts; a shared one would mix mutants into ordinary builds.
            self.assertEqual(str(Path(build_dir).resolve()), expected_target, f"cargo build dir for {argv}\n{output}")

    def test_normal_run_builds_in_scratch_and_leaves_the_source(self):
        result = self.run_linux_test()
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn("ok    a cfg(linux) compile error fails the gate", output)
        self.assert_source_unchanged(output)
        self.assert_built_in_scratch(output)

    def test_relative_tmpdir_keeps_absolute_build_paths_and_cleans_up(self):
        result = self.run_linux_test(TMPDIR="../run-tmp")
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assert_built_in_scratch(output)
        self.assert_source_unchanged(output)

    def test_host_control_observes_the_planted_error_before_restore(self):
        for host in ("aarch64-apple-darwin", "x86_64-unknown-linux-gnu"):
            with self.subTest(host=host):
                witness = self.base / "host-witness"
                witness.write_text("")
                result = self.run_linux_test(STUB_RUST_HOST=host)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(witness.read_text(), "PLANTED\n")
                self.assert_source_unchanged(host)

    def test_missing_toolchain_or_target_is_unavailable_not_green(self):
        for setting in ("STUB_TOOLCHAIN_MISSING", "STUB_TARGET_MISSING"):
            with self.subTest(setting=setting):
                result = self.run_linux_test(**{setting: "1"})
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assert_source_unchanged(setting)
                self.assertEqual(list(self.run_tmp.iterdir()), [])

    def test_inherited_cargo_directories_are_overridden(self):
        result = self.run_linux_test(CARGO_TARGET_DIR=str(self.base / "shared-target"),
                                     CARGO_BUILD_BUILD_DIR=str(self.base / "shared-build"))
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assert_built_in_scratch(output)
        self.assertFalse((self.base / "shared-build").exists(), output)
        self.assertFalse((self.base / "shared-target").exists(), output)

    def test_an_inherited_strict_coverage_setting_does_not_change_the_partial_case(self):
        result = self.run_linux_test(ASSAY_LINUX_CHECK_REQUIRE_FULL="1")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("ok    partial mode reports an unavailable build", result.stdout)

    def test_hook_git_environment_never_reaches_the_source(self):
        # Under a commit hook these point at the source repository; the scratch must not inherit them.
        git_dir = str(self.repo / ".git")
        result = self.run_linux_test(GIT_DIR=git_dir, GIT_INDEX_FILE=f"{git_dir}/index", GIT_WORK_TREE=str(self.repo))
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assert_source_unchanged(output)
        self.assert_built_in_scratch(output)

    def test_a_line_break_in_tmpdir_never_turns_cleanup_onto_another_path(self):
        # Refuse line breaks before allocation; cleanup must never reinterpret a split path.
        prefix, sibling = self.base / "pre", self.base / "sibling"
        for directory in (prefix, sibling):
            directory.mkdir()
            (directory / "sentinel").write_text("keep\n")
        odd = self.base / "pre\nfix"
        odd.mkdir()
        result = self.run_linux_test(TMPDIR=str(odd))
        output = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, output)
        self.assertTrue((prefix / "sentinel").exists(), "cleanup removed the path before the line break")
        self.assertTrue((sibling / "sentinel").exists(), "cleanup reached a sibling directory")
        self.assertEqual(sorted(odd.iterdir()), [], "the refused scratch must be removed by the helper")
        self.assert_source_unchanged(output)

    def test_failing_run_leaves_the_source(self):
        result = self.run_linux_test(STUB_HOST_RC="1")
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 1, output)
        self.assert_source_unchanged(output)
        self.assert_built_in_scratch(output)

    def test_sigkill_with_the_planted_error_leaves_the_source(self):
        reached, returncode, output = kill_when_started(
            ["bash", LINUX_TEST], self.repo, dict(self.run_env, STUB_BLOCK="1"), self.started)
        self.assertTrue(reached, "the planted error never reached cargo:\n" + output)
        self.assertEqual(returncode, -signal.SIGKILL, output)
        self.assert_source_unchanged("after SIGKILL with the planted error in place")
        cwd = Path(self.calls()[-1][0])
        self.assertIn(PLANT, (cwd / MONITOR).read_text(), "positive control: the kill hit the planted state")


class LinuxFixtureCI(unittest.TestCase):
    """This fast required-CI suite independently pins the real compiler step's call site.

    Deliberately one supported step shape, not a workflow interpreter. Existing gate-coverage
    contracts bind the test matrix to required CI; this pins ownership, setup and invocation.
    """

    STEP = '''      - name: Linux compile gate (offline compiler fixture)
        if: runner.os == 'Linux'
        shell: bash
        run: |
          set -euo pipefail
          toolchain="$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("rust-toolchain.toml").read_text())["toolchain"]["channel"])')"
          echo "Linux fixture toolchain: $toolchain"
          rustup toolchain install "$toolchain" --profile minimal --no-self-update
          rustup target add --toolchain "$toolchain" x86_64-unknown-linux-gnu
          bash scripts/ci/test-check-linux.sh
'''

    def check_wiring(self, text):
        jobs = re.findall(r"(?ms)^  test:\n(.*?)(?=^ {0,2}[^\s#]|\Z)", text)
        self.assertEqual(len(jobs), 1, "one test matrix must own the compiler fixture")
        job = jobs[0]
        header = job.split("    steps:\n", 1)[0]
        self.assertIn("    if: needs.scope.outputs.lightweight_only != 'true'\n", header)
        self.assertIn("    runs-on: ${{ matrix.os }}\n", header)
        self.assertIn("        os: [ubuntu-24.04, macos-latest, windows-latest]\n", header)
        self.assertNotRegex(header, r"(?m)^    ['\"]?(?:continue-on-error|defaults|<<)['\"]?:")
        heading = self.STEP.splitlines(keepends=True)[0]
        self.assertEqual(job.count(heading), 1, "one Linux compiler fixture step must run")
        tail = job.split(heading, 1)[1]
        block = heading + re.split(r"(?m)^      - ", tail, maxsplit=1)[0]
        # Blank/comment separator lines belong to the following step.
        lines = block.splitlines(keepends=True)
        while lines and re.match(r"^(?: *| {0,6}#.*)$", lines[-1].rstrip("\n")):
            lines.pop()
        self.assertEqual("".join(lines), self.STEP, "the compiler fixture step must execute exactly")

    def test_real_compiler_fixture_is_required_in_the_linux_matrix(self):
        self.check_wiring((ROOT / ".github/workflows/ci.yml").read_text())

    def test_disabled_or_reowned_compiler_fixture_is_refused(self):
        text = (ROOT / ".github/workflows/ci.yml").read_text()
        self.check_wiring(text)
        self.assertEqual(text.count(self.STEP), 1)
        heading = self.STEP.splitlines(keepends=True)[0]
        cases = {
            "removed": "",
            "duplicate": self.STEP * 2,
            "disabled": self.STEP.replace("runner.os == 'Linux'", "false"),
            "ignored failure": self.STEP.replace("        shell:", "        continue-on-error: true\n        shell:"),
            "inert shell": self.STEP.replace("shell: bash", "shell: echo {0}"),
            "early exit": self.STEP.replace("set -euo pipefail", "set -euo pipefail\n          exit 0"),
            "no invocation": self.STEP.replace("          bash scripts/ci/test-check-linux.sh\n", ""),
            "no toolchain setup": self.STEP.replace('          rustup toolchain install "$toolchain" --profile minimal --no-self-update\n', ""),
            "optional sibling takes step": "  optional-fixture:\n    if: false\n    steps:\n" + self.STEP,
        }
        for name, replacement in cases.items():
            with self.subTest(mutant=name), self.assertRaises(AssertionError):
                self.check_wiring(text.replace(self.STEP, replacement))
        # A comment mentioning the command is harmless, but cannot stand in for an active step.
        self.check_wiring(text.replace(heading, "      # fixture guard\n" + heading))

    def test_all_fixture_inputs_select_full_ci(self):
        paths = [LINUX_TEST, GATE, "scripts/ci/test_mutation_scratch_isolation.py",
                 "scripts/ci/lib/clear-git-repository-env.sh", "scripts/ci/lib/process_group.py",
                 "scripts/precommit/run-with-timeout.py", "scripts/precommit/run-with-timeout.sh",
                 "rust-toolchain.toml", ".github/workflows/ci.yml"]
        with tempfile.TemporaryDirectory() as tmp:
            changed = Path(tmp) / "changed"
            for path in paths:
                with self.subTest(path=path):
                    changed.write_text(path + "\n")
                    result = subprocess.run(
                        ["bash", str(ROOT / "scripts/ci/classify-lightweight-changes.sh"), str(changed)],
                        text=True, capture_output=True, timeout=10, check=True)
                    self.assertEqual(result.stdout.strip(), "false")

    def test_local_hooks_select_their_fixture_inputs(self):
        config = (ROOT / ".pre-commit-config.yaml").read_text()
        fixture_inputs = (LINUX_TEST, GATE, "rust-toolchain.toml",
                          "scripts/ci/test_mutation_scratch_isolation.py",
                          "scripts/ci/lib/clear-git-repository-env.sh", "scripts/ci/lib/process_group.py",
                          "scripts/precommit/run-with-timeout.py", "scripts/precommit/run-with-timeout.sh")
        for hook, additional in {
            "linux-gate-self-test": (),
            "mutation-scratch-isolation": ("scripts/ci/lib/mutation_scratch.py",
                                           "scripts/ci/classify-lightweight-changes.sh",
                                           ".github/workflows/ci.yml", ".pre-commit-config.yaml"),
        }.items():
            block = config.split(f"      - id: {hook}\n", 1)[1].split("      - id:", 1)[0]
            selector = re.search(r"(?m)^        files: (.+)$", block).group(1)
            for path in fixture_inputs + additional:
                with self.subTest(hook=hook, path=path):
                    self.assertRegex(path, selector)


class ScratchCopyRules(unittest.TestCase):
    """The copy rules, exercised directly on a throwaway repository."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.env = clean_env()
        self.repo = make_repo(self.base, {
            "a/tracked.txt": (b"committed\n", 0o644),
            "a/tool.sh": (b"#!/bin/sh\n", 0o755),
            "a/private.txt": (b"private\n", 0o640),
            "a/deleted.txt": (b"gone soon\n", 0o644),
            ".gitignore": (b"/target\n/ignored.txt\n", 0o644),
        }, self.env)
        (self.repo / "a/tracked.txt").write_bytes(b"uncommitted\n")
        (self.repo / "a/untracked.txt").write_bytes(b"new\n")
        (self.repo / "ignored.txt").write_bytes(b"ignored\n")
        (self.repo / "target").mkdir()
        (self.repo / "target/artifact").write_bytes(b"artifact\n")
        (self.repo / "a/deleted.txt").unlink()
        old = time.time() - 86400
        os.utime(self.repo / "a/tool.sh", (old, old))
        self.helper = load_helper()
        self.scratch_parent = self.base / "tmp"
        self.scratch_parent.mkdir()

    def prepare(self, **extra: str):
        environment = dict(clean_env(), TMPDIR=str(self.scratch_parent))
        environment.update(extra)
        saved = dict(os.environ)
        os.environ.clear()
        os.environ.update(environment)
        try:
            return self.helper.prepare(self.repo)
        finally:
            os.environ.clear()
            os.environ.update(saved)

    def test_copies_the_worktree_bytes_and_modes_and_nothing_else(self):
        before = source_state(self.repo, ["a/tracked.txt", "a/tool.sh", "a/private.txt"], self.env)
        scratch, target = self.prepare()
        self.assertEqual((scratch / "a/tracked.txt").read_bytes(), b"uncommitted\n", "worktree bytes, not HEAD")
        self.assertEqual((scratch / "a/untracked.txt").read_bytes(), b"new\n")
        self.assertFalse((scratch / "a/deleted.txt").exists())
        self.assertFalse((scratch / "ignored.txt").exists())
        self.assertFalse((scratch / "target").exists())
        self.assertEqual(stat.S_IMODE((scratch / "a/tool.sh").stat().st_mode), 0o755)
        self.assertEqual(stat.S_IMODE((scratch / "a/private.txt").stat().st_mode), 0o640)
        source, copy = (self.repo / "a/tool.sh").stat(), (scratch / "a/tool.sh").stat()
        self.assertNotEqual((source.st_dev, source.st_ino), (copy.st_dev, copy.st_ino))
        self.assertGreater(copy.st_mtime, source.st_mtime, "fresh mtimes, so cargo rebuilds the copy")
        self.assertTrue(str(scratch.resolve()).startswith(str(self.scratch_parent.resolve())))
        self.assertEqual(target, (self.repo / "target/mutation-scratch").resolve())
        top = git(scratch, "rev-parse", "--show-toplevel", env=self.env).strip()
        self.assertEqual(Path(top).resolve(), scratch.resolve(), "the gate must find the copy, not the source")
        self.assertEqual(source_state(self.repo, ["a/tracked.txt", "a/tool.sh", "a/private.txt"], self.env), before)

    def test_hook_git_environment_does_not_redirect_the_scratch(self):
        git_dir = str(self.repo / ".git")
        before = source_state(self.repo, ["a/tracked.txt"], self.env)
        scratch, _target = self.prepare(GIT_DIR=git_dir, GIT_INDEX_FILE=f"{git_dir}/index", GIT_WORK_TREE=str(self.repo))
        self.assertTrue((scratch / ".git").is_dir(), "the scratch has its own repository")
        top = git(scratch, "rev-parse", "--show-toplevel", env=self.env).strip()
        self.assertEqual(Path(top).resolve(), scratch.resolve())
        self.assertEqual(source_state(self.repo, ["a/tracked.txt"], self.env), before)

    def test_a_tracked_directory_replaced_by_a_file_is_copied_as_the_file(self):
        (self.repo / "b").mkdir()
        (self.repo / "b/old.txt").write_text("old\n")
        git(self.repo, "add", "b/old.txt", env=self.env)
        git(self.repo, "commit", "-q", "-m", "directory b", env=self.env)
        shutil.rmtree(self.repo / "b")
        (self.repo / "b").write_text("now a file\n")  # the index still lists b/old.txt
        scratch, _target = self.prepare()
        self.assertEqual((scratch / "b").read_text(), "now a file\n")

    def test_refuses_a_tmpdir_inside_the_checkout(self):
        # A SIGKILL would leave a mutated copy of the source inside the checkout.
        for inside in ("tmp", "target/tmp"):
            with self.subTest(tmpdir=inside):
                parent = self.repo / inside
                parent.mkdir(parents=True)
                with self.assertRaises(SystemExit):
                    self.prepare(TMPDIR=str(parent))
                self.assertEqual(sorted(parent.iterdir()), [], "refused before allocating")

    def test_refuses_a_symlink(self):
        (self.repo / "a/link").symlink_to("tracked.txt")
        with self.assertRaises(SystemExit):
            self.prepare()

    def test_refuses_a_target_dir_that_would_be_copied(self):
        # `a/` is not ignored, so a target dir there would be copied into the scratch.
        with self.assertRaises(SystemExit):
            self.prepare(CARGO_TARGET_DIR="a")

    def test_inode_guard_refuses_a_shared_file(self):
        with self.assertRaises(SystemExit):
            self.helper.assert_separate(self.repo / "a/tool.sh", self.repo / "a/tool.sh")


if __name__ == "__main__":
    unittest.main()
