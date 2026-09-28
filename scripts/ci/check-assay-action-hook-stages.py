#!/usr/bin/env python3
"""Pin the small executable hook contract; never interpret a shell language.

Consumer paths remain open-ended: both hooks always run. Exact fields and wrapper
bytes deliberately reject equivalent spellings. Existing required-CI policy owns
its job, interpreter, environment, command order, and reachability checks.
"""
from pathlib import Path
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'conformance/tests'))
from test_completion_scope import assert_hard_run_command  # noqa: E402

WRAPPER = '''#!/usr/bin/env bash
# Commit-time real-input checks; mutation batteries run at pre-push and in CI.
set -euo pipefail
[[ "$#" -eq 0 ]] || exit 2
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
python3 scripts/ci/check-assay-action-hook-stages.py
bash scripts/ci/check-assay-action-pin.sh
python3 scripts/ci/check-assay-action-consumer-compat.py .github/dependabot.yml docs/PINNED-ACTIONS.md CHANGELOG.md
bash scripts/ci/test-action-discovery-junction.sh --live-only
'''

# Ruby/Psych is already required by the pin and junction checkers. Reject
# duplicate/complex keys before safe_load can silently collapse a mapping.
RUBY = r'''
require "json"
require "yaml"
text = STDIN.read
walk = lambda do |node|
  if node.is_a?(Psych::Nodes::Mapping)
    seen = []
    node.children.each_slice(2) do |key, value|
      abort "complex YAML key" unless key.is_a?(Psych::Nodes::Scalar)
      abort "duplicate YAML key: #{key.value}" if seen.include?(key.value)
      seen << key.value
    end
  end
  (node.children || []).each { |child| walk.call(child) }
end
walk.call(Psych.parse_stream(text))
puts JSON.generate(YAML.safe_load(text, aliases: false))
'''


def load_yaml(text):
    result = subprocess.run(['ruby', '-EUTF-8:UTF-8', '-e', RUBY], input=text,
                            text=True, capture_output=True, timeout=30)
    if result.returncode:
        raise ValueError(result.stderr.strip() or 'YAML parse failed')
    return json.loads(result.stdout)


def check(root=ROOT):
    config = load_yaml((root / '.pre-commit-config.yaml').read_text())
    if config.get('default_install_hook_types') != ['pre-commit', 'pre-push']:
        raise ValueError('both hook types must be installed by default')
    expected = {
        'assay-action-consumer-pin': ('Assay Action consumer live checks',
                                     'bash scripts/ci/check-assay-action-consumer-live.sh',
                                     'pre-commit'),
        'assay-action-consumer-pin-mutations': ('Assay Action consumer mutation tests',
                                               'bash scripts/ci/test-check-assay-action-pin.sh',
                                               'pre-push'),
    }
    for identity, (name, entry, stage) in expected.items():
        found = [(repo, hook) for repo in config['repos'] for hook in repo.get('hooks', [])
                 if hook.get('id') == identity]
        wanted = {'id': identity, 'name': name, 'entry': entry, 'language': 'system',
                  'pass_filenames': False, 'always_run': True, 'stages': [stage], 'files': '^'}
        if len(found) != 1 or found[0][0].get('repo') != 'local' or found[0][1] != wanted:
            raise ValueError(f'{identity}: exact local entry/stage/total-selector contract differs')
    if (root / 'scripts/ci/check-assay-action-consumer-live.sh').read_text() != WRAPPER:
        raise ValueError('live wrapper effective body differs')
    workflow = (root / '.github/workflows/ci.yml').read_text()
    load_yaml(workflow)  # duplicate keys are invalid even outside the guarded step
    assert_hard_run_command(workflow, 'ci', 'Verify CI hardening contracts')


def main():
    if len(sys.argv) != 1:
        print('usage: check-assay-action-hook-stages.py', file=sys.stderr)
        return 2
    try:
        check()
    except (AssertionError, ValueError, KeyError, TypeError, OSError,
            subprocess.SubprocessError) as error:
        print(f'action hook stages: {error}', file=sys.stderr)
        return 1
    print('action hook stages: PASS')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
