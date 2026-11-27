#!/usr/bin/env python3
"""Behavioral regression tests for the action-consumer stage split."""
from pathlib import Path
import json
import subprocess
import unittest
import importlib.util
import shlex
import shutil
import tempfile
import os
import runpy
import sys
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
LIVE_ID = 'assay-action-consumer-pin'
BATTERY_ID = 'assay-action-consumer-pin-mutations'
BATTERY = 'scripts/ci/test-check-assay-action-pin.sh'
LIST_CHECKER = 'scripts/ci/check-assay-action-pin.sh'
SELECTOR = 'scripts/ci/run-assay-action-pin-battery-if-affected.sh'
STUB_BATTERY = '#!/usr/bin/env bash\nprintf "battery %s\\n" "$*" >>"$CALLS"\nexit "${STUB_RC:-0}"\n'


def configured_hooks():
    """The two action hooks exactly as .pre-commit-config.yaml declares them."""
    raw = subprocess.check_output(
        ['ruby', '-rjson', '-ryaml', '-e',
         'puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))'],
        input=(ROOT / '.pre-commit-config.yaml').read_bytes(),
    )
    config = json.loads(raw)
    hooks = [hook for repo in config['repos'] if repo['repo'] == 'local'
             for hook in repo['hooks'] if hook['id'] in (LIVE_ID, BATTERY_ID)]
    assert sorted(hook['id'] for hook in hooks) == sorted([LIVE_ID, BATTERY_ID]), hooks
    return {hook['id']: hook for hook in hooks}


def runs_at_pre_push(hook):
    # Both hooks are always_run (pinned by the stage contract), so at pre-push pre-commit selects
    # each one exactly when its stages include pre-push. No stages key means every stage.
    assert hook.get('always_run') is True, hook
    return 'pre-push' in hook.get('stages', ['pre-push'])


def entry_argv(hook):
    argv = shlex.split(hook['entry'])
    assert len(argv) == 2 and argv[0] == 'bash' and argv[1].startswith('scripts/ci/'), argv
    return argv


def scratch_env(**extra):
    """No inherited Git or pre-commit state: the outer push must not leak into the scratch repo."""
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('GIT_', 'PRE_COMMIT'))}
    env.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               GIT_AUTHOR_NAME='scratch', GIT_AUTHOR_EMAIL='scratch@example.invalid',
               GIT_COMMITTER_NAME='scratch', GIT_COMMITTER_EMAIL='scratch@example.invalid')
    env.update(extra)
    return env


def git(tree, *args, env):
    return subprocess.run(['git', '-C', str(tree), '-c', 'core.hooksPath=/dev/null',
                           '-c', 'commit.gpgsign=false', *args],
                          env=env, check=True, capture_output=True, text=True).stdout.strip()


def listed_paths():
    return subprocess.check_output(['bash', str(ROOT / LIST_CHECKER), '--list-paths'], text=True).splitlines()


class CompatibilityInterface(unittest.TestCase):
    def test_unexpected_paths_refuse_before_any_read(self):
        helper = ROOT / 'scripts/ci/check-assay-action-consumer-compat.py'
        for argument in ('/tmp/not-a-consumer-input', '../outside'):
            with self.subTest(argument=argument), \
                 mock.patch.object(sys, 'argv', [str(helper), argument]), \
                 mock.patch.object(Path, 'read_text', side_effect=AssertionError('read before argument refusal')):
                with self.assertRaises(SystemExit) as refused:
                    runpy.run_path(str(helper), run_name='__main__')
                self.assertEqual(refused.exception.code, 2)


    def test_canonical_inputs_and_each_read_are_required(self):
        helper = ROOT / 'scripts/ci/check-assay-action-consumer-compat.py'
        spec = importlib.util.spec_from_file_location('compatibility', helper)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        paths = [ROOT / '.github/dependabot.yml', ROOT / 'docs/PINNED-ACTIONS.md', ROOT / 'CHANGELOG.md']
        contents = [path.read_text() for path in paths]
        observed = []
        def read(path, **kwargs):
            observed.append(path)
            return contents[paths.index(path)]
        with mock.patch.object(sys, 'argv', [str(helper)]), mock.patch.object(Path, 'read_text', read):
            self.assertEqual(module.main(), 0)
        self.assertEqual(observed, paths)
        self.assertEqual(module.check_compatibility(*contents), [])
        for index in range(3):
            changed = list(contents)
            changed[index] = ''
            self.assertTrue(module.check_compatibility(*changed), 'removing input must refuse')


