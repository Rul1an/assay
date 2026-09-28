#!/usr/bin/env python3
"""One process-group signalling rule, and every place that must follow it.

scripts/ci/lib/process_group.py owns the rule. Standalone files that cannot import
it carry a copy that must be the same function, statement for statement. Files that can
import it must not signal a process group any other way. The expected answers
below are literals owned by this test, so the rule is never its own oracle.
"""

from __future__ import annotations

import ast
import errno
import importlib.util
import itertools
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
LIB = ROOT / "scripts/ci/lib/process_group.py"
COPIES = (
    ROOT / "scripts/ci/published_release_proxy_phase.py",
    ROOT / "scripts/ci/published_release_offline_phase.py",
    ROOT / "examples/mcp-quickstart/run.py",
    ROOT / "scripts/ci/claude_plugin_install_workflow.py",
)
IMPORTERS = (
    ROOT / "scripts/ci/verify-release.sh",
    ROOT / "scripts/ci/test-verify-release.sh",
    ROOT / "scripts/ci/test-s1b-coverage-gate.sh",
)
RULE = "signal_process_group"
POSIX = hasattr(os, "killpg")


def load(path: Path):
    spec = importlib.util.spec_from_file_location(f"parity_{path.stem.replace('-', '_')}", path)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(path)
    module = importlib.util.module_from_spec(spec)
    # dataclasses resolve their module through sys.modules while the body executes.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def python_sources(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".py":
        return [text]
    bodies = re.findall(r"<<'PY'\n(.*?)\n[ \t]*PY\n", text, flags=re.DOTALL)
    if not bodies:
        raise AssertionError(f"{path.relative_to(ROOT)} has no Python heredoc")
    return bodies


def rule_definition(source: str) -> ast.FunctionDef | None:
    found = [
        node
        for node in ast.walk(ast.parse(source))
        if isinstance(node, ast.FunctionDef) and node.name == RULE
    ]
    if len(found) > 1:
        raise AssertionError(f"{RULE} is defined {len(found)} times")
    return found[0] if found else None


def group_signals_outside_rule(source: str) -> list[int]:
    """Line numbers of os.killpg / os.kill(-pgid) calls not inside the rule."""
    tree = ast.parse(source)
    inside: set[int] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef) and node.name == RULE:
            inside.update(id(child) for child in ast.walk(node))
    lines = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call) or id(node) in inside:
            continue
        func = node.func
        if not (isinstance(func, ast.Attribute) and isinstance(func.value, ast.Name)
                and func.value.id == "os"):
            continue
        negative_pid = (func.attr == "kill" and node.args
                        and isinstance(node.args[0], ast.UnaryOp)
                        and isinstance(node.args[0].op, ast.USub))
        if func.attr == "killpg" or negative_pid:
            lines.append(node.lineno)
    return lines


def rule_calls(source: str) -> int:
    return sum(
        1
        for node in ast.walk(ast.parse(source))
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == RULE
    )


def eperm() -> PermissionError:
    return PermissionError(errno.EPERM, "Operation not permitted")


def esrch() -> ProcessLookupError:
    return ProcessLookupError(errno.ESRCH, "No such process")


# (case, killpg answers in order, expected outcome, killpg calls, leader polls)
# None means the signal landed. The clock advances 0.4s per read against the 1.0s
# default window, so a third EPERM is the first one past the deadline.
TABLE = (
    ("signal lands", [None], None, 1, 0),
    ("group already absent", [esrch], None, 1, 0),
    ("orphans reaped by launchd mid-drain", [eperm, eperm, esrch], None, 3, 2),
    ("reaped leader lets the signal land", [eperm, None], None, 2, 1),
    ("member stays unsignalable", [eperm, eperm, eperm], PermissionError, 3, 3),
    ("other kernel refusal is not retried",
     [lambda: OSError(errno.EINVAL, "Invalid argument")], OSError, 1, 0),
)


