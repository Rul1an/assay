#!/usr/bin/env python3
"""Behavioral regression tests for the action-consumer stage split."""
from pathlib import Path
import json
import subprocess
import unittest
import importlib.util
import shutil
import tempfile
import os

ROOT = Path(__file__).resolve().parents[2]


class HookStages(unittest.TestCase):
    def test_commit_runs_only_live_wrapper(self):
        # The baseline commit hook runs the expensive battery; this must fail
        # before changing the hook, without requiring an unimplemented checker.
        raw = subprocess.check_output(
            ['ruby', '-rjson', '-ryaml', '-e',
             'puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))'],
            input=(ROOT / '.pre-commit-config.yaml').read_bytes(),
        )
        config = json.loads(raw)
        hooks = [hook for repo in config['repos'] if repo['repo'] == 'local'
                 for hook in repo['hooks'] if hook['id'] == 'assay-action-consumer-pin']
        self.assertEqual(len(hooks), 1)
        self.assertEqual(hooks[0]['entry'], 'bash scripts/ci/check-assay-action-consumer-live.sh')
        self.assertEqual(hooks[0]['stages'], ['pre-commit'])


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
                (paths[0], 'stages: [pre-commit]\n        files: ^', 'stages: [manual]\n        files: ^'),
                (paths[0], 'stages: [pre-push]\n        files: ^', 'stages: [pre-commit]\n        files: ^'),
                (paths[0], 'always_run: true\n        stages: [pre-commit]', 'always_run: false\n        stages: [pre-commit]'),
                (paths[0], 'files: ^\n\n      - id: assay-action-consumer-pin-mutations', 'files: ^$\n\n      - id: assay-action-consumer-pin-mutations'),
                (paths[0], 'entry: bash scripts/ci/check-assay-action-consumer-live.sh', 'entry: echo bash scripts/ci/check-assay-action-consumer-live.sh'),
                (paths[0], 'entry: bash scripts/ci/test-check-assay-action-pin.sh', 'entry: bash scripts/ci/test-action-discovery-junction.sh --live-only'),
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
                    self.assertEqual(calls[2], 'check-assay-action-consumer-compat.py .github/dependabot.yml docs/PINNED-ACTIONS.md CHANGELOG.md')
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
