#!/usr/bin/env python3
"""Run and retain the genuine streaming install; never author installer policy.

Preflight and execution are different HTTP responses. Executed-byte equality is
checked AFTER the real curl | tee | sh pipeline has completed, not before it.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_download import download
from cosign_release_pin import read_pin

LIVE_URL = 'https://getassay.dev/install.sh'
INSTALLER_LIMIT = 65536
OUTPUT_LIMIT = 4 * 1024 * 1024
PIPELINE = r'''set -o pipefail
"$1" -fsSL --connect-timeout 10 --max-time 60 --max-filesize 65536 "$4" 2>&"$7" | "$2" "$5" 2>&"$8" | "$3" 2>&"$9"
codes=("${PIPESTATUS[@]}")
printf '%s\n' "${codes[@]}" > "$6"
for code in "${codes[@]}"; do
  if [ "$code" -ne 0 ]; then exit 1; fi
done
'''


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def supervise(argv, env, output, *, timeout=180, output_limit=OUTPUT_LIMIT, extra_streams=()):
    """Cap each retained pipe before writing; bound and reap our process group."""
    output = Path(output)
    output.mkdir()
    streams, writers = {}, []
    for name in extra_streams:
        reader, writer = os.pipe()
        streams[name] = os.fdopen(reader, 'rb', buffering=0)
        writers.append(writer)
    argv = [*argv, *(str(fd) for fd in writers)]
    record = {'argv': argv, 'status': 'running', 'timeout_seconds': timeout,
              'output_limit_bytes_per_stream': output_limit}
    started = time.monotonic()
    try:
        process = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True, pass_fds=tuple(writers))
    except BaseException:
        for stream in streams.values():
            stream.close()
        raise
    finally:
        for writer in writers:
            os.close(writer)
    streams.update(stdout=process.stdout, stderr=process.stderr)
    selector = selectors.DefaultSelector()
    handles = {}
    sizes = dict.fromkeys(streams, 0)
    failure = None
    for name, stream in streams.items():
        selector.register(stream, selectors.EVENT_READ, name)
        handles[name] = (output / name).open('xb')
    try:
        while selector.get_map():
            if time.monotonic() - started > timeout:
                raise TimeoutError('owned process group exceeded wall-time limit')
            for key, _ in selector.select(0.05):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                name = key.data
                require(sizes[name] + len(chunk) <= output_limit, f'{name} output ceiling exceeded')
                handles[name].write(chunk)
                sizes[name] += len(chunk)
        process.wait(timeout=max(0.1, timeout - (time.monotonic() - started)))
    except BaseException as error:
        failure = error
    finally:
        # Even an exited leader can leave descendants holding pipes or running.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        for handle in handles.values():
            handle.close()
        for stream in streams.values():
            stream.close()
        selector.close()
        record.update(exit_code=process.returncode, sizes=sizes,
                      elapsed_seconds=time.monotonic() - started,
                      status='failed' if failure or process.returncode else 'completed')
        if failure:
            record['failure'] = str(failure)
        write_json(output / 'command.json', record)
    if failure:
        raise failure
    return record


def run_pipeline(curl, tee, shell, url, capture, env, output, *, timeout=180, output_limit=OUTPUT_LIMIT):
    capture = Path(capture)
    require(not capture.exists(), 'execution capture must be fresh')
    statuses = capture.with_suffix('.pipeline-status')
    require(not statuses.exists(), 'pipeline status must be fresh')
    argv = ['/bin/bash', '--noprofile', '--norc', '-c', PIPELINE, 'installer-pipeline',
            str(curl), str(tee), str(shell), url, str(capture), str(statuses)]
    result = supervise(argv, env, output, timeout=timeout, output_limit=output_limit,
                       extra_streams=('curl-stderr', 'tee-stderr', 'sh-stderr'))
    codes = [int(line) for line in statuses.read_text().splitlines()] if statuses.exists() else []
    result['pipeline_status'] = codes
    write_json(Path(output) / 'command.json', result)
    require(result['exit_code'] == 0 and codes == [0, 0, 0], f'installer pipeline failed: {codes}')
    require(capture.is_file() and 0 < capture.stat().st_size <= INSTALLER_LIMIT,
            'execution capture empty or over ceiling')
    return result


def child_environment(home, prefix, scratch, cosign=None):
    # An allowlist, not a claim that clearing token names sandboxes a process.
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home),
           'TMPDIR': str(scratch), 'LANG': 'C', 'LC_ALL': 'C',
           'ASSAY_INSTALL_DIR': str(prefix / 'bin')}
    if cosign:
        env['ASSAY_COSIGN'] = str(cosign)
    return env


def assert_install(mode, output, archive, archive_sha, release_tag):
    text = (Path(output) / 'stdout').read_text() + (Path(output) / 'sh-stderr').read_text()
    require(f'verification=checksum_verified asset={archive} sha256={archive_sha}' in text,
            'installer did not verify the selected archive digest')
    require('verification=provenance_not_requested' in text, 'installer opt-in provenance unexpectedly selected')
    signed = f'verification=signed_manifest_verified asset={archive} identity=https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/{release_tag}'
    skipped = 'verification=signed_manifest_skipped reason=cosign_not_installed'
    if mode == 'default':
        require(skipped in text and 'verification=signed_manifest_verified' not in text,
                'default installer did not take the absent-cosign path')
    else:
        require(signed in text and 'signed_manifest_skipped' not in text,
                'signed installer did not verify the manifest')


def assert_binary(binary, reference, version, env, output):
    require(binary.is_file() and not binary.is_symlink() and os.access(binary, os.X_OK),
            'installed executable missing or unsafe')
    require(digest(binary) == digest(reference), 'installed binary differs from verified archive member')
    result = supervise([str(binary), 'version'], env, output)
    require(result['exit_code'] == 0 and (Path(output) / 'stdout').read_text().strip() == version,
            'installed version differs from selected release')
    long_version = Path(str(output) + '-long')
    result = supervise([str(binary), '--version'], env, long_version)
    require(result['exit_code'] == 0 and (long_version / 'stdout').read_text().strip() == 'assay ' + version,
            'installed --version differs from selected release')
    return digest(binary)


def fetch_asset(api, name, dest, ceiling):
    matches = [row for row in api['assets'] if row['name'] == name]
    require(len(matches) == 1, f'release asset is not unique: {name}')
    row = matches[0]
    require(0 < row['size'] <= ceiling, 'release asset size outside ceiling')
    url = f"https://github.com/Rul1an/assay/releases/download/{api['tag_name']}/{name}"
    require(row['browser_download_url'] == url, 'unexpected asset URL')
    download(url, dest, max_bytes=ceiling)
    require(dest.stat().st_size == row['size'] and f'sha256:{digest(dest)}' == row['digest'],
            'retained proof asset differs from release API')


# A closed data adapter, not a replacement product or an untrusted payload.
ADAPTER = r'''import json, os, pathlib, shutil, sys
args = sys.argv[1:]
url = args[-1]
rows = json.loads(pathlib.Path(os.environ['PROOF_TRANSFER_MAP']).read_text())
with open(os.environ['PROOF_TRANSFER_LOG'], 'a') as stream:
    stream.write(json.dumps(args) + '\n')
if url not in rows or '-o' not in args:
    raise SystemExit(90)
source = pathlib.Path(rows[url])
if '--max-filesize' in args and source.stat().st_size > int(args[args.index('--max-filesize') + 1]):
    raise SystemExit(63)
shutil.copyfile(source, args[args.index('-o') + 1])
if '-w' in args:
    sys.stdout.write('200')
'''


def contrasts(source, archive, api, env, root, reference, version):
    data = root / 'contrast-data'
    data.mkdir()
    for name, bound in [(archive.name + '.sha256', 4096), ('checksums.txt', 65536),
                        ('checksums.txt.sigstore.json', 1048576)]:
        fetch_asset(api, name, data / name, bound)
    original = (data / 'checksums.txt').read_bytes()
    mapping = {f"https://github.com/Rul1an/assay/releases/download/{api['tag_name']}/{p.name}": str(p)
               for p in [archive, *data.iterdir()]}
    map_path = root / 'transfer-map.json'
    write_json(map_path, mapping)
    adapter_dir = root / 'transfer-bin'
    adapter_dir.mkdir()
    adapter = adapter_dir / 'curl'
    adapter.write_text('#!' + sys.executable + '\n' + ADAPTER)
    adapter.chmod(0o755)
    child = dict(env, PATH=str(adapter_dir) + ':' + env['PATH'],
                 PROOF_TRANSFER_MAP=str(map_path), PROOF_TRANSFER_LOG=str(root / 'transfer.ndjson'))
    records = []
    for name, override, bad_data, reason in [
        ('empty-version', {'ASSAY_VERSION': ''}, False, 'ASSAY_VERSION must be'),
        ('empty-cosign', {'ASSAY_COSIGN': ''}, False, 'ASSAY_COSIGN must be'),
        ('bad-manifest', {}, True, 'signed checksum manifest verification failed'),
        ('restored-manifest', {}, False, None),
    ]:
        prefix = root / name
        (prefix / 'bin').mkdir(parents=True)
        binary = prefix / 'bin/assay'
        shutil.copy2(reference, binary)
        before = digest(binary)
        (data / 'checksums.txt').write_bytes(b'\n' + original if bad_data else original)
        transfer_log = Path(child['PROOF_TRANSFER_LOG'])
        prior_transfers = transfer_log.read_bytes() if transfer_log.exists() else b''
        case_env = dict(child, ASSAY_INSTALL_DIR=str(prefix / 'bin'), **override)
        result = supervise(['/bin/sh', str(source)], case_env, root / (name + '-command'))
        text = (root / (name + '-command') / 'stderr').read_text() + (root / (name + '-command') / 'stdout').read_text()
        if reason:
            require(result['exit_code'] != 0 and reason in text and 'Assay installed to:' not in text,
                    f'{name}: missing expected refusal')
            require(digest(binary) == before, f'{name}: prior binary changed')
            if override:
                require((transfer_log.read_bytes() if transfer_log.exists() else b'') == prior_transfers,
                        'malformed-input contrast accessed the transfer adapter')
        else:
            require(result['exit_code'] == 0, 'restored manifest control failed')
            assert_binary(binary, reference, version, child, root / 'restored-version')
        records.append({'name': name, 'exit_code': result['exit_code'],
                        'expected_reason': reason, 'binary_before': before,
                        'binary_after': digest(binary), 'passed': True})
        binary.unlink()  # Identity is retained; do not upload repeated executable copies.
    (data / 'checksums.txt').write_bytes(original)
    write_json(root / 'contrasts.json', records)


def release_identity(document):
    return {key: document[key] for key in ('id', 'tag_name', 'draft', 'prerelease')} | {
        'assets': sorted((row['id'], row['name'], row['size'], row.get('digest'))
                         for row in document['assets'])}


def main():
    if len(sys.argv) != 1:
        raise SystemExit('usage: published_release_installer.py (from the staged harness)')
    harness = Path(__file__).resolve().parents[2]
    require(harness.name == 'harness', 'installer phase requires the staged manifest-verified harness')
    run_root = harness.parent
    results = run_root / 'results'
    root = results / 'installer'
    root.mkdir()
    release_api = results / 'release-api.json'
    tag_ref = results / 'tag-ref.json'
    receipt = {'schema': 'assay.published_installer.v1', 'status': 'failed',
               'claim': 'instrumented curl | tee | sh; executed-byte equality checked after execution',
               'limits': {'installer_transfer': INSTALLER_LIMIT, 'stream_output': OUTPUT_LIMIT,
                          'phase_timeout_seconds': 180, 'scratch_quota': None}, 'installations': []}
    try:
        api = json.loads(release_api.read_text())
        release_tag = api['tag_name']
        source_sha = (results / 'journey-source-sha.txt').read_text().strip()
        cosign_release = read_pin(harness / '.github/workflows/release.yml')
        receipt.update(release_tag=release_tag, source_commit=source_sha)
        require(cosign_release == os.environ.get('PUBLISHED_COSIGN_RELEASE'), 'cosign action pin differs from release owner')
        require(re.fullmatch(r'v\d+\.\d+\.\d+', release_tag), 'invalid release tag')
        require(re.fullmatch(r'[0-9a-f]{40}', source_sha), 'invalid source SHA')
        version = release_tag[1:]
        require(api['tag_name'] == release_tag, 'release identity mismatch')
        assets = [p for p in (results / 'release-assets').glob('assay-*.tar.gz')
                  if not p.name.startswith('assay-mcp-server-')]
        require(len(assets) == 1, 'installer reference CLI archive is not unique')
        archive = assets[0]
        references = [p for p in (run_root / 'cli-extract').rglob('assay')
                      if p.is_file() and not p.is_symlink() and os.access(p, os.X_OK)]
        require(len(references) == 1, 'verified archive CLI member is not unique')
        reference = references[0]
        prefix = run_root / 'install'
        receipt.update(archive=archive.name, archive_sha256=digest(archive), reference_binary_sha256=digest(reference))
        source = root / 'released-install.sh'
        download(f'https://raw.githubusercontent.com/Rul1an/assay/{source_sha}/scripts/install.sh', source, max_bytes=INSTALLER_LIMIT)
        preflight = root / 'preflight-install.sh'
        download(LIVE_URL, preflight, max_bytes=INSTALLER_LIMIT)
        require(preflight.read_bytes() == source.read_bytes(), 'live preflight differs from selected released installer')
        receipt['installer_sha256'] = digest(source)
        curl = shutil.which('curl', path='/usr/bin:/bin')
        require(curl is not None, 'system curl unavailable')
        tool_version = subprocess.check_output([curl, '--version'], text=True, timeout=10)
        parsed = re.match(r'curl (\d+)\.(\d+)\.(\d+)', tool_version)
        require(parsed and tuple(map(int, parsed.groups())) >= (8, 4, 0), 'curl lacks streamed max-filesize enforcement')
        receipt['curl_version'] = tool_version
        cosign = Path(shutil.which('cosign') or '').resolve(strict=True)
        require(str(cosign) == os.path.realpath(os.environ.get('PUBLISHED_COSIGN', '')), 'cosign invocation differs from workflow selection')
        require(cosign.is_file() and os.access(cosign, os.X_OK), 'native cosign unavailable')
        for mode in ('default', 'signed'):
            mode_root = root / mode
            mode_root.mkdir()
            home, scratch = mode_root / 'home', mode_root / 'tmp'
            home.mkdir(); scratch.mkdir()
            selected_prefix = prefix if mode == 'default' else mode_root / 'install'
            require(not (selected_prefix / 'bin/assay').exists(), 'install destination already populated')
            env = child_environment(home, selected_prefix, scratch, cosign if mode == 'signed' else None)
            env['ASSAY_VERSION'] = version
            require(shutil.which('cosign', path=env['PATH']) is None, 'default PATH unexpectedly provides cosign')
            if mode == 'signed':
                cv = supervise([str(cosign), 'version'], env, mode_root / 'cosign-version')
                actual = (mode_root / 'cosign-version/stdout').read_text()
                require(cv['exit_code'] == 0 and re.search(r'(?m)^GitVersion:\s*' + re.escape(cosign_release) + r'\s*$', actual), 'actual cosign version differs from pin')
                receipt['cosign'] = {'path': str(cosign), 'sha256': digest(cosign), 'version': actual}
            capture = mode_root / 'executed-install.sh'
            result = run_pipeline(curl, '/usr/bin/tee', '/bin/sh', LIVE_URL, capture, env, mode_root / 'pipeline')
            require(capture.read_bytes() == source.read_bytes(), 'executed streaming response differs from released installer')
            assert_install(mode, mode_root / 'pipeline', archive.name, digest(archive), release_tag)
            binary_sha = assert_binary(selected_prefix / 'bin/assay', reference, version, env, mode_root / 'version')
            receipt['installations'].append({'mode': mode, 'argv': result['argv'], 'environment': env,
                                            'pipeline_status': result['pipeline_status'],
                                            'capture_sha256': digest(capture), 'binary_sha256': binary_sha,
                                            'prefix': str(selected_prefix), 'version': version})
        contrasts(source, archive, api, env, root, reference, version)
        (root / 'signed/install/bin/assay').unlink()
        require(digest(prefix / 'bin/assay') == digest(reference), 'default binary changed during sidephase')
        for name, endpoint, old in [('release', f'releases/tags/{release_tag}', api),
                                    ('tag', f'git/ref/tags/{release_tag}', json.loads(tag_ref.read_text()))]:
            path = root / f'post-{name}.json'
            download('https://api.github.com/repos/Rul1an/assay/' + endpoint, path, max_bytes=2097152,
                     accept='application/vnd.github+json')
            after = json.loads(path.read_text())
            require((release_identity(after) == release_identity(old)) if name == 'release' else after['object'] == old['object'],
                    f'{name} metadata changed during installation')
        receipt['status'] = 'completed'
    except Exception as error:
        receipt['failure'] = str(error)
        raise
    finally:
        write_json(root / 'receipt.json', receipt)
        retained = []
        for path in sorted(root.rglob('*')):
            if path.is_file() and not path.is_symlink():
                retained.append({'path': path.relative_to(root).as_posix(),
                                 'size': path.stat().st_size, 'sha256': digest(path)})
        write_json(root / 'retained-files.json', {'files': retained})


if __name__ == '__main__':
    main()
