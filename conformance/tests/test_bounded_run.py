#!/usr/bin/env python3
"""Behavioural tests for the shared child-output ceiling. Standard library only.

A fast child can finish a write far above the cap between 250 ms polls when
stdout/stderr land on TemporaryFile objects. These tests require continuous
pipe drains, one combined counter, and a process-group kill that still works
after the leader has exited. On Windows this helper kills only the direct
child; it makes no process-tree claim there.
Source-string mutations are supplementary; the cases above are the contract.
"""

from __future__ import annotations

import os
import hashlib
import json
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import bounded_run as br  # noqa: E402
import run_all  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE = (REPO_ROOT / "bounded_run.py").read_text(encoding="utf-8")
PIN_PATH = REPO_ROOT / "bounded_run.upstream.json"
UPSTREAM_COPY = (
    REPO_ROOT
    / "upstream/corpus-adequacy/08e3b3f013c72f016131b32dfae03f749e432228"
    / "bounded_run.py"
)
UPSTREAM_LICENSE = UPSTREAM_COPY.with_name("LICENSE")
UPSTREAM_WINDOWS_DOC = (
    "    Windows: process/batch already refuse where fcntl is missing, and this\n"
    "    helper kills only the direct child. That is not a process-tree claim.\n"
)
ASSAY_WINDOWS_DOC = (
    "    On Windows this helper kills only the direct child. That is not a\n"
    "    process-tree claim.\n"
)
TEST_CAP = 64 * 1024
BURST = 256 * 1024
# Each stream stays under TEST_CAP; together they cross it.
INTERLEAVE_EACH = 40 * 1024
# One completed crossing write, then hold: a larger write is still in
# progress when the parent closes the pipe, and SIGPIPE kills the
# descendant even without a group kill.
HOLD = TEST_CAP + 64 * 1024
POSIX = hasattr(os, "killpg") and hasattr(os, "setsid")
DESCENDANT = """\
import os, signal, sys, time
from pathlib import Path
signal.signal(signal.SIGHUP, signal.SIG_IGN)
pid_path = Path(sys.argv[1])
n = int(sys.argv[2])
if os.fork() == 0:
    pid_path.write_text(str(os.getpid()))
    sys.stdout.buffer.write(b'x' * n)
    sys.stdout.buffer.flush()
    time.sleep(30)
    os._exit(0)
for _ in range(200):
    if pid_path.exists() and pid_path.stat().st_size:
        break
    time.sleep(0.01)
os._exit(0)
"""


class _CountPipe:
    def __init__(self, raw, counter):
        self._raw = raw
        self._counter = counter

    def read(self, n=-1):
        data = self._raw.read(n)
        self._counter[0] += len(data)
        return data

    def close(self):
        return self._raw.close()

    def __getattr__(self, name):
        return getattr(self._raw, name)


def _probe(tmp: Path, body: str, *args: str) -> list[str]:
    script = tmp / "probe.py"
    script.write_text(body, encoding="utf-8")
    return [sys.executable, str(script), *args]


def _pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def _reap_pid(pid: int) -> None:
    try:
        os.kill(pid, signal.SIGKILL)
    except OSError:
        return


def _mutated_run(test: unittest.TestCase, old: str, new: str):
    test.assertEqual(SOURCE.count(old), 1, old)
    ns: dict = {}
    exec(compile(SOURCE.replace(old, new, 1), "<mutated-bounded-run>", "exec"), ns)
    ns["OUTPUT_CAP_BYTES"] = TEST_CAP
    return ns["_run_capped"]


def _interleaved_body() -> str:
    chunk = INTERLEAVE_EACH // 4
    return (
        "import sys\n"
        "for _ in range(4):\n"
        "    sys.stdout.buffer.write(b'O' * %d)\n"
        "    sys.stderr.buffer.write(b'E' * %d)\n"
        "    sys.stdout.buffer.flush()\n"
        "    sys.stderr.buffer.flush()\n"
        % (chunk, chunk)
    )


