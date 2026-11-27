#!/usr/bin/env python3
"""Pre-push deadlines stop the command's own process group, with or without timeout(1), and say so.

scripts/precommit/run-with-timeout.py (POSIX) runs a hook's command under a deadline. At the
deadline it sends SIGTERM to the command's process group, sends SIGKILL to that group after a short
grace, and exits 124; a cleanup that fails exits 125 and is bounded. It needs no timeout(1) or
gtimeout(1). GNU timeout was not enough: when the first process exits on SIGTERM, `timeout -k`
reports 124 and never sends SIGKILL, so a descendant that ignores SIGTERM keeps running. A
descendant that leaves the group (setsid) is outside what these tests or the wrapper cover.

cargo-clippy.sh and check-linux.sh use it for every deadline above 0, and run the command
directly, in the hook's own process group, when the deadline is 0. check-linux.sh no longer
installs the Linux target during a push, and asks rustup under the same deadline.

Every process these tests start is synthetic (a Python sleeper, stub cargo, stub rustup) and
records its pid. A deadline case passes only when those pids are gone afterwards; tearDown kills
any that are not. No real cargo, rustup or network is used.
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]
WRAPPER = ROOT / "scripts/precommit/run-with-timeout.py"
LIB = ROOT / "scripts/ci/lib/process_group.py"
CLIPPY = ROOT / "scripts/precommit/cargo-clippy.sh"
LINUX = ROOT / "scripts/ci/check-linux.sh"
CLASSIFIER = ROOT / "scripts/ci/classify-lightweight-changes.sh"
TARGET = "x86_64-unknown-linux-gnu"
GRACE = "0.3"
# Every run gets this outer bound, so a hang is a failed assertion, never a hung test.
OUTER = 15.0
# The tools the two hooks and their stubs need, for a PATH that has no timeout or gtimeout.
FARM_TOOLS = ("bash", "sh", "env", "git", "grep", "sed", "awk", "cut", "sort", "tr", "head",
              "tail", "cat", "mktemp", "rm", "mkdir", "dirname", "basename", "wc", "ps", "sleep",
              "ls", "cp", "chmod", "touch", "date")

# A leader with one descendant. Each records its pid before the deadline can matter: the leader
# waits until the child has written its line. "obey" keeps SIGTERM's default action.
TREE = r'''
import os, signal, subprocess, sys, time
pidfile, leader, child = sys.argv[1], sys.argv[2], sys.argv[3]
text = sys.argv[4] if len(sys.argv) > 4 else ""
if leader.startswith("exit:"):
    sys.exit(int(leader[5:]))
if leader == "selfkill":
    os.kill(os.getpid(), signal.SIGKILL)
if leader == "ignore":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
with open(pidfile, "a") as fh:
    fh.write(f"leader {os.getpid()}\n")
if child != "none":
    code = ("import os, signal, sys, time\n"
            + ("signal.signal(signal.SIGTERM, signal.SIG_IGN)\n" if child == "ignore" else "")
            + "open(sys.argv[1], 'a').write(f'child {os.getpid()}\\n')\n"
            + "time.sleep(60)\n")
    subprocess.Popen([sys.executable, "-c", code, pidfile])
    deadline = time.monotonic() + 5
    while "child " not in open(pidfile).read() and time.monotonic() < deadline:
        time.sleep(0.01)
if text:
    print(text, flush=True)
time.sleep(60)
'''

STUB_CARGO = r'''#!/usr/bin/env bash
echo "cargo $* pgid=$(ps -o pgid= -p $$ | tr -d ' ')" >>"$STUB_LOG"
mode="${STUB_MODE:-ok}"
if [ "${1:-}" = check ] && [ "${3:-}" != "${STUB_TREE_CRATE:-}" ]; then mode=ok; fi
case "$mode" in
  ok) exit 0 ;;
  fail) echo "error: stub clippy found a problem"; exit 101 ;;
  tree) exec "$STUB_PYTHON" "$STUB_TREE" "$STUB_PIDS" "${STUB_LEADER:-obey}" "${STUB_CHILD:-ignore}" "${STUB_PRINT:-}" ;;
esac
echo "stub cargo: unknown mode $mode" >&2
exit 97
'''

STUB_RUSTUP = r'''#!/usr/bin/env bash
echo "rustup $*" >>"$STUB_LOG"
case "$*" in
  "target list --installed")
    case "${STUB_RUSTUP:-installed}" in
      installed) echo x86_64-unknown-linux-gnu; exit 0 ;;
      missing) exit 0 ;;
      tree) exec "$STUB_PYTHON" "$STUB_TREE" "$STUB_PIDS" "${STUB_LEADER:-obey}" "${STUB_CHILD:-ignore}" ;;
    esac ;;
  "target add "*) exit 0 ;;
esac
echo "stub rustup: unexpected arguments: $*" >&2
exit 99
'''


# Loads the wrapper as a module and makes its group signal fail, synthetically: no permission on
# any real process changes. deny-kill fails only SIGKILL; deny-all fails every group signal.
FAULT_DRIVER = r'''
import importlib.util, signal, sys
spec = importlib.util.spec_from_file_location("wrapper_under_test", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
real, mode = module.signal_process_group, sys.argv[2]
def injected(pgid, signum, leader=None, drain_seconds=1.0):
    if mode == "deny-all" or (mode == "deny-kill" and signum == signal.SIGKILL):
        raise PermissionError(1, "injected EPERM")
    return real(pgid, signum, leader, drain_seconds)
module.signal_process_group = injected
sys.exit(module.main(sys.argv[3:]))
'''

# A stand-in for run-with-timeout.py that reports a failed cleanup (status 125) for a command
# whose first word is STUB_CLEANUP_FAILS, and runs every other command unchanged.
FAILED_CLEANUP_WRAPPER = r'''
import os, subprocess, sys
command = sys.argv[2:]
if command and os.path.basename(command[0]) == os.environ.get("STUB_CLEANUP_FAILS"):
    print("run-with-timeout: injected cleanup failure", file=sys.stderr)
    sys.exit(125)
sys.exit(subprocess.call(command))
'''


def clean_env(**extra: str) -> dict[str, str]:
    """No Git, pre-commit or cargo state from the caller may reach a fixture."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("GIT_", "PRE_COMMIT", "CLIPPY_", "ASSAY_", "RUN_WITH_TIMEOUT"))
           and key not in ("CARGO_TARGET_DIR", "CARGO_BUILD_BUILD_DIR")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_NAME="fixture", GIT_AUTHOR_EMAIL="fixture@example.invalid",
               GIT_COMMITTER_NAME="fixture", GIT_COMMITTER_EMAIL="fixture@example.invalid",
               RUN_WITH_TIMEOUT_GRACE=GRACE)
    env.update(extra)
    return env


