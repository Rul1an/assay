#!/usr/bin/env python3
"""Clippy selection uses the real pushed range; fake Cargo proves selection, not linting."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HOOK = 'scripts/precommit/cargo-clippy.sh'
CLASSIFIER = 'scripts/ci/classify-lightweight-changes.sh'
LIB = 'scripts/ci/lib/pushed-range-changes.sh'


class ClippyPushRange(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(prefix='assay-clippy-range-')
        self.addCleanup(tmp.cleanup)
        self.top = Path(tmp.name)
        self.repo = self.top / 'repo'
        self.repo.mkdir()
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(('GIT_', 'PRE_COMMIT', 'CARGO_', 'ASSAY_', 'CLIPPY_'))}
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1',
                        GIT_AUTHOR_NAME='fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                        GIT_COMMITTER_NAME='fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid')
        for rel in (HOOK, CLASSIFIER, LIB):
            if (ROOT / rel).exists():
                self.write(rel, (ROOT / rel).read_text())
        self.write('crates/probe/src/lib.rs', 'pub fn probe() {}\n')
        self.write('docs/archive/history.md', 'initial\n')
        self.git('init', '-q', '-b', 'main')
        self.base = self.commit('base')
        self.write('crates/probe/src/lib.rs', 'pub fn changed() {}\n')
        self.heavy = self.commit('rust change')
        self.git('branch', 'tracking', self.heavy)
        self.git('branch', '--set-upstream-to=tracking', 'main')
        self.write('docs/archive/history.md', 'documented\n')
        self.head = self.commit('archive change')
        stub = self.top / 'cargo-home/bin/cargo'
        stub.parent.mkdir(parents=True)
        stub.write_text('#!/bin/sh\nprintf "cargo %s\\n" "$*" >> "$STUB_LOG"\nexit 0\n')
        stub.chmod(0o755)
        self.calls = self.top / 'calls'
        self.env.update(CARGO_HOME=str(stub.parent.parent), CLIPPY_TIMEOUT='0',
                        CARGO_TARGET_DIR=str(self.top / 'target'), STUB_LOG=str(self.calls))
        found = subprocess.check_output(['bash', '-c',
            'export PATH="$CARGO_HOME/bin:$PATH"; command -v cargo'], env=self.env, text=True).strip()
        self.assertEqual(found, str(stub), 'refuse a real cargo invocation')

    def write(self, rel, text):
        path = self.repo / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def git(self, *args):
        return subprocess.check_output(['git', '-c', 'core.hooksPath=/dev/null',
            '-c', 'commit.gpgsign=false', *args], cwd=self.repo, env=self.env,
            text=True, stderr=subprocess.PIPE).strip()

    def commit(self, message):
        self.git('add', '-A')  # Only this disposable fixture.
        self.git('commit', '-q', '-m', message)
        return self.git('rev-parse', 'HEAD')

    def check(self, called, **overrides):
        env = dict(self.env, PRE_COMMIT_FROM_REF=self.base, PRE_COMMIT_TO_REF=self.head)
        env.update(overrides)
        env = {k: v for k, v in env.items() if v is not None}
        self.calls.write_text('')
        result = subprocess.run(['bash', HOOK], cwd=self.repo, env=env,
                                capture_output=True, text=True, timeout=15)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(bool(self.calls.read_text()), called, output)
        if called:
            self.assertIn('cargo clippy --workspace --all-targets', self.calls.read_text())
            self.assertNotIn('lightweight-only diff detected', output)
        else:
            self.assertIn('lightweight-only diff detected', output)

    def test_upstream_cannot_hide_a_rust_commit_in_the_push(self):
        self.check(True)

    def test_override_cannot_narrow_the_push(self):
        self.check(True, ASSAY_PREPUSH_BASE_REF=self.heavy)

    def test_actual_archive_only_range_skips_despite_wider_upstream(self):
        self.git('branch', '-f', 'tracking', self.base)
        self.check(False, PRE_COMMIT_FROM_REF=self.heavy)

    def test_manual_explicit_base_remains_available(self):
        self.check(False, PRE_COMMIT_FROM_REF=None, PRE_COMMIT_TO_REF=None,
                   ASSAY_PREPUSH_BASE_REF=self.heavy)

    def test_incomplete_or_unresolvable_push_does_not_fall_back_to_upstream(self):
        for refs in ({'PRE_COMMIT_FROM_REF': None}, {'PRE_COMMIT_TO_REF': None},
                     {'PRE_COMMIT_FROM_REF': '0' * 40}, {'PRE_COMMIT_TO_REF': 'missing-ref'},
                     {'PRE_COMMIT_FROM_REF': '', 'PRE_COMMIT_TO_REF': ''}):
            with self.subTest(refs=refs):
                self.check(True, **refs)

    def test_empty_push_range_does_not_count_as_lightweight(self):
        self.check(True, PRE_COMMIT_FROM_REF=self.head)

    def test_manual_override_does_not_hide_dirty_tracked_code(self):
        self.write('crates/probe/src/lib.rs', 'pub fn dirty() {}\n')
        self.check(True, PRE_COMMIT_FROM_REF=None, PRE_COMMIT_TO_REF=None,
                   ASSAY_PREPUSH_BASE_REF=self.heavy)

    def test_pushed_tree_must_match_checked_out_head(self):
        self.write('crates/probe/src/lib.rs', 'pub fn not_in_the_push() {}\n')
        self.commit('checked-out HEAD differs from the archive-only pushed tip')
        self.check(True, PRE_COMMIT_FROM_REF=self.heavy)

    def test_tracked_dirty_or_staged_rust_change_runs(self):
        self.write('crates/probe/src/lib.rs', 'pub fn dirty() {}\n')
        self.check(True, PRE_COMMIT_FROM_REF=self.heavy)
        self.git('add', 'crates/probe/src/lib.rs')
        self.check(True, PRE_COMMIT_FROM_REF=self.heavy)

    def test_non_fast_forward_push_runs(self):
        self.write('crates/probe/src/lib.rs', 'pub fn later() {}\n')
        later = self.commit('later rust')
        self.git('reset', '--hard', self.head)  # Keep the branch/upstream; only this fixture.
        self.check(True, PRE_COMMIT_FROM_REF=later)

    def test_diverged_push_counts_code_removed_from_the_old_tip(self):
        self.git('checkout', '-q', '-b', 'old-tip', self.heavy)
        self.write('crates/probe/src/lib.rs', 'pub fn removed_by_force_push() {}\n')
        old_tip = self.commit('old tip contains different code')
        self.git('checkout', '-q', 'main')
        self.check(True, PRE_COMMIT_FROM_REF=old_tip)

    def test_rename_from_crate_to_archive_runs(self):
        self.git('mv', 'crates/probe/src/lib.rs', 'docs/archive/moved.md')
        head = self.commit('rename')
        self.check(True, PRE_COMMIT_FROM_REF=self.head, PRE_COMMIT_TO_REF=head)

    def test_carriage_return_path_runs_full_lint(self):
        self.write('docs/archive/one\rhistory.md', 'odd filename\n')
        head = self.commit('carriage return path')
        self.check(True, PRE_COMMIT_FROM_REF=self.head, PRE_COMMIT_TO_REF=head)

    def test_newline_path_is_not_split_into_lightweight_entries(self):
        self.write('docs/archive/one\nmkdocs.yml', 'odd filename\n')
        head = self.commit('newline path')
        self.check(True, PRE_COMMIT_FROM_REF=self.head, PRE_COMMIT_TO_REF=head)


if __name__ == '__main__':
    unittest.main()
