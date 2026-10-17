#!/usr/bin/env python3
"""Behaviour and wiring contract for the Windows cache probe (#3113).

The probe is observability only: it must report what it could not see as unknown rather than as
zero, never print names, contents or environment, stay bounded, and never change a test result.
The wiring half pins where the two measurements sit in the `test` job: after the rust-cache
restore and the sccache environment, before the first compiling step, and a final `always()`
measurement as the job's last regular step (so before any post-action cache save).
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "scripts/ci/windows_cache_probe.py"
WORKFLOW = ROOT / ".github/workflows/ci.yml"
OUTPUT_CEILING = 8192
SECRET_NAME = "SECRETNAME_3113"
SECRET_CONTENT = "SECRETCONTENT_3113"
SECRET_ENV = "SECRETENV_3113"
SECRET_PATH = "SECRETPATH_3113"

# Shape of `sccache --show-stats --stats-format json` at v0.18.0, the version the pinned action
# installed on main: `ServerInfo` (src/server.rs:1811) serialised with serde_json
# (src/commands.rs:740); hit/miss counts are `PerLanguageCount`, whose total is the sum of
# `counts.values()` (src/server.rs:1674). An empty `counts` map is a real zero.
FAKE_SCCACHE = textwrap.dedent(
    f"""
    import json, sys, time
    mode = sys.argv[1]

    def per_language(n):
        return {{"counts": ({{"Rust": n}} if n else {{}}), "adv_counts": ({{"rust": n}} if n else {{}})}}

    def server_info(hits, misses):
        zero = {{"secs": 0, "nanos": 0}}
        return {{
            "stats": {{
                "compile_requests": hits + misses, "requests_unsupported_compiler": 0,
                "requests_not_compile": 0, "requests_not_cacheable": 0,
                "requests_executed": hits + misses, "cache_errors": per_language(0),
                "cache_hits": per_language(hits), "cache_misses": per_language(misses),
                "cache_timeouts": 0, "cache_read_errors": 0, "non_cacheable_compilations": 0,
                "forced_recaches": 0, "cache_write_errors": 0, "cache_writes": misses,
                "cache_write_duration": zero, "cache_read_hit_duration": zero,
                "compilations": misses, "compiler_write_duration": zero, "compile_fails": 0,
                "not_cached": {{}}, "dist_compiles": {{}}, "dist_errors": 0,
            }},
            "cache_location": 'Local disk: "C:\\\\{SECRET_PATH}\\\\.sccache"',
            "cache_size": 123456,
            "max_cache_size": 10737418240,
            "use_preprocessor_cache_mode": False,
            "version": "0.18.0",
            "basedirs": ["C:\\\\{SECRET_PATH}"],
        }}

    if mode == "ok":
        sys.stdout.write(json.dumps(server_info(32, 42)))
    elif mode == "emptycounts":
        sys.stdout.write(json.dumps(server_info(0, 0)))
    elif mode == "nomap":
        info = server_info(32, 42)
        del info["stats"]["cache_hits"]
        del info["stats"]["cache_misses"]
        sys.stdout.write(json.dumps(info))
    elif mode == "boolcounts":
        info = server_info(32, 42)
        info["stats"]["cache_hits"] = {{"counts": {{"Rust": True}}, "adv_counts": {{}}}}
        sys.stdout.write(json.dumps(info))
    elif mode == "negativecounts":
        info = server_info(32, 42)
        info["stats"]["cache_hits"] = {{"counts": {{"Rust": -1}}, "adv_counts": {{}}}}
        sys.stdout.write(json.dumps(info))
    elif mode == "floatcounts":
        info = server_info(32, 42)
        info["stats"]["cache_hits"] = {{"counts": {{"Rust": 1.5}}, "adv_counts": {{}}}}
        sys.stdout.write(json.dumps(info))
    elif mode == "stringcounts":
        info = server_info(32, 42)
        info["stats"]["cache_hits"] = {{"counts": {{"Rust": "1"}}, "adv_counts": {{}}}}
        sys.stdout.write(json.dumps(info))
    elif mode == "boolrequest":
        info = server_info(32, 42)
        info["stats"]["compile_requests"] = True
        sys.stdout.write(json.dumps(info))
    elif mode == "negativerequest":
        info = server_info(32, 42)
        info["stats"]["compile_requests"] = -1
        sys.stdout.write(json.dumps(info))
    elif mode == "noncounters":
        print(json.dumps({{"stats": {{"cache_hits": "many"}}, "version": 7}}))
    elif mode == "spawn_descendant":
        import os, subprocess
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        pid_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "descendant.pid")
        with open(pid_path, "w", encoding="utf-8") as fh:
            fh.write(str(child.pid))
    elif mode == "nonzero":
        sys.stderr.write("server failed\\n")
        sys.exit(3)
    elif mode == "sleep":
        time.sleep(30)
    elif mode == "oversized":
        sys.stdout.write("x" * (2 * 1024 * 1024))
    elif mode == "malformed":
        print("this is not json")
    """
)


def run_probe(args: list[str], *, env_extra: dict[str, str] | None = None, cwd: Path | None = None):
    env = {**os.environ, SECRET_ENV: SECRET_ENV + "_VALUE"}
    env.pop("GITHUB_STEP_SUMMARY", None)
    if env_extra:
        env.update(env_extra)
    started = time.monotonic()
    proc = subprocess.run(
        [sys.executable, "-B", str(PROBE), *args],
        capture_output=True,
        text=True,
        env=env,
        cwd=cwd,
        timeout=60,
    )
    return proc, time.monotonic() - started


def report_of(proc) -> dict:
    lines = [line for line in proc.stdout.splitlines() if line.startswith("{")]
    if len(lines) != 1:
        raise AssertionError(f"expected exactly one JSON report line, got {len(lines)}: {proc.stdout!r}")
    return json.loads(lines[0])


class ProbeBehaviourTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.fake = self.tmp / "fake_sccache.py"
        self.fake.write_text(FAKE_SCCACHE, encoding="utf-8")

    def tearDown(self) -> None:
        for path in self.tmp.rglob("*"):
            try:
                os.chmod(path, 0o700)
            except OSError:
                pass
        self._tmp.cleanup()

    def sccache(self, mode: str) -> list[str]:
        return ["--sccache", sys.executable, "--sccache", str(self.fake), "--sccache", mode]

    def probe(self, *extra: str, sccache_mode: str = "ok", env_extra=None):
        proc, elapsed = run_probe(
            ["--phase", "pre", "--cache-hit", "false", *self.sccache(sccache_mode), *extra],
            env_extra=env_extra,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertLessEqual(len(proc.stdout.encode("utf-8")), OUTPUT_CEILING)
        return report_of(proc), proc, elapsed

    # -- cache-hit output ---------------------------------------------------------------------

    def test_cache_hit_is_true_false_or_unknown_never_guessed(self):
        for raw, expected in (("true", "true"), ("false", "false"), ("", "unknown"), ("maybe", "unknown")):
            with self.subTest(raw=raw):
                proc, _ = run_probe(["--phase", "pre", "--cache-hit", raw, *self.sccache("ok")])
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertEqual(report_of(proc)["rust_cache_exact_hit"], expected)
                self.assertNotIn("maybe", proc.stdout)

    # -- directories --------------------------------------------------------------------------

    def test_absent_directory_has_no_counters(self):
        report, _, _ = self.probe("--dir", f"target={self.tmp / 'missing'}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "absent")
        for key in ("files", "dirs", "bytes"):
            self.assertIsNone(entry[key], key)

    def test_empty_directory_reports_real_zeros(self):
        (self.tmp / "empty").mkdir()
        report, _, _ = self.probe("--dir", f"target={self.tmp / 'empty'}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "empty")
        self.assertEqual((entry["files"], entry["dirs"], entry["bytes"]), (0, 0, 0))
        self.assertFalse(entry["lower_bound"])

    def test_nonempty_directory_counts_aggregates_without_names_or_contents(self):
        root = self.tmp / "target"
        (root / "sub").mkdir(parents=True)
        (root / f"{SECRET_NAME}.txt").write_text(SECRET_CONTENT, encoding="utf-8")
        (root / "sub" / "b.bin").write_bytes(b"\0" * 10)
        summary = self.tmp / "summary.md"
        report, proc, _ = self.probe(
            "--dir", f"target={root}", env_extra={"GITHUB_STEP_SUMMARY": str(summary)}
        )
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "present")
        self.assertEqual(entry["files"], 2)
        self.assertEqual(entry["dirs"], 1)
        self.assertEqual(entry["bytes"], len(SECRET_CONTENT) + 10)
        self.assertFalse(entry["lower_bound"])
        written = summary.read_text(encoding="utf-8")
        self.assertIn("pre", written)
        for text in (proc.stdout, proc.stderr, written):
            for secret in (SECRET_NAME, SECRET_CONTENT, SECRET_ENV, SECRET_PATH, str(root)):
                self.assertNotIn(secret, text)

    def test_file_where_directory_expected(self):
        target = self.tmp / "afile"
        target.write_text("x", encoding="utf-8")
        report, _, _ = self.probe("--dir", f"target={target}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "not_a_directory")
        self.assertIsNone(entry["files"])

    @unittest.skipUnless(hasattr(os, "symlink"), "no symlink support")
    def test_symlinks_are_counted_not_followed(self):
        root = self.tmp / "target"
        outside = self.tmp / "outside"
        outside.mkdir()
        for i in range(5):
            (outside / f"f{i}").write_text("y", encoding="utf-8")
        root.mkdir()
        try:
            os.symlink(outside, root / "link", target_is_directory=True)
        except OSError as exc:
            self.skipTest(f"symlink not permitted here: {exc}")
        report, _, _ = self.probe("--dir", f"target={root}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["files"], 0)
        self.assertEqual(entry["symlinks"], 1)

    @unittest.skipUnless(
        os.name == "posix" and hasattr(os, "geteuid") and os.geteuid() != 0,
        "permission denial is only reliably constructible as a non-root POSIX user",
    )
    def test_unreadable_root_is_unknown_not_empty(self):
        root = self.tmp / "locked"
        root.mkdir()
        (root / "f").write_text("x", encoding="utf-8")
        os.chmod(root, 0)
        report, proc, _ = self.probe("--dir", f"target={root}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "unreadable")
        self.assertIsNone(entry["files"])
        self.assertIn("::warning::", proc.stdout)

    @unittest.skipUnless(
        os.name == "posix" and hasattr(os, "geteuid") and os.geteuid() != 0,
        "permission denial is only reliably constructible as a non-root POSIX user",
    )
    def test_unreadable_subtree_marks_counts_as_lower_bound(self):
        root = self.tmp / "target"
        (root / "locked").mkdir(parents=True)
        (root / "locked" / "f").write_text("x", encoding="utf-8")
        (root / "ok").write_text("x", encoding="utf-8")
        os.chmod(root / "locked", 0)
        report, _, _ = self.probe("--dir", f"target={root}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "partial")
        self.assertTrue(entry["lower_bound"])
        self.assertGreaterEqual(entry["errors"], 1)

    def test_entry_cap_truncates_and_marks_lower_bound(self):
        root = self.tmp / "target"
        root.mkdir()
        for i in range(10):
            (root / f"f{i}").write_text("z", encoding="utf-8")
        report, _, _ = self.probe("--dir", f"target={root}", "--max-entries", "3")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "truncated")
        self.assertTrue(entry["lower_bound"])
        self.assertLessEqual(entry["files"], 3)

    def test_time_budget_truncates(self):
        root = self.tmp / "target"
        root.mkdir()
        (root / "f").write_text("z", encoding="utf-8")
        report, _, _ = self.probe("--dir", f"target={root}", "--time-budget", "0")
        self.assertEqual(report["dirs"]["target"]["status"], "truncated")
        self.assertTrue(report["dirs"]["target"]["lower_bound"])

    @unittest.skipUnless(
        os.name == "posix" and hasattr(os, "geteuid") and os.geteuid() != 0,
        "permission denial is only reliably constructible as a non-root POSIX user",
    )
    def test_target_below_unreadable_parent_is_unreadable_not_absent(self):
        parent = self.tmp / "locked-parent"
        target = parent / "target"
        target.mkdir(parents=True)
        (target / "f").write_text("x", encoding="utf-8")
        os.chmod(parent, 0)
        try:
            report, proc, _ = self.probe("--dir", f"target={target}")
        finally:
            os.chmod(parent, 0o700)
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "unreadable")
        self.assertIsNone(entry["files"])
        self.assertIsNone(entry["bytes"])
        self.assertIn("::warning::", proc.stdout)

    @unittest.skipUnless(hasattr(os, "symlink"), "no symlink support")
    def test_symlink_root_is_not_followed(self):
        real = self.tmp / "real"
        real.mkdir()
        (real / "f").write_text("x", encoding="utf-8")
        link = self.tmp / "linkroot"
        try:
            os.symlink(real, link, target_is_directory=True)
        except OSError as exc:
            self.skipTest(f"symlink not permitted here: {exc}")
        report, _, _ = self.probe("--dir", f"target={link}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "not_a_directory")
        self.assertIsNone(entry["files"])

    @unittest.skipUnless(os.name == "nt", "Windows junction semantics require native Windows")
    def test_windows_junction_root_is_not_followed(self):
        real = self.tmp / "real"
        real.mkdir()
        (real / "f").write_text("x", encoding="utf-8")
        # Ordinary directory positive control: must be observed normally.
        report, _, _ = self.probe("--dir", f"target={real}")
        self.assertEqual(report["dirs"]["target"]["status"], "present")
        self.assertEqual(report["dirs"]["target"]["files"], 1)
        junction = self.tmp / "junctionroot"
        created = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(junction), str(real)],
            capture_output=True, text=True,
        )
        if created.returncode != 0:
            self.fail(f"could not create Windows junction fixture: {created.returncode}")
        self.assertTrue(os.path.isjunction(str(junction)))
        report, _, _ = self.probe("--dir", f"target={junction}")
        entry = report["dirs"]["target"]
        self.assertEqual(entry["status"], "not_a_directory")
        self.assertIsNone(entry["files"])

    @unittest.skipUnless(os.name == "nt", "Windows junction semantics require native Windows")
    def test_windows_junction_entry_is_counted_not_followed(self):
        outside = self.tmp / "outside"
        outside.mkdir()
        for i in range(5):
            (outside / f"f{i}").write_text("y", encoding="utf-8")
        root = self.tmp / "target"
        root.mkdir()
        # Ordinary directory positive control.
        (root / "plain").mkdir()
        (root / "plain" / "a").write_text("z", encoding="utf-8")
        junction = root / "junction-entry"
        created = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(junction), str(outside)],
            capture_output=True, text=True,
        )
        if created.returncode != 0:
            self.fail(f"could not create Windows junction fixture: {created.returncode}")
        self.assertTrue(os.path.isjunction(str(junction)))
        report, _, _ = self.probe("--dir", f"target={root}")
        entry = report["dirs"]["target"]
        # Junction target's 5 files must not be counted; plain file counts once.
        self.assertEqual(entry["files"], 1)
        self.assertEqual(entry["symlinks"], 1)

    def test_caps_cannot_be_raised_above_defaults(self):
        proc, _ = run_probe(
            ["--phase", "pre", "--cache-hit", "true", "--max-entries", "999999999", *self.sccache("ok")]
        )
        self.assertEqual(proc.returncode, 0)
        self.assertIn("::warning::", proc.stdout)
        self.assertLessEqual(report_of(proc)["limits"]["max_entries"], 200_000)

    # -- sccache stats ------------------------------------------------------------------------

    def test_sccache_ok_reports_counters_and_backend_kind_only(self):
        report, proc, _ = self.probe()
        stats = report["sccache"]
        self.assertEqual(stats["status"], "ok")
        self.assertEqual(stats["hits"], 32)
        self.assertEqual(stats["misses"], 42)
        self.assertEqual(stats["compile_requests"], 74)
        self.assertEqual(stats["cache_size_bytes"], 123456)
        self.assertEqual(stats["backend"], "Local disk")
        self.assertEqual(stats["version"], "0.18.0")
        self.assertNotIn(SECRET_PATH, proc.stdout)

    def test_sccache_empty_counts_map_is_a_real_zero(self):
        report, _, _ = self.probe(sccache_mode="emptycounts")
        stats = report["sccache"]
        self.assertEqual(stats["status"], "ok")
        self.assertEqual((stats["hits"], stats["misses"], stats["compile_requests"]), (0, 0, 0))

    def test_sccache_missing_counts_map_is_unknown(self):
        report, _, _ = self.probe(sccache_mode="nomap")
        stats = report["sccache"]
        self.assertEqual(stats["status"], "ok")
        self.assertIsNone(stats["hits"])
        self.assertIsNone(stats["misses"])
        self.assertEqual(stats["compile_requests"], 74)

    def test_sccache_failures_never_fabricate_zero_counters(self):
        cases = {
            "nonzero": "nonzero_exit",
            "oversized": "oversized",
            "malformed": "malformed",
            "noncounters": "ok",
        }
        for mode, status in cases.items():
            with self.subTest(mode=mode):
                report, proc, _ = self.probe(sccache_mode=mode)
                stats = report["sccache"]
                self.assertEqual(stats["status"], status)
                for key in ("hits", "misses", "compile_requests", "cache_size_bytes"):
                    self.assertIsNone(stats[key], key)
                if status != "ok":
                    self.assertIn("::warning::", proc.stdout)
                self.assertNotIn("server failed", proc.stdout)

    def test_sccache_missing_binary(self):
        proc, _ = run_probe(
            ["--phase", "post", "--cache-hit", "true", "--sccache", str(self.tmp / "no-such-sccache")]
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        stats = report_of(proc)["sccache"]
        self.assertEqual(stats["status"], "missing")
        self.assertIsNone(stats["hits"])
        self.assertIn("::warning::", proc.stdout)

    def test_sccache_bool_and_non_counter_values_are_unknown(self):
        for mode, key in (
            ("boolcounts", "hits"), ("negativecounts", "hits"),
            ("floatcounts", "hits"), ("stringcounts", "hits"),
            ("boolrequest", "compile_requests"), ("negativerequest", "compile_requests"),
        ):
            with self.subTest(mode=mode):
                report, _, _ = self.probe(sccache_mode=mode)
                self.assertEqual(report["sccache"]["status"], "ok")
                self.assertIsNone(report["sccache"][key], key)

    def test_argument_errors_do_not_echo_supplied_values(self):
        marker = "SECRET_PATH_3113"
        summary = self.tmp / "summary-args.md"
        cases = (
            ["--phase", "pre", "--time-budget", marker],
            ["--phase", "pre", "--max-entries", marker],
            ["--phase", "pre", "--sccache-timeout", marker],
            ["--phase", marker],
            ["--phase", "pre", "--unknown-flag-3113", marker],
        )
        for args in cases:
            with self.subTest(args=args):
                proc, _ = run_probe(
                    [*args, *self.sccache("ok")],
                    env_extra={"GITHUB_STEP_SUMMARY": str(summary)},
                )
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn("::warning::", proc.stdout)
                combined = proc.stdout + proc.stderr
                if summary.exists():
                    combined += summary.read_text(encoding="utf-8")
                self.assertNotIn(marker, combined)

    def test_sccache_stdout_pipe_is_closed_without_resource_warning(self):
        harness = textwrap.dedent(
            """
            import gc, importlib.util, sys
            spec = importlib.util.spec_from_file_location("probe", sys.argv[1])
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            result = mod.sccache_stats([sys.executable, sys.argv[2], sys.argv[3]], float(sys.argv[4]))
            gc.collect()
            print(result["status"])
            gc.collect()
            """
        )
        for mode, timeout, expected in (
            ("ok", "5", "ok"), ("nonzero", "5", "nonzero_exit"),
            ("oversized", "5", "oversized"), ("sleep", "1", "timeout"),
        ):
            with self.subTest(mode=mode):
                proc = subprocess.run(
                    [sys.executable, "-W", "always::ResourceWarning", "-B", "-c", harness,
                     str(PROBE), str(self.fake), mode, timeout],
                    capture_output=True, text=True, timeout=60,
                )
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn(expected, proc.stdout)
                self.assertNotIn("ResourceWarning", proc.stderr)
                self.assertNotIn("unclosed", proc.stderr)

    def test_sccache_inherited_pipe_and_direct_child_bounded_no_resource_warning(self):
        # Direct long child (sleep) must be killed/reaped; exited parent whose descendant
        # inherits stdout (spawn_descendant) must not hang cleanup. Both: timeout=1, <10s,
        # no ResourceWarning in the harness child's stderr (asserted there, not via -W error).
        harness = textwrap.dedent(
            """
            import gc, importlib.util, sys
            spec = importlib.util.spec_from_file_location("probe", sys.argv[1])
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            result = mod.sccache_stats([sys.executable, sys.argv[2], sys.argv[3]], float(sys.argv[4]))
            gc.collect()
            print(result["status"], flush=True)
            gc.collect()
            """
        )
        pid_file = self.tmp / "descendant.pid"
        for mode in ("sleep", "spawn_descendant"):
            with self.subTest(mode=mode):
                if pid_file.exists():
                    pid_file.unlink()
                out_path = self.tmp / f"harness-{mode}.out"
                err_path = self.tmp / f"harness-{mode}.err"
                started = time.monotonic()
                try:
                    with open(out_path, "wb") as out_fh, open(err_path, "wb") as err_fh:
                        proc = subprocess.run(
                            [sys.executable, "-W", "always::ResourceWarning", "-B", "-c", harness,
                             str(PROBE), str(self.fake), mode, "1"],
                            stdout=out_fh, stderr=err_fh, timeout=12,
                        )
                except subprocess.TimeoutExpired:
                    self.fail(f"sccache_stats hung >12s for mode={mode} (timeout was 1s)")
                finally:
                    # Only ever kill the descendant this fixture itself spawned.
                    if pid_file.exists():
                        try:
                            descendant = int(pid_file.read_text(encoding="utf-8").strip())
                            os.kill(descendant, 9)
                        except (OSError, ValueError):
                            pass
                elapsed = time.monotonic() - started
                self.assertEqual(proc.returncode, 0, err_path.read_text(encoding="utf-8"))
                self.assertLess(elapsed, 10, f"mode={mode} took {elapsed:.1f}s for a 1s timeout")
                self.assertIn("timeout", out_path.read_text(encoding="utf-8"))
                err_text = err_path.read_text(encoding="utf-8")
                self.assertNotIn("ResourceWarning", err_text)
                self.assertNotIn("unclosed", err_text)

    def test_sccache_timeout_is_bounded(self):
        report, _, elapsed = self.probe("--sccache-timeout", "1", sccache_mode="sleep")
        self.assertEqual(report["sccache"]["status"], "timeout")
        self.assertIsNone(report["sccache"]["hits"])
        self.assertLess(elapsed, 20)

    # -- failure of the probe itself ----------------------------------------------------------

    def test_bad_arguments_stay_visible_and_exit_zero(self):
        for args in (
            ["--phase", "middle"],
            ["--phase", "pre", "--dir", "no-equals-sign"],
            ["--phase", "pre", "--dir", "BAD LABEL=x"],
        ):
            with self.subTest(args=args):
                proc, _ = run_probe([*args, *self.sccache("ok")])
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn("::warning::", proc.stdout)

    def test_unwritable_summary_is_a_warning(self):
        report, proc, _ = self.probe(
            env_extra={"GITHUB_STEP_SUMMARY": str(self.tmp / "no-dir" / "summary.md")}
        )
        self.assertEqual(report["phase"], "pre")
        self.assertIn("::warning::", proc.stdout)


# -- workflow wiring ----------------------------------------------------------------------------

STEP_SPLIT = re.compile(r"(?m)^      - (?:name:|uses:|id:|if:)")
PROBE_INVOCATION = "python3 scripts/ci/windows_cache_probe.py"
HELPER_TEST_INVOCATION = "python3 -B scripts/ci/test_windows_cache_probe.py"
CACHE_HIT_ENV = "CACHE_HIT: ${{ steps.rust-cache.outputs.cache-hit }}"
PRE_NAME = "Windows cache probe (pre-compile)"
POST_NAME = "Windows cache probe (final)"
HELPER_NAME = "Test Windows cache probe helper"


def split_test_job(text: str) -> tuple[str, list[str], str]:
    match = re.search(r"(?ms)^  test:\s*$\n.*?(?=^  [A-Za-z0-9_-]+:\s*$|\Z)", text)
    if match is None:
        raise AssertionError("ci.yml has no test job")
    job = match.group(0)
    steps_at = job.find("\n    steps:\n")
    if steps_at < 0:
        raise AssertionError("test job has no steps")
    head = text[: match.start()] + job[: steps_at + len("\n    steps:\n")]
    body = job[steps_at + len("\n    steps:\n") :]
    starts = [m.start() for m in STEP_SPLIT.finditer(body)]
    if not starts:
        raise AssertionError("test job steps did not parse")
    lead = body[: starts[0]]
    blocks = [body[a:b] for a, b in zip(starts, starts[1:] + [len(body)])]
    # Comment lines before a step stay attached to the block above it; that only matters for
    # mutations, which move whole blocks and so keep every line.
    return head + lead, blocks, text[match.end() :]


def step_field(block: str, field: str) -> str | None:
    m = re.search(rf"(?m)^(?:      - |        ){field}: (.*)$", block)
    return None if m is None else m.group(1).strip()


def active_run(block: str) -> str:
    m = re.search(r"(?ms)^        run: \|\s*$\n((?:^          .*\n?|^\s*\n)*)", block)
    if m is not None:
        lines = m.group(1).splitlines()
    else:
        single = re.search(r"(?m)^        run: (.+)$", block)
        lines = [single.group(1)] if single else []
    active = [ln.strip() for ln in lines if ln.strip() and not ln.strip().startswith("#")]
    return " ".join(ln.removesuffix("\\").strip() for ln in active)


def windows_applicable(block: str) -> bool:
    cond = step_field(block, "if")
    if cond is None:
        return True
    if "runner.os == 'Windows'" in cond or "runner.os != 'Linux'" in cond or "runner.os != 'macOS'" in cond:
        return True
    if re.search(r"runner\.os == '(Linux|macOS)'", cond) or "runner.os != 'Windows'" in cond:
        return False
    return True  # unknown condition: assume it may run, which can only move the bound earlier


def compiles(block: str) -> bool:
    run = active_run(block)
    return bool(re.search(r"(^|[\s;&|(])cargo\s", run) or "--with-cargo" in run)


def wiring_errors(text: str) -> list[str]:
    errors: list[str] = []
    _, blocks, _ = split_test_job(text)
    names = [step_field(b, "name") for b in blocks]

    def index(name: str) -> int | None:
        hits = [i for i, n in enumerate(names) if n == name]
        if len(hits) != 1:
            errors.append(f"expected exactly one step named {name!r}, found {len(hits)}")
            return None
        return hits[0]

    cache = index("Rust cache")
    env = index("Configure Environment (with sccache)")
    helper = index(HELPER_NAME)
    pre = index(PRE_NAME)
    post = index(POST_NAME)
    if None in (cache, env, helper, pre, post):
        return errors

    if step_field(blocks[cache], "id") != "rust-cache":
        errors.append("Rust cache step must carry id: rust-cache so its cache-hit output is wired")

    first_compile = next(
        (i for i, b in enumerate(blocks) if windows_applicable(b) and compiles(b)), None
    )
    if first_compile is None:
        errors.append("no compiling step found for the Windows leg")
        return errors
    if not cache < env < pre < first_compile:
        errors.append(
            f"pre probe must follow rust-cache restore ({cache}) and sccache env ({env}) and "
            f"precede the first Windows compile ({first_compile}); it is at {pre}"
        )
    if helper > pre:
        errors.append("helper tests must run before the pre probe")
    if post != len(blocks) - 1:
        errors.append("final probe must be the last regular step (before post-action cache save)")

    helper_block = blocks[helper]
    if not windows_applicable(helper_block):
        errors.append("helper tests must run on the Windows leg")
    if active_run(helper_block) != HELPER_TEST_INVOCATION:
        errors.append(f"helper step must run exactly {HELPER_TEST_INVOCATION!r} (fail-closed)")

    for label, i, phase, cond in (
        ("pre", pre, "pre", "runner.os == 'Windows'"),
        ("final", post, "post", "always() && runner.os == 'Windows'"),
    ):
        block = blocks[i]
        if step_field(block, "if") != cond:
            errors.append(f"{label} probe must have if: {cond}")
        if CACHE_HIT_ENV not in block:
            errors.append(f"{label} probe must read cache-hit from steps.rust-cache")
        for forbidden in ("continue-on-error", "timeout-minutes"):
            if forbidden in block:
                errors.append(f"{label} probe must not set {forbidden}")
        run = active_run(block)
        if not run.startswith(PROBE_INVOCATION + " "):
            errors.append(f"{label} probe must invoke {PROBE_INVOCATION}")
            continue
        command, sep, fallback = run.partition(" || ")
        if not sep or not fallback.startswith('echo "::warning::'):
            errors.append(f"{label} probe failure must become a visible warning, not a step failure")
        argv = shlex.split(command)
        expected = [
            "--phase", phase, "--cache-hit", "$CACHE_HIT",
            "--dir", "target=target", "--dir", "sccache=$SCCACHE_DIR",
            "--sccache", "sccache",
        ]
        if argv[2:] != expected:
            errors.append(f"{label} probe argv drifted: {argv[2:]!r}")
    return errors


def mutate_move(text: str, name: str, after: str) -> str:
    head, blocks, tail = split_test_job(text)
    names = [step_field(b, "name") for b in blocks]
    block = blocks.pop(names.index(name))
    names = [step_field(b, "name") for b in blocks]
    blocks.insert(names.index(after) + 1, block)
    return head + "".join(blocks) + tail


class WorkflowWiringTest(unittest.TestCase):
    def setUp(self) -> None:
        self.text = WORKFLOW.read_text(encoding="utf-8")

    def test_wiring_contract_holds(self):
        self.assertEqual(wiring_errors(self.text), [])

    def test_workflow_invocation_is_accepted_by_the_real_helper(self):
        _, blocks, _ = split_test_job(self.text)
        names = [step_field(b, "name") for b in blocks]
        with tempfile.TemporaryDirectory() as tmp:
            for name in (PRE_NAME, POST_NAME):
                with self.subTest(step=name):
                    run = active_run(blocks[names.index(name)])
                    argv = shlex.split(run.partition(" || ")[0])
                    self.assertEqual(argv[:2], ["python3", "scripts/ci/windows_cache_probe.py"])
                    args = [
                        a.replace("$CACHE_HIT", "true").replace("$SCCACHE_DIR", str(Path(tmp) / "sc"))
                        for a in argv[2:]
                    ]
                    args[args.index("--sccache") + 1] = str(Path(tmp) / "absent-sccache")
                    proc, _ = run_probe([*args, "--sccache-timeout", "5"], cwd=Path(tmp))
                    self.assertEqual(proc.returncode, 0, proc.stderr)
                    report = report_of(proc)
                    self.assertEqual(set(report["dirs"]), {"target", "sccache"})
                    self.assertEqual(report["rust_cache_exact_hit"], "true")
                    self.assertNotIn("argument", " ".join(report.get("warnings", [])))

    def test_existing_job_policy_is_untouched(self):
        for line in (
            "    timeout-minutes: ${{ matrix.os == 'windows-latest' && 30 || 20 }}",
            "        uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2",
            "            . -> target",
            "            ~/.sccache",
            "          cache-on-failure: true",
            '            echo "SCCACHE_DIR=$HOME/.sccache"',
        ):
            self.assertIn(line + "\n", self.text)

    def test_mutations_turn_the_contract_red(self):
        def replace_once(old: str, new: str) -> str:
            self.assertEqual(self.text.count(old), 1, old)
            return self.text.replace(old, new, 1)

        mutations = {
            "pre probe after first compile": lambda: mutate_move(
                self.text, PRE_NAME, "Test MCP no-pass-through E2E (E6a.3)"
            ),
            "pre probe before sccache env": lambda: mutate_move(self.text, PRE_NAME, "Rust cache"),
            "final probe not last": lambda: mutate_move(
                self.text, POST_NAME, "Test policy transaction recovery (macOS and Windows)"
            ),
            "helper after pre probe": lambda: mutate_move(self.text, HELPER_NAME, PRE_NAME),
            "cache output id dropped": lambda: replace_once(
                "      - name: Rust cache\n        id: rust-cache\n", "      - name: Rust cache\n"
            ),
            "final not always": lambda: replace_once(
                "        if: always() && runner.os == 'Windows'\n        env:\n          CACHE_HIT",
                "        if: runner.os == 'Windows'\n        env:\n          CACHE_HIT",
            ),
            "warning fallback dropped": lambda: re.sub(
                r'--sccache sccache \\\n\s+\|\| echo "::warning::[^\n]*\n',
                "--sccache sccache\n",
                self.text,
                count=1,
            ),
            "phase swapped": lambda: replace_once("--phase pre", "--phase post"),
            "env wiring broken": lambda: self.text.replace(
                '--dir sccache="$SCCACHE_DIR"', '--dir sccache="$HOME/.sccache"', 1
            ),
            "helper softened": lambda: replace_once(
                HELPER_TEST_INVOCATION + "\n", HELPER_TEST_INVOCATION + " || true\n"
            ),
        }
        for label, build in mutations.items():
            with self.subTest(mutation=label):
                mutated = build()
                self.assertNotEqual(mutated, self.text, "mutation did not apply")
                self.assertNotEqual(wiring_errors(mutated), [], f"stayed green: {label}")


if __name__ == "__main__":
    unittest.main()