def alive(pid: int) -> bool:
    """True while the pid is a live process; a zombie awaiting its reaper counts as gone."""
    state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True,
                           text=True).stdout.strip()
    return bool(state) and not state.startswith("Z")


class Case(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="assay-prepush-timeout."))
        self.pidfile = self.tmp / "pids"
        self.pidfile.touch()
        self.tree = self.tmp / "tree.py"
        self.tree.write_text(TREE, encoding="utf-8")
        self.extra_pids: list[int] = []

    def tearDown(self) -> None:
        for pid in self.pids() + self.extra_pids:
            if alive(pid):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        shutil.rmtree(self.tmp, ignore_errors=True)

    def pids(self) -> list[int]:
        return [int(line.split()[1]) for line in self.pidfile.read_text().splitlines() if line.strip()]

    def assert_tree_gone(self, roles: tuple[str, ...] = ("leader", "child")) -> None:
        recorded = {line.split()[0]: int(line.split()[1])
                    for line in self.pidfile.read_text().splitlines() if line.strip()}
        for role in roles:
            self.assertIn(role, recorded, f"the synthetic {role} never started")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and any(alive(pid) for pid in recorded.values()):
            time.sleep(0.05)
        left = {role: pid for role, pid in recorded.items() if alive(pid)}
        self.assertEqual(left, {}, "own synthetic processes outlived the deadline")

    def run_bounded(self, cmd: list[str], env: dict[str, str], cwd: Path | None = None
                    ) -> tuple[int | None, str, float]:
        """Run cmd in its own session under OUTER. A run that outlives OUTER returns None."""
        out_path = self.tmp / f"out-{time.monotonic_ns()}"
        start = time.monotonic()
        with open(out_path, "wb") as out:
            process = subprocess.Popen(cmd, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=out,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            try:
                rc: int | None = process.wait(timeout=OUTER)
            except subprocess.TimeoutExpired:
                rc = None
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
        self.hook_pid = process.pid
        return rc, out_path.read_text(errors="replace"), time.monotonic() - start

    def farm(self, without: tuple[str, ...] = ()) -> Path:
        """A PATH directory with the tools the hooks need and neither timeout nor gtimeout."""
        farm = self.tmp / "farm"
        farm.mkdir(exist_ok=True)
        tools = FARM_TOOLS + (() if "python3" in without else ("python3",))
        for tool in tools:
            if tool in without:
                continue
            found = shutil.which(tool)
            self.assertIsNotNone(found, f"the farm needs {tool}")
            link = farm / tool
            if not link.exists():
                link.symlink_to(found)
        for absent in ("timeout", "gtimeout", *without):
            self.assertIsNone(shutil.which(absent, path=str(farm)), f"{absent} leaked into the farm")
        return farm


class WrapperTest(Case):
    """The wrapper on its own, with synthetic commands."""

    def wrap(self, deadline: str, *command: str, wrapper: Path = WRAPPER, path: str | None = None
             ) -> tuple[int | None, str, float]:
        env = clean_env() if path is None else clean_env(PATH=path)
        self.assertTrue(wrapper.is_file(), f"{wrapper} does not exist")
        return self.run_bounded([sys.executable, str(wrapper), deadline, *command], env)

    def tree_cmd(self, leader: str, child: str, text: str = "") -> list[str]:
        return [sys.executable, str(self.tree), str(self.pidfile), leader, child, text]

    def test_exit_codes_pass_through(self) -> None:
        for leader, want in (("exit:0", 0), ("exit:3", 3), ("selfkill", 128 + signal.SIGKILL)):
            rc, out, _ = self.wrap("5", *self.tree_cmd(leader, "none"))
            self.assertEqual(rc, want, f"{leader}: {out}")

    def test_output_before_the_deadline_is_streamed(self) -> None:
        rc, out, elapsed = self.wrap("1", *self.tree_cmd("obey", "none", "partial-line-before-hang"))
        self.assertEqual(rc, 124, out)
        self.assertIn("partial-line-before-hang", out)
        self.assertIn("deadline", out)
        self.assertLess(elapsed, 1 + float(GRACE) + 3)
        self.assert_tree_gone(("leader",))

    def test_leader_obeys_term_descendant_ignores_it(self) -> None:
        rc, out, elapsed = self.wrap("1", *self.tree_cmd("obey", "ignore"))
        self.assertEqual(rc, 124, out)
        self.assertLess(elapsed, 1 + float(GRACE) + 3)
        self.assert_tree_gone()

    def test_leader_and_descendant_both_ignore_term(self) -> None:
        rc, out, elapsed = self.wrap("1", *self.tree_cmd("ignore", "ignore"))
        self.assertEqual(rc, 124, out)
        self.assertLess(elapsed, 1 + float(GRACE) + 3)
        self.assert_tree_gone()

    def test_deadline_holds_without_timeout_or_gtimeout(self) -> None:
        empty = self.tmp / "empty-path"
        empty.mkdir()
        for tool in ("timeout", "gtimeout"):
            self.assertIsNone(shutil.which(tool, path=str(empty)))
        rc, out, _ = self.wrap("1", *self.tree_cmd("obey", "ignore"), path=str(empty))
        self.assertEqual(rc, 124, out)
        self.assert_tree_gone()

    def test_invalid_deadlines_are_refused_before_the_command_starts(self) -> None:
        witness = self.tmp / "started"
        for value in ("0", "-1", "abc", "", "nan", "inf", "1e999"):
            rc, out, _ = self.wrap(value, sys.executable, "-c", f"open({str(witness)!r}, 'w')")
            self.assertEqual(rc, 2, f"{value!r}: {out}")
            self.assertFalse(witness.exists(), f"{value!r}: the command ran")

    def inject(self, mode: str, deadline: str, *command: str) -> tuple[int | None, str, float]:
        driver = self.tmp / "fault_driver.py"
        driver.write_text(FAULT_DRIVER, encoding="utf-8")
        return self.run_bounded([sys.executable, str(driver), str(WRAPPER), mode, deadline, *command],
                                clean_env())

    def test_a_failed_cleanup_after_a_normal_exit_is_not_clean(self) -> None:
        rc, out, _ = self.inject("deny-kill", "5", *self.tree_cmd("exit:0", "none"))
        self.assertEqual(rc, 125, out)
        self.assertIn("injected EPERM", out)

    def test_a_failed_stop_at_the_deadline_is_bounded_and_not_clean(self) -> None:
        # Every group signal fails, so the TERM-ignoring leader survives; tearDown stops it.
        rc, out, elapsed = self.inject("deny-all", "1", *self.tree_cmd("ignore", "none"))
        self.assertEqual(rc, 125, out)
        self.assertLess(elapsed, 1 + 2 * float(GRACE) + 3)
        self.assertIn("injected EPERM", out)
        self.assertIn("may still be running", out)

    def test_missing_command_is_127(self) -> None:
        rc, out, _ = self.wrap("5", str(self.tmp / "no-such-command"))
        self.assertEqual(rc, 127, out)

    def test_term_to_the_wrapper_stops_the_command(self) -> None:
        out_path = self.tmp / "out-term"
        with open(out_path, "wb") as out:
            process = subprocess.Popen([sys.executable, str(WRAPPER), "30", *self.tree_cmd("obey", "ignore")],
                                       env=clean_env(), stdout=out, stderr=subprocess.STDOUT,
                                       start_new_session=True)
            self.extra_pids.append(process.pid)
            self._wait_for_child()
            process.send_signal(signal.SIGTERM)
            rc = process.wait(timeout=OUTER)
        self.assertEqual(rc, 128 + signal.SIGTERM, out_path.read_text())
        self.assert_tree_gone()

    def test_caller_killed_with_its_group_stops_the_command(self) -> None:
        # The wrapper leaves its caller's process group, so a group SIGKILL aimed at the caller
        # (a supervisor stopping a push) cannot kill the wrapper and orphan the command; the
        # wrapper sees it lost its parent and stops the command.
        wpid = self.tmp / "wrapper-pid"
        script = (f'"{sys.executable}" "{WRAPPER}" 30 "{sys.executable}" "{self.tree}" '
                  f'"{self.pidfile}" obey ignore & echo $! > "{wpid}"; wait')
        caller = subprocess.Popen(["/bin/sh", "-c", script], env=clean_env(), stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL, start_new_session=True)
        self._wait_for_child()
        self.extra_pids.append(int(wpid.read_text()))
        os.killpg(caller.pid, signal.SIGKILL)
        caller.wait()
        self.assert_tree_gone()
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and alive(int(wpid.read_text())):
            time.sleep(0.05)
        self.assertFalse(alive(int(wpid.read_text())), "the wrapper outlived its caller")

    def test_a_mutant_that_signals_only_the_direct_child_is_caught(self) -> None:
        # Positive control for the pid witness: a wrapper that stops only its first process must
        # leave the TERM-ignoring descendant alive, and the witness must see that.
        source = WRAPPER.read_text(encoding="utf-8")
        anchor = "    signal_process_group(process.pid, signum, process)\n"
        self.assertEqual(source.count(anchor), 1, "the group-signal anchor moved; update the mutant")
        mutant_root = self.tmp / "mutant"
        (mutant_root / "scripts/precommit").mkdir(parents=True)
        (mutant_root / "scripts/ci/lib").mkdir(parents=True)
        mutant = mutant_root / "scripts/precommit/run-with-timeout.py"
        mutant.write_text(source.replace(anchor, "    process.send_signal(signum)\n"), encoding="utf-8")
        shutil.copy(LIB, mutant_root / "scripts/ci/lib/process_group.py")
        rc, out, _ = self.wrap("1", *self.tree_cmd("obey", "ignore"), wrapper=mutant)
        self.assertEqual(rc, 124, out)
        child = [int(line.split()[1]) for line in self.pidfile.read_text().splitlines()
                 if line.startswith("child ")]
        self.assertEqual(len(child), 1)
        time.sleep(float(GRACE) + 0.5)
        self.assertTrue(alive(child[0]), "the direct-child mutant was not caught by the pid witness")

    def _wait_for_child(self) -> None:
        deadline = time.monotonic() + 5
        while "child " not in self.pidfile.read_text() and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertIn("child ", self.pidfile.read_text(), "the synthetic child never started")


class HookCase(Case):
    def stubs(self) -> Path:
        stub_bin = self.tmp / "stub-bin"
        stub_bin.mkdir()
        for name, body in (("cargo", STUB_CARGO), ("rustup", STUB_RUSTUP)):
            (stub_bin / name).write_text(body, encoding="utf-8")
            (stub_bin / name).chmod(0o755)
        self.stub_log = self.tmp / "stub.log"
        self.stub_log.touch()
        return stub_bin

    def stub_env(self, stub_bin: Path, path_tail: str, **extra: str) -> dict[str, str]:
        env = clean_env(PATH=f"{stub_bin}{os.pathsep}{path_tail}", STUB_LOG=str(self.stub_log),
                        STUB_PYTHON=sys.executable, STUB_TREE=str(self.tree),
                        STUB_PIDS=str(self.pidfile), **extra)
        # cargo-clippy.sh puts $CARGO_HOME/bin first; it must hold the stub, never a real cargo.
        cargo_home_bin = Path(env.get("CARGO_HOME", str(Path.home() / ".cargo"))) / "bin"
        effective = f"{cargo_home_bin}{os.pathsep}{env['PATH']}"
        for tool in ("cargo", "rustup"):
            resolved = shutil.which(tool, path=effective)
            if resolved is None or Path(resolved).parent not in (stub_bin, cargo_home_bin) or \
                    Path(resolved).read_text(errors="replace") not in (STUB_CARGO, STUB_RUSTUP):
                raise AssertionError(f"{tool} resolves to {resolved}, not a stub; refusing to run")
        return env

    def copy_into(self, repo: Path, *paths: Path) -> None:
        for path in paths:
            if path.exists():
                dest = repo / path.relative_to(ROOT)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, dest)

    def git(self, repo: Path, *args: str) -> str:
        return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True,
                              text=True, env=clean_env()).stdout.strip()

    def calls(self, prefix: str) -> list[str]:
        return [line for line in self.stub_log.read_text().splitlines() if line.startswith(prefix)]


