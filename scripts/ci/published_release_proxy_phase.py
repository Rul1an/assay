#!/usr/bin/env python3
"""Execute and record the published-release proxy phase from one argv value."""

from __future__ import annotations

import argparse
import hashlib
import re
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

try:
    import resource
except ModuleNotFoundError:  # Windows has no resource module.
    resource = None


MAX_REQUEST_BYTES = 1_048_576
MAX_OUTPUT_BYTES = 16_777_216


def bounded_seconds(value: str) -> int:
    seconds = int(value)
    if not 1 <= seconds <= 300:
        raise argparse.ArgumentTypeError("timeout must be between 1 and 300 seconds")
    return seconds


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-seconds", type=bounded_seconds, default=60)
    parser.add_argument("--fixture-dir", type=Path)
    parser.add_argument("--policy", choices=("deny", "allow"), default="deny")
    parser.add_argument("--expect", choices=("deny", "allow", "unsupported"))
    return parser.parse_args()


def append_command_record(path: Path, exit_code: int, argv: list[str]) -> None:
    record = {"name": "proxy-enforce", "exit_code": exit_code, "argv": argv}
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def child_environment() -> dict[str, str]:
    allowed = ("HOME", "PATH", "LANG", "LC_ALL", "TZ")
    return {key: value for key, value in os.environ.items()
            if key in allowed or (sys.platform == "win32" and key.upper() == "SYSTEMROOT")}


def limit_child_output() -> None:
    if resource is None:
        raise RuntimeError("RLIMIT_FSIZE is unavailable")
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT_BYTES, MAX_OUTPUT_BYTES))


def stop_process_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def read_records(path: Path) -> list[dict]:
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON member: {key}")
            result[key] = value
        return result

    with path.open("rb") as handle:
        data = handle.read(MAX_OUTPUT_BYTES + 1)
    if len(data) > MAX_OUTPUT_BYTES:
        raise ValueError(f"{path.name} exceeds output ceiling")
    records = [json.loads(line, object_pairs_hook=unique_object) for line in data.splitlines() if line.strip()]
    if not all(isinstance(record, dict) for record in records):
        raise ValueError(f"{path.name} contains a non-object record")
    return records


def case_request_ids(expected: str) -> tuple[int, ...]:
    return (1, 9) if expected == "allow" else (9,)


def exchange_case(process: subprocess.Popen[bytes], request: bytes, output: Path,
                  expected_lines: int, timeout: int) -> int:
    # communicate() closes stdin immediately. Keep it open until the proxy has drained replies.
    # stdout stays file-backed with RLIMIT_FSIZE; nonblocking writes share the same deadline.
    deadline = time.monotonic() + timeout
    pending = memoryview(request)
    os.set_blocking(process.stdin.fileno(), False)
    lines = 0
    with output.open("rb") as reader:
        while True:
            if pending:
                try:
                    pending = pending[os.write(process.stdin.fileno(), pending):]
                except BlockingIOError:
                    pass
                except BrokenPipeError:
                    break
            lines += reader.read(65536).count(b"\n")
            if (not pending and lines >= expected_lines) or process.poll() is not None:
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise subprocess.TimeoutExpired(process.args, timeout)
            time.sleep(min(0.01, remaining))
    process.stdin.close()
    return process.wait(timeout=max(0.001, deadline - time.monotonic()))