def _assert_mutated_leaves_descendant(test: unittest.TestCase, run, n: int = HOLD) -> None:
    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        pid_path = tmp / "desc.pid"
        finished = []

        def go():
            try:
                finished.append(run(_probe(tmp, DESCENDANT, str(pid_path), str(n)), tmp, 5))
            except Exception as exc:  # noqa: BLE001 - mutation outcome
                finished.append(exc)

        worker = threading.Thread(target=go)
        worker.start()
        try:
            deadline = time.monotonic() + 2
            while (not pid_path.exists() or not pid_path.stat().st_size) and (
                time.monotonic() < deadline
            ):
                time.sleep(0.05)
            pid = int(pid_path.read_text())
            worker.join(timeout=1.0)
            test.assertTrue(_pid_alive(pid), "mutated kill must leave the descendant")
        finally:
            if pid_path.exists() and pid_path.stat().st_size:
                _reap_pid(int(pid_path.read_text()))
            worker.join(timeout=2)


class _WithTestCap(unittest.TestCase):
    def setUp(self):
        patcher = mock.patch.object(br, "OUTPUT_CAP_BYTES", TEST_CAP)
        patcher.start()
        self.addCleanup(patcher.stop)


class UpstreamSourceContract(unittest.TestCase):
    def test_local_runner_matches_the_reviewed_source_pin(self):
        pin = json.loads(PIN_PATH.read_text(encoding="utf-8"))
        self.assertEqual(pin["repository"], "corpus-adequacy/corpus-adequacy")
        self.assertEqual(
            pin["commit"], "08e3b3f013c72f016131b32dfae03f749e432228"
        )
        self.assertEqual(pin["path"], "bounded_run.py")
        self.assertEqual(pin["license"], "MIT")
        self.assertEqual(pin["license_path"], "LICENSE")
        self.assertEqual(
            hashlib.sha256(UPSTREAM_LICENSE.read_bytes()).hexdigest(),
            pin["license_sha256"],
        )
        self.assertEqual(
            pin["upstream_sha256"],
            "71dedb0e8d62a110b25b97f68b5f0ead1b1e9abf3251b284a6401ae91968636a",
        )
        upstream_bytes = UPSTREAM_COPY.read_bytes()
        self.assertEqual(
            hashlib.sha256(upstream_bytes).hexdigest(), pin["upstream_sha256"]
        )
        upstream = upstream_bytes.decode("utf-8")
        self.assertEqual(upstream.count(UPSTREAM_WINDOWS_DOC), 1)
        expected_local = upstream.replace(UPSTREAM_WINDOWS_DOC, ASSAY_WINDOWS_DOC)
        local_bytes = (REPO_ROOT / "bounded_run.py").read_bytes()
        self.assertEqual(local_bytes, expected_local.encode("utf-8"))
        local = hashlib.sha256(local_bytes).hexdigest()
        self.assertEqual(local, pin["local_sha256"])
        self.assertEqual(
            pin["transformations"],
            [
                "Replace the corpus-adequacy caller-specific Windows docstring with "
                "Assay's direct-child-only non-claim; bytecode instructions are unchanged "
                "and only the runtime-visible docstring constant differs."
            ],
        )


class RetainedDiskBoundary(_WithTestCap):
    def test_fast_burst_never_uses_a_tempfile_sink_beyond_the_cap(self):
        retained_fds = []
        seen = {}
        real = subprocess.Popen

        def spy(*args, **kwargs):
            seen["stdout"] = kwargs.get("stdout")
            seen["stderr"] = kwargs.get("stderr")
            for key in ("stdout", "stderr"):
                sink = kwargs.get(key)
                if hasattr(sink, "fileno"):
                    retained_fds.append(os.dup(sink.fileno()))
            return real(*args, **kwargs)

        try:
            with tempfile.TemporaryDirectory() as raw:
                tmp = Path(raw)
                cmd = _probe(
                    tmp, "import sys\nsys.stdout.buffer.write(b'x' * %d)\n" % BURST
                )
                with mock.patch.object(subprocess, "Popen", side_effect=spy):
                    with self.assertRaises(br._OutputTooLarge):
                        br._run_capped(cmd, tmp, 5)
            retained = sum(os.fstat(fd).st_size for fd in retained_fds)
            self.assertEqual(
                retained,
                0,
                "temporary output sinks retained %d bytes above the %d-byte cap"
                % (retained, TEST_CAP),
            )
            self.assertIs(seen.get("stdout"), subprocess.PIPE)
            self.assertIs(seen.get("stderr"), subprocess.PIPE)
        finally:
            for fd in retained_fds:
                os.close(fd)