class ClippyHookTest(HookCase):
    """The real cargo-clippy.sh, with stub cargo, on a diff that is not lightweight."""

    def setUp(self) -> None:
        super().setUp()
        self.repo = self.tmp / "repo"
        self.repo.mkdir()
        self.copy_into(self.repo, CLIPPY, WRAPPER, CLASSIFIER, LIB)
        (self.repo / "crates/demo/src").mkdir(parents=True)
        (self.repo / "crates/demo/src/lib.rs").write_text("pub fn a() {}\n")
        self.git(self.repo, "init", "-q")
        self.git(self.repo, "add", "-A")
        self.git(self.repo, "commit", "-q", "-m", "base")
        self.base = self.git(self.repo, "rev-parse", "HEAD")
        (self.repo / "crates/demo/src/lib.rs").write_text("pub fn b() {}\n")
        self.git(self.repo, "commit", "-q", "-am", "rust change")
        self.stub_bin = self.stubs()
        self.cargo_home = self.tmp / "cargo-home"
        (self.cargo_home / "bin").mkdir(parents=True)
        for name in ("cargo", "rustup"):
            shutil.copy2(self.stub_bin / name, self.cargo_home / "bin" / name)

    def hook(self, timeout: str, path_tail: str | None = None, **extra: str
             ) -> tuple[int | None, str, float]:
        env = self.stub_env(self.stub_bin, os.environ["PATH"] if path_tail is None else path_tail,
                            CARGO_HOME=str(self.cargo_home), ASSAY_PREPUSH_BASE_REF=self.base,
                            CARGO_TARGET_DIR=str(self.tmp / "target"), CLIPPY_TIMEOUT=timeout, **extra)
        return self.run_bounded(["bash", str(self.repo / "scripts/precommit/cargo-clippy.sh")], env,
                                cwd=self.repo)

    def test_success_and_failure_codes_are_unchanged(self) -> None:
        for mode, want in (("ok", 0), ("fail", 101)):
            rc, out, _ = self.hook("5", STUB_MODE=mode)
            self.assertEqual(rc, want, f"{mode}: {out}")

    def test_hang_with_a_term_ignoring_descendant_is_stopped(self) -> None:
        rc, out, _ = self.hook("1", STUB_MODE="tree", STUB_PRINT="clippy-partial-output")
        self.assertEqual(rc, 124, out)
        self.assertIn("timed out after 1s", out)
        self.assertIn("clippy-partial-output", out)
        self.assert_tree_gone()

    def test_deadline_holds_without_timeout_or_gtimeout(self) -> None:
        rc, out, _ = self.hook("1", path_tail=str(self.farm()), STUB_MODE="tree")
        self.assertEqual(rc, 124, out)
        self.assert_tree_gone()

    def test_zero_runs_clippy_in_the_hooks_own_process_group(self) -> None:
        rc, out, _ = self.hook("0", path_tail=str(self.farm(without=("python3",))))
        self.assertEqual(rc, 0, out)
        cargo = self.calls("cargo clippy")
        self.assertEqual(len(cargo), 1, self.stub_log.read_text())
        self.assertTrue(cargo[0].endswith(f"pgid={self.hook_pid}"), cargo[0])

    def test_invalid_values_are_refused_before_cargo(self) -> None:
        for value in ("abc", "-1", "1.5", "5s", "18446744073709551616"):
            self.stub_log.write_text("")
            rc, out, _ = self.hook(value)
            self.assertEqual(rc, 2, f"{value!r}: {out}")
            self.assertIn("CLIPPY_TIMEOUT", out)
            self.assertEqual(self.calls("cargo"), [], f"{value!r}: cargo ran")

    def test_a_failed_cleanup_fails_the_hook_visibly(self) -> None:
        (self.repo / "scripts/precommit/run-with-timeout.py").write_text(FAILED_CLEANUP_WRAPPER)
        rc, out, _ = self.hook("5", STUB_CLEANUP_FAILS="bash")
        self.assertEqual(rc, 125, out)
        self.assertIn("could not stop", out)

    def test_a_deadline_without_python3_is_refused_before_cargo(self) -> None:
        rc, out, _ = self.hook("5", path_tail=str(self.farm(without=("python3",))))
        self.assertEqual(rc, 2, out)
        self.assertIn("CLIPPY_TIMEOUT=0", out)
        self.assertEqual(self.calls("cargo"), [])


