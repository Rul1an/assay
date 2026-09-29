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
        # Exact-block pin (round 3, reviewer F2): the previous guard enumerated
        # forbidden keys (run:/shell:/if:/continue-on-error:) and stayed green
        # while a single step- or job-level `env:` line neutralised the battery
        # -- `SHELLOPTS: noexec` makes bash read without executing, and
        # `BASH_ENV` pointing at an `exit 0` file makes bash source it before
        # the script. This is the second round in which an enumerated
        # forbidden-key list turned out open, so the mechanism changes from
        # enumeration to closure: the whole job block, from its header to the
        # next job header with trailing blank lines normalised, must equal the
        # literal below. Any inserted, changed, or removed line -- env,
        # working-directory, defaults, second step, comment-out, `|| true`,
        # if:, continue-on-error:, inert shell -- fails this test while every
        # other guard stays green.
        #
        # Self-check (reviewer's question, in our words from round 2): what
        # execution-neutralising variant of each pinned command, shell, env or
        # step condition must fail this test while every other guard stays
        # green? Answered by the mutation controls below: each variant is
        # applied to the real block text and must fail the equality check.
        #
        # Pins float by suffix only (round 4, reviewer F4): the previous
        # whole-line mask let the action identity float, so swapping the same
        # `uses:` line in BOTH jobs to another action stayed green while that
        # action ran before the battery in the same workspace. The `uses:`
        # repositories are now literal (`actions/checkout`, `actions/setup-python`);
        # only the ref suffix (`<40-hex sha> # vX.Y.Z`) floats, and each suffix
        # must equal the same action's suffix in `release-asset-contract`,
        # which Dependabot bumps in the same PR. The joint-bump control below
        # stays green to prove the literal does not freeze pins; a bump applied
        # to only one of the two jobs fails by design -- resync both lines.
        # A suffix that is not a 40-hex SHA plus version comment fails outright,
        # so `@v5` or a bare-SHA foreign action never matches. The literal below
        # carries `<CHECKOUT-PIN>` / `<SETUP-PYTHON-PIN>` placeholders (reviewer
        # F6), never a copied SHA, so it cannot go stale.
        #
        # Self-check answer (round 4): no `uses:`-line change in one job or both
        # substitutes another program while staying green -- the repo is literal
        # and each suffix must be well-formed and match per-action. The only
        # green `uses:` change is a joint same-repo SHA bump (Dependabot's case),
        # so the residual trust is in review of the bump PR itself.
        #
        # Out of scope: workflow-level `env:` / `defaults:` would neutralise
        # the `ci` gate's own evaluate step too; no per-job pin can own that.
        #
        # False-red warning for the next builder: this trades flexibility for
        # closure -- ANY edit to the job block, even a benign one (added
        # comment, reformatting, renamed step `name:`, `timeout-minutes` bump,
        # runner-label change, `concurrency:`), fails this test by design.
        # Update the literal deliberately after verifying the edit keeps the
        # battery executing and failing red.
        job = 'published-release-golden-path-contract'
        reference = 'release-asset-contract'
        command = 'bash scripts/ci/test-published-release-golden-path-contract.sh'
        variable = 'PUBLISHED_RELEASE_GOLDEN_PATH_CONTRACT_RESULT'
        binding = variable + ': ${{ needs.' + job + '.result }}'
        triple = '"' + job + '|${' + variable + '}|required"'
        def ci_needs(text):
            import re
            rest = text.split('\n  ci:\n', 1)[1]
            end = re.search(r'\n  [A-Za-z0-9_-]+:', rest)
            block = rest if end is None else rest[:end.start()]
            needs = block.split('needs: [', 1)[1].split(']', 1)[0]
            return [item.strip() for item in needs.split(',')]
        def job_slice(text, name):
            import re
            rest = text.split('\n  ' + name + ':\n', 1)[1]
            end = re.search(r'\n  [A-Za-z0-9_-]+:', rest)
            body = rest if end is None else rest[:end.start()]
            return '\n  ' + name + ':\n' + body
        def normalise(block):
            return block.rstrip() + '\n'
        def uses_entries(block):
            entries = []
            for line in block.splitlines():
                stripped = line.strip()
                if stripped.startswith('uses:') or stripped.startswith('- uses:'):
                    token = stripped.split('uses:', 1)[1].strip()
                    if '@' in token:
                        repo, suffix = token.split('@', 1)
                        entries.append((repo.strip(), suffix.strip()))
                    else:
                        entries.append((token.strip(), None))
            return entries
        def mask_suffix(block):
            pins = {'actions/checkout': '<CHECKOUT-PIN>',
                    'actions/setup-python': '<SETUP-PYTHON-PIN>'}
            out = []
            for line in block.splitlines():
                stripped = line.strip()
                if ((stripped.startswith('uses:') or stripped.startswith('- uses:'))
                        and '@' in stripped.split('uses:', 1)[1]):
                    repo = stripped.split('uses:', 1)[1].split('@', 1)[0].strip()
                    out.append(line.split('@', 1)[0] + '@' + pins.get(repo, '<UNKNOWN-PIN>'))
                else:
                    out.append(line)
            return '\n'.join(out).rstrip() + '\n'
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
        expected = (
            '\n  ' + job + ':\n'
            '    name: Published release golden-path contract\n'
            '    runs-on: ubuntu-latest\n'
            '    timeout-minutes: 10\n'
            '    permissions:\n'
            '      contents: read\n'
            '    steps:\n'
            '      - uses: actions/checkout@<CHECKOUT-PIN>\n'
            '        with:\n'
            '          persist-credentials: false\n'
            '      - name: Set up Python\n'
            '        uses: actions/setup-python@<SETUP-PYTHON-PIN>\n'
            '        with:\n'
            '          python-version: "3.12"\n'
            '      - name: Verify published release golden-path contract\n'
            '        shell: bash\n'
            '        run: ' + command + '\n'
        )
        def check(text):
            import re
            self.assertIn('\n  ' + job + ':\n', text, 'required contract job missing from ci.yml')
            actual = normalise(job_slice(text, job))
            reference_block = normalise(job_slice(text, reference))
            actual_entries = uses_entries(actual)
            reference_entries = uses_entries(reference_block)
            self.assertEqual([repo for repo, _ in actual_entries],
                             ['actions/checkout', 'actions/setup-python'],
                             'required contract uses: action identity must stay actions/checkout and actions/setup-python')
            suffix_shape = re.compile(r'^[0-9a-f]{40} # v[0-9]+\.[0-9]+\.[0-9]+$')
            for repo, suffix in actual_entries + reference_entries:
                self.assertIsNotNone(suffix, 'uses: %s must carry a pinned ref, not a bare action name' % repo)
                self.assertRegex(suffix, suffix_shape,
                                 'uses: %s ref must be a 40-hex SHA plus version comment' % repo)
            self.assertEqual(dict(actual_entries), dict(reference_entries),
                             'required contract uses: pin suffixes must match release-asset-contract (Dependabot bumps both)')
            self.assertEqual(mask_suffix(actual), normalise(expected),
                             'required contract job block must equal the pinned literal')
            self.assertIn(job, ci_needs(text), 'required contract job missing from ci needs')
            self.assertEqual(text.count(binding), 1, 'required contract result binding missing or duplicate')
            self.assertEqual(text.count(triple), 1, 'required contract result not evaluated')
        text = (ROOT / '.github/workflows/ci.yml').read_text()
        name_line = '      - name: Verify published release golden-path contract\n'
        header = '\n  ' + job + ':\n'
        check(text)
        with self.assertRaisesRegex(AssertionError, 'required contract job'):
            check(text.replace(header, '\n', 1))
        with self.assertRaisesRegex(AssertionError, 'required contract job missing from ci needs'):
            check(text.replace(', ' + job, '', 1))
        with self.assertRaisesRegex(AssertionError, 'required contract result binding'):
            check(text.replace(binding, ':', 1))
        with self.assertRaisesRegex(AssertionError, 'required contract result not evaluated'):
            check(text.replace(triple, ':', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('run: ' + command, 'run: ' + command + ' || true', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('run: ' + command, 'run: "true"  # ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(name_line, name_line + '        if: false\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    if: false\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command,
                               '        continue-on-error: true\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(name_line + '        shell: bash\n',
                               name_line + '        shell: echo {0}\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command,
                               '        env:\n          SHELLOPTS: noexec\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    env:\n      SHELLOPTS: noexec\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command,
                               '        env:\n          BASH_ENV: /tmp/contract-bypass-env\n'
                               '        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    env:\n      BASH_ENV: /tmp/contract-bypass-env\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command,
                               '        env:\n          WORKFLOW: /dev/null\n'
                               '        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    env:\n      WORKFLOW: /dev/null\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command,
                               '        working-directory: scripts\n        run: ' + command, 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    working-directory: scripts\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace('        run: ' + command + '\n',
                               '        run: ' + command + '\n'
                               '      - name: Extra step\n'
                               '        shell: bash\n'
                               '        run: echo extra\n', 1))
        with self.assertRaisesRegex(AssertionError, 'contract job block'):
            check(text.replace(header, header + '    defaults:\n      run:\n        shell: bash\n', 1))
        # Dependabot-style pin bump applied to BOTH jobs stays green: the
        # literal does not freeze pins. Suffixes are read off the live file, so
        # neither the green control nor the red ones below can go stale
        # (reviewer F6); check(text) above already proved they are well-formed,
        # so slicing off the 40-hex SHA keeps the ` # vX.Y.Z` comment intact.
        live_suffix = dict(uses_entries(normalise(job_slice(text, reference))))
        checkout_suffix = live_suffix['actions/checkout']
        setup_suffix = live_suffix['actions/setup-python']
        checkout_token = 'uses: actions/checkout@' + checkout_suffix
        joint_checkout = '0' * 40 + checkout_suffix[40:]
        bumped = text.replace(checkout_suffix, joint_checkout).replace(
            setup_suffix, '1' * 40 + setup_suffix[40:])
        check(bumped)
        # Reviewer F5: each of these must FAIL. The two single-job bumps are the
        # only controls that bite through the suffix-equality assertion -- delete
        # it and they stop raising while the masked literal still passes.
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
        with self.assertRaisesRegex(AssertionError, '40-hex SHA plus version comment'):
            check(both_v5)
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