class HookStages(unittest.TestCase):
    def test_live_wrapper_runs_at_commit_and_push(self):
        hook = configured_hooks()[LIVE_ID]
        self.assertEqual(hook['entry'], 'bash scripts/ci/check-assay-action-consumer-live.sh')
        self.assertEqual(hook['stages'], ['pre-commit', 'pre-push'])


class PushSelection(unittest.TestCase):
    """Run the configured pre-push battery entry in a scratch repository with a stub battery.

    Only the push decision is under test; required CI runs the real battery on every change.
    """

    LISTED = 'docs/guides/github-action.md'
    UNRELATED = 'crates/assay-core/src/lib.rs'

    def setUp(self):
        hook = configured_hooks()[BATTERY_ID]
        self.assertTrue(runs_at_pre_push(hook))
        self.entry = entry_argv(hook)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.tree = Path(temporary.name) / 'repo'
        self.calls = Path(temporary.name) / 'calls'
        self.tmp = Path(temporary.name) / 'tmp'  # the selector's own TMPDIR, checked after every push
        self.tmp.mkdir()
        self.env = scratch_env(CALLS=str(self.calls), TMPDIR=str(self.tmp))
        files = {path: path + '\n' for path in listed_paths()}
        files.update({self.UNRELATED: '// lib\n', 'docs/x.md': 'x\n', 'docs/PINNED-ACTIONS.md': 'pins\n',
                      'CHANGELOG.md': 'changes\n', '.pre-commit-config.yaml': 'repos: []\n',
                      '.github/dependabot.yml': 'version: 2\n', 'conformance/x.py': '# x\n'})
        for path, text in files.items():
            self.write(path, text)
        for path in {LIST_CHECKER, self.entry[1]} - {BATTERY}:
            self.write(path, (ROOT / path).read_text())
        self.write(BATTERY, STUB_BATTERY)
        self.git('init', '-q', '-b', 'main')
        self.commit('base')
        self.base = self.git('rev-parse', 'HEAD')

    def write(self, path, text):
        target = self.tree / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def git(self, *args):
        return git(self.tree, *args, env=self.env)

    def commit(self, message):
        self.git('add', '-A')
        self.git('commit', '-q', '--allow-empty', '-m', message)
        return self.git('rev-parse', 'HEAD')

    def push(self, from_ref=None, to_ref=None, **extra):
        if self.calls.exists():
            self.calls.unlink()
        env = dict(self.env, PRE_COMMIT_FROM_REF=from_ref or self.base,
                   PRE_COMMIT_TO_REF=to_ref or self.git('rev-parse', 'HEAD'))
        env.update(extra)
        env = {key: value for key, value in env.items() if value is not None}
        result = subprocess.run(self.entry, cwd=self.tree, env=env, capture_output=True, text=True, timeout=60)
        self.assertEqual(sorted(self.tmp.iterdir()), [], 'selector left scratch files behind')
        return result.returncode, result.stdout + result.stderr, self.calls.exists()

    def assert_skipped(self, result):
        rc, output, called = result
        self.assertFalse(called, 'battery started for a push that changes none of its inputs:\n' + output)
        self.assertEqual(rc, 0, output)
        self.assertIn('Required CI still runs it.', output)
        self.assertNotIn('running in full', output)

    def assert_full(self, result, reason):
        rc, output, called = result
        self.assertTrue(called, 'battery did not start:\n' + output)
        self.assertEqual(rc, 0, output)
        self.assertIn(reason, output)
        # The battery replaces the wrapper; a run that also reports a skip did not hand over.
        self.assertNotIn('skipped;', output)

    def test_unrelated_change_skips(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.assert_skipped(self.push())

    def test_empty_range_on_clean_head_skips(self):
        self.assert_skipped(self.push(from_ref=self.base, to_ref=self.base))

    def test_listed_consumer_edit_runs(self):
        self.write(self.LISTED, 'changed\n')
        self.commit('edit consumer')
        self.assert_full(self.push(), 'listed consumer changed: ' + self.LISTED)

    def test_listed_consumer_delete_runs(self):
        self.git('rm', '-q', self.LISTED)
        self.commit('delete consumer')
        self.assert_full(self.push(), 'listed consumer changed: ' + self.LISTED)

    def test_listed_consumer_renamed_out_runs(self):
        (self.tree / 'docs/archive').mkdir(parents=True)
        self.git('mv', self.LISTED, 'docs/archive/github-action.md')
        head = self.commit('rename consumer away')
        # Precondition: Git itself reports this as a rename, so only the old side can trigger.
        self.assertTrue(self.git('diff', '-M', '--name-status', self.base, head).startswith('R'))
        self.assert_full(self.push(), 'listed consumer changed: ' + self.LISTED)

    def test_rename_into_input_prefix_runs(self):
        self.git('mv', 'docs/x.md', '.github/x.md')
        head = self.commit('rename into .github')
        self.assertTrue(self.git('diff', '-M', '--name-status', self.base, head).startswith('R'))
        self.assert_full(self.push(), 'battery input changed: .github/x.md')

    def test_each_input_prefix_and_file_runs(self):
        for path in ['scripts/ci/new-check.sh', 'conformance/x.py', '.github/dependabot.yml',
                     'CHANGELOG.md', 'docs/PINNED-ACTIONS.md', '.pre-commit-config.yaml']:
            with self.subTest(path=path):
                self.git('reset', '-q', '--hard', self.base)
                self.write(path, 'changed\n')
                self.commit('touch ' + path)
                self.assert_full(self.push(), 'battery input changed: ' + path)

    def test_paths_are_read_nul_separated(self):
        # Git quotes such a name in line output, so a line reader would miss the prefix.
        self.write('scripts/ci/new\nline é.sh', 'x\n')
        self.commit('odd name')
        self.assert_full(self.push(), 'battery input changed: scripts/ci/new')

    def test_missing_range_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.assert_full(self.push(PRE_COMMIT_FROM_REF=None, PRE_COMMIT_TO_REF=None), 'no pushed range')

    def test_unknown_from_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.assert_full(self.push(from_ref='1' * 40), 'cannot list the pushed changes')

    def test_unknown_to_runs(self):
        self.assert_full(self.push(to_ref='2' * 40), 'does not resolve')

    def test_unrelated_history_runs(self):
        empty_tree = self.git('hash-object', '-t', 'tree', '/dev/null')
        orphan = self.git('commit-tree', empty_tree, '-m', 'orphan')  # shares no history with main
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.assert_full(self.push(from_ref=orphan), 'cannot list the pushed changes')

    def old_tip_changing_the_listed_consumer(self):
        self.write(self.LISTED, 'changed on the old tip\n')
        old = self.commit('old tip edits the consumer')
        self.git('checkout', '-q', '--detach', self.base)
        return old

    def test_diverged_update_with_changed_listed_consumer_runs(self):
        old = self.old_tip_changing_the_listed_consumer()
        self.write(self.UNRELATED, '// changed\n')
        new = self.commit('new tip changes only a crate file')
        # Precondition: the merge-base diff misses the consumer the old tip changed.
        self.assertEqual(self.git('diff', '--no-renames', '--name-only', f'{old}...{new}'), self.UNRELATED)
        self.assert_full(self.push(from_ref=old, to_ref=new), 'is not a fast-forward')

    def test_backward_update_with_changed_listed_consumer_runs(self):
        old = self.old_tip_changing_the_listed_consumer()
        self.assertEqual(self.git('diff', '--no-renames', '--name-only', f'{old}...{self.base}'), '')
        self.assert_full(self.push(from_ref=old, to_ref=self.base), 'is not a fast-forward')

    def test_non_fast_forward_run_keeps_the_battery_exit_code(self):
        old = self.old_tip_changing_the_listed_consumer()
        rc, output, called = self.push(from_ref=old, to_ref=self.base, STUB_RC='37')
        self.assertTrue(called, output)
        self.assertEqual(rc, 37, output)

    def test_undecidable_ancestry_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        shim = self.tree.parent / 'git-shim'  # outside the repository, so the tree stays clean
        shim.mkdir()
        real = shutil.which('git', path=self.env['PATH'])
        (shim / 'git').write_text('#!/usr/bin/env bash\n'
                                  '[[ "$1 $2" != "merge-base --is-ancestor" ]] || exit 128\n'
                                  f'exec {shlex.quote(real)} "$@"\n')
        (shim / 'git').chmod(0o755)
        self.assert_full(self.push(PATH=f'{shim}{os.pathsep}{self.env["PATH"]}'), 'cannot decide whether')

    def test_pushed_commit_that_is_not_head_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        pushed = self.commit('unrelated')
        self.git('checkout', '-q', '--detach', self.base)
        self.assert_full(self.push(to_ref=pushed), 'is not the checked-out HEAD')

    def test_staged_tracked_change_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.write(self.UNRELATED, '// staged, not committed\n')
        self.git('add', self.UNRELATED)
        self.assert_full(self.push(), 'tracked files differ from HEAD')

    def test_staged_change_with_clean_worktree_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.write(self.UNRELATED, '// staged only\n')
        self.git('add', self.UNRELATED)
        self.write(self.UNRELATED, '// changed\n')  # the worktree matches HEAD again; the index does not
        self.assert_full(self.push(), 'tracked files differ from HEAD')

    def test_unstaged_tracked_change_runs(self):
        self.write(self.UNRELATED, '// changed\n')
        self.commit('unrelated')
        self.write(self.UNRELATED, '// edited, not staged\n')
        self.assert_full(self.push(), 'tracked files differ from HEAD')

    def replace_list(self, body):
        self.write(LIST_CHECKER, '#!/usr/bin/env bash\n' + body)
        head = self.commit('replace consumer list')
        return self.push(from_ref=head, to_ref=head)

    def test_failing_consumer_list_runs(self):
        self.assert_full(self.replace_list('exit 1\n'), 'consumer list command failed')

    def test_empty_consumer_list_runs(self):
        self.assert_full(self.replace_list('exit 0\n'), 'consumer list is empty')

    def test_invalid_consumer_list_entry_runs(self):
        for entry in ['/etc/passwd', '../outside.md', 'docs/../outside.md', 'docs/..']:
            with self.subTest(entry=entry):
                self.git('reset', '-q', '--hard', self.base)
                self.assert_full(self.replace_list(f"printf '%s\\n' docs/index.md '{entry}'\n"),
                                 'consumer list has an invalid entry')
        self.git('reset', '-q', '--hard', self.base)
        self.assert_full(self.replace_list("printf 'docs/index.md\\n\\ndocs/README.md\\n'\n"),
                         'consumer list has an invalid entry')

    def test_battery_failure_propagates(self):
        self.write(self.LISTED, 'changed\n')
        self.commit('edit consumer')
        rc, output, called = self.push(STUB_RC='1')
        self.assertTrue(called, output)
        self.assertEqual(rc, 1, output)


class SelectorInterface(unittest.TestCase):
    def test_refuses_arguments_before_running_anything(self):
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary)
            for path in (SELECTOR, LIST_CHECKER):
                (tree / path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / path, tree / path)
            (tree / BATTERY).write_text(STUB_BATTERY)
            calls = tree / 'calls'
            result = subprocess.run(['bash', str(tree / SELECTOR), '--all'], cwd=tree,
                                    env=scratch_env(CALLS=str(calls)), capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertFalse(calls.exists())


class LiveAtPush(unittest.TestCase):
    def test_push_adding_an_unlisted_consumer_is_refused_by_a_selected_hook(self):
        hooks = configured_hooks()
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary) / 'repo'
            for directory in ['scripts/ci', '.github', 'conformance']:
                shutil.copytree(ROOT / directory, tree / directory, ignore=shutil.ignore_patterns('__pycache__'))
            for path in [*listed_paths(), 'docs/PINNED-ACTIONS.md', 'CHANGELOG.md', '.pre-commit-config.yaml']:
                (tree / path).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / path, tree / path)
            (tree / BATTERY).write_text(STUB_BATTERY)
            calls = Path(temporary) / 'calls'
            (Path(temporary) / 'tmp').mkdir()
            env = scratch_env(CALLS=str(calls), TMPDIR=str(Path(temporary) / 'tmp'))
            git(tree, 'init', '-q', '-b', 'main', env=env)
            git(tree, 'add', '-A', env=env)
            git(tree, 'commit', '-q', '-m', 'base', env=env)
            base = git(tree, 'rev-parse', 'HEAD', env=env)
            consumer = tree / 'docs/new-consumer-h4.md'
            consumer.write_text('\n```yaml\n- uses: Rul1an/assay-action@v3\n```\n')
            git(tree, 'add', '-A', env=env)
            git(tree, 'commit', '-q', '-m', 'unlisted consumer', env=env)
            head = git(tree, 'rev-parse', 'HEAD', env=env)
            env.update(PRE_COMMIT_FROM_REF=base, PRE_COMMIT_TO_REF=head)
            results = []
            for hook in (hooks[LIVE_ID], hooks[BATTERY_ID]):  # configuration order
                if runs_at_pre_push(hook):
                    result = subprocess.run(entry_argv(hook), cwd=tree, env=env, capture_output=True,
                                            text=True, timeout=120)
                    results.append((hook['id'], result.returncode, result.stdout + result.stderr))
            refusal = 'docs/new-consumer-h4.md: assay-action uses is not on the owner snippet list'
            refused = [identity for identity, rc, output in results if rc != 0 and refusal in output]
            self.assertEqual(refused, [LIVE_ID], results)