class LiveRunAllCaller(_WithTestCap):
    def _suite(self, tmp: Path, body: str) -> dict:
        suite_dir = tmp / "suite"
        suite_dir.mkdir()
        (suite_dir / "check.py").write_text(body, encoding="utf-8")
        return {
            "id": "bounded-run-probe",
            "path": "suite",
            "expect_status": "contradiction",
        }

    def _run(self, body: str) -> tuple[str, str]:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            suite = self._suite(tmp, body)
            with mock.patch.object(run_all, "REPO", tmp):
                return run_all._stdlib_jsonrpc(suite)

    def test_live_caller_classifies_combined_overflow_as_unproved(self):
        grade, detail = self._run(
            "import sys\n"
            "sys.stdout.buffer.write(b'O' * 40000)\n"
            "sys.stderr.buffer.write(b'E' * 40000)\n"
        )
        self.assertEqual(grade, run_all.UNPROVED)
        self.assertIn("output exceeded", detail)

    def test_live_caller_preserves_a_normal_json_report(self):
        grade, detail = self._run(
            "import json\n"
            "print(json.dumps({'status': 'contradiction', 'summary': {'ok': 1}}))\n"
        )
        self.assertEqual(grade, run_all.PROVED)
        self.assertIn("contradiction", detail)


class ContinuousCap(_WithTestCap):
    def test_fast_burst_uses_pipes_and_stays_inside_the_read_margin(self):
        seen = {}
        pulled = [0]
        real = subprocess.Popen

        def spy(*args, **kwargs):
            seen["stdout"] = kwargs.get("stdout")
            seen["stderr"] = kwargs.get("stderr")
            proc = real(*args, **kwargs)
            if proc.stdout is not None:
                proc.stdout = _CountPipe(proc.stdout, pulled)
            if proc.stderr is not None:
                proc.stderr = _CountPipe(proc.stderr, pulled)
            return proc

        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            cmd = _probe(tmp, "import sys\nsys.stdout.buffer.write(b'x' * %d)\n" % BURST)
            popen = mock.patch.object(subprocess, "Popen", side_effect=spy)
            popen.start()
            self.addCleanup(popen.stop)
            with self.assertRaises(br._OutputTooLarge):
                br._run_capped(cmd, tmp, 5)
        self.assertIs(seen.get("stdout"), subprocess.PIPE)
        self.assertIs(seen.get("stderr"), subprocess.PIPE)
        self.assertTrue(hasattr(br, "READ_CHUNK_BYTES"))
        self.assertEqual(br.READ_CHUNK_BYTES, 64 * 1024)
        self.assertGreater(pulled[0], TEST_CAP)
        self.assertLessEqual(pulled[0], TEST_CAP + 2 * br.READ_CHUNK_BYTES)

    def test_interleaved_stdout_and_stderr_share_one_combined_cap(self):
        self.assertLess(INTERLEAVE_EACH, TEST_CAP)
        self.assertGreater(2 * INTERLEAVE_EACH, TEST_CAP)
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(br._OutputTooLarge):
                br._run_capped(_probe(tmp, _interleaved_body()), tmp, 5)

    def test_stderr_only_burst_is_still_capped(self):
        body = "import sys\nsys.stderr.buffer.write(b'E' * %d)\n" % BURST
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(br._OutputTooLarge):
                br._run_capped(_probe(tmp, body), tmp, 5)

    def test_exact_cap_is_success(self):
        body = "import sys\nsys.stdout.buffer.write(b'x' * %d)\n" % TEST_CAP
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = br._run_capped(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(done.stdout, "x" * TEST_CAP)
        self.assertEqual(done.stderr, "")

    def test_one_byte_past_cap_is_too_large_and_not_retained(self):
        self.assertTrue(hasattr(br, "_charge_before_retain"))
        kept, overflow = br._charge_before_retain(TEST_CAP, TEST_CAP, b"Z")
        self.assertEqual(kept, b"")
        self.assertTrue(overflow)
        kept, overflow = br._charge_before_retain(TEST_CAP - 1, TEST_CAP, b"xy")
        self.assertEqual(kept, b"x")
        self.assertTrue(overflow)
        kept, overflow = br._charge_before_retain(0, TEST_CAP, b"x" * TEST_CAP)
        self.assertEqual(kept, b"x" * TEST_CAP)
        self.assertFalse(overflow)
        body = "import sys\nsys.stdout.buffer.write(b'x' * %d)\n" % (TEST_CAP + 1)
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(br._OutputTooLarge):
                br._run_capped(_probe(tmp, body), tmp, 5)

    def test_reader_failure_stops_the_child_without_waiting_out_the_timeout(self):
        real = subprocess.Popen

        def spy(*args, **kwargs):
            proc = real(*args, **kwargs)

            class _Boom:
                def __init__(self, raw):
                    self._raw = raw

                def read(self, n=-1):
                    raise OSError("reader boom")

                def close(self):
                    return self._raw.close()

            proc.stdout = _Boom(proc.stdout)
            return proc

        popen = mock.patch.object(subprocess, "Popen", side_effect=spy)
        popen.start()
        self.addCleanup(popen.stop)
        started = time.monotonic()
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(OSError):
                br._run_capped(_probe(tmp, "import time\ntime.sleep(30)\n"), tmp, 10)
        self.assertLess(time.monotonic() - started, 2)

    def test_timeout_stays_timeout_expired(self):
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(subprocess.TimeoutExpired):
                br._run_capped(_probe(tmp, "import time\ntime.sleep(30)\n"), tmp, 1)

    def test_timeout_wins_over_a_reader_error(self):
        real = subprocess.Popen

        def spy(*args, **kwargs):
            proc = real(*args, **kwargs)

            class _LateBoom:
                def __init__(self, raw):
                    self._raw = raw

                def read(self, n=-1):
                    time.sleep(1.5)
                    raise OSError("late reader boom")

                def close(self):
                    return self._raw.close()

            proc.stdout = _LateBoom(proc.stdout)
            return proc

        popen = mock.patch.object(subprocess, "Popen", side_effect=spy)
        popen.start()
        self.addCleanup(popen.stop)
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(subprocess.TimeoutExpired):
                br._run_capped(_probe(tmp, "import time\ntime.sleep(30)\n"), tmp, 1)

    def test_timeout_outranks_reader_error_when_poll_blocks_past_deadline(self):
        # 38c070f: first poll() blocks until the reader OSError and
        # kill_boundary reaps the child, then returns a returncode.
        # `while proc.poll() is None` never enters, so OSError wins.
        real = subprocess.Popen
        clock = [0.0]
        poll_entered = threading.Event()
        reader_raised = threading.Event()
        child_reaped = threading.Event()

        def fake_monotonic():
            return clock[0]

        def spy(*args, **kwargs):
            proc = real(*args, **kwargs)
            real_poll = proc.poll
            real_wait = proc.wait
            first = {"done": False}

            def wrapping_wait(*a, **k):
                result = real_wait(*a, **k)
                if proc.returncode is not None:
                    child_reaped.set()
                return result

            def slow_first_poll(*a, **k):
                if not first["done"]:
                    first["done"] = True
                    poll_entered.set()
                    if not reader_raised.wait(2):
                        raise AssertionError("reader did not fail while poll blocked")
                    if not child_reaped.wait(2):
                        raise AssertionError("reader-kill did not reap the child")
                    clock[0] = 1.2
                    rc = real_poll(*a, **k)
                    if rc is None:
                        raise AssertionError("poll after reader-kill returned None")
                    return rc
                return real_poll(*a, **k)

            proc.wait = wrapping_wait
            proc.poll = slow_first_poll

            class _Boom:
                def __init__(self, raw):
                    self._raw = raw

                def read(self, n=-1):
                    poll_entered.wait(2)
                    try:
                        raise OSError("reader after poll stall")
                    finally:
                        reader_raised.set()

                def close(self):
                    return self._raw.close()

            proc.stdout = _Boom(proc.stdout)
            return proc

        popen = mock.patch.object(subprocess, "Popen", side_effect=spy)
        popen.start()
        self.addCleanup(popen.stop)
        mono = mock.patch.object(br.time, "monotonic", side_effect=fake_monotonic)
        mono.start()
        self.addCleanup(mono.stop)
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            with self.assertRaises(subprocess.TimeoutExpired):
                br._run_capped(_probe(tmp, "import time\ntime.sleep(30)\n"), tmp, 1)

    def test_nonzero_and_signal_returncodes_are_preserved(self):
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = br._run_capped(_probe(tmp, "import sys\nsys.exit(3)\n"), tmp, 5)
        self.assertEqual(done.returncode, 3)
        if not POSIX:
            return
        body = (
            "import os, signal\n"
            "os.kill(os.getpid(), signal.SIGTERM)\n"
        )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = br._run_capped(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, -signal.SIGTERM)

    def test_normal_completion_keeps_rc0_and_both_streams(self):
        body = (
            "import sys\n"
            "sys.stdout.buffer.write(b'hello-out\\n')\n"
            "sys.stderr.buffer.write(b'hello-err\\n')\n"
        )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = br._run_capped(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(done.stdout, "hello-out\n")
        self.assertEqual(done.stderr, "hello-err\n")

    def test_fast_sub_cap_payload_is_byte_exact_on_both_streams(self):
        # Keep both readers behind the leader-exit observation. A stop.set()
        # on clean exit then permits one read but drops the remaining bytes.
        chunk = mock.patch.object(br, "READ_CHUNK_BYTES", 256)
        chunk.start()
        self.addCleanup(chunk.stop)
        n = 4 * 1024
        body = (
            "import sys\n"
            "sys.stdout.buffer.write(b'O' * %d)\n"
            "sys.stderr.buffer.write(b'E' * %d)\n"
            % (n, n)
        )
        pulled = [0]
        real = subprocess.Popen

        def spy(*args, **kwargs):
            proc = real(*args, **kwargs)
            allow_read = threading.Event()
            leader_observed = threading.Event()

            def release_readers_after_leader_exit():
                if leader_observed.is_set():
                    return
                leader_observed.set()
                threading.Timer(0.1, allow_read.set).start()

            real_poll = proc.poll
            real_wait = proc.wait

            def coordinated_poll():
                returncode = real_poll()
                if returncode is not None:
                    release_readers_after_leader_exit()
                return returncode

            def coordinated_wait(*wait_args, **wait_kwargs):
                returncode = real_wait(*wait_args, **wait_kwargs)
                if returncode is not None:
                    release_readers_after_leader_exit()
                return returncode

            class _GatedPipe(_CountPipe):
                def read(self, size=-1):
                    if not allow_read.wait(timeout=2):
                        raise RuntimeError("leader exit was not observed")
                    return super().read(size)

            if proc.stdout is not None:
                proc.stdout = _GatedPipe(proc.stdout, pulled)
            if proc.stderr is not None:
                proc.stderr = _GatedPipe(proc.stderr, pulled)
            proc.poll = coordinated_poll
            proc.wait = coordinated_wait
            return proc

        popen = mock.patch.object(subprocess, "Popen", side_effect=spy)
        popen.start()
        self.addCleanup(popen.stop)
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = br._run_capped(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(done.stdout, "O" * n)
        self.assertEqual(done.stderr, "E" * n)
        self.assertEqual(pulled[0], 2 * n)

    def test_utf8_replacement_is_deterministic_after_chunk_joins(self):
        chunk = mock.patch.object(br, "READ_CHUNK_BYTES", 1, create=True)
        chunk.start()
        self.addCleanup(chunk.stop)
        body = "import sys\nsys.stdout.buffer.write(b'ok\\xc3\\xa9\\xff')\n"
        texts = []
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            cmd = _probe(tmp, body)
            for _ in range(2):
                done = br._run_capped(cmd, tmp, 5)
                texts.append(done.stdout)
        self.assertEqual(texts[0], texts[1])
        self.assertEqual(texts[0], "oké\ufffd")
        self.assertEqual(done.returncode, 0)


class DescendantPipes(_WithTestCap):
    """Leader-exit must not lose the captured POSIX group."""

    def test_descendant_holding_pipes_is_group_killed(self):
        if not POSIX:
            self.skipTest(
                "this helper makes no Windows process-tree claim"
            )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            pid_path = tmp / "desc.pid"
            cmd = _probe(tmp, DESCENDANT, str(pid_path), str(HOLD))
            try:
                with self.assertRaises(br._OutputTooLarge):
                    br._run_capped(cmd, tmp, 5)
                pid = int(pid_path.read_text())
                deadline = time.monotonic() + 2
                while _pid_alive(pid) and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertFalse(_pid_alive(pid), "descendant still held the pipes")
            finally:
                if pid_path.exists() and pid_path.stat().st_size:
                    _reap_pid(int(pid_path.read_text()))


class Mutations(unittest.TestCase):
    """Source-string edits that must bite. Not a substitute for the cases above."""

    def test_removing_the_cap_check_lets_a_burst_complete(self):
        run = _mutated_run(self, "if len(data) > room:", "if False and len(data) > room:")
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            cmd = _probe(tmp, "import sys\nsys.stdout.buffer.write(b'x' * %d)\n" % BURST)
            done = run(cmd, tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(len(done.stdout), BURST)

    def test_counting_only_stdout_misses_stderr_overflow(self):
        run = _mutated_run(
            self, "total += len(data)", "total += len(data) if which == 1 else 0"
        )
        body = "import sys\nsys.stderr.buffer.write(b'E' * %d)\n" % BURST
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = run(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(len(done.stderr), BURST)

    def test_separate_per_stream_caps_miss_combined_overflow(self):
        run = _mutated_run(
            self,
            "_charge_before_retain(total, cap, data)",
            "_charge_before_retain(len(b''.join(chunks[which])), cap, data)",
        )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = run(_probe(tmp, _interleaved_body()), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(len(done.stdout), INTERLEAVE_EACH)
        self.assertEqual(len(done.stderr), INTERLEAVE_EACH)

    def test_leader_only_kill_leaves_a_posix_descendant(self):
        if not POSIX:
            self.skipTest(
                "this helper makes no Windows process-tree claim"
            )
        run = _mutated_run(self, "os.killpg(pgid, signal.SIGKILL)", "proc.kill()")
        _assert_mutated_leaves_descendant(self, run)

    def test_late_getpgid_leaves_a_posix_descendant(self):
        if not POSIX:
            self.skipTest(
                "this helper makes no Windows process-tree claim"
            )
        run = _mutated_run(
            self,
            "os.killpg(pgid, signal.SIGKILL)",
            "os.killpg(os.getpgid(proc.pid), signal.SIGKILL)",
        )
        # Leader exits first (tiny write), then getpgid(proc.pid) is ESRCH.
        _assert_mutated_leaves_descendant(self, run, n=0)

    def test_removing_timeout_lets_a_sleeper_finish(self):
        run = _mutated_run(
            self,
            "if time.monotonic() > deadline:",
            "if False and time.monotonic() > deadline:",
        )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = run(_probe(tmp, "import time\ntime.sleep(1.2)\n"), tmp, 1)
        self.assertEqual(done.returncode, 0)

    def test_swapping_normal_streams_breaks_rc0_payload(self):
        run = _mutated_run(
            self,
            "cmd, proc.returncode, stdout_text, stderr_text",
            "cmd, proc.returncode, stderr_text, stdout_text",
        )
        body = (
            "import sys\n"
            "sys.stdout.buffer.write(b'hello-out\\n')\n"
            "sys.stderr.buffer.write(b'hello-err\\n')\n"
        )
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            done = run(_probe(tmp, body), tmp, 5)
        self.assertEqual(done.returncode, 0)
        self.assertEqual(done.stdout, "hello-err\n")
        self.assertEqual(done.stderr, "hello-out\n")


if __name__ == "__main__":
    unittest.main(verbosity=1)
