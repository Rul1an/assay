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
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci/lib/mutation_scratch.py"
LINUX_TEST = "scripts/ci/test-check-linux.sh"
GATE = "scripts/ci/check-linux.sh"
MONITOR = "crates/assay-monitor/src/lib.rs"
CLI_LINUX = "crates/assay-cli/src/linux.rs"
PLANT = "A_VALUE_THAT_DOES_NOT_EXIST"

STUB_CARGO = r"""#!/usr/bin/env bash
printf '%s\t%s\t%s\n' "$PWD" "${CARGO_TARGET_DIR:-}" "$*" >>"$STUB_LOG"
case " $* " in
  *" -p assay-cli "*"--target"*)
    echo 'error: failed to run custom build command for `ring v0.17.14`'
    exit 101 ;;
  *" -p assay-monitor "*"--target"*)
    if grep -q A_VALUE_THAT_DOES_NOT_EXIST crates/assay-monitor/src/lib.rs; then
      echo 'crates/assay-monitor/src/lib.rs:9:13: error[E0425]: cannot find value `A_VALUE_THAT_DOES_NOT_EXIST`'
      exit 101
    fi
    exit 0 ;;
  *) exit "${STUB_HOST_RC:-0}" ;;
esac
"""
# The gate asks rustup before it creates its own log file, so blocking here holds the run with the
# planted error in place and nothing of the gate's left to leak when the group is killed.
STUB_RUSTUP = r"""#!/usr/bin/env bash
printf '%s\t%s\t%s\n' "$PWD" "${CARGO_TARGET_DIR:-}" "rustup $*" >>"$STUB_LOG"
if grep -q A_VALUE_THAT_DOES_NOT_EXIST crates/assay-monitor/src/lib.rs 2>/dev/null; then
  : >"$STUB_STARTED"
  if [ "${STUB_BLOCK:-0}" = 1 ]; then sleep 600; fi
fi
[ "$*" = "target list --installed" ] && echo x86_64-unknown-linux-gnu
exit 0
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def clean_env(**extra: str) -> dict[str, str]:
    """No Git, pre-commit or cargo state from the outer run may reach a fixture."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("GIT_", "PRE_COMMIT")) and key != "CARGO_TARGET_DIR"}
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
    spec.loader.exec_module(module)
    return module


class LinuxGateTestIsolation(unittest.TestCase):
    """Runs the production scripts/ci/test-check-linux.sh from a throwaway source repository."""

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
        self.repo = make_repo(self.base, files, self.env)
        self.paths = [MONITOR, CLI_LINUX, LINUX_TEST, GATE]
        self.before = source_state(self.repo, self.paths, self.env)
        stubs = self.base / "stubs"
        stubs.mkdir()
        for name, body in (("cargo", STUB_CARGO), ("rustup", STUB_RUSTUP)):
            (stubs / name).write_text(body)
            (stubs / name).chmod(0o755)
        self.run_tmp = self.base / "run-tmp"
        self.run_tmp.mkdir()
        self.log = self.base / "cargo.log"
        self.started = self.base / "started"
        self.run_env = dict(self.env, PATH=f"{stubs}{os.pathsep}{os.environ['PATH']}", TMPDIR=str(self.run_tmp),
                            STUB_LOG=str(self.log), STUB_STARTED=str(self.started),
                            ASSAY_LINUX_TARGET_TIMEOUT="0")  # no timeout wrapper, so one process group

    def calls(self):
        return [line.split("\t") for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def run_linux_test(self, **extra: str):
        return subprocess.run(["bash", LINUX_TEST], cwd=self.repo, env=dict(self.run_env, **extra),
                              capture_output=True, text=True, timeout=120)

    def assert_source_unchanged(self, context: str):
        self.assertEqual(source_state(self.repo, self.paths, self.env), self.before, context)

    def assert_built_in_scratch(self, output: str):
        calls = self.calls()
        self.assertTrue(calls, output)
        expected_target = str((self.repo / "target/mutation-scratch").resolve())
        for cwd, target, argv in calls:
            self.assertNotEqual(Path(cwd).resolve(), self.repo.resolve(), f"cargo ran in the source: {argv}\n{output}")
            self.assertEqual(target, expected_target, f"cargo target dir for {argv}\n{output}")

    def test_normal_run_builds_in_scratch_and_leaves_the_source(self):
        result = self.run_linux_test()
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn("ok    a cfg(linux) compile error fails the gate", output)
        self.assert_source_unchanged(output)
        self.assert_built_in_scratch(output)

    def test_hook_git_environment_never_reaches_the_source(self):
        # Under a commit hook these point at the source repository; the scratch must not inherit them.
        git_dir = str(self.repo / ".git")
        result = self.run_linux_test(GIT_DIR=git_dir, GIT_INDEX_FILE=f"{git_dir}/index", GIT_WORK_TREE=str(self.repo))
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assert_source_unchanged(output)
        self.assert_built_in_scratch(output)

    def test_a_line_break_in_tmpdir_never_turns_cleanup_onto_another_path(self):
        # The helper prints one path per line. A TMPDIR holding a line break must be refused, or the
        # shell would take the part before the break for the scratch and remove that instead.
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
        environment = dict(clean_env(), TMPDIR=str(self.scratch_parent), **extra)
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
