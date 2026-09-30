#!/usr/bin/env python3
"""Prepare a local release-pin promotion from supplied metadata (no authentication)."""
from __future__ import annotations

import argparse
import difflib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent / 'lib'))
from published_release import load_metadata, published_tag, version

# Closed current promotion scope. Counts constrain active lines, not historical versions.
INSTALL = r'^.*cargo install assay-cli --version '
RULES = {
    'README.md': [(INSTALL, 1), (r'^For v', 1), (r'^- Published v', 1), (r'^Current release:', 1)],
    'SECURITY.md': [(r'^Assay supports the current published release,', 1)],
    'docs/AIcontext/user-flows.md': [(INSTALL, 1)],
    'docs/getting-started/ci-integration.md': [(INSTALL, 4), (r'^For environments without a preinstalled Rust toolchain,', 1)],
    'docs/getting-started/index.md': [(INSTALL, 1)],
    'docs/getting-started/installation.md': [
        (INSTALL, 2), (r'^The current release is Assay ', 1), (r'^Download the asset for ', 1),
        (r'^assay-v.*-x86_64-pc-windows-msvc.zip$', 1), (r'^The `v.* image index is ', 1),
        (r'^assay [0-9]', 1), (r'^The generated \[agent golden path\]', 1),
        (r'^Behavior merged after ', 1)],
    'docs/getting-started/quickstart.md': [(INSTALL, 1)],
    'docs/guides/editor-mcp-recipe.md': [(INSTALL, 1), (r'^cargo install assay-mcp-server --version ', 1)],
    'docs/index.md': [(r'^Current release:', 1)],
    'docs/reference/cli/index.md': [(INSTALL, 1), (r'^# assay [0-9]', 1)],
    'docs/use-cases/air-gapped.md': [(r'^.*assay-v[0-9].*', 10)],
    'docs/use-cases/ci-gate.md': [(INSTALL, 1)],
    'examples/mcp-quickstart/README.md': [(INSTALL, 1), (r'^For v', 1)],
}
GENERATED = {
    'docs/generated/agent-golden-path.json', 'docs/guides/agent-golden-path.md',
    'packaging/agent-plugin/skills/assay-golden-path/references/agent-golden-path.json',
    'packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json',
}
PIN_PATHS = {'.github/assay-release-tag', '.github/assay-release-run-id'}
SURFACES = set(RULES) | GENERATED | PIN_PATHS | {'docs/reference/release.md'}
VERSION = re.compile(r'(?<![\w.])(?:v)?[0-9]+\.[0-9]+\.[0-9]+(?![\w.])')
OWNED_CODE = ('scripts/ci/promote-release-pin.py', 'scripts/ci/lib/published_release.py')


