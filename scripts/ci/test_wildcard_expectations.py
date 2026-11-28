#!/usr/bin/env python3
"""Run the real wildcard gate on tiny offline crates, including warm lint caches."""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts/ci/check-wildcard-expectations.sh"
NEEDED = """pub enum Choice { A, B, C }
#[expect(clippy::wildcard_enum_match_arm)]
pub fn select(choice: Choice) -> u8 {
    match choice { Choice::A => 0, _ => 1 }
}
"""
UNNEEDED = NEEDED.replace("Choice::A => 0, _ => 1",
                          "Choice::A => 0, Choice::B => 1, Choice::C => 2")


class WildcardGateTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(prefix="assay-wildcard-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root / "crates/probe/src/lib.rs"
        self.source.parent.mkdir(parents=True)
        self.binary_source = self.source.with_name("main.rs")
        self.binary_source.write_text("fn main() {}\n")
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/probe"]\nresolver = "2"\n')
        (self.root / "crates/probe/Cargo.toml").write_text(
            '[package]\nname = "wildcard-probe"\nversion = "0.0.0"\nedition = "2021"\n')
        shutil.copyfile(ROOT / "rust-toolchain.toml", self.root / "rust-toolchain.toml")
        self.env = {k: v for k, v in os.environ.items()
                    if k in ("CARGO_HOME", "RUSTUP_HOME")
                    or not k.startswith(("GIT_", "PRE_COMMIT", "CARGO_", "RUST"))}
        # Keep installation homes, but strip caller environment overrides for compiler/flags/cache.
        self.env.update(CARGO_TARGET_DIR=str(self.root / "target"),
                        CARGO_BUILD_BUILD_DIR=str(self.root / "target"), CARGO_NET_OFFLINE="true")
        self.run_command(["git", "init", "-q"], 0)

    def run_command(self, argv: list[str], expected: int) -> str:
        result = subprocess.run(argv, cwd=self.root, env=self.env, text=True,
                                capture_output=True, timeout=45)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expected, output)
        return output

    def write_source(self, source: str) -> None:
        self.source.write_text(source)

    def gate(self, expected: int, message: str) -> None:
        sources = (self.source, self.binary_source)
        before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in sources]
        output = self.run_command(["bash", str(GATE)], expected)
        self.assertIn(message, output)
        self.assertEqual([(p.read_bytes(), p.stat().st_mtime_ns) for p in sources], before,
                         "wildcard gate must not modify source bytes or mtime")

    def test_needed_expectation_passes_without_touching_source(self) -> None:
        # An unsuppressed wildcard is still advisory, not a reason for this gate to reject.
        self.write_source(NEEDED + "\npub fn advisory(c: Choice) -> u8 {\n"
                          "    match c { Choice::A => 0, _ => 1 }\n}\n")
        self.gate(0, "all still needed")
        self.gate(0, "all still needed")

    def test_unfulfilled_expectation_is_rejected_after_other_flags_and_when_warm(self) -> None:
        self.write_source(UNNEEDED)
        # Seed a real cache whose successful output lacks the diagnostic. The gate must not reuse
        # that answer after changing lint arguments, or lose the diagnostic on its own warm rerun.
        output = self.run_command(
            ["cargo", "clippy", "--offline", "--workspace", "--lib", "--bins",
             "--message-format=short", "--", "-A", "unfulfilled_lint_expectations"], 0)
        self.assertNotIn("this lint expectation is unfulfilled", output)
        self.gate(1, "no longer needed")
        self.gate(1, "no longer needed")

    def test_compilation_failure_is_not_a_clean_expectation_check(self) -> None:
        self.write_source(NEEDED + "this is not Rust;\n")
        self.gate(1, "clippy did not complete")


if __name__ == "__main__":
    unittest.main()