def checker():
    spec = importlib.util.spec_from_file_location('hook_contract', ROOT / 'scripts/ci/check-assay-action-hook-stages.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class EffectiveWiring(unittest.TestCase):
    def test_real_contract_and_behavioral_negatives(self):
        guard = checker()
        guard.check()
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary)
            paths = ['.pre-commit-config.yaml', '.github/workflows/ci.yml',
                     'scripts/ci/check-assay-action-consumer-live.sh']
            originals = {path: (ROOT / path).read_text() for path in paths}
            def reset():
                for path, text in originals.items():
                    target = tree / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(text)
            cases = [
                (paths[0], 'stages: [pre-commit, pre-push]\n        files: ^', 'stages: [manual]\n        files: ^'),
                (paths[0], 'stages: [pre-commit, pre-push]\n        files: ^', 'stages: [pre-commit]\n        files: ^'),
                (paths[0], 'stages: [pre-push]\n        files: ^', 'stages: [pre-commit]\n        files: ^'),
                (paths[0], 'always_run: true\n        stages: [pre-commit, pre-push]', 'always_run: false\n        stages: [pre-commit, pre-push]'),
                (paths[0], 'always_run: true\n        verbose: true', 'always_run: false\n        verbose: true'),
                (paths[0], '        verbose: true\n', ''),
                (paths[0], 'files: ^\n\n      - id: assay-action-consumer-pin-mutations', 'files: ^$\n\n      - id: assay-action-consumer-pin-mutations'),
                (paths[0], 'entry: bash scripts/ci/check-assay-action-consumer-live.sh', 'entry: echo bash scripts/ci/check-assay-action-consumer-live.sh'),
                (paths[0], 'entry: bash scripts/ci/run-assay-action-pin-battery-if-affected.sh', 'entry: bash scripts/ci/test-action-discovery-junction.sh --live-only'),
                (paths[0], 'name: Assay Action consumer live checks', 'name: Assay Action consumer live checks\n        exclude: .*'),
                (paths[0], 'name: Assay Action consumer live checks', 'name: Assay Action consumer live checks\n        entry: true'),
                (paths[1], '          bash scripts/ci/test-check-assay-action-pin.sh', '          echo bash scripts/ci/test-check-assay-action-pin.sh'),
                (paths[1], '          bash scripts/ci/check-assay-action-consumer-live.sh', '          exit 0\n          bash scripts/ci/check-assay-action-consumer-live.sh'),
                (paths[1], '      - name: Verify CI hardening contracts\n        shell: bash', '      - name: Verify CI hardening contracts\n        shell: echo {0}'),
                (paths[1], '      - name: Verify CI hardening contracts', '      - name: Verify CI hardening contracts\n        if: false'),
                (paths[1], '      - name: Verify CI hardening contracts', '      - name: Verify CI hardening contracts\n        continue-on-error: true'),
                (paths[2], 'bash scripts/ci/check-assay-action-pin.sh', '# bash scripts/ci/check-assay-action-pin.sh'),
                (paths[2], 'bash scripts/ci/test-action-discovery-junction.sh --live-only', ': <<EOF\nbash scripts/ci/test-action-discovery-junction.sh --live-only\nEOF'),
                (paths[2], 'set -euo pipefail', 'exit 0\nset -euo pipefail'),
            ]
            for path, before, after in cases:
                with self.subTest(path=path, mutation=after):
                    reset()
                    self.assertIn(before, originals[path])
                    offset = originals[path].index('      - id: assay-action-consumer-pin\n') if path == paths[0] else 0
                    prefix, subject = originals[path][:offset], originals[path][offset:]
                    self.assertIn(before, subject)
                    (tree / path).write_text(prefix + subject.replace(before, after, 1))
                    with self.assertRaises((ValueError, AssertionError)):
                        guard.check(tree)
            reset()
            # A harmless configuration comment remains green.
            (tree / paths[0]).write_text('# control comment\n' + originals[paths[0]])
            guard.check(tree)
            config = guard.load_yaml(originals[paths[0]])
            repo = next(repo for repo in config['repos'] if any(h['id'] == 'assay-action-consumer-pin' for h in repo.get('hooks', [])))
            repo['repo'] = 'https://example.invalid/foreign-hooks'
            (tree / paths[0]).write_text(json.dumps(config))
            with self.assertRaises(ValueError):
                guard.check(tree)

    def test_actual_wrapper_propagates_each_child_failure(self):
        children = ['scripts/ci/check-assay-action-hook-stages.py',
                    'scripts/ci/check-assay-action-pin.sh',
                    'scripts/ci/check-assay-action-consumer-compat.py',
                    'scripts/ci/test-action-discovery-junction.sh']
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary)
            wrapper = tree / 'scripts/ci/check-assay-action-consumer-live.sh'
            wrapper.parent.mkdir(parents=True)
            shutil.copy2(ROOT / wrapper.relative_to(tree), wrapper)
            record = tree / 'calls'
            for child in children:
                target = tree / child
                if child.endswith('.py'):
                    target.write_text('import os,sys\nfrom pathlib import Path\np=Path(os.environ["CALLS"])\nwith p.open("a") as f:f.write(Path(__file__).name+" "+" ".join(sys.argv[1:])+"\\n")\nraise SystemExit(37 if os.environ["FAIL_CHILD"]==Path(__file__).name else 0)\n')
                else:
                    target.write_text('#!/usr/bin/env bash\nprintf "%s %s\\n" "${0##*/}" "$*" >>"$CALLS"\n[[ "$FAIL_CHILD" != "${0##*/}" ]] || exit 37\n')
            for fail in ['', *[Path(child).name for child in children]]:
                if record.exists(): record.unlink()
                env = {**os.environ, 'CALLS': str(record), 'FAIL_CHILD': fail}
                result = subprocess.run(['bash', str(wrapper)], env=env, capture_output=True, text=True)
                expected_count = len(children) if not fail else [Path(child).name for child in children].index(fail) + 1
                calls = record.read_text().splitlines()
                self.assertEqual(result.returncode, 0 if not fail else 37, result.stderr)
                self.assertEqual([line.split()[0] for line in calls], [Path(c).name for c in children[:expected_count]])
                if not fail:
                    self.assertEqual(calls[2].split(), ['check-assay-action-consumer-compat.py'])
                    self.assertEqual(calls[3], 'test-action-discovery-junction.sh --live-only')


