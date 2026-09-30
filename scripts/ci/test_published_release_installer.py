#!/usr/bin/env python3
"""Behavioral controls for the retained, credential-free streaming install."""
import unittest
import os
import shutil
import tempfile
import time
import json
import sys
from unittest import mock
import published_release_installer as subject
from cosign_release_pin import read_pin
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class InstallerContract(unittest.TestCase):
    def test_installer_produced_cli_drives_journey(self):
        driver = (ROOT / 'scripts/ci/published-release-golden-path.sh').read_text()
        self.assertIn('published_release_installer.py', driver,
                      'Unix journey still copies the archive CLI instead of running the served installer')
        self.assertNotIn('cp "${cli_candidates[0]}" "$install_root/bin/assay"', driver)
        self.assertLess(driver.index('published_release_installer.py'), driver.index('run_capture "assay-version"'))

    def test_entry_rejects_arguments_before_filesystem_reads(self):
        with mock.patch.object(sys, 'argv', ['installer', '../foreign']), \
             mock.patch.object(subject.Path, 'read_text', side_effect=AssertionError('unexpected read')):
            with self.assertRaisesRegex(SystemExit, 'usage:'):
                subject.main()

    def test_missing_metadata_retains_failure_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = root / 'harness/scripts/ci/published_release_installer.py'
            helper.parent.mkdir(parents=True)
            (root / 'results').mkdir()
            with mock.patch.object(subject, '__file__', str(helper)), \
                 mock.patch.object(sys, 'argv', ['installer']):
                with self.assertRaises(FileNotFoundError):
                    subject.main()
            receipt = json.loads((root / 'results/installer/receipt.json').read_text())
            self.assertEqual(receipt['status'], 'failed')
            self.assertIn('release-api.json', receipt['failure'])

    def test_required_ci_job_wiring_and_removal_control(self):
        # Parsed-object pin (round 5, reviewer F7): the round-3/round-4 guard
        # matched the job as TEXT -- first occurrence of the header, slice to
        # the next job header, compare against a literal. The reviewer beat it
        # without touching the text the pin read: a top-level `run-name: |2`
        # block scalar before `jobs:` holding a verbatim copy of the pinned
        # block satisfied every "header occurs once" / "after jobs:" style
        # patch, while the real job's run line gained `|| true`. Every guard
        # stayed green while PyYAML and Psych both parsed the real job as
        # neutralised. Matching YAML as text is the open mechanism, so the
        # mechanism changes from text to parse: ci.yml is loaded with Ruby
        # Psych in a subprocess exactly like
        # scripts/ci/test-ci-job-timeouts-contract.sh
        # (`YAML.safe_load_file(path, aliases: false)`, JSON over stdout),
        # and the assertion is on the parsed
        # `jobs["published-release-golden-path-contract"]` object -- exact key
        # set at every level, exact step shapes and values. A decoy scalar, a
        # comment, or any other spelling that parses to the same object stays
        # green by design (it changes nothing GitHub executes); any spelling
        # that parses to a different object fails. Earlier rounds enumerated
        # spellings (round 1-2: forbidden keys; round 3: text closure); this
        # round pins what the runner reads.
        #
        # Self-check (reviewer's question, in our words): which way of writing
        # ci.yml makes GitHub's parsed job differ from the object this test
        # pins? Pure-spelling variants (quotes, comments, key order, flow
        # style) parse identically, so they cannot differ -- green by design.
        # The one shape where two YAML engines may legitimately disagree is a
        # duplicated job key (Psych takes the last silently; another engine
        # could take the first), so the test additionally requires every
        # jobs key unique on the Psych parse tree, case-insensitively, and
        # the contract key exactly once -- a narrow uniqueness guard, never
        # a content pin (issue #3262: counting the key as text missed
        # quoted, tagged, explicit-`?` and case-variant spellings). Anchors
        # on keys fail closed in the tree guard and aliases in values are
        # rejected by the load itself (`aliases: false`, fail closed --
        # GitHub rejects anchors too); explicit tags on plain scalars are
        # dropped by Psych without changing the value, so a tag alone cannot
        # neutralise while staying green -- neutralising still needs a value
        # change, which the object equality catches.
        #
        # Parser-gap challenge (coordinator's suggestion, challenged before
        # building): Psych is not GitHub's parser, so the pin trusts both to
        # read the same job object for anchor-free, tag-free, duplicate-free
        # mappings -- the shape this file is in, enforced by the load flags
        # plus the uniqueness guard. What falls outside: the top-level `on:`
        # key parses as boolean true under Psych's YAML 1.1 rules, so this
        # test never reads the trigger subtree, only `jobs`; line comments
        # are inert to both. If ruby is missing or the load fails for any
        # reason, the loader raises AssertionError -- red, never green (fail
        # closed); the ruby-missing and corrupt-YAML controls below prove it.
        # The duplicate-key probe (`{"a"=>2}`, last wins) and the
        # aliases-rejected probe were observed on ruby 3.3.3/Psych 5, not
        # assumed.
        #
        # Pins float by suffix only (round 4, reviewer F4, unchanged): the
        # `uses:` repositories stay literal (`actions/checkout`,
        # `actions/setup-python`); only the 40-hex SHA suffix floats, and each
        # must equal the same action's suffix in the parsed
        # `release-asset-contract` steps, which Dependabot bumps in the same
        # PR. The joint-bump control below stays green to prove the pin does
        # not freeze SHAs; a bump applied to only one of the two jobs fails
        # by design -- resync both lines. A suffix that is not a 40-hex SHA
        # fails outright, so `@v5` or a bare-SHA foreign action never matches.
        # No SHA is copied into this file, so nothing here can go stale
        # (reviewer F6). A YAML parser drops `# vX.Y.Z` comments, so the
        # version convention is pinned by a narrow text check: each parsed SHA
        # must appear in the file text as `actions/<repo>@<sha> # vX.Y.Z`.
        # That check deliberately does NOT locate the job's line -- doing so
        # would rebuild the F7 text locator. The SHA is pinned by the parsed
        # object; the comment itself is execution-inert, so a comment moved to
        # a decoy line changes nothing GitHub runs.
        #
        # Round-4 correction (reviewer F8): swapping the checkout and
        # setup-python SHA suffixes in both jobs stays GREEN here and always
        # did -- the earlier report claim that it fails was wrong. It is not
        # a bypass: `actions/checkout@<setup-python-sha>` names a ref that
        # does not exist in that repository, so GitHub fails to resolve the
        # action and the job goes red at runtime (fail closed). Pinning the
        # swap here would need an external oracle for which SHA belongs to
        # which repo, which is exactly what the bump-PR review is; the
        # swapped-suffixes control below locks in the green so the wrong claim
        # is never re-made. Residual trust, stated plainly: this test proves
        # which repos run and that both jobs agree -- not that the SHAs sit
        # on the right repo lines.
        #
        # Issue #3262 (reviewer F9 on #3263): the exactly-once guard above was a
        # TEXT regex (`^  <job>\s*:`). A neutralised copy first under a
        # differently spelled key -- single-quoted, double-quoted, `!!str`
        # tagged, or explicit `? ` key -- with the pristine job last passes
        # the object pin (Psych is last-wins) while the regex still counts
        # one. The rule for this repo (from #3260, #3204) is parse, never
        # match text, when a test asserts what a workflow does -- so the
        # regex is replaced by a uniqueness check on the Psych PARSE TREE:
        # `Psych.parse_file` down to the top-level `jobs` mapping node, every
        # key node's scalar value collected, compared case-insensitively
        # (actionlint treats job ids that way; GitHub rejects duplicates),
        # over ALL job keys, not only this one.
        #
        # Challenge of the suggestion, before building: a key the tree
        # reports differently can still be the same job for GitHub, so style
        # alone cannot fail anything. Single/double quotes, `!!str` tags and
        # the explicit `? key` form all parse to the same Scalar value
        # (observed on ruby 3.3.3/Psych 5.1.2), so they count by value and a
        # pure-spelling rename stays green by design. What fails closed
        # instead: any jobs key node that is not a Scalar (a Sequence key
        # loads under safe_load while JSON mangles it -- observed -- so the
        # loaded Hash cannot see it), any key carrying an anchor (an unused
        # anchor loads silently while GitHub rejects anchors -- observed),
        # and any `<<` key (merge semantics need an explicit value check; no
        # class check can see them). Alias values stay refused by the
        # `aliases: false` load itself. Residuals, stated plainly: Unicode
        # case pairs where Python `lower()` disagrees with GitHub's
        # comparison (unreachable for valid job ids, which are ASCII-only).
        # Round 2 closes the two cheap residuals the reviewer named: G1, a
        # duplicated top-level `jobs:` key (same last-wins mechanism one
        # level up -- the guard now aborts unless exactly one root-mapping
        # key equals `jobs` case-insensitively by scalar value, anchor-free,
        # the same way job keys are counted); G2, a second YAML document
        # (parse_file/safe_load_file read only the first, actionlint stays
        # green -- the guard now aborts unless the stream holds exactly one
        # document). A top-level merge key injecting `jobs` stays fail-closed
        # on both sides (alias refusal here, anchor rejection on GitHub),
        # never green.
        #
        # Self-check answer (rounds 4-5): no `uses:`-line change in one job or
        # both substitutes another program while staying green -- the repo is
        # literal and each suffix must be well-formed and match per-action.
        # The only green `uses:` changes are a joint same-repo SHA bump
        # (Dependabot's case) and the cross-repo SHA swap above (caught at
        # resolve time, not here), so the residual trust is in review of the
        # bump PR itself.
        #
        # Out of scope (unchanged): workflow-level `env:` / `defaults:` would
        # neutralise the `ci` gate's own evaluate step too; no per-job pin can
        # own that.
        #
        # False-red warning for the next builder: this trades flexibility for
        # closure -- ANY semantic edit to the job object, even a benign one
        # (renamed step `name:`, `timeout-minutes` bump, runner-label change,
        # `concurrency:`), fails this test by design. Pure-spelling edits
        # (comments, quoting, key order) now stay green -- that is the point
        # of round 5. Update the expected object deliberately after verifying
        # the edit keeps the battery executing and failing red.
        job = 'published-release-golden-path-contract'
        reference = 'release-asset-contract'
        command = 'bash scripts/ci/test-published-release-golden-path-contract.sh'
        variable = 'PUBLISHED_RELEASE_GOLDEN_PATH_CONTRACT_RESULT'
        binding_value = '${{ needs.' + job + '.result }}'
        triple = '"' + job + '|${' + variable + '}|required"'
        evaluate_step = 'Evaluate required job results'
        # Same load shape as scripts/ci/test-ci-job-timeouts-contract.sh:
        # ruby reads the script on stdin (`-`), the workflow path comes from
        # ARGV, aliases stay off, the document crosses to Python as JSON.
        ruby_loader = (
            'path = ARGV.fetch(0)\n'
            'doc = YAML.safe_load_file(path, aliases: false)\n'
            'abort "ci.yml must be a mapping" unless doc.is_a?(Hash)\n'
            'puts JSON.generate(doc)\n'
        )
        # Parse-tree job-key guard (issue #3262): one predicate, one place.
        # Ruby extracts the `jobs` key nodes structurally; Python below
        # decides uniqueness. Round 2: the stream must hold exactly one
        # document (G2) and exactly one root-mapping key may equal `jobs`
        # case-insensitively by scalar value, anchor-free (G1) -- the pinned
        # node is that unique match, never last-wins. A GUARD: abort is the
        # guard's own red verdict and is relayed as-is; any other ruby
        # failure is fail-closed.
        ruby_key_guard = (
            'path = ARGV.fetch(0)\n'
            'stream = Psych.parse_stream(File.read(path))\n'
            'abort "GUARD:ci.yml must contain a single YAML document"'
            ' unless stream.children.size == 1\n'
            'tree = Psych.parse_file(path)\n'
            'root = tree.root\n'
            'abort "GUARD:ci.yml job-key guard is fail-closed: top-level mapping expected"'
            ' unless root.is_a?(Psych::Nodes::Mapping)\n'
            'pairs = root.children.each_slice(2).to_a\n'
            'top = pairs.select do |k, _v|\n'
            '  k.is_a?(Psych::Nodes::Scalar) && k.value.downcase == "jobs"\n'
            'end\n'
            'abort "GUARD:ci.yml top-level jobs key must occur exactly once"'
            ' unless top.size == 1\n'
            'abort "GUARD:ci.yml top-level jobs keys must not carry anchors"'
            ' unless top.first.first.anchor.nil?\n'
            'jobs_node = top.first.last\n'
            'entries = []\n'
            'if jobs_node.is_a?(Psych::Nodes::Mapping)\n'
            '  jobs_node.children.each_slice(2) do |k, _v|\n'
            '    abort "GUARD:ci.yml jobs keys must be plain scalars"'
            ' unless k.is_a?(Psych::Nodes::Scalar)\n'
            '    abort "GUARD:ci.yml jobs contains merge key" if k.value == "<<"\n'
            '    abort "GUARD:ci.yml jobs keys must not carry anchors" unless k.anchor.nil?\n'
            '    entries << k.value\n'
            '  end\n'
            'end\n'
            'puts JSON.generate(entries)\n'
        )
        def run_ruby(script, yaml_text, guard_name):
            import subprocess
            import tempfile as tempfile_module
            with tempfile_module.TemporaryDirectory() as directory:
                candidate = Path(directory) / 'ci.yml'
                candidate.write_text(yaml_text)
                try:
                    return subprocess.run(
                        ['ruby', '-ryaml', '-rjson', '-', str(candidate)],
                        input=script, capture_output=True, text=True, timeout=120)
                except FileNotFoundError as error:
                    raise AssertionError(
                        'ci.yml %s is fail-closed: ruby unavailable (%s)' % (guard_name, error))
                except OSError as error:
                    raise AssertionError(
                        'ci.yml %s is fail-closed: ruby could not run (%s)' % (guard_name, error))
                except subprocess.SubprocessError as error:
                    raise AssertionError(
                        'ci.yml %s is fail-closed: ruby run failed (%s)' % (guard_name, error))
        def load_doc(yaml_text):
            import json as json_module
            completed = run_ruby(ruby_loader, yaml_text, 'parsed-job pin')
            if completed.returncode != 0:
                detail = (completed.stderr or '').strip().splitlines()
                raise AssertionError(
                    'ci.yml parsed-job pin is fail-closed: Psych load failed (%s)'
                    % (detail[-1][-200:] if detail else 'exit %d' % completed.returncode))
            try:
                doc = json_module.loads(completed.stdout)
            except ValueError as error:
                raise AssertionError(
                    'ci.yml parsed-job pin is fail-closed: ruby emitted non-JSON (%s)' % error)
            if not isinstance(doc, dict):
                raise AssertionError(
                    'ci.yml parsed-job pin is fail-closed: top-level mapping expected')
            return doc
        def job_keys_via_parse_tree(yaml_text):
            import json as json_module
            completed = run_ruby(ruby_key_guard, yaml_text, 'job-key guard')
            if completed.returncode != 0:
                detail = (completed.stderr or '').strip().splitlines()
                message = (detail[-1][-200:] if detail else 'exit %d' % completed.returncode)
                if 'GUARD:' in message:
                    raise AssertionError(message.split('GUARD:', 1)[1])
                raise AssertionError(
                    'ci.yml job-key guard is fail-closed: Psych parse failed (%s)' % message)
            try:
                keys = json_module.loads(completed.stdout)
            except ValueError as error:
                raise AssertionError(
                    'ci.yml job-key guard is fail-closed: ruby emitted non-JSON (%s)' % error)
            if not isinstance(keys, list) or not all(isinstance(key, str) for key in keys):
                raise AssertionError(
                    'ci.yml job-key guard is fail-closed: key list expected')
            return keys
        def parsed_uses_suffixes(steps):
            found = {}
            for step in steps or []:
                if not isinstance(step, dict):
                    continue
                uses = step.get('uses')
                if not isinstance(uses, str) or '@' not in uses:
                    continue
                repo, suffix = uses.split('@', 1)
                if repo in ('actions/checkout', 'actions/setup-python'):
                    found.setdefault(repo, []).append(suffix)
            return found
        # Mutant construction only: these locate text to BUILD broken
        # variants. The oracle below never reads through them -- it asserts
        # on the Psych-parsed object, which is exactly the F7 lesson.
        def job_slice(text, name):
            import re
            rest = text.split('\n  ' + name + ':\n', 1)[1]
            end = re.search(r'\n  [A-Za-z0-9_-]+:', rest)
            body = rest if end is None else rest[:end.start()]
            return '\n  ' + name + ':\n' + body
        def replace_in_job(text, name, old, new, count=1):
            marker = '\n  ' + name + ':\n'
            start = text.index(marker)
            import re
            rest = text[start + len(marker):]
            end = re.search(r'\n  [A-Za-z0-9_-]+:', rest)
            stop = start + len(marker) + (len(rest) if end is None else end.start())
            block = text[start:stop]
            self.assertIn(old, block, 'fixture assumption broken for ' + name)
            return text[:start] + block.replace(old, new, count) + text[stop:]
        def check(text):
            import re
            # Parse-tree uniqueness runs FIRST: a merge-key or exotic-key
            # variant must bite here, not merely trip the safe_load below.
            keys = job_keys_via_parse_tree(text)
            doc = load_doc(text)
            jobs = doc.get('jobs')
            self.assertIsInstance(jobs, dict, 'ci.yml jobs must be a mapping')
            self.assertIn(job, jobs, 'required contract job missing from ci.yml')
            actual = jobs[job]
            self.assertIsInstance(actual, dict, 'required contract job body must be a mapping')
            self.assertEqual(set(actual.keys()),
                             {'name', 'runs-on', 'timeout-minutes', 'permissions', 'steps'},
                             'required contract job keys must equal the pinned set (no extra keys)')
            self.assertEqual(actual.get('name'), 'Published release golden-path contract',
                             'required contract name must stay pinned')
            self.assertEqual(actual.get('runs-on'), 'ubuntu-24.04',
                             'required contract runs-on must stay ubuntu-24.04')
            self.assertEqual(actual.get('timeout-minutes'), 10,
                             'required contract timeout-minutes must stay 10')
            self.assertEqual(actual.get('permissions'), {'contents': 'read'},
                             'required contract permissions must stay contents-read-only')
            steps = actual.get('steps')
            self.assertIsInstance(steps, list, 'required contract steps must be a sequence')
            self.assertEqual(len(steps), 3,
                             'required contract must have exactly three steps')
            for index, step in enumerate(steps):
                self.assertIsInstance(step, dict, 'required contract step %d must be a mapping' % index)
            self.assertEqual(set(steps[0].keys()), {'uses', 'with'},
                             'required contract checkout step must be exactly uses/with')
            self.assertEqual(steps[0].get('with'), {'persist-credentials': False},
                             'required contract checkout must disable credential persistence')
            self.assertEqual(set(steps[1].keys()), {'name', 'uses', 'with'},
                             'required contract setup-python step must be exactly name/uses/with')
            self.assertEqual(steps[1].get('name'), 'Set up Python',
                             'required contract setup-python name must stay pinned')
            self.assertEqual(steps[1].get('with'), {'python-version': '3.12'},
                             'required contract python-version must stay 3.12')
            self.assertEqual(set(steps[2].keys()), {'name', 'shell', 'run'},
                             'required contract verify step must be exactly name/shell/run')
            self.assertEqual(steps[2].get('name'), 'Verify published release golden-path contract',
                             'required contract verify step name must stay pinned')
            self.assertEqual(steps[2].get('shell'), 'bash',
                             'required contract shell must stay bash')
            self.assertEqual(steps[2].get('run'), command,
                             'required contract run line must equal the pinned battery command')
            actual_suffixes = parsed_uses_suffixes(steps)
            self.assertEqual(sorted(actual_suffixes.keys()),
                             ['actions/checkout', 'actions/setup-python'],
                             'required contract uses: action identity must stay actions/checkout and actions/setup-python')
            self.assertEqual({repo: len(suffixes) for repo, suffixes in actual_suffixes.items()},
                             {'actions/checkout': 1, 'actions/setup-python': 1},
                             'required contract uses: each action must occur exactly once')
            suffix_shape = re.compile(r'^[0-9a-f]{40}$')
            for repo, suffixes in actual_suffixes.items():
                self.assertRegex(suffixes[0], suffix_shape,
                                 'uses: %s ref must be a 40-hex SHA' % repo)
            self.assertIn(reference, jobs, 'reference contract job missing from ci.yml')
            reference_job = jobs[reference]
            self.assertIsInstance(reference_job, dict, 'reference contract job body must be a mapping')
            reference_suffixes = parsed_uses_suffixes(reference_job.get('steps'))
            for repo in ('actions/checkout', 'actions/setup-python'):
                self.assertIn(repo, reference_suffixes,
                              'reference job must still carry uses: %s' % repo)
                self.assertEqual(actual_suffixes[repo][0], reference_suffixes[repo][0],
                                 'required contract uses: %s pin suffix must match release-asset-contract' % repo)
            for repo in ('actions/checkout', 'actions/setup-python'):
                sha = actual_suffixes[repo][0]
                self.assertRegex(text, re.compile(
                    r'uses:\s*' + re.escape(repo) + r'@' + re.escape(sha)
                    + r'\s+#\s*v[0-9]+\.[0-9]+\.[0-9]+'),
                    'uses: %s pin must carry its version comment' % repo)
            # Parse-tree uniqueness (#3262, replaces the text regex): every
            # jobs key node's scalar value, compared case-insensitively over
            # ALL job keys -- quotes, tags and explicit `? ` form count by
            # value, so only a true duplicate (or case variant) fails.
            folded = [key.lower() for key in keys]
            self.assertEqual(folded.count(job.lower()), 1,
                             'required contract job key must occur exactly once (parse-tree)')
            self.assertEqual(len(set(folded)), len(folded),
                             'ci.yml jobs keys must be unique case-insensitively (parse-tree)')
            rollup = jobs.get('ci')
            self.assertIsInstance(rollup, dict, 'ci rollup job must be a mapping')
            needs = rollup.get('needs')
            self.assertIsInstance(needs, list, 'ci rollup needs must be a sequence')
            self.assertIn(job, needs, 'required contract job missing from ci needs')
            evaluate = [step for step in rollup.get('steps', [])
                        if isinstance(step, dict) and step.get('name') == evaluate_step]
            self.assertEqual(len(evaluate), 1,
                             'ci evaluate step must occur exactly once')
            env = evaluate[0].get('env')
            self.assertIsInstance(env, dict, 'ci evaluate step env must be a mapping')
            self.assertEqual(env.get(variable), binding_value,
                             'required contract result binding missing or changed')
            run = evaluate[0].get('run')
            self.assertIsInstance(run, str, 'ci evaluate step run must be a string')
            self.assertIn(triple, run, 'required contract result not evaluated')
        text = (ROOT / '.github/workflows/ci.yml').read_text()
        name_line = '      - name: Verify published release golden-path contract\n'
        header = '\n  ' + job + ':\n'
        check(text)
        # Removal controls, read off the parsed object: missing job, missing
        # needs entry, rebound binding, dropped decision triple.
        with self.assertRaisesRegex(AssertionError, 'required contract job'):
            check(text.replace(header, '\n', 1))
        with self.assertRaisesRegex(AssertionError, 'required contract job missing from ci needs'):
            check(text.replace(', ' + job, '', 1))
        self.assertEqual(text.count(binding_value), 1, 'fixture assumption: binding value occurs once')
        with self.assertRaisesRegex(AssertionError, 'result binding'):
            check(text.replace(binding_value, '${{ needs.scope.result }}', 1))
        self.assertEqual(text.count(triple), 1, 'fixture assumption: decision triple occurs once')
        with self.assertRaisesRegex(AssertionError, 'required contract result not evaluated'):
            check(text.replace(triple, ':', 1))
        # Round-1..4 neutralisations, every one parsed-neutralised and every
        # one red on the object (each was green against some earlier guard).
        with self.assertRaisesRegex(AssertionError, 'pinned battery command'):
            check(text.replace('run: ' + command, 'run: ' + command + ' || true', 1))
        with self.assertRaisesRegex(AssertionError, 'pinned battery command'):
            check(text.replace('run: ' + command, 'run: "true"  # ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace(name_line, name_line + '        if: false\n', 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    if: false\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace('        run: ' + command,
                               '        continue-on-error: true\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'must stay bash'):
            check(text.replace(name_line + '        shell: bash\n',
                               name_line + '        shell: echo {0}\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace('        run: ' + command,
                               '        env:\n          SHELLOPTS: noexec\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    env:\n      SHELLOPTS: noexec\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace('        run: ' + command,
                               '        env:\n          BASH_ENV: /tmp/contract-bypass-env\n'
                               '        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    env:\n      BASH_ENV: /tmp/contract-bypass-env\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace('        run: ' + command,
                               '        env:\n          WORKFLOW: /dev/null\n'
                               '        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    env:\n      WORKFLOW: /dev/null\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly name/shell/run'):
            check(text.replace('        run: ' + command,
                               '        working-directory: scripts\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    working-directory: scripts\n', 1))
        with self.assertRaisesRegex(AssertionError, 'exactly three steps'):
            check(text.replace('        run: ' + command + '\n',
                               '        run: ' + command + '\n'
                               '      - name: Extra step\n'
                               '        shell: bash\n'
                               '        run: echo extra\n', 1))
        with self.assertRaisesRegex(AssertionError, 'pinned set'):
            check(text.replace(header, header + '    defaults:\n      run:\n        shell: bash\n', 1))
        with self.assertRaisesRegex(AssertionError, 'must stay pinned'):
            check(text.replace('    name: Published release golden-path contract\n',
                               '    name: Renamed contract\n', 1))
        with self.assertRaisesRegex(AssertionError, 'must stay ubuntu-24.04'):
            check(replace_in_job(text, job, '    runs-on: ubuntu-24.04\n', '    runs-on: macos-latest\n'))
        with self.assertRaisesRegex(AssertionError, 'must stay 10'):
            check(replace_in_job(text, job, '    timeout-minutes: 10\n', '    timeout-minutes: 20\n'))
        with self.assertRaisesRegex(AssertionError, 'contents-read-only'):
            check(replace_in_job(text, job,
                                 '    permissions:\n      contents: read\n',
                                 '    permissions:\n      contents: read\n      actions: read\n'))
        # Reviewer F7, first shape: a top-level `run-name: |2` block scalar
        # holding a verbatim copy of the pinned block, plus `|| true` on the
        # real run line. The decoy parses as an unrelated top-level key; the
        # real job parses neutralised, so the object equality fails.
        copied = job_slice(text, job).strip('\n')
        decoy = 'run-name: |2\n' + copied + '\n'
        # Neutralise FIRST: once the decoy is in, its pristine copy owns the
        # first text occurrence of the run line, so a naive global replace
        # would edit the decoy and leave the real job clean.
        f7_decoy = replace_in_job(text, job, 'run: ' + command, 'run: ' + command + ' || true')
        f7_decoy = f7_decoy.replace('\njobs:\n', '\n' + decoy + 'jobs:\n', 1)
        self.assertIn('run-name', load_doc(f7_decoy), 'F7 fixture must carry the decoy scalar')
        with self.assertRaisesRegex(AssertionError, 'pinned battery command'):
            check(f7_decoy)
        # Reviewer F7, second shape: a comment-suffixed real key plus a
        # trailing neutralised copy of the whole block. Psych takes the last
        # duplicate silently, so the parsed job IS the copy -- and the
        # object equality fails on its run line.
        neutralised_copy = job_slice(text, job).replace(
            'run: ' + command, 'run: ' + command + ' || true', 1)
        f7_dup = (text.replace(header, '\n  ' + job + ': # comment-suffixed real key\n', 1)
                      .replace('\n  mcp-registry-foundation:\n',
                               neutralised_copy + '\n  mcp-registry-foundation:\n', 1))
        with self.assertRaisesRegex(AssertionError, 'pinned battery command'):
            check(f7_dup)
        # Duplicate-key backstop: an IDENTICAL trailing copy parses to the
        # same object, so only the exactly-once key guard can fail it. This
        # is the Psych/GitHub divergence shape (last-wins vs first-wins),
        # closed by counting on the parse tree, never by content matching.
        identical_dup = text.replace('\n  mcp-registry-foundation:\n',
                                     job_slice(text, job) + '\n  mcp-registry-foundation:\n', 1)
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(identical_dup)
        # Issue #3262 (reviewer F9 on #3263): a neutralised copy FIRST under
        # a differently spelled key, pristine job last. Psych is last-wins,
        # so the loaded object is pristine and every guard above stays
        # green -- the old text regex counted one canonical key too. Only
        # the parse-tree uniqueness guard bites, through 'exactly once'.
        # Each variant reuses neutralised_copy (built above for F7): the
        # copy's run line is neutralised, the real job's is pristine.
        def prepend_copy(spelling):
            copy = neutralised_copy.replace(header, spelling, 1)
            return text.replace(header, copy + header, 1)
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(prepend_copy("\n  '" + job + "':\n"))
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(prepend_copy('\n  "' + job + '":\n'))
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(prepend_copy('\n  !!str ' + job + ':\n'))
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(prepend_copy('\n  ? ' + job + '\n  :\n'))
        with self.assertRaisesRegex(AssertionError, 'exactly once'):
            check(prepend_copy('\n  Published-Release-Golden-Path-Contract:\n'))
        # Pure-spelling rename (no duplicate) stays green by design: quotes
        # change nothing GitHub executes, so the tree-value count is still
        # one. This locks in that style alone never fails the guard.
        check(text.replace(header, "\n  '" + job + "':\n", 1))
        # The guard covers ALL job keys: an identical duplicate of an
        # unrelated job leaves the parsed contract object, the needs list
        # and every other guard green -- only 'unique' bites.
        self.assertEqual(text.count('\n  scope:\n'), 1, 'fixture assumption: scope header unique')
        scope_dup = text.replace('\n  deps-security:\n',
                                 job_slice(text, 'scope') + '\n  deps-security:\n', 1)
        with self.assertRaisesRegex(AssertionError, 'unique'):
            check(scope_dup)
        # Fail-closed key shapes: an anchored duplicate (unused anchors load
        # silently while GitHub rejects them), a `<<` merge key smuggling an
        # alias, and a non-scalar key (safe_load mangles it through JSON).
        # Each must bite here with the guard's own verdict -- the merge and
        # exotic shapes would also trip the safe_load below, so the message
        # match below is what proves the guard bit first.
        with self.assertRaisesRegex(AssertionError, 'anchors'):
            check(prepend_copy('\n  &contract_anchor ' + job + ':\n'))
        self.assertEqual(text.count('\n  scope:\n'), 1, 'fixture assumption: scope header unique')
        anchored_scope = text.replace('\n  scope:\n', '\n  scope: &scope_anchor\n', 1)
        merge_dup = anchored_scope.replace('\njobs:\n', '\njobs:\n  <<: *scope_anchor\n', 1)
        with self.assertRaisesRegex(AssertionError, 'merge key'):
            check(merge_dup)
        exotic_dup = text.replace('\n  mcp-registry-foundation:\n',
                                  '\n  ? [exotic-key]\n  : 1\n  mcp-registry-foundation:\n', 1)
        with self.assertRaisesRegex(AssertionError, 'plain scalars'):
            check(exotic_dup)
        # Round 2, G1: a duplicated TOP-LEVEL `jobs:` key. A neutralised
        # first `jobs:` mapping with the pristine one last leaves the parsed
        # object pristine (Psych last-wins for plain/quoted; a case-variant
        # first key leaves doc['jobs'] pristine too), so every assertion
        # above stays green -- only the new top-level exactly-once guard
        # bites. Quoting counts by scalar value, case counts
        # case-insensitively, the same way job keys are counted.
        self.assertEqual(text.count('\njobs:\n'), 1, 'fixture assumption: top-level jobs header unique')
        def top_level_dup_first(spelling):
            return text.replace('\njobs:\n', '\n' + spelling + ':' + neutralised_copy + '\njobs:\n', 1)
        with self.assertRaisesRegex(AssertionError, 'top-level jobs key must occur exactly once'):
            check(top_level_dup_first('jobs'))
        with self.assertRaisesRegex(AssertionError, 'top-level jobs key must occur exactly once'):
            check(top_level_dup_first('"jobs"'))
        with self.assertRaisesRegex(AssertionError, 'top-level jobs key must occur exactly once'):
            check(top_level_dup_first('Jobs'))
        # Round 2, G1 reverse order: pristine first, neutralised last. Still
        # two top-level `jobs` keys, so the same new guard bites first --
        # the object pin (pinned battery command) never gets its turn. The
        # message match is what proves the guard bit before the load.
        reverse_top = text + '\njobs:' + neutralised_copy
        with self.assertRaisesRegex(AssertionError, 'top-level jobs key must occur exactly once'):
            check(reverse_top)
        # Round 2, G2: a second YAML document. The pristine file followed by
        # `---` and a neutralised copy passes the object pin (parse_file and
        # safe_load_file read only the first document; actionlint is green
        # too), so only the new single-document guard bites.
        neutralised_full = text.replace('run: ' + command, 'run: ' + command + ' || true', 1)
        separator = '' if text.endswith('\n') else '\n'
        second_doc = text + separator + '---\n' + neutralised_full
        with self.assertRaisesRegex(AssertionError, 'single YAML document'):
            check(second_doc)
        # Fail-closed controls: ruby missing, and YAML Psych refuses to load.
        # Neither may read green -- both must raise, not pass and not crash
        # with anything but AssertionError.
        with mock.patch('subprocess.run', side_effect=FileNotFoundError(2, 'ruby')):
            with self.assertRaisesRegex(AssertionError, 'fail-closed'):
                check(text)
        with self.assertRaisesRegex(AssertionError, 'fail-closed'):
            check('jobs:\n\tbroken-tab-indent: true\n')
        # Dependabot-style pin bump applied to BOTH jobs stays green: the pin
        # does not freeze SHAs. Suffixes are read off the live parsed object,
        # so neither the green control nor the red ones below can go stale
        # (reviewer F6); check(text) above already proved them well-formed.
        live_suffix = parsed_uses_suffixes(load_doc(text)['jobs'][reference]['steps'])
        checkout_suffix = live_suffix['actions/checkout'][0]
        setup_suffix = live_suffix['actions/setup-python'][0]
        checkout_token = 'uses: actions/checkout@' + checkout_suffix
        joint_checkout = '0' * 40 + checkout_suffix[40:]
        bumped = text.replace(checkout_suffix, joint_checkout).replace(
            setup_suffix, '1' * 40 + setup_suffix[40:])
        check(bumped)
        # Reviewer F5: each of these must FAIL. The two single-job bumps are the
        # only controls that bite through the suffix-equality assertion -- delete
        # it and they stop raising while the object equality still passes.
        with self.assertRaisesRegex(AssertionError, 'must match release-asset-contract'):
            check(replace_in_job(text, job, checkout_suffix, joint_checkout))
        with self.assertRaisesRegex(AssertionError, 'must match release-asset-contract'):
            check(replace_in_job(text, reference, checkout_suffix, joint_checkout))
        # Action-identity swaps applied to BOTH jobs must FAIL: another action runs
        # before the battery in the same workspace, where it can overwrite the
        # battery script or write SHELLOPTS/BASH_ENV to $GITHUB_ENV (reviewer F4).
        noop_token = 'uses: ./.github/actions/noop'
        both_noop = replace_in_job(replace_in_job(text, job, checkout_token, noop_token),
                                   reference, checkout_token, noop_token)
        with self.assertRaisesRegex(AssertionError, 'action identity'):
            check(both_noop)
        foreign_token = 'uses: evil/set-shellopts@' + '2' * 40
        both_foreign = replace_in_job(replace_in_job(text, job, checkout_token, foreign_token),
                                      reference, checkout_token, foreign_token)
        with self.assertRaisesRegex(AssertionError, 'action identity'):
            check(both_foreign)
        # A non-SHA ref in BOTH jobs must FAIL even though the action name is right.
        v5_token = 'uses: actions/checkout@v5'
        both_v5 = replace_in_job(replace_in_job(text, job, checkout_token, v5_token),
                                 reference, checkout_token, v5_token)
        with self.assertRaisesRegex(AssertionError, '40-hex SHA'):
            check(both_v5)
        # Reviewer F8 correction, locked green: swapping the two SHA suffixes
        # in BOTH jobs stays green (the old report claim it fails was wrong).
        # Not a bypass -- GitHub cannot resolve `actions/checkout@<a SHA that
        # exists only in actions/setup-python>` and fails the job at resolve
        # time. This control pins the green so the claim is never re-made.
        swap_job = replace_in_job(text, job,
                                  'actions/checkout@' + checkout_suffix,
                                  'actions/checkout@' + setup_suffix)
        swap_job = replace_in_job(swap_job, job,
                                  'actions/setup-python@' + setup_suffix,
                                  'actions/setup-python@' + checkout_suffix)
        swap_both = replace_in_job(swap_job, reference,
                                   'actions/checkout@' + checkout_suffix,
                                   'actions/checkout@' + setup_suffix)
        swap_both = replace_in_job(swap_both, reference,
                                   'actions/setup-python@' + setup_suffix,
                                   'actions/setup-python@' + checkout_suffix)
        check(swap_both)
        b1 = (ROOT / 'scripts/ci/test-ci-hardening-b1.sh').read_text()
        self.assertNotIn('test-published-release-golden-path-contract.sh', b1,
                         'golden-path battery must run in its own job, not inside ci-hardening-b1')
        with self.assertRaisesRegex(AssertionError, 'own job, not inside ci-hardening-b1'):
            self.assertNotIn('test-published-release-golden-path-contract.sh',
                             b1 + '\nbash "$ROOT/scripts/ci/test-published-release-golden-path-contract.sh"\n',
                             'own job, not inside ci-hardening-b1')


class InstallerMainFunnel(unittest.TestCase):
    """Transport/process boundaries are offline fakes; main's identity decisions are real."""
    def run_case(self, mismatch=None):
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            helper = root / 'harness/scripts/ci/published_release_installer.py'
            helper.parent.mkdir(parents=True)
            pin = root / 'harness/.github/workflows/release.yml'
            pin.parent.mkdir(parents=True)
            pin.write_text('          cosign-release: v3.1.3\n')
            results = root / 'results'
            (results / 'release-assets').mkdir(parents=True)
            archive = results / 'release-assets/assay-v6.9.0-x86_64-unknown-linux-gnu.tar.gz'
            archive.write_bytes(b'opaque verified archive fixture')
            reference = root / 'cli-extract/assay'
            reference.parent.mkdir()
            reference.write_bytes(b'opaque matching executable bytes')
            reference.chmod(0o755)
            cosign = root / 'cosign'
            cosign.write_bytes(b'opaque tool fixture')
            cosign.chmod(0o755)
            api = {'id': 71, 'tag_name': 'v6.9.0', 'draft': False,
                   'prerelease': False, 'assets': []}
            tag = {'object': {'type': 'commit', 'sha': 'a' * 40}}
            (results / 'release-api.json').write_text(json.dumps(api))
            (results / 'tag-ref.json').write_text(json.dumps(tag))
            (results / 'journey-source-sha.txt').write_text('a' * 40)
            source = b'# released source fixture; never executed\n'
            stages = []

            def transport(request, timeout):
                import io
                url = request.full_url
                expected = "application/vnd.github+json" if url.startswith("https://api.github.com/") else "application/octet-stream"
                self.assertEqual(request.get_header("Accept"), expected, "actual metadata request representation")
                self.assertNotIn('/git/ref/', url, 'tag identity must be re-read over git, not the REST API')
                data = json.dumps(api).encode() if '/releases/tags/' in url else source
                response = io.BytesIO(data)
                response.headers = {"Content-Length": str(len(data))}
                return response

            def pipeline(curl, tee, shell, url, capture, env, output):
                mode = output.parent.name
                stages.append(mode)
                capture.write_bytes(source + (b'# distinct response\n' if mismatch == 'capture' else b''))
                binary = Path(env['ASSAY_INSTALL_DIR']) / 'assay'
                binary.parent.mkdir(parents=True)
                binary.write_bytes(reference.read_bytes() + (b'different' if mismatch == 'binary' else b''))
                binary.chmod(0o755)
                output.mkdir()
                observation = ('signed_manifest_skipped reason=cosign_not_installed' if mode == 'default' else
                               'signed_manifest_verified asset=' + archive.name +
                               ' identity=https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v6.9.0')
                (output / 'stdout').write_text('verification=checksum_verified asset=' + archive.name +
                                              ' sha256=' + subject.digest(archive) +
                                              '\nverification=provenance_not_requested\nverification=' + observation)
                (output / 'sh-stderr').write_text('')
                return {'argv': ['offline-transport'], 'exit_code': 0, 'pipeline_status': [0, 0, 0]}

            def process(argv, env, output):
                output.mkdir()
                value = 'GitVersion: v3.1.3' if str(argv[0]) == str(cosign) else (
                    'assay 6.9.0' if argv[-1] == '--version' else '6.9.0')
                (output / 'stdout').write_text(value + '\n')
                return {'exit_code': 0}

            def which(name, path=None):
                return '/usr/bin/curl' if name == 'curl' else (str(cosign) if path is None else None)

            with ExitStack() as stack:
                stack.enter_context(mock.patch('bounded_download.urllib.request.urlopen', side_effect=transport))
                for name, value in [('__file__', str(helper)),
                                    ('run_pipeline', pipeline), ('supervise', process),
                                    ('contrasts', lambda *args: None),
                                    ('remote_tag_identity', lambda release_tag, scratch: {
                                        'argv': ['git', 'ls-remote'], 'object_sha': ('b' if mismatch == 'tag' else 'a') * 40,
                                        'commit_sha': ('c' if mismatch == 'peeled' else 'a') * 40})]:
                    stack.enter_context(mock.patch.object(subject, name, value))
                stack.enter_context(mock.patch.object(subject.shutil, 'which', which))
                stack.enter_context(mock.patch.object(subject.subprocess, 'check_output', return_value='curl 8.4.0'))
                stack.enter_context(mock.patch.object(sys, 'argv', ['installer']))
                stack.enter_context(mock.patch.dict(os.environ, {'PUBLISHED_COSIGN': str(cosign),
                                                                'PUBLISHED_COSIGN_RELEASE': 'v3.1.3'}))
                failure = None
                try:
                    subject.main()
                except ValueError as error:
                    failure = str(error)
            receipt = json.loads((results / 'installer/receipt.json').read_text())
            return failure, receipt, stages

    def test_matching_main_funnel_completes_both_modes(self):
        failure, receipt, stages = self.run_case()
        self.assertIsNone(failure)
        self.assertEqual(receipt['status'], 'completed')
        self.assertEqual(stages, ['default', 'signed'])
        self.assertEqual([row['mode'] for row in receipt['installations']], stages)

    def test_main_refuses_distinct_executed_source(self):
        failure, receipt, stages = self.run_case('capture')
        self.assertEqual(failure, 'executed streaming response differs from released installer')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default'])

    def test_main_refuses_a_tag_moved_during_installation(self):
        failure, receipt, stages = self.run_case('tag')
        self.assertEqual(failure, 'tag metadata changed during installation')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default', 'signed'])

    def test_main_refuses_a_tag_that_now_peels_to_another_commit(self):
        failure, receipt, stages = self.run_case('peeled')
        self.assertEqual(failure, 'tag metadata changed during installation')
        self.assertEqual(receipt['status'], 'failed')

    def test_tag_listing_accepts_annotated_and_lightweight_tags_only(self):
        a, c = 'a' * 40, 'c' * 40
        self.assertEqual(subject.parse_tag_listing(f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.0^{{}}\n', 'v6.9.0'),
                         {'object_sha': a, 'commit_sha': c})
        self.assertEqual(subject.parse_tag_listing(f'{a}\trefs/tags/v6.9.0\n', 'v6.9.0'),
                         {'object_sha': a, 'commit_sha': a})
        for text in ('', f'{c}\trefs/tags/v6.9.0^{{}}\n', f'{a}\trefs/tags/v6.9.1\n',
                     f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.0^{{}}\n{c}\trefs/heads/main\n',
                     f'{a}\trefs/tags/v6.9.0\n{c}\trefs/tags/v6.9.00\n',
                     f'{a}\trefs/tags/v6.9.0\n{a}\trefs/tags/v6.9.0\n', 'not-a-sha\trefs/tags/v6.9.0\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                subject.parse_tag_listing(text, 'v6.9.0')

    def test_tag_reread_uses_credential_free_git(self):
        seen = {}
        def run(argv, env, **kwargs):
            seen.update(argv=argv, env=env, kwargs=kwargs)
            return mock.Mock(returncode=0, stdout=('a' * 40 + '\trefs/tags/v6.9.0\n').encode())
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(subject.shutil, 'which', return_value='/usr/bin/git'), \
                mock.patch.object(subject.subprocess, 'run', side_effect=run), \
                mock.patch.dict(os.environ, {'GH_TOKEN': 'must-not-leak', 'GITHUB_TOKEN': 'must-not-leak'}):
            identity = subject.remote_tag_identity('v6.9.0', Path(directory) / 'git')
        self.assertEqual(identity['commit_sha'], 'a' * 40)
        self.assertIn('credential.helper=', seen['argv'])
        self.assertIn('https://github.com/Rul1an/assay.git', seen['argv'])
        self.assertEqual(seen['env']['GIT_TERMINAL_PROMPT'], '0')
        self.assertFalse({'GH_TOKEN', 'GITHUB_TOKEN'} & set(seen['env']))
        self.assertEqual(seen['kwargs']['timeout'], 60)

    def test_main_refuses_distinct_installed_binary(self):
        failure, receipt, stages = self.run_case('binary')
        self.assertEqual(failure, 'installed binary differs from verified archive member')
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(stages, ['default'])


class ReleaseRereadRetries(unittest.TestCase):
    """GitHub's documented rate-limit protocol, bounded; everything else stops at once."""

    def run_reread(self, responses, now=1_000_000, attempt_seconds=0.0):
        import io, urllib.error
        sleeps, log, calls, deadlines = [], [], [], []
        mono = [5000.0]
        def fake_sleep(seconds):
            sleeps.append(seconds)
            mono[0] += seconds
        def fake_bounded(function, seconds):
            deadlines.append(seconds)
            return function()
        def fake_download(url, destination, **kwargs):
            calls.append(destination.name)
            mono[0] += attempt_seconds
            status, headers, body = responses.pop(0)
            if status == 200:
                destination.write_text('{"ok": true}')
                return
            raise urllib.error.HTTPError(url, status, 'refused', headers, io.BytesIO(body.encode()))
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.object(subject, 'download', side_effect=fake_download):
            destination = Path(directory) / 'post-release.json'
            try:
                subject.reread_release('https://api.github.com/x', destination, log,
                                       sleep=fake_sleep, clock=lambda: now, monotonic=lambda: mono[0],
                                       bounded=fake_bounded)
                error = None
            except urllib.error.HTTPError as caught:
                error = caught.code
            present = destination.exists()
        self.deadlines, self.elapsed = deadlines, mono[0] - 5000.0
        return error, sleeps, log, calls, present

    def test_primary_limit_waits_until_the_documented_reset(self):
        error, sleeps, log, calls, present = self.run_reread([
            (403, {'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1000030'}, 'rate limit exceeded'),
            (200, {}, '')])
        self.assertIsNone(error)
        self.assertTrue(present)
        self.assertEqual(sleeps, [31])
        self.assertEqual([row['status'] for row in log], [403, 200])
        self.assertEqual(log[0]['headers']['x-ratelimit-remaining'], '0')
        self.assertEqual(len(set(calls)), 2, 'each attempt uses a fresh destination')

    def test_reset_beyond_the_budget_stops_without_waiting(self):
        error, sleeps, log, _, present = self.run_reread([
            (403, {'x-ratelimit-remaining': '0', 'x-ratelimit-reset': str(1_000_000 + 3600)}, 'rate limit exceeded'),
            (200, {}, '')])
        self.assertEqual(error, 403)
        self.assertEqual(sleeps, [])
        self.assertFalse(present)
        self.assertEqual(log[0]['documented_wait_seconds'], 3601)

    def test_retry_after_is_honoured(self):
        error, sleeps, _, _, _ = self.run_reread([(429, {'retry-after': '7'}, ''), (200, {}, '')])
        self.assertIsNone(error)
        self.assertEqual(sleeps, [7])

    def test_secondary_limit_without_headers_waits_one_minute(self):
        error, sleeps, _, _, _ = self.run_reread([
            (403, {}, 'You have exceeded a secondary rate limit.'), (200, {}, '')])
        self.assertIsNone(error)
        self.assertEqual(sleeps, [60])

    def test_non_rate_limit_failures_are_not_retried(self):
        for status, headers, body in ((404, {}, 'Not Found'), (403, {}, 'Resource not accessible'),
                                      (500, {'retry-after': '1'}, 'boom')):
            with self.subTest(status=status, body=body):
                error, sleeps, log, _, _ = self.run_reread([(status, headers, body), (200, {}, '')])
                self.assertEqual(error, status)
                self.assertEqual(sleeps, [])
                self.assertEqual(len(log), 1)

    def test_persistent_rate_limit_stops_after_the_attempt_bound(self):
        error, sleeps, log, _, present = self.run_reread([(429, {'retry-after': '10'}, '')] * 3 + [(200, {}, '')])
        self.assertEqual(error, 429)
        self.assertEqual(sleeps, [10, 10])
        self.assertEqual(len(log), subject.METADATA_ATTEMPTS)
        self.assertFalse(present)

    def test_every_attempt_shares_one_total_deadline(self):
        error, sleeps, _, _, _ = self.run_reread([(429, {'retry-after': '5'}, ''), (429, {'retry-after': '5'}, ''),
                                                  (200, {}, '')], attempt_seconds=10)
        self.assertIsNone(error)
        # Each attempt gets exactly what is left of the one shared deadline.
        deadline = float(subject.METADATA_DEADLINE)
        self.assertEqual(self.deadlines, [deadline, deadline - 15, deadline - 30])
        self.assertEqual(sleeps, [5, 5])

    def test_a_wait_that_leaves_no_time_for_an_attempt_stops_at_once(self):
        # The first attempt itself consumed 300 s: a 60 s wait fits the wait budget but not the deadline.
        error, sleeps, log, _, _ = self.run_reread([(429, {'retry-after': '60'}, ''), (200, {}, '')],
                                                   attempt_seconds=subject.METADATA_DEADLINE - 60)
        self.assertEqual(error, 429)
        self.assertEqual(sleeps, [])
        self.assertEqual(len(log), 1)
        self.assertLessEqual(self.elapsed, subject.METADATA_DEADLINE)

    def test_deadline_expiry_is_not_retried(self):
        seconds = []
        def expire(function, remaining):
            seconds.append(remaining)
            raise subject.DownloadRejected('metadata re-read deadline exceeded')
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, 'deadline'):
                subject.reread_release('https://api.github.com/x', Path(directory) / 'p.json', [],
                                       sleep=self.fail, clock=lambda: 0, monotonic=lambda: 10.0, bounded=expire)
        self.assertEqual(seconds, [subject.METADATA_DEADLINE])

    def test_interval_timer_interrupts_a_blocking_call_and_is_restored(self):
        import signal as real_signal
        previous = real_signal.getsignal(real_signal.SIGALRM)
        started = time.monotonic()
        with self.assertRaisesRegex(ValueError, 'deadline exceeded'):
            subject.call_with_deadline(lambda: time.sleep(5), 0.2)
        self.assertLess(time.monotonic() - started, 2)
        self.assertEqual(real_signal.getitimer(real_signal.ITIMER_REAL), (0.0, 0.0))
        self.assertIs(real_signal.getsignal(real_signal.SIGALRM), previous)
        self.assertEqual(subject.call_with_deadline(lambda: 'done', 5), 'done')
        with self.assertRaises(ValueError):
            subject.call_with_deadline(lambda: 'never', 0)

    def test_total_deadline_is_declared_inside_the_job_timeouts(self):
        import re
        self.assertLessEqual(subject.METADATA_WAIT_BUDGET, subject.METADATA_DEADLINE)
        # Measured anonymous-quota resets on macOS arm64: 294 s (run 36492337364), 461 s (run 36542544734).
        self.assertGreaterEqual(subject.METADATA_WAIT_BUDGET, 461 + 60)
        workflow = (ROOT / '.github/workflows/published-release-golden-path.yml').read_text()
        timeouts = {}
        for job in ('published-linux-journey', 'published-darwin-journey'):
            block = workflow.split('\n  ' + job + ':\n', 1)[1].split('\n  published-', 1)[0]
            timeouts[job] = int(re.search(r'\n    timeout-minutes: (\d+)\n', block).group(1)) * 60
        # The installer phase runs in both jobs; leave at least four minutes for the rest of the journey.
        self.assertLessEqual(subject.METADATA_DEADLINE + 240, min(timeouts.values()), timeouts)

    def test_cumulative_wait_never_exceeds_the_budget(self):
        # Two waits that exceed the wait budget but would still fit the total deadline, so only the
        # budget clause can refuse the second one (independent review F1 on 78701262).
        wait = subject.METADATA_WAIT_BUDGET // 2 + 10
        self.assertGreater(2 * wait, subject.METADATA_WAIT_BUDGET)
        self.assertLessEqual(2 * wait + subject.ATTEMPT_RESERVE, subject.METADATA_DEADLINE)
        error, sleeps, _, _, _ = self.run_reread([(429, {'retry-after': str(wait)}, ''),
                                                  (429, {'retry-after': str(wait)}, ''), (200, {}, '')])
        self.assertEqual(error, 429)
        self.assertEqual(sleeps, [wait])
        self.assertLessEqual(sum(sleeps), subject.METADATA_WAIT_BUDGET)


class PipelineBehavior(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.env = {'PATH': '/usr/bin:/bin', 'HOME': str(self.root)}

    def executable(self, name, body):
        path = self.root / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)
        return str(path)

    def pipeline(self, curl_body, tee='/usr/bin/tee'):
        curl = self.executable('curl', curl_body)
        return subject.run_pipeline(curl, tee, '/bin/sh', 'https://example.invalid/install',
                                    self.root / 'captured', self.env, self.root / 'output', timeout=3)

    def test_real_stdin_and_exact_capture(self):
        result = self.pipeline("printf '%s\\n' 'printf stdin-ok'")
        self.assertEqual(result['pipeline_status'], [0, 0, 0])
        self.assertEqual((self.root / 'captured').read_bytes(), b'printf stdin-ok\n')
        self.assertEqual((self.root / 'output/stdout').read_bytes(), b'stdin-ok')

    def test_stage_stderr_is_separate_and_bounded(self):
        tee = self.executable('observed-tee', 'printf tee-note >&2; exec /usr/bin/tee "$@"')
        self.pipeline("printf curl-note >&2; printf '%s\\n' 'printf shell-note >&2'", tee)
        for stage, text in [('curl', b'curl-note'), ('tee', b'tee-note'), ('sh', b'shell-note')]:
            self.assertEqual((self.root / 'output' / (stage + '-stderr')).read_bytes(), text)
        for stage in ('curl', 'tee', 'sh'):
            producer = self.executable('producer-' + stage, "yes x >&2" if stage == 'curl' else
                                       "printf '%s\\n' 'yes x >&2'" if stage == 'sh' else
                                       "printf '%s\\n' 'exit 0'")
            selected_tee = self.executable('noisy-tee', 'yes x >&2') if stage == 'tee' else '/usr/bin/tee'
            output = self.root / ('overflow-' + stage)
            with self.assertRaisesRegex(ValueError, stage + '-stderr output ceiling'):
                subject.run_pipeline(producer, selected_tee, '/bin/sh', 'https://example.invalid/',
                                     self.root / ('capture-' + stage), self.env, output,
                                     output_limit=1024)
            self.assertLessEqual((output / (stage + '-stderr')).stat().st_size, 1024)
            record = json.loads((output / 'command.json').read_text())
            self.assertIn(stage + '-stderr output ceiling', record['failure'])

    def test_curl_partial_successful_shell_is_red(self):
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 0'; exit 23")
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [23, 0, 0])

    def test_tee_failure_is_red(self):
        tee = self.executable('bad-tee', 'cat >/dev/null; exit 19')
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 0'", tee)
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [0, 19, 0])

    def test_shell_failure_is_red(self):
        with self.assertRaisesRegex(ValueError, 'pipeline failed'):
            self.pipeline("printf '%s\\n' 'exit 17'")
        self.assertEqual(json.loads((self.root / 'output/command.json').read_text())['pipeline_status'], [0, 0, 17])

    def test_output_bound_is_applied_before_retention(self):
        with self.assertRaisesRegex(ValueError, 'output ceiling'):
            subject.supervise([sys.executable, '-c', 'print("x"*10000)'], self.env,
                              self.root / 'cap', output_limit=1024)
        self.assertLessEqual((self.root / 'cap/stdout').stat().st_size, 1024)
        self.assertEqual(json.loads((self.root / 'cap/command.json').read_text())['status'], 'failed')

    def test_descendant_does_not_survive_timeout(self):
        import time
        def exercise(name):
            marker = self.root / (name + '-survived')
            program = self.executable(name, f'(echo descendant-ready; sleep 1; touch "{marker}") & wait')
            with self.assertRaises(TimeoutError):
                subject.supervise([program], self.env, self.root / (name + '-output'), timeout=0.3)
            self.assertIn(b'descendant-ready', (self.root / (name + '-output') / 'stdout').read_bytes(),
                          'mandatory descendant witness absent')
            time.sleep(1.1)
            self.assertFalse(marker.exists(), 'descendant survived process-group cleanup')
        exercise('owned-child')
        with mock.patch.object(subject.os, 'killpg', return_value=None):
            with self.assertRaisesRegex(AssertionError, 'descendant survived'):
                exercise('cleanup-removed')

    def test_environment_is_total_allowlist(self):
        with mock.patch.dict(os.environ, {'GH_TOKEN': 'sentinel', 'ASSAY_REQUIRE_PROVENANCE': '1'}):
            env = subject.child_environment(self.root, self.root, self.root)
        self.assertNotIn('GH_TOKEN', env)
        self.assertNotIn('ASSAY_REQUIRE_PROVENANCE', env)
        self.assertNotIn('ASSAY_COSIGN', env)
        self.assertNotIn('GITHUB_TOKEN', env)

    def test_actual_curl_bounds_unknown_length_stream(self):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        import threading
        import subprocess
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200)
                self.end_headers()  # Deliberately no Content-Length.
                try:
                    self.wfile.write(b'x' * 131072)
                except (BrokenPipeError, ConnectionResetError):
                    pass
            def log_message(self, *args):
                pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            path = self.root / 'bounded-transfer'
            result = subprocess.run(['/usr/bin/curl', '-fsSL', '--max-filesize', '65536',
                                     '--max-time', '5', f'http://127.0.0.1:{server.server_port}/',
                                     '-o', str(path)], capture_output=True, timeout=6)
            self.assertEqual(result.returncode, 63, result.stderr)
            self.assertLessEqual(path.stat().st_size, 65536)
        finally:
            server.shutdown(); server.server_close(); thread.join()

    def test_pin_unique_and_same_owner(self):
        path = self.root / 'release.yml'
        path.write_text('          cosign-release: v3.1.3\n')
        self.assertEqual(read_pin(path), 'v3.1.3')
        path.write_text(path.read_text() * 2)
        with self.assertRaises(ValueError):
            read_pin(path)
        path.write_text('cosign-release: latest\n')
        with self.assertRaises(ValueError):
            read_pin(path)


class DocumentedRecipe(unittest.TestCase):
    def test_actual_python_fence_uses_released_assets_and_matches_proxy_argv(self):
        import io
        import types
        import published_release_proxy_phase as proxy
        source = (ROOT / 'docs/guides/installed-release-journey.md').read_text()
        program = proxy.guide_blocks(source)['deny-python']
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            fixture = root / 'packaged'
            (fixture / 'policies').mkdir(parents=True)
            for name in ('mock_github_mcp.py', 'baseline-approved.json', 'policies/no-allowance.yaml'):
                (fixture / name).write_text('trusted fixture')
            output = io.StringIO(json.dumps({'id': 9, 'error': {'code': -31999, 'data': {'reason': 'no_declared_allowance'}}}) + '\n')
            child = mock.Mock(stdin=io.StringIO(), stdout=output)
            child.wait.return_value = 0
            child.poll.return_value = 0
            old = Path.cwd()
            try:
                os.chdir(root)
                with mock.patch('subprocess.Popen', return_value=child) as spawn, mock.patch.object(sys, 'argv', ['deny.py', '/released/server', str(fixture)]):
                    exec(compile(program, 'documented-deny.py', 'exec'), {})
                documented = spawn.call_args.args[0]
                with mock.patch.object(sys, 'argv', ['proxy', '--fixture-dir', str(fixture)]), \
                     mock.patch.object(sys, 'stdin', types.SimpleNamespace(buffer=io.BytesIO(b'{}\n'))), \
                     mock.patch.object(proxy.shutil, 'which', return_value='/released/server'), \
                     mock.patch.object(proxy, 'run_proxy_child', return_value=0) as run:
                    self.assertEqual(proxy.main(), 0)
                executed = run.call_args.args[0]
                # The public recipe names cwd outputs; the harness makes them absolute.
                for flag in ('--enforcement-decision-out', '--denied-call-observation-out'):
                    index = documented.index(flag) + 1
                    documented[index] = str(root / documented[index])
                self.assertEqual(documented, executed)
            finally:
                os.chdir(old)

    def test_explicit_profile_and_windows_route_are_published(self):
        guide = (ROOT / 'docs/guides/installed-release-journey.md').read_text()
        install = (ROOT / 'docs/getting-started/installation.md').read_text()
        self.assertIn('```powershell', guide)
        self.assertIn('assay evidence verify-privileged-mcp-action action.bundle.tar.gz --profile-version v1 --format json', guide)
        self.assertIn('assay evidence verify-privileged-mcp-action <bundle> --profile-version v1 --format json', install)
        self.assertIn('default remains profile v0', install)
        self.assertNotIn('git clone', guide)
        self.assertNotIn('conformance/', guide)


class ModeAssertions(unittest.TestCase):
    def test_default_and_signed_observations_cannot_be_confused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive, sha, tag = 'asset.tar.gz', 'a' * 64, 'v6.9.0'
            common = f'verification=checksum_verified asset={archive} sha256={sha}\nverification=provenance_not_requested\n'
            skipped = 'verification=signed_manifest_skipped reason=cosign_not_installed\n'
            signed = f'verification=signed_manifest_verified asset={archive} identity=https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/{tag}\n'
            (root / 'sh-stderr').write_text('')
            for mode, extra in [('default', skipped), ('signed', signed)]:
                (root / 'stdout').write_text(common + extra)
                subject.assert_install(mode, root, archive, sha, tag)
                with self.assertRaises(ValueError):
                    subject.assert_install('signed' if mode == 'default' else 'default', root, archive, sha, tag)
                with self.assertRaises(ValueError):
                    subject.assert_install(mode, root, archive, 'b' * 64, tag)

    def test_release_identity_ignores_download_count_but_not_digest(self):
        data = {'id': 1, 'tag_name': 'v6.9.0', 'draft': False, 'prerelease': False,
                'assets': [{'id': 2, 'name': 'asset', 'size': 3, 'digest': 'sha256:a', 'download_count': 4}]}
        before = subject.release_identity(data)
        data['assets'][0]['download_count'] += 1
        self.assertEqual(before, subject.release_identity(data))
        data['assets'][0]['digest'] = 'sha256:b'
        self.assertNotEqual(before, subject.release_identity(data))


if __name__ == '__main__':
    unittest.main()
