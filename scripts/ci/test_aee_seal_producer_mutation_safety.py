#!/usr/bin/env python3
"""Source safety for the ADR-045 seal producer mutation hook.

The mutation script must never write the source tree it tests: not on a normal run, not when the
suite fails, not on SIGINT or SIGTERM, and not on SIGKILL, where no handler runs at all. Every case
runs the production entry point from a throwaway source repository with a stub `cargo`, and
requires that repository's producer bytes, mode, index entry, index file and status to be unchanged.
The real tree is only read: the residue check and the static checks.
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

ROOT = Path(__file__).resolve().parents[2]
REL_TARGET = "crates/assay-cli/src/aee_seal.rs"
REL_SCRIPT = "scripts/ci/test_aee_seal_producer_mutations.py"
MUTATION_SCRIPT = ROOT / REL_SCRIPT
RUN_BINDING = "run-binding preimage"
MUTANT_MARKER = '"subject": "x",'

STUB_CARGO = r"""#!/usr/bin/env bash
f=crates/assay-cli/src/aee_seal.rs
printf '%s\t%s\t%s\t%s\t%s\n' "$PWD" "${CARGO_TARGET_DIR:-}" "$(shasum -a 256 "$f" | cut -c1-64)" \
  "$(grep -c -- "${STUB_MARKER:-no-marker-requested}" "$f")" "${GIT_DIR:-}${GIT_INDEX_FILE:-}${GIT_WORK_TREE:-}" >>"$STUB_LOG"