def load_windows_launcher():
    path = Path(__file__).with_name("published_release_offline_windows.py")
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", path)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_proxy_child(
    argv: list[str],
    request: bytes,
    stdout_path: Path,
    stderr_path: Path,
    *,
    expected_lines: int | None,
    timeout: int,
    observation: dict | None = None,
) -> int:
    if sys.platform == "win32":
        result = load_windows_launcher().launch_interactive_job(
            argv,
            child_environment(),
            request,
            timeout,
            expected_lines,
            MAX_OUTPUT_BYTES,
        )
        if observation is not None and isinstance(result, dict):
            observation.update({key: result.get(key) for key in (
                "create_process", "job_closed", "wait_result", "truncated", "job_total_processes", "exit")})
        stdout = result.get("stdout") if isinstance(result, dict) else b""
        stderr = result.get("stderr") if isinstance(result, dict) else b""
        if not isinstance(stdout, bytes) or not isinstance(stderr, bytes):
            stdout, stderr = b"", b"invalid Windows job output types"
        stdout_path.write_bytes(stdout[:MAX_OUTPUT_BYTES])
        stderr_path.write_bytes(stderr[:MAX_OUTPUT_BYTES])
        total = result.get("job_total_processes") if isinstance(result, dict) else None
        valid = (
            isinstance(result, dict)
            and result.get("create_process") is True
            and result.get("job_closed") is True
            and result.get("wait_result") == "exited"
            and result.get("truncated") is False
            and isinstance(total, int)
            and not isinstance(total, bool)
            and total >= 1
            and len(stdout) <= MAX_OUTPUT_BYTES
            and len(stderr) <= MAX_OUTPUT_BYTES
        )
        if not valid:
            return 124 if isinstance(result, dict) and result.get("wait_result") == "timeout" else 125
        status = result.get("exit")
        return status if isinstance(status, int) and not isinstance(status, bool) and 0 <= status <= 255 else 125

    with stdout_path.open("wb") as stdout_handle, stderr_path.open("wb") as stderr_handle:
        try:
            process = subprocess.Popen(
                argv,
                stdin=subprocess.PIPE,
                stdout=stdout_handle,
                stderr=stderr_handle,
                env=child_environment(),
                preexec_fn=limit_child_output,
                start_new_session=True,
            )
            try:
                if expected_lines is not None:
                    status = exchange_case(process, request, stdout_path, expected_lines, timeout)
                else:
                    process.communicate(input=request, timeout=timeout)
                    status = process.returncode
            except subprocess.TimeoutExpired:
                status = 124
            finally:
                stop_process_group(process)
        except OSError as error:
            stderr_handle.write(f"proxy process failed to start: {error}\n".encode())
            status = 127
    return status


def validate_case(results: Path, expected: str) -> None:
    def require(condition, message):
        if not condition:
            raise ValueError(message)

    wire = read_records(results / "proxy.jsonl")
    request_ids = case_request_ids(expected)
    require(len(wire) == len(request_ids), "unexpected response cardinality for request case")
    for record, request_id in zip(wire, request_ids):
        require(record.get("jsonrpc") == "2.0" and type(record.get("id")) is int
                and record["id"] == request_id, "wire request identity drifted")
    if expected == "allow":
        require(isinstance(wire[0].get("result"), dict) and "error" not in wire[0], "initialize failed")
    reply = wire[-1]
    if expected == "allow":
        require("error" not in reply and isinstance(reply.get("result"), dict), "allow request failed")
        require(reply["result"].get("isError") is False and reply["result"].get("content") == [
            {"type": "text", "text": "forwarded-ok (mock; no real GitHub call)"}
        ], "allow reply is not the credential-free mock result")
    else:
        code, reason = ((-31997, "method_not_allowlisted") if expected == "unsupported"
                        else (-31999, "no_declared_allowance"))
        error = reply.get("error")
        require("result" not in reply and isinstance(error, dict), "expected a typed proxy error")
        require(type(error.get("code")) is int and error["code"] == code
                and isinstance(error.get("data"), dict)
                and error["data"].get("origin") == "assay-proxy"
                and error["data"].get("reason") == reason, "proxy error contract drifted")
    decisions = results / "decisions.ndjson"
    observations = results / "denied-observations.ndjson"
    if expected == "unsupported":
        require(not decisions.exists() and not observations.exists(), "unsupported request retained evidence")
        return
    rows = read_records(decisions)
    require(len(rows) == 1, "expected exactly one policy decision")
    row = rows[0]
    require(row.get("schema") == "assay.enforcement_decision.v0"
            and row.get("decision") == expected
            and row.get("reason") == ("allow" if expected == "allow" else "no_declared_allowance"),
            "policy decision does not match the request case")
    require(isinstance(row.get("tool"), dict) and row["tool"].get("name") == "github.add_deploy_key"
            and isinstance(row.get("action"), dict)
            and row["action"].get("target") == {"provider": "github", "owner": "acme", "repo": "prod-app"},
            "policy decision does not describe the single fixture call")
    if expected == "allow":
        require(not observations.exists(), "allow request retained denied observations")
    else:
        require(len(read_records(observations)) == 1, "expected exactly one denied observation")