def run(args, root, **kwargs):
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(('GIT_', 'ASSAY_')) and k not in ('ROOT', 'GITHUB_OUTPUT')}
    env['PYTHONDONTWRITEBYTECODE'] = '1'
    result = subprocess.run(args, cwd=root, env=env, capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise ValueError(f'{args[0]} failed: {result.stdout}{result.stderr}')
    return result.stdout


def _require_single_successful_job(jobs, allowed_names, expected, label):
    # One shared cardinality/strict identity rule for every required
    # publication job group. Both the fixed-name route and the closed
    # x64-alias route call this; do not re-implement the predicate inline.
    # Retains strict type identity plus equality on run/head/attempt/status/
    # conclusion, and exactly-one cardinality over the closed name set.
    found = [job for job in jobs if isinstance(job, dict) and job.get('name') in allowed_names]
    if len(found) != 1 or any(
        type(found[0].get(key)) is not type(value) or found[0].get(key) != value
        for key, value in expected.items()
    ):
        raise ValueError(f'required successful job missing, ambiguous, or mismatched: {label}')


def identity(data):
    tag = published_tag(data['release'])
    execution, listing, image = data['run'], data['jobs'], data['image_binding']
    run_id, head = execution.get('id'), execution.get('head_sha')
    if type(run_id) is not int or run_id <= 0 or not re.fullmatch(r'[0-9a-f]{40}', str(head)):
        raise ValueError('invalid release run id or head SHA')
    if (execution.get('repository', {}).get('full_name') != 'Rul1an/assay'
        or execution.get('path') != '.github/workflows/release.yml'
        or execution.get('event') != 'push' or execution.get('head_branch') != tag
        or execution.get('status') != 'completed'):
        raise ValueError('release run must be a completed tag-push Release run for the supplied tag')
    attempt = execution.get('run_attempt')
    if type(attempt) is not int or attempt < 1:
        raise ValueError('missing release run attempt')
    jobs = listing.get('jobs')
    if not isinstance(jobs, list) or type(listing.get('total_count')) is not int or listing['total_count'] != len(jobs):
        raise ValueError('jobs response must be complete, not paginated or truncated')
    expected = {
        'run_id': run_id, 'head_sha': head, 'run_attempt': attempt,
        'status': 'completed', 'conclusion': 'success',
    }
    for name in ('Create Release', 'Publish to crates.io', 'Verify published image (ubuntu-24.04-arm)'):
        _require_single_successful_job(jobs, (name,), expected, name)
    # The x64 verify job renders from matrix.os: pre-pin receipts record
    # 'Verify published image (ubuntu-latest)', post-pin runs record
    # 'Verify published image (ubuntu-24.04)'. Accept exactly one of the two
    # closed identities so historical receipts still promote; both present,
    # neither present, or any other name (e.g. ubuntu-22.04) still refuses.
    # The run/head/attempt/success binding above is unchanged: this route
    # calls the same shared helper rather than a second inline copy.
    # Considered simpler alternative: one loop over all groups. Kept the
    # existing fixed-name loop plus one x64 call so the diff stays minimal
    # and each required identity keeps its own label.
    x64_names = ('Verify published image (ubuntu-24.04)', 'Verify published image (ubuntu-latest)')
    _require_single_successful_job(jobs, x64_names, expected, 'Verify published image (ubuntu-24.04)')
    digest = image.get('digest')
    if (type(image.get('run_id')) is not int or image.get('tag') != tag or image.get('run_id') != run_id or image.get('head_sha') != head
        or not re.fullmatch(r'sha256:[0-9a-f]{64}', str(digest))):
        raise ValueError('image binding must match the release tag, run, head, and digest')
    return tag, str(run_id), digest


def read_file(root, name):
    path = root / name
    if (path.is_symlink() or not path.is_file() or root not in path.resolve().parents
        or any(parent.is_symlink() for parent in path.parents if parent != root and root in parent.parents)):
        raise ValueError(f'{name}: expected an in-tree regular file, not a symlink')
    with path.open('rb') as source:
        data = source.read(4 * 1024 * 1024 + 1)
    if len(data) > 4 * 1024 * 1024:
        raise ValueError(f'{name}: exceeds 4 MiB ceiling')
    return data


def replace_lines(text, rules, old, new, path):
    lines = text.splitlines(keepends=True)
    for pattern, count in rules:
        positions = [i for i, line in enumerate(lines) if re.search(pattern, line)]
        if len(positions) != count:
            raise ValueError(f'{path}: expected {count} active lines for {pattern!r}, found {len(positions)}')
        for index in positions:
            matches = VERSION.findall(lines[index])
            if not matches or any(value.removeprefix('v') != old.removeprefix('v') for value in matches):
                raise ValueError(f'{path}: stale or ambiguous active release claim')
            lines[index] = VERSION.sub(lambda m: new if m[0].startswith('v') else new[1:], lines[index])
    return ''.join(lines)



def installability(root, tag):
    return run(['bash', '-c', 'source scripts/ci/release_asset_contract.sh; release_installability_markdown "$1"', 'matrix', tag], root)


def matrix_block(text):
    pattern = r'(?s)(<!-- release-installability-matrix:start -->\n)(.*?)(<!-- release-installability-matrix:end -->)'
    matches = list(re.finditer(pattern, text))
    if len(matches) != 1:
        raise ValueError('release installability matrix markers missing or ambiguous')
    return matches[0]


def snapshot(root):
    names = set(filter(None, run(['git', 'ls-files', '-z'], root).split('\0')))
    names.update(name for name in OWNED_CODE if (root / name).is_file())
    # Every input is bounded before scratch materialization. Untracked files are not inputs.
    result, total = {}, 0
    for name in sorted(names):
        data = read_file(root, name)
        total += len(data)
        if total > 128 * 1024 * 1024:
            raise ValueError('tracked snapshot exceeds 128 MiB ceiling')
        result[name] = data
    if not SURFACES <= result.keys():
        raise ValueError('promotion surface missing from tracked snapshot')
    return result


def plan(root, metadata):
    tag, run_id, digest = identity(metadata)
    original = snapshot(root)
    old = run(['bash', 'scripts/ci/read-assay-release-tag.sh'], root).strip()
    if version(tag) < version(old) or (version(tag) == version(old) and tag != old):
        raise ValueError('release promotion cannot downgrade the install pin')
    with tempfile.TemporaryDirectory(prefix='assay-pin-') as temp:
        scratch = Path(temp)
        for name, data in original.items():
            path = scratch / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            path.chmod(stat.S_IMODE((root / name).stat().st_mode))
        run(['git', 'init', '-q'], scratch)
        locks = [name for name in original if name.endswith('Cargo.lock')]
        # Preserve known tracked locks even when their checked-in ignore rules
        # would reject a fresh add. Do not discover or force-add other files.
        run(['git', 'update-index', '--add', '--', *locks], scratch)
        # Baseline drift must not be silently repaired during a promotion.
        run([sys.executable, 'scripts/docs/generate-agent-golden-path.py', '--check'], scratch)
        run(['bash', 'scripts/ci/check-release-surface.sh'], scratch)
        for path, rules in RULES.items():
            replace_lines((scratch / path).read_text(), rules, old, old, path)
        baseline_matrix = (scratch / 'docs/reference/release.md').read_text()
        if matrix_block(baseline_matrix)[2] != installability(scratch, old):
            raise ValueError('release installability matrix drifted')
        install_path = scratch / 'docs/getting-started/installation.md'
        install = install_path.read_text()
        old_digests = set(re.findall(r'ghcr.io/rul1an/assay-mcp-server@(sha256:[0-9a-f]{64})', install))
        if len(old_digests) != 1:
            raise ValueError('installation image digest is missing or inconsistent')
        if old == tag:
            if original['.github/assay-release-run-id'] != (run_id + '\n').encode() or old_digests != {digest}:
                raise ValueError('same-tag run/digest change requires separate review')
            return original, {}
        for path, rules in RULES.items():
            current = (scratch / path).read_text()
            (scratch / path).write_text(replace_lines(current, rules, old, tag, path))
        install = install_path.read_text().replace(next(iter(old_digests)), digest)
        # Minor tags are convenience aliases on this one release-index line only.
        install = re.sub(r'(?m)^(The `v.* image index is .*?)`[0-9]+\.[0-9]+`',
                         lambda m: m[1] + '`' + '.'.join(tag[1:].split('.')[:2]) + '`', install)
        proof = re.compile(r'^.*\[release run [0-9]+\]\(https://github.com/Rul1an/assay/actions/runs/[0-9]+\).*$', re.M)
        if len(proof.findall(install)) != 1:
            raise ValueError('installation requires exactly one release-run reference')
        install = proof.sub(
            f'For producer-side image verification results, see [release run {run_id}]'
            f'(https://github.com/Rul1an/assay/actions/runs/{run_id}). '
            'Verify the selected digest with the commands above; the link is not independent verification.', install)
        install_path.write_text(install)
        (scratch / '.github/assay-release-tag').write_text(tag + '\n')
        (scratch / '.github/assay-release-run-id').write_text(run_id + '\n')
        release_doc = scratch / 'docs/reference/release.md'
        text = release_doc.read_text()
        block = matrix_block(text)
        release_doc.write_text(text[:block.start(2)] + installability(scratch, tag) + text[block.end(2):])
        run([sys.executable, 'scripts/docs/generate-agent-golden-path.py'], scratch)
        run(['bash', 'scripts/ci/check-assay-release-pin.sh'], scratch)
        run(['bash', 'scripts/ci/check-release-surface.sh'], scratch)
        outputs = {p.relative_to(scratch).as_posix() for p in scratch.rglob('*')
                   if p.is_file() and '.git' not in p.relative_to(scratch).parts}
        if outputs - original.keys():
            raise ValueError(f'generated output outside promotion scope: {sorted(outputs - original.keys())}')
        changed = {name: (scratch / name).read_bytes() for name, old_bytes in original.items()
                   if (scratch / name).read_bytes() != old_bytes}
        if not changed.keys() <= SURFACES:
            raise ValueError(f'generated output outside promotion scope: {sorted(changed.keys() - SURFACES)}')
        return original, changed


def install_bytes(path, data):
    descriptor, name = tempfile.mkstemp(prefix='.assay-pin-', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'wb') as stream:
            stream.write(data)
        os.chmod(name, stat.S_IMODE(path.stat().st_mode))
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def apply(root, original, changed):
    for name in original:
        if read_file(root, name) != original[name]:
            raise ValueError(f'{name}: changed after preflight')
    written = []
    try:
        for name in sorted(changed):
            install_bytes(root / name, changed[name])
            written.append(name)
    except OSError:
        for name in reversed(written):
            install_bytes(root / name, original[name])
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--metadata', type=Path, required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--apply', action='store_true')
    mode.add_argument('--check', action='store_true')
    args = parser.parse_args()
    try:
        original, changed = plan(args.root.resolve(), load_metadata(args.metadata))
        for name in sorted(changed):
            sys.stdout.writelines(difflib.unified_diff(original[name].decode().splitlines(True),
                changed[name].decode().splitlines(True), fromfile='a/' + name, tofile='b/' + name))
        if args.apply:
            apply(args.root.resolve(), original, changed)
        return 1 if args.check and changed else 0
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f'release-pin promotion refused: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