: >"$STUB_STARTED"
if [ "${STUB_BLOCK:-0}" = 1 ]; then sleep 600; fi
if [ -n "${STUB_OUT:-}" ]; then cat "$STUB_OUT"; fi
exit 101
"""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_mode(path: Path) -> int:
    return stat.S_IMODE(path.stat().st_mode)


def clean_env(**extra: str) -> dict[str, str]:
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


def source_state(repo: Path, env: dict[str, str]) -> dict[str, object]:
    index = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-path", "index", env=env).strip())
    return {
        "bytes": sha256_bytes((repo / REL_TARGET).read_bytes()),
        "mode": file_mode(repo / REL_TARGET),
        "index": git(repo, "ls-files", "-s", "--", REL_TARGET, env=env),
        "index_file": sha256_bytes(index.read_bytes()),
        "status": git(repo, "status", "--porcelain", "--untracked-files=all", "--", REL_TARGET, env=env),
    }


def load_mutation_module(script: Path = MUTATION_SCRIPT):
    spec = importlib.util.spec_from_file_location("aee_seal_producer_mutations", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


REAL_STATE: dict[str, object] = {}


def setUpModule() -> None:
    REAL_STATE.update(source_state(ROOT, clean_env()))


def tearDownModule() -> None:
    after = source_state(ROOT, clean_env())
    if after != REAL_STATE:
        raise AssertionError(f"the safety suite changed the real {REL_TARGET}: {REAL_STATE!r} -> {after!r}")


class SourceIsNeverWritten(unittest.TestCase):
    """The production entry point, run from a throwaway source repository."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.env = clean_env()
        self.repo = self.base / "source"
        files = {REL_TARGET: ((ROOT / REL_TARGET).read_bytes(), 0o640),
                 REL_SCRIPT: (MUTATION_SCRIPT.read_bytes(), 0o755),
                 ".gitignore": (b"/target\n", 0o644)}
        for path in sorted((ROOT / "scripts/ci/lib").rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts:
                files[str(path.relative_to(ROOT))] = (path.read_bytes(), file_mode(path))
        for rel, (data, mode) in files.items():
            (self.repo / rel).parent.mkdir(parents=True, exist_ok=True)
            (self.repo / rel).write_bytes(data)
            os.chmod(self.repo / rel, mode)
        git(self.repo, "init", "-q", "-b", "main", env=self.env)
        git(self.repo, "add", "-A", env=self.env)
        git(self.repo, "commit", "-q", "-m", "fixture", env=self.env)
        stubs = self.base / "stubs"
        stubs.mkdir()
        (stubs / "cargo").write_text(STUB_CARGO)
        (stubs / "cargo").chmod(0o755)
        names = sorted({expect for _name, _search, _replace, expect in load_mutation_module().MUTATIONS})
        self.caught = self.base / "caught.txt"
        self.caught.write_text("test result: FAILED. 0 passed; 1 failed\n" + "\n".join(names) + "\n")
        self.run_tmp = self.base / "run-tmp"
        self.run_tmp.mkdir()
        self.log = self.base / "cargo.log"
        self.started = self.base / "started"
        self.run_env = dict(self.env, PATH=f"{stubs}{os.pathsep}{os.environ['PATH']}", TMPDIR=str(self.run_tmp),
                            STUB_LOG=str(self.log), STUB_STARTED=str(self.started), PYTHONDONTWRITEBYTECODE="1")

    def run_mutator(self, *args: str, **extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, REL_SCRIPT, *args], cwd=self.repo, env=dict(self.run_env, **extra),
                              capture_output=True, text=True, timeout=120)

    def calls(self) -> list[list[str]]:
        return [line.split("\t") for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def assert_mutants_built_in_scratch(self, source_sha: str, output: str) -> None:
        calls = self.calls()
        self.assertTrue(calls, output)
        target = str((self.repo / "target/mutation-scratch").resolve())
        for cwd, target_dir, seen, _marker, git_state in calls:
            self.assertNotEqual(Path(cwd).resolve(), self.repo.resolve(), "cargo ran in the source:\n" + output)
            self.assertEqual(git_state, "", "a hook's GIT_* environment reached cargo")
            self.assertEqual(target_dir, target, "mutant builds must not share the ordinary artifact cache")
            self.assertNotEqual(seen, source_sha, "cargo saw the unmutated producer")

    def test_normal_run_tests_every_mutant_in_scratch_and_leaves_the_source(self):
        before = source_state(self.repo, self.env)
        result = self.run_mutator(STUB_OUT=str(self.caught))
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(source_state(self.repo, self.env), before, output)
        self.assert_mutants_built_in_scratch(before["bytes"], output)
        self.assertEqual(len({call[2] for call in self.calls()}), len(self.calls()),
                         "every mutant is a distinct producer")
        self.assertEqual(sorted(self.run_tmp.iterdir()), [], "a finished run removes its scratch")

    def test_uncommitted_bytes_are_what_cargo_sees(self):
        marker = "// uncommitted-producer-probe-h9"
        (self.repo / REL_TARGET).write_bytes((self.repo / REL_TARGET).read_bytes() + f"\n{marker}\n".encode())
        before = source_state(self.repo, self.env)
        result = self.run_mutator(STUB_OUT=str(self.caught), STUB_MARKER=marker)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(source_state(self.repo, self.env), before, output)
        self.assertTrue(all(int(call[3]) >= 1 for call in self.calls()), "cargo did not see the worktree bytes")

    def test_failing_suite_leaves_the_source(self):
        before = source_state(self.repo, self.env)
        result = self.run_mutator()  # the stub reports no failure, so every mutant survives
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 1, output)
        self.assertIn("suite passed under the mutation", output)
        self.assertEqual(source_state(self.repo, self.env), before, output)
        self.assert_mutants_built_in_scratch(before["bytes"], output)

    def test_sigterm_and_sigint_leave_the_source(self):
        for signame, signum in (("SIGTERM", signal.SIGTERM), ("SIGINT", signal.SIGINT)):
            with self.subTest(signal=signame):
                before = source_state(self.repo, self.env)
                result = self.run_mutator(ASSAY_AEE_SEAL_MUTATION_INTERRUPT_AFTER=RUN_BINDING,
                                          ASSAY_AEE_SEAL_MUTATION_INTERRUPT_SIGNAL=signame)
                self.assertEqual(result.returncode, 128 + signum, result.stdout + result.stderr)
                self.assertEqual(source_state(self.repo, self.env), before)

    def test_sigkill_leaves_the_source(self):
        before = source_state(self.repo, self.env)
        process = subprocess.Popen([sys.executable, REL_SCRIPT], cwd=self.repo, env=dict(self.run_env, STUB_BLOCK="1"),
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        reached = False
        try:
            deadline = time.monotonic() + 60
            while process.poll() is None and time.monotonic() < deadline:
                if self.started.exists():
                    reached = True
                    break
                time.sleep(0.05)
        finally:
            # Stop and reap the run's own group before reading: a live pipe would block the test.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=30)
            output = process.stdout.read().decode(errors="replace")
            process.stdout.close()
        self.assertTrue(reached, "no mutant reached cargo:\n" + output)
        self.assertEqual(process.returncode, -signal.SIGKILL, output)
        self.assertEqual(source_state(self.repo, self.env), before, "SIGKILL changed the source")
        cwd, _target, seen, _marker, _git_state = self.calls()[-1]
        self.assertIn(MUTANT_MARKER, (Path(cwd) / REL_TARGET).read_text(), "positive control: a mutant was live")
        self.assertNotEqual(seen, before["bytes"])

    def test_target_is_refused_in_mutation_mode(self):
        before = source_state(self.repo, self.env)
        result = self.run_mutator("--target", REL_TARGET, STUB_OUT=str(self.caught))
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [], "a refused run must not reach cargo")
        self.assertEqual(source_state(self.repo, self.env), before)

    def test_hook_git_environment_never_reaches_the_source(self):
        before = source_state(self.repo, self.env)
        git_dir = str(self.repo / ".git")
        result = self.run_mutator(STUB_OUT=str(self.caught), GIT_DIR=git_dir, GIT_INDEX_FILE=f"{git_dir}/index",
                                  GIT_WORK_TREE=str(self.repo))
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(source_state(self.repo, self.env), before, output)
        self.assert_mutants_built_in_scratch(before["bytes"], output)