def file_digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def record_binary_continuity(source: Path, destination: Path, receipt: Path, phase: str) -> None:
    if phase not in ('before', 'after'):
        raise ValueError('invalid continuity phase')
    row = {'source': str(source), 'destination': str(destination),
           'source_sha256': file_digest(source), 'destination_sha256': file_digest(destination)}
    if row['source_sha256'] != row['destination_sha256']:
        raise ValueError('binary continuity: source and destination differ')
    document = {} if phase == 'before' else json.loads(receipt.read_text())
    if phase == 'after' and document['before']['source_sha256'] != row['source_sha256']:
        raise ValueError('binary continuity: source changed during execution')
    document[phase] = row
    receipt.write_text(json.dumps(document, indent=2) + '\n')


def record_literal_verification(results: Path, cli: Path, offline, phase: str, *, windows=False) -> None:
    """Bind observed verifier inputs independently of the product's projected report."""
    root = results / 'documented-route'
    bundle = root / 'acquisition/project/action.bundle.tar.gz'
    receipt = root / 'input-identity.json'
    current = {'bundle_sha256': file_digest(bundle), 'cli_sha256': file_digest(cli)}
    if phase == 'before':
        if receipt.exists():
            raise ValueError('literal input receipt already exists')
        receipt.write_text(json.dumps({'before': current}) + '\n')
        return
    if phase != 'after':
        raise ValueError('invalid literal identity phase')
    record = json.loads(receipt.read_text())
    if record['before'] != current:
        raise ValueError('literal verification input changed')
    arguments = ['evidence', 'verify-privileged-mcp-action', str(bundle), '--profile-version', 'v1', '--format', 'json']
    connected = [row for row in read_records(results / 'commands.ndjson')
                 if row.get('name') == 'verify-documented-connected']
    expected_connected = ['assay', *arguments]
    operations = [row for row in read_records(root / 'offline-operations.ndjson')
                  if row.get('name') == 'verify-produced-bundle-offline']
    expected_offline = ([offline.harness_binary(), *arguments] if windows else
                        offline.isolation_argv(['assay', *arguments]))
    if len(connected) != 1 or connected[0].get('exit_code') != 0 or connected[0].get('argv') != expected_connected:
        raise ValueError('literal connected verifier input identity differs')
    if (len(operations) != 1 or operations[0].get('exit_code') != 0
            or operations[0].get('classification') != 'verified'
            or operations[0].get('argv') != expected_offline):
        raise ValueError('literal offline verifier input identity differs')
    record.update(after=current, connected_argv=expected_connected, offline_argv=expected_offline, status='verified')
    receipt.write_text(json.dumps(record, indent=2) + '\n')


def guide_blocks(text: str) -> dict[str, str]:
    matches = re.findall(r'^<!-- assay-route: ([a-z-]+) -->\n```(?:bash|sh|powershell|python)\n(.*?)^```[ \t]*$', text, re.S | re.M)
    if len(matches) != len(re.findall(r'^<!-- assay-route:', text, re.M)) or any('```' in code for _, code in matches):
        raise ValueError('malformed documented route fence')
    blocks = {}
    for name, code in matches:
        if name in blocks:
            raise ValueError('duplicate guide block: ' + name)
        blocks[name] = code
    expected = {'download-python', 'acquire-unix', 'acquire-windows', 'open-unix', 'open-windows', 'cli-init', 'cli-doctor', 'cli-policy', 'cli-run', 'deny-python',
                'deny-unix', 'deny-windows', 'cli-import', 'cli-show', 'cli-verify', 'sarif-unix', 'sarif-windows'}
    if set(blocks) != expected:
        raise ValueError('documented route block inventory differs')
    return blocks