class LiveInputs(unittest.TestCase):
    def test_real_inputs_still_refuse_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            tree = Path(temporary)
            for directory in ['scripts/ci', '.github', 'conformance']:
                shutil.copytree(ROOT / directory, tree / directory, ignore=shutil.ignore_patterns('__pycache__'))
            paths = subprocess.check_output(['bash', str(ROOT / 'scripts/ci/check-assay-action-pin.sh'), '--list-paths'], text=True).splitlines()
            for path in [*paths, 'docs/PINNED-ACTIONS.md', 'CHANGELOG.md', '.pre-commit-config.yaml']:
                target = tree / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / path, target)
            wrapper = tree / 'scripts/ci/check-assay-action-consumer-live.sh'
            def run():
                result = subprocess.run(['bash', str(wrapper)], cwd=tree, text=True, capture_output=True, timeout=90)
                return result.returncode, result.stdout + result.stderr
            rc, output = run()
            self.assertEqual(rc, 0, output)
            for witness in ['owned-producer-behavioral', 'fixture-discover-default-three-level',
                            'fixture-discover-explicit-glob-bounded', 'docs-globstar-and-dry-run-qualifiers']:
                self.assertIn(witness, output)
            self.assertNotIn('mutation-', output)
            self.assertNotIn('mock-esrch', output)
            # New paths and CHANGELOG fences are open discovery, not an owner list.
            snippet = '\n```yaml\n- uses: Rul1an/assay-action@v3\n```\n'
            cases = [
                ('docs/new-consumer-3141.md', lambda s: s + snippet, 'is not on the owner snippet list'),
                ('CHANGELOG.md', lambda s: s + snippet, 'is not on the owner snippet list'),
                ('.github/dependabot.yml', lambda s: s.replace('Rul1an/assay-action', 'assay-dev/assay-action'), 'Dependabot'),
                ('docs/guides/github-action.md', lambda s: s.replace('enable `globstar`', 'enable recursive matching'), 'docs must name globstar'),
                ('scripts/ci/produce-default-discovery-sandbox-evidence.sh', lambda s: s.replace('set -euo pipefail', 'exit 0\nset -euo pipefail'), 'owned producer effective body mismatch'),
            ]
            for path, mutate, diagnostic in cases:
                with self.subTest(path=path):
                    target = tree / path
                    original = target.read_text() if target.exists() else None
                    target.write_text(mutate(original or ''))
                    rc, output = run()
                    self.assertNotEqual(rc, 0, output)
                    self.assertIn(diagnostic, output)
                    if original is None: target.unlink()
                    else: target.write_text(original)
            # Execute the actual selected fixture discovery body in live mode.
            # A broken default find must fail here, beyond the static presence checks.
            fixture = tree / 'scripts/ci/fixtures/assay-action-pin/action.yml'
            original_fixture = fixture.read_text()
            self.assertEqual(original_fixture.count('          find . '), 1)
            fixture.write_text(original_fixture.replace('          find . ', '          find . -maxdepth 3 '))
            result = subprocess.run(['bash', str(tree / 'scripts/ci/test-action-discovery-junction.sh'), '--live-only'],
                                    cwd=tree, text=True, capture_output=True, timeout=60)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('default fixture discover must hit top+mid+deep', result.stdout + result.stderr)
            fixture.write_text(original_fixture)
            rc, output = run()
            self.assertEqual(rc, 0, output)


if __name__ == '__main__':
    unittest.main()