class ResidueAndStaticChecks(unittest.TestCase):
    def test_residue_check_detects_planted_mutant_read_only(self):
        planted = (ROOT / REL_TARGET).read_bytes().replace(b'"subject": env.subject_digest,', MUTANT_MARKER.encode(), 1)
        self.assertIn(MUTANT_MARKER.encode(), planted)
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / "aee_seal.rs"
            fixture.write_bytes(planted)
            probe = subprocess.run([sys.executable, str(MUTATION_SCRIPT), "--check-residue", "--target", str(fixture)],
                                   cwd=ROOT, capture_output=True, text=True)
            self.assertEqual(fixture.read_bytes(), planted, "residue check rewrote the fixture")
        self.assertNotEqual(probe.returncode, 0)
        self.assertIn(RUN_BINDING, probe.stderr)

    def test_clean_residue_check_on_the_real_tree_is_read_only(self):
        before = source_state(ROOT, clean_env())
        probe = subprocess.run([sys.executable, str(MUTATION_SCRIPT), "--check-residue"], cwd=ROOT,
                               capture_output=True, text=True)
        self.assertEqual(probe.returncode, 0, probe.stderr)
        self.assertEqual(source_state(ROOT, clean_env()), before)

    def test_atomic_write_preserves_exact_mode(self):
        module = load_mutation_module()
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / "aee_seal.rs"
            fixture.write_bytes(b"mode-preservation-probe\n")
            for mode in (0o644, 0o640):
                os.chmod(fixture, mode)
                module.write_bytes_atomic(fixture, fixture.read_bytes())
                self.assertEqual(file_mode(fixture), mode)

    def test_removing_fchmod_reopens_mode_gap(self):
        text = MUTATION_SCRIPT.read_text()
        pattern = r"(?m)^\s*os\.fchmod\(fd, mode\)\n"
        self.assertEqual(len(re.findall(pattern, text)), 1)
        with tempfile.TemporaryDirectory() as temporary:
            probe = Path(temporary) / "mutations.py"
            probe.write_text(re.sub(pattern, "", text, count=1))
            fixture = Path(temporary) / "aee_seal.rs"
            fixture.write_bytes(b"mode-gap-mutation\n")
            os.chmod(fixture, 0o644)
            load_mutation_module(probe).write_bytes_atomic(fixture, fixture.read_bytes())
            self.assertEqual(file_mode(fixture), 0o600, "the fchmod path is what keeps the mode")

    def test_precommit_trigger_excludes_config_yaml(self):
        config = (ROOT / ".pre-commit-config.yaml").read_text()
        block = re.search(r"- id: aee-seal-producer-mutations\n(?:.*\n)*?        files: (.+)\n", config)
        self.assertIsNotNone(block, "aee-seal-producer-mutations files filter missing")
        files = block.group(1)
        self.assertNotIn(r"\.pre-commit-config\.yaml", files)
        self.assertIn("aee_seal\\.rs", files)
        self.assertIn("test_aee_seal_producer_mutations\\.py", files)

    def test_no_git_restore_in_mutation_script(self):
        text = MUTATION_SCRIPT.read_text()
        for pattern in (r'\[\s*["\']git["\']\s*,\s*["\']restore["\']', r'\[\s*["\']git["\']\s*,\s*["\']checkout["\']',
                        r'os\.system\([^)]*git\s+(?:restore|checkout)'):
            self.assertIsNone(re.search(pattern, text), pattern)


if __name__ == "__main__":
    unittest.main()