def windows_native_guard(receipt: Path, stage: str) -> str:
    """Capture the immediate native status before any other command can mask it."""
    def quote(value):
        return "'" + str(value).replace("'", "''") + "'"
    # Record first: a missing status must leave a row, not look like an unreached guard.
    return ("$assayNativeStatus = $LASTEXITCODE\n"
            "[ordered]@{stage=" + quote(stage) + "; exit_code=$assayNativeStatus; "
            'powershell_version=$PSVersionTable.PSVersion.ToString(); ps_home="$PSHOME"} | '
            "ConvertTo-Json -Compress | Add-Content -LiteralPath " + quote(receipt) + "\n"
            "if ($assayNativeStatus -isnot [int]) { throw 'Native exit status is missing' }\n"
            'if ($assayNativeStatus -ne 0) { throw "Native command failed: $assayNativeStatus" }\n')


def validate_native_exits(path: Path, stages: list[str]) -> list[dict]:
    """Every Windows stage must leave one integer zero status, in execution order."""
    rows = read_records(path) if path.is_file() else []
    if ([row.get('stage') for row in rows] != stages
            or any(type(row.get('exit_code')) is not int or row['exit_code'] != 0
                   or not row.get('powershell_version') for row in rows)):
        raise ValueError('documented native stage exits differ or are missing')
    return rows


def windows_native_command(code: str, receipt: Path, stage: str) -> str:
    # Preference-driven native errors still reach the same explicit status guard.
    # Other terminating errors propagate; they must not become successful stages.
    return ("$global:LASTEXITCODE = $null\ntry {\n" + code +
            "\n} catch [System.Management.Automation.NativeCommandExitException] { }\n" +
            windows_native_guard(receipt, stage))


def verify_native_fail_fast(shell: str, output: Path) -> list[dict]:
    """Observe actual native exits and first/middle-stage stopping on hosted Windows."""
    output.mkdir()
    records = []
    pwsh = '& "$PSHOME/pwsh.exe" -NoProfile -NonInteractive -Command "exit {}"'
    # The route itself runs python and the release executables, not nested pwsh.
    python = "& '" + sys.executable.replace("'", "''") + "' -c \"import sys; sys.exit({})\""
    cases = (('positive', pwsh, (0, 0, 0)), ('negative', pwsh, (7, 0, 0)), ('middle', pwsh, (0, 7, 0)),
             ('python-negative', python, (7, 0, 0)))
    for name, template, native_statuses in cases:
        script = output / (name + '.ps1')
        native_receipt = output / (name + '.native.ndjson')
        lines = ["$ErrorActionPreference = 'Stop'", "$PSNativeCommandUseErrorActionPreference = $true"]
        markers = [output / (name + '.' + str(index) + '.marker') for index in range(3)]
        for index, native_status in enumerate(native_statuses):
            command = template.format(native_status)
            lines.extend([windows_native_command(command, native_receipt, str(index)),
                          "Set-Content -LiteralPath '" + str(markers[index]).replace("'", "''") + "' -Value continued"])
        script.write_text('\n'.join(lines) + '\n')
        argv = [shell, '-NoProfile', '-NonInteractive', '-File', str(script)]
        launcher = {}
        status = run_proxy_child(argv, b'', output / (name + '.stdout'), output / (name + '.stderr'),
                                 expected_lines=None, timeout=30, observation=launcher)
        observed = read_records(native_receipt) if native_receipt.is_file() else []
        wanted = [0, 0, 0] if name == 'positive' else ([0, 7] if name == 'middle' else [7])
        continued = [marker.exists() for marker in markers]
        record = {'name': name, 'argv': argv, 'exit_code': status, 'launcher': launcher,
                  'native_exit_codes': [row.get('exit_code') for row in observed],
                  'native_observations': observed, 'subsequent_commands_ran': continued}
        records.append(record)
        (output / 'receipt.json').write_text(json.dumps(records, indent=2) + '\n')
        if (record['native_exit_codes'] != wanted or any(type(row.get('exit_code')) is not int
                or row.get('stage') != str(index) or not row.get('powershell_version')
                or not row.get('ps_home') for index, row in enumerate(observed))):
            raise ValueError('native inner exit diagnostics differ or are missing')
        expected_markers = [value == 0 for value in wanted] + [False] * (3 - len(wanted))
        if continued != expected_markers:
            raise ValueError('native failure was masked by a subsequent command')
        if (name == 'positive' and status != 0) or (name != 'positive' and status in (0, 124, 125, 127)):
            raise ValueError('native failure was masked or probe did not execute')
    return records