class RuleTableTests(unittest.TestCase):
    def check(self, module, case, answers, expected, calls, polls, with_leader) -> None:
        def killpg(pgid, signum):
            self.assertEqual((pgid, signum), (4242, signal.SIGKILL))
            answer = answers[killpg.calls]
            killpg.calls += 1
            if answer is not None:
                raise answer()

        killpg.calls = 0
        leader = mock.Mock(spec=["poll"]) if with_leader else None
        with (
            mock.patch.object(module.os, "killpg", killpg, create=True),
            mock.patch.object(module.time, "monotonic", side_effect=itertools.count(0.0, 0.4)),
            mock.patch.object(module.time, "sleep"),
        ):
            if expected is None:
                getattr(module, RULE)(4242, signal.SIGKILL, leader)
            else:
                with self.assertRaises(expected) as raised:
                    getattr(module, RULE)(4242, signal.SIGKILL, leader)
                self.assertIs(type(raised.exception), expected, case)
        self.assertEqual(killpg.calls, calls, case)
        if leader is not None:
            self.assertEqual(leader.poll.call_count, polls, case)

    def test_every_implementation_answers_the_table(self) -> None:
        for path in (LIB, *COPIES):
            module = load(path)
            for case, answers, expected, calls, polls in TABLE:
                for with_leader in (True, False):
                    with self.subTest(path=str(path.relative_to(ROOT)), case=case, leader=with_leader):
                        self.check(module, case, answers, expected, calls, polls, with_leader)


class ParityTests(unittest.TestCase):
    def test_copies_are_the_rule_statement_for_statement(self) -> None:
        rule = ast.dump(rule_definition(LIB.read_text(encoding="utf-8")))
        for path in COPIES:
            with self.subTest(path=str(path.relative_to(ROOT))):
                copy = rule_definition(path.read_text(encoding="utf-8"))
                self.assertIsNotNone(copy, f"{path.relative_to(ROOT)} lost its copy of {RULE}")
                self.assertEqual(ast.dump(copy), rule, "copy drifted from lib/process_group.py")

    def test_no_group_signal_bypasses_the_rule(self) -> None:
        for path in (*COPIES, *IMPORTERS):
            with self.subTest(path=str(path.relative_to(ROOT))):
                sources = python_sources(path)
                bypasses = [line for source in sources for line in group_signals_outside_rule(source)]
                self.assertEqual(bypasses, [], "group signal outside signal_process_group")
                self.assertGreater(sum(rule_calls(source) for source in sources), 0,
                                   f"{path.relative_to(ROOT)} no longer calls {RULE}")

    def test_importers_do_not_carry_a_copy(self) -> None:
        for path in IMPORTERS:
            with self.subTest(path=str(path.relative_to(ROOT))):
                for source in python_sources(path):
                    self.assertIsNone(rule_definition(source), "importer defines its own rule")


def wait_until_exited_unreaped(process: subprocess.Popen) -> None:
    deadline = time.monotonic() + 10
    while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
        if time.monotonic() >= deadline:
            raise AssertionError("leader did not exit")
        time.sleep(0.01)


@unittest.skipUnless(POSIX, "process groups are POSIX")
class KernelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.rule = getattr(load(LIB), RULE)

    def test_unreaped_leader_is_reaped_then_proven_absent(self) -> None:
        # State A: only this process can clear its zombie leader.
        leader = subprocess.Popen(["/bin/sh", "-c", "exit 7"], start_new_session=True)
        wait_until_exited_unreaped(leader)
        self.rule(leader.pid, signal.SIGKILL, leader)
        self.assertEqual(leader.returncode, 7)

    def test_unreaped_leader_without_its_handle_is_not_a_clean_stop(self) -> None:
        leader = subprocess.Popen(["/bin/sh", "-c", "exit 0"], start_new_session=True)
        try:
            wait_until_exited_unreaped(leader)
            if sys.platform == "darwin":
                with self.assertRaises(PermissionError):
                    self.rule(leader.pid, signal.SIGKILL, None, 0.2)
            else:
                # Linux signals a zombie-only group without error.
                self.rule(leader.pid, signal.SIGKILL, None, 0.2)
        finally:
            leader.wait()

    def test_orphaned_descendants_are_drained_after_the_leader_is_reaped(self) -> None:
        # State B, the proxy-phase timeout: TERM, reap the leader, then sweep with KILL
        # while the orphaned grandchild may still be a zombie awaiting launchd.
        for _ in range(10):
            leader = subprocess.Popen(
                ["/bin/sh", "-c", "sleep 30 & sleep 30; wait"], start_new_session=True
            )
            time.sleep(0.05)
            self.rule(leader.pid, signal.SIGTERM, leader)
            leader.wait(timeout=5)
            self.rule(leader.pid, signal.SIGKILL, leader)
            with self.assertRaises(ProcessLookupError):
                deadline = time.monotonic() + 2
                while True:
                    try:
                        os.killpg(leader.pid, 0)
                    except PermissionError:
                        pass
                    if time.monotonic() >= deadline:
                        raise AssertionError("process group outlived the sweep")
                    time.sleep(0.01)


if __name__ == "__main__":
    unittest.main()