class LinuxHookTest(HookCase):
    """The real check-linux.sh, with stub cargo and stub rustup."""

    def setUp(self) -> None:
        super().setUp()
        self.repo = self.tmp / "repo"
        (self.repo / "crates/assay-alpha/src").mkdir(parents=True)
        (self.repo / "crates/assay-alpha/src/lib.rs").write_text('#[cfg(target_os = "linux")]\nfn a() {}\n')
        self.copy_into(self.repo, LINUX, WRAPPER, LIB)
        self.git(self.repo, "init", "-q")
        self.stub_bin = self.stubs()

    def gate(self, timeout: str, path_tail: str | None = None, **extra: str
             ) -> tuple[int | None, str, float]:
        extra.setdefault("ASSAY_LINUX_CHECK_REQUIRE_FULL", "0")
        env = self.stub_env(self.stub_bin, os.environ["PATH"] if path_tail is None else path_tail,
                            CARGO_HOME=str(self.tmp / "no-cargo-home"),
                            ASSAY_LINUX_TARGET_TIMEOUT=timeout, **extra)
        return self.run_bounded(["bash", str(self.repo / "scripts/ci/check-linux.sh")], env, cwd=self.repo)

    def test_missing_target_is_not_installed_during_the_push(self) -> None:
        for strict, want in (("0", 0), ("1", 1)):
            self.stub_log.write_text("")
            rc, out, _ = self.gate("5", STUB_RUSTUP="missing", ASSAY_LINUX_CHECK_REQUIRE_FULL=strict)
            self.assertEqual(rc, want, out)
            self.assertIn(f"rustup target add {TARGET}", out)
            self.assertIn("Nothing was checked", out)
            self.assertEqual(self.calls("rustup target add"), [], "the hook installed the target")
            self.assertEqual(self.calls("cargo"), [])

    def test_a_hanging_rustup_is_bounded(self) -> None:
        for strict, want in (("0", 0), ("1", 1)):
            self.pidfile.write_text("")
            rc, out, _ = self.gate("1", STUB_RUSTUP="tree", ASSAY_LINUX_CHECK_REQUIRE_FULL=strict)
            self.assertEqual(rc, want, out)
            self.assertIn("did not answer within 1s", out)
            self.assert_tree_gone()

    def test_a_hanging_crate_is_unchecked_and_its_tree_stopped(self) -> None:
        rc, out, _ = self.gate("1", STUB_MODE="tree", STUB_TREE_CRATE="assay-alpha")
        self.assertEqual(rc, 0, out)
        self.assertIn("SKIP assay-alpha -- could not be checked: timed out after 1s", out)
        self.assertIn("1 ok, 0 broken, 1 unchecked", out)
        self.assert_tree_gone()

    def test_strict_mode_refuses_the_timed_out_crate(self) -> None:
        rc, out, _ = self.gate("1", STUB_MODE="tree", STUB_TREE_CRATE="assay-alpha",
                               ASSAY_LINUX_CHECK_REQUIRE_FULL="1")
        self.assertEqual(rc, 1, out)
        self.assertIn("coverage is incomplete", out)
        self.assert_tree_gone()

    def test_deadline_holds_without_timeout_or_gtimeout(self) -> None:
        rc, out, _ = self.gate("1", path_tail=str(self.farm()), STUB_MODE="tree",
                               STUB_TREE_CRATE="assay-alpha")
        self.assertEqual(rc, 0, out)
        self.assertIn("timed out after 1s", out)
        self.assert_tree_gone()

    def test_zero_runs_cargo_in_the_hooks_own_process_group(self) -> None:
        rc, out, _ = self.gate("0", path_tail=str(self.farm(without=("python3",))))
        self.assertEqual(rc, 0, out)
        cargo = self.calls("cargo check")
        self.assertEqual(len(cargo), 2, self.stub_log.read_text())
        for call in cargo:
            self.assertTrue(call.endswith(f"pgid={self.hook_pid}"), call)

    def test_invalid_values_are_refused_before_rustup(self) -> None:
        for value in ("abc", "-1", "1.5", "5s", "18446744073709551616"):
            self.stub_log.write_text("")
            rc, out, _ = self.gate(value)
            self.assertEqual(rc, 2, f"{value!r}: {out}")
            self.assertIn("ASSAY_LINUX_TARGET_TIMEOUT", out)
            self.assertEqual(self.stub_log.read_text(), "", f"{value!r}: a stub ran")

    def test_a_failed_cleanup_fails_the_gate(self) -> None:
        for first_word in ("rustup", "cargo"):
            self.stub_log.write_text("")
            (self.repo / "scripts/precommit/run-with-timeout.py").write_text(FAILED_CLEANUP_WRAPPER)
            rc, out, _ = self.gate("5", STUB_CLEANUP_FAILS=first_word)
            self.assertEqual(rc, 1, f"{first_word}: {out}")
            self.assertIn("could not stop", out)

    def test_a_deadline_without_python3_checks_nothing_and_says_why(self) -> None:
        rc, out, _ = self.gate("5", path_tail=str(self.farm(without=("python3",))))
        self.assertEqual(rc, 0, out)
        self.assertIn("cannot enforce", out)
        self.assertEqual(self.calls("cargo"), [])


if __name__ == "__main__":
    unittest.main()