def admit_documented_archive(acquired: Path, reference: Path, destination: Path) -> None:
    if not acquired.is_file() or acquired.is_symlink() or acquired.stat().st_size > 134217728:
        raise ValueError('documented archive exceeds compressed ceiling or is unsafe')
    expected = file_digest(reference)
    if file_digest(acquired) != expected:
        raise ValueError('documented acquisition differs from verified release archive')
    path = Path(__file__).with_name('safe_extract_release_archive.py')
    spec = importlib.util.spec_from_file_location('documented_safe_extract', path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    module.extract_archive(acquired, destination, max_decoded_bytes=134217728, max_members=32)
    if file_digest(acquired) != expected:
        raise ValueError('documented archive changed during bounded admission')


def run_documented_route(guide: Path, results: Path, cli: Path, archive: Path, *,
                         timeout=300, output_limit=4 * 1024 * 1024) -> None:
    blocks = guide_blocks(guide.read_text())
    output = results / 'documented-route'
    output.mkdir()
    acquisition = output / 'acquisition'
    acquisition.mkdir()
    project = acquisition / 'project'
    project.mkdir()
    (project / 'deny.py').write_text(blocks['deny-python'])
    windows = sys.platform == 'win32'
    platform = 'windows' if windows else 'unix'
    names = ['acquire-' + platform, 'open-' + platform, 'cli-init', 'cli-doctor', 'cli-policy', 'cli-run', 'deny-' + platform,
             'cli-import', 'cli-show', 'cli-verify', 'sarif-' + platform]
    before = file_digest(cli)
    receipt = {'status': 'failed', 'guide_sha256': file_digest(guide), 'cli': str(cli),
               'cli_before_sha256': before,
               'limits': {'timeout_seconds': timeout, 'captured_stream_bytes': MAX_OUTPUT_BYTES if windows else output_limit,
                          'scratch_quota': None}, 'blocks': {name: hashlib.sha256(code.encode()).hexdigest()
                                                      for name, code in blocks.items()}}
    powershell = shutil.which('pwsh') if windows else None
    if windows and not powershell:
        raise ValueError('native PowerShell is unavailable')
    (acquisition / 'acquire.py').write_text(blocks['download-python'])
    deadline = time.monotonic() + timeout
    def execute_fences(selected, label):
        script = output / (label + ('.ps1' if windows else '.sh'))
        def quote(value):
            return "'" + str(value).replace("'", "''" if windows else "'\"'\"'") + "'"
        lines = (["$ErrorActionPreference = 'Stop'", '$PSNativeCommandUseErrorActionPreference = $true']
                 if windows else ['set -euo pipefail'])
        lines.append(('Set-Location ' if windows else 'cd ') + quote(acquisition))
        for name in selected:
            if name == 'cli-init':
                lines.append(('Set-Location ' if windows else 'cd ') + quote(project))
            lines.append(("Add-Content -LiteralPath " + quote(output / 'stages-begun.txt') + " -Value " + quote(name))
                         if windows else "printf '%s\\n' " + quote(name) + ' >> ' + quote(output / 'stages-begun.txt'))
            if windows:
                lines.append(windows_native_command(blocks[name], output / 'native-exits.ndjson', name))
            else:
                lines.extend(['{', blocks[name], '}'])
            lines.append(("Add-Content -LiteralPath " + quote(output / 'stages.txt') + " -Value " + quote(name))
                         if windows else "printf '%s\\n' " + quote(name) + ' >> ' + quote(output / 'stages.txt'))
        script.write_text('\n'.join(lines) + '\n')
        argv = ([powershell, '-NoProfile', '-NonInteractive', '-File', str(script)] if windows else
                ['/bin/bash', '--noprofile', '--norc', str(script)])
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('documented route deadline')
        execution = output / ('acquisition-execution' if label == 'acquire' else 'execution')
        if windows:
            execution.mkdir()
            status = run_proxy_child(argv, b'', execution / 'stdout', execution / 'stderr',
                                     expected_lines=None, timeout=remaining)
        else:
            path = Path(__file__).with_name('published_release_installer.py')
            spec = importlib.util.spec_from_file_location('route_supervisor', path)
            supervisor = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(supervisor)
            status = supervisor.supervise(argv, child_environment(), execution,
                                          timeout=remaining, output_limit=output_limit)['exit_code']
        receipt['argv'], receipt['exit_code'] = argv, status
        if status != 0:
            raise ValueError('documented route failed: ' + str(status))
        return argv, status
    try:
        if windows:
            receipt['native_fail_fast'] = verify_native_fail_fast(powershell, output / 'native-fail-fast')
        execute_fences(names[:1], 'acquire')
        acquired = acquisition / archive.name
        admit_documented_archive(acquired, archive, output / 'bounded-archive-preflight')
        receipt['archive_admission'] = {'sha256': file_digest(acquired), 'compressed_max_bytes': 134217728,
                                        'decoded_max_bytes': 134217728, 'max_members': 32,
                                        'before_opening': True}
        argv, status = execute_fences(names[1:], 'route')
        receipt['argv'], receipt['exit_code'] = argv, status
        if status != 0:
            raise ValueError('documented route failed: ' + str(status))
        if windows:
            receipt['native_exits'] = validate_native_exits(output / 'native-exits.ndjson', names)
        if (output / 'stages.txt').read_text().splitlines() != names:
            raise ValueError('documented route stage completion differs')
        receipt['cli_after_sha256'] = file_digest(cli)
        if receipt['cli_after_sha256'] != before:
            raise ValueError('documented route changed installed CLI')
        acquired = acquisition / archive.name
        if file_digest(acquired) != file_digest(archive):
            raise ValueError('documented acquisition differs from verified release archive')
        receipt['archive_sha256'] = file_digest(acquired)
        for name in ('decisions.ndjson', 'denied-observations.ndjson', 'action.bundle.tar.gz', 'enforcement.sarif'):
            path = project / name
            if not path.is_file() or path.stat().st_size == 0:
                raise ValueError('documented route output missing: ' + name)
        receipt['status'] = 'completed'
    except Exception as error:
        receipt['failure'] = str(error)
        raise
    finally:
        begun_path, completed_path = output / 'stages-begun.txt', output / 'stages.txt'
        begun = begun_path.read_text().splitlines() if begun_path.exists() else []
        completed = completed_path.read_text().splitlines() if completed_path.exists() else []
        receipt['stage_records'] = [{'name': name, 'exit_code': 0 if index < len(completed)
                                     else receipt.get('exit_code')} for index, name in enumerate(begun)]
        if begun != names[:len(begun)] or completed != begun[:len(completed)] or len(begun) - len(completed) not in (0, 1):
            receipt['status'] = 'failed'
            receipt['failure'] = 'documented stage records differ from actual route order'
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        if receipt.get('failure') == 'documented stage records differ from actual route order':
            raise ValueError(receipt['failure'])


def main() -> int:
    if sys.argv[1:] in (['--documented-route'], ['--windows-copy-before'], ['--windows-copy-after'],
                        ['--literal-input-before'], ['--literal-input-after']):
        harness = Path(__file__).resolve().parents[2]
        if harness.name != 'harness':
            raise ValueError('documented route requires staged manifest-verified harness')
        root = harness.parent
        results = root / 'results'
        cli = root / 'install/bin' / ('assay.exe' if sys.platform == 'win32' else 'assay')
        if sys.argv[1] == '--documented-route':
            archives = list((results / 'release-assets').glob('assay-v*'))
            archives = [path for path in archives if path.name.endswith(('.tar.gz', '.zip'))]
            if len(archives) != 1:
                raise ValueError('documented route requires one verified CLI archive')
            run_documented_route(harness / 'docs/guides/installed-release-journey.md',
                                 results, cli, archives[0])
        else:
            path = Path(__file__).with_name('published_release_offline_phase.py')
            spec = importlib.util.spec_from_file_location('offline_phase', path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            phase = 'before' if sys.argv[1].endswith('before') else 'after'
            if sys.argv[1].startswith('--literal-input-'):
                record_literal_verification(results, cli, module, phase, windows=sys.platform == 'win32')
            else:
                record_binary_continuity(cli, Path(module.harness_binary()), results / 'offline-binary-continuity.json', phase)
        return 0
    args = parse_args()
    results = Path.cwd().resolve()
    harness_root = Path(__file__).resolve().parents[2]
    fixture_root = args.fixture_dir if args.fixture_dir is not None else harness_root / "examples/privileged-action-gate"
    if args.fixture_dir is not None:
        if not fixture_root.is_absolute() or not fixture_root.is_dir() or fixture_root.is_symlink():
            raise SystemExit("explicit fixture directory must be an absolute existing directory")
        for relative in ("mock_github_mcp.py", "baseline-approved.json", "policies/no-allowance.yaml"):
            path = fixture_root / relative
            if not path.is_file() or path.is_symlink() or path.stat().st_size > 65536:
                raise SystemExit("packaged example input missing, unsafe or over ceiling")
    decisions = results / "decisions.ndjson"
    observations = results / "denied-observations.ndjson"
    if args.expect and any(path.exists() for path in (
        decisions, observations, results / "proxy.jsonl", results / "proxy.stderr"
    )):
        raise SystemExit("request case requires fresh output paths")
    request = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(request) > MAX_REQUEST_BYTES:
        raise SystemExit("proxy request exceeds 1 MiB ceiling")

    mcp_bin = shutil.which("assay-mcp-server", path=child_environment().get("PATH"))
    if mcp_bin is None:
        raise SystemExit("assay-mcp-server is absent from the restricted PATH")

    argv = [
        mcp_bin,
        "proxy-enforce",
        "--upstream-command",
        sys.executable,
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        str(fixture_root / "mock_github_mcp.py"),
        "--enforce-policy",
        str(fixture_root / "policies" / ("allow.yaml" if args.policy == "allow" else "no-allowance.yaml")),
        "--declared-mcp-manifest",
        str(fixture_root / "baseline-approved.json"),
        "--enforcement-decision-out",
        str(decisions),
        "--denied-call-observation-out",
        str(observations),
    ]
    expected_lines = len(case_request_ids(args.expect)) if args.expect else None
    status = run_proxy_child(
        argv,
        request,
        results / "proxy.jsonl",
        results / "proxy.stderr",
        expected_lines=expected_lines,
        timeout=args.timeout_seconds,
    )
    append_command_record(results / "commands.ndjson", status, argv)
    if status == 0 and args.expect:
        try:
            validate_case(results, args.expect)
        except (OSError, ValueError, RecursionError) as error:
            print(f"request case failed: {error}", file=sys.stderr)
            return 1
    return status if 0 <= status <= 255 else 125


if __name__ == "__main__":
    raise SystemExit(main())
