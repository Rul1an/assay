"""Drive and self-test the disposable Claude plugin installation contract."""

from __future__ import annotations

import json
import os
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

MAX_BYTES = 1_048_576
DEFAULT_TIMEOUT_SECONDS = 30.0
EXPECTED_TOOLS = [
    "assay_check_args",
    "assay_check_coverage",
    "assay_check_sequence",
    "assay_explain_trace",
    "assay_policy_decide",
]
AUTH_ENV_PREFIXES = ("ANTHROPIC_", "CLAUDE_CODE_OAUTH_", "ASSAY_AUTH_")


class WorkflowError(RuntimeError):
    def __init__(self, phase: str, reason: str, next_step: str) -> None:
        super().__init__(reason)
        self.phase = phase
        self.reason = reason
        self.next_step = next_step


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: bytes
    stderr: bytes


def fail(phase: str, reason: str, next_step: str) -> "None":
    raise WorkflowError(phase, reason, next_step)


def clean_env(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.upper().startswith(AUTH_ENV_PREFIXES)
    }
    env.pop("CLAUDE_PROJECT_DIR", None)
    if extra:
        env.update(extra)
    return env


def terminate_tree(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1.0)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def run_bounded(
    phase: str,
    argv: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    stdin: bytes = b"",
    allowed_codes: Iterable[int] = (0,),
) -> CommandResult:
    timeout = float(os.environ.get("ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS", DEFAULT_TIMEOUT_SECONDS))
    process = subprocess.Popen(
        argv,
        cwd=cwd,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None

    if stdin:
        try:
            process.stdin.write(stdin)
            process.stdin.flush()
        except BrokenPipeError:
            pass
    process.stdin.close()

    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    deadline = time.monotonic() + timeout

    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                terminate_tree(process)
                fail(
                    phase,
                    f"process tree exceeded {timeout:g}s deadline",
                    "retry after checking the client/server command for a prompt, hang, or inherited pipe",
                )
            events = selector.select(min(remaining, 0.1))
            for key, _ in events:
                chunk = os.read(key.fd, 65_536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                target = buffers[key.data]
                target.extend(chunk)
                if len(target) > MAX_BYTES:
                    terminate_tree(process)
                    fail(
                        phase,
                        f"{key.data} exceeded {MAX_BYTES}-byte ceiling",
                        "inspect the command directly; the bounded workflow will not retain unbounded diagnostics",
                    )

            if process.poll() is not None and not events:
                # Descendants can inherit a pipe after the direct child exits. The
                # same absolute deadline continues to govern that drain.
                continue
    finally:
        selector.close()

    remaining = max(0.0, deadline - time.monotonic())
    try:
        returncode = process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        terminate_tree(process)
        process.wait()
        fail(
            phase,
            f"process tree did not reap within {timeout:g}s deadline",
            "inspect descendant processes spawned by the client command",
        )

    result = CommandResult(returncode, bytes(buffers["stdout"]), bytes(buffers["stderr"]))
    if returncode not in set(allowed_codes):
        diagnostic = (result.stderr or result.stdout).decode("utf-8", "replace").strip()
        diagnostic = diagnostic[-600:] if diagnostic else "no diagnostic output"
        fail(
            phase,
            f"command exited {returncode}: {diagnostic}",
            "run the named phase directly with the same fresh config and consumer directory",
        )
    return result


def read_bounded(path: Path, phase: str) -> bytes:
    try:
        stat = path.lstat()
    except FileNotFoundError:
        fail(phase, f"missing file: {path}", "reinstall or update the plugin, then inspect its cache path")
    if path.is_symlink() or not path.is_file():
        fail(phase, f"expected regular non-symlink file: {path}", "replace the cache entry from the marketplace source")
    if stat.st_size > MAX_BYTES:
        fail(phase, f"file exceeds {MAX_BYTES}-byte ceiling: {path}", "inspect the package before installing it")
    return path.read_bytes()


def descendants_are_regular(root: Path, phase: str) -> list[Path]:
    files: list[Path] = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            fail(phase, f"symlink is not allowed in plugin package: {path}", "replace it with a generated regular file")
        if path.is_file():
            read_bounded(path, phase)
            files.append(path)
        elif not path.is_dir():
            fail(phase, f"unsupported plugin package entry: {path}", "retain only regular files and directories")
    return files


def plugin_record(payload: object) -> dict[str, object]:
    records: list[object]
    if isinstance(payload, list):
        records = payload
    elif isinstance(payload, dict):
        candidate = payload.get("plugins", payload.get("installedPlugins", []))
        records = candidate if isinstance(candidate, list) else []
    else:
        records = []
    for record in records:
        if isinstance(record, dict) and record.get("id") == "assay@assay":
            return record
    fail("installed_cache", "plugin list omitted assay@assay", "run `claude plugin list --json` and reinstall assay@assay")


def assert_under(path: Path, parent: Path, phase: str, message: str) -> None:
    try:
        path.relative_to(parent)
    except ValueError:
        fail(phase, message, "remove the stale install and reinstall with a fresh CLAUDE_CONFIG_DIR")


def compare_installed_package(source_package: Path, installed: Path) -> None:
    source_files = descendants_are_regular(source_package, "installed_cache")
    descendants_are_regular(installed, "installed_cache")
    for source in source_files:
        relative = source.relative_to(source_package)
        cached = installed / relative
        if read_bounded(source, "installed_cache") != read_bounded(cached, "installed_cache"):
            fail(
                "installed_cache",
                f"installed bytes drifted for {relative}",
                "run marketplace update and plugin update, then restart Claude Code",
            )


def parse_protocol(stdout: bytes) -> dict[int, dict[str, object]]:
    responses: dict[int, dict[str, object]] = {}
    for raw_line in stdout.splitlines():
        if not raw_line.strip():
            continue
        try:
            value = json.loads(raw_line)
        except json.JSONDecodeError as error:
            fail("initialize", f"server emitted non-JSON stdout: {error}", "run assay-mcp-server directly and keep logs on stderr")
        if isinstance(value, dict) and isinstance(value.get("id"), int):
            responses[value["id"]] = value
    return responses


def drive_cached_manifest(installed: Path, consumer: Path, env: dict[str, str], expected_server: Path) -> None:
    manifest_path = installed / ".mcp.json"
    try:
        manifest = json.loads(read_bounded(manifest_path, "initialize"))
        entry = manifest["mcpServers"]["assay"]
        command = entry["command"]
        args = entry["args"]
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        fail("initialize", f"installed MCP manifest is invalid: {error}", "reinstall the plugin from the reviewed marketplace")
    if not isinstance(command, str) or not isinstance(args, list) or not all(isinstance(arg, str) for arg in args):
        fail("initialize", "installed MCP command/args are not strings", "restore the typed plugin manifest")
    if args != ["--policy-root", "."]:
        fail(
            "policy_root_resolved_to_consumer",
            f"installed MCP args do not pin the consumer cwd: {args}",
            "restore `--policy-root .` or use an explicit absolute project override",
        )
    resolved = shutil.which(command, path=env.get("PATH"))
    if resolved is None:
        fail("binary_spawn", f"{command} is not on PATH", "install assay-mcp-server and restart the client")
    if Path(resolved).resolve() != expected_server.resolve():
        fail("binary_spawn", f"PATH resolved an unexpected server: {resolved}", "put the exact-SHA assay-mcp-server first on the isolated PATH")

    policy = consumer / "install-surface-policy.yaml"
    if (consumer / "policies").exists():
        fail(
            "policy_root_resolved_to_consumer",
            "consumer probe unexpectedly contains the server's default policies directory",
            "remove the default-path fixture so the probe discriminates `.` from `policies`",
        )
    policy.write_text("blocklist:\n  - install_surface_probe\n", encoding="utf-8")
    requests = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "claude-plugin-workflow", "version": "1.0"},
            },
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "assay_policy_decide",
                "arguments": {
                    "tool": "install_surface_probe",
                    "policy": policy.name,
                },
            },
        },
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "assay_policy_decide",
                "arguments": {
                    "tool": "install_surface_probe",
                    "policy": "missing-install-surface-policy.yaml",
                },
            },
        },
    ]
    stdin = b"".join(json.dumps(request, separators=(",", ":")).encode() + b"\n" for request in requests)
    result = run_bounded("initialize", [resolved, *args], cwd=consumer, env=env, stdin=stdin)
    responses = parse_protocol(result.stdout)
    initialize = responses.get(1, {})
    if initialize.get("result", {}).get("protocolVersion") != "2024-11-05":
        fail("initialize", "server did not negotiate protocol 2024-11-05", "inspect the installed server version and manifest argv")
    print("initialize=pass")

    tools = responses.get(2, {}).get("result", {}).get("tools", [])
    names = sorted(tool.get("name") for tool in tools if isinstance(tool, dict))
    if names != EXPECTED_TOOLS:
        fail("tools_list", f"release tool names drifted: {names}", "install the release server built from the same source SHA")
    print("tools_list=pass")

    content = responses.get(3, {}).get("result", {}).get("content", [])
    try:
        decision = json.loads(content[0]["text"])
    except (IndexError, KeyError, TypeError, json.JSONDecodeError) as error:
        fail("policy_root_resolved_to_consumer", f"policy response is invalid: {error}", "inspect assay_policy_decide output")
    if decision.get("allowed") is not False:
        fail(
            "policy_root_resolved_to_consumer",
            "consumer-only policy did not deny install_surface_probe",
            "override the project MCP entry with an absolute --policy-root for hosts that use another cwd",
        )
    print("policy_root_resolved_to_consumer=pass")

    missing_content = responses.get(4, {}).get("result", {}).get("content", [])
    try:
        missing = json.loads(missing_content[0]["text"])
    except (IndexError, KeyError, TypeError, json.JSONDecodeError) as error:
        fail("missing_policy_refused", f"missing-policy response is invalid: {error}", "inspect assay_policy_decide error output")
    if missing.get("error", {}).get("code") != "E_POLICY_NOT_FOUND":
        fail(
            "missing_policy_refused",
            f"missing policy did not return E_POLICY_NOT_FOUND: {missing}",
            "keep missing policy distinct from a clean allow or deny verdict",
        )
    print("missing_policy_refused=pass")


def verify_workflow() -> None:
    script = Path(os.environ["ASSAY_CLAUDE_WORKFLOW_SCRIPT"]).resolve()
    source_root = Path(os.environ.get("ASSAY_SOURCE_ROOT", script.parents[2])).resolve()
    source_package = source_root / "packaging/claude-plugin"
    marketplace = source_root / ".claude-plugin/marketplace.json"
    claude = Path(os.environ.get("ASSAY_CLAUDE_BIN", shutil.which("claude") or ""))
    server = Path(os.environ.get("ASSAY_MCP_SERVER_BIN", shutil.which("assay-mcp-server") or ""))
    if not claude.is_file():
        fail("prerequisite", "Claude Code executable not found", "install Claude Code and set ASSAY_CLAUDE_BIN if needed")
    if not server.is_file():
        fail("prerequisite", "assay-mcp-server executable not found", "install the exact-SHA server and set ASSAY_MCP_SERVER_BIN")
    read_bounded(marketplace, "plugin_validate")

    git = run_bounded("source_sha", ["git", "rev-parse", "HEAD"], cwd=source_root, env=clean_env())
    source_sha = git.stdout.decode().strip()
    if len(source_sha) != 40 or any(char not in "0123456789abcdef" for char in source_sha):
        fail("source_sha", f"invalid git SHA: {source_sha!r}", "run from a committed git worktree")

    with tempfile.TemporaryDirectory(prefix="assay-claude-plugin-") as temporary:
        temp = Path(temporary).resolve()
        config = temp / "config"
        consumer = temp / "consumer"
        isolated_bin = temp / "bin"
        config.mkdir()
        consumer.mkdir()
        isolated_bin.mkdir()
        server_link = isolated_bin / "assay-mcp-server"
        server_link.symlink_to(server.resolve())
        env = clean_env(
            {
                "CLAUDE_CONFIG_DIR": str(config),
                "PATH": os.pathsep.join([str(isolated_bin), str(claude.parent), os.environ.get("PATH", "")]),
            }
        )

        version = run_bounded("claude_version", [str(claude), "--version"], cwd=consumer, env=env)
        claude_version = version.stdout.decode("utf-8", "replace").strip()
        run_bounded("plugin_validate", [str(claude), "plugin", "validate", str(marketplace)], cwd=consumer, env=env)
        print("plugin_validate=pass")
        run_bounded("marketplace_add", [str(claude), "plugin", "marketplace", "add", str(source_root)], cwd=consumer, env=env)
        print("marketplace_add=pass")
        run_bounded("plugin_install", [str(claude), "plugin", "install", "assay@assay", "--scope", "local"], cwd=consumer, env=env)
        print("plugin_install=pass")
        run_bounded("marketplace_update", [str(claude), "plugin", "marketplace", "update", "assay"], cwd=consumer, env=env)
        print("marketplace_update=pass")
        run_bounded("plugin_update", [str(claude), "plugin", "update", "assay@assay", "--scope", "local"], cwd=consumer, env=env)
        print("plugin_update=pass")

        listing = run_bounded("installed_cache", [str(claude), "plugin", "list", "--json"], cwd=consumer, env=env)
        try:
            record = plugin_record(json.loads(listing.stdout))
            installed = Path(str(record["installPath"])).resolve()
            cache_version = str(record["version"])
        except (KeyError, TypeError, json.JSONDecodeError) as error:
            fail("installed_cache", f"plugin list JSON is invalid: {error}", "run `claude plugin list --json` and inspect assay@assay")
        assert_under(installed, config, "installed_cache", "installed cache escaped the fresh Claude config")
        try:
            installed.relative_to(source_root)
        except ValueError:
            pass
        else:
            fail("installed_cache", "installed plugin resolves inside the source checkout", "install through the marketplace instead of reading source files directly")
        compare_installed_package(source_package, installed)
        print("installed_cache=pass")

        mcp_list = run_bounded("mcp_list_connected", [str(claude), "mcp", "list"], cwd=consumer, env=env)
        mcp_text = (mcp_list.stdout + mcp_list.stderr).decode("utf-8", "replace")
        if "assay" not in mcp_text or "Connected" not in mcp_text:
            fail("mcp_list_connected", "Claude MCP health output did not report assay connected", "check binary PATH, project cwd, and plugin cache")
        print("mcp_list_connected=pass")

        drive_cached_manifest(installed, consumer, env, server_link)

        debug = temp / "session-debug.log"
        session = run_bounded(
            "actual_session",
            [
                str(claude),
                "-p",
                "Use the assay golden-path skill and report only its first contract step.",
                "--debug-file",
                str(debug),
                "--output-format",
                "json",
            ],
            cwd=consumer,
            env=env,
            allowed_codes=range(0, 256),
        )
        debug_text = read_bounded(debug, "actual_session").decode("utf-8", "replace")
        if "Successfully connected" not in debug_text or "assay" not in debug_text:
            fail("actual_session_mcp_connected", "actual session did not connect the assay MCP server", "inspect the session debug log for plugin or spawn errors")
        print("actual_session_mcp_connected=pass")
        if "assay:assay-golden-path" not in debug_text:
            fail("skill_discovery", "actual session did not discover assay:assay-golden-path", "inspect the installed skill and restart Claude Code")
        print("skill_discovery=pass")

        combined = ((session.stdout + session.stderr).decode("utf-8", "replace") + debug_text).lower()
        model_status = "not_exercised" if session.returncode == 0 else "unavailable"
        if model_status == "unavailable" and not any(token in combined for token in ("api key", "auth", "login", "credential")):
            fail("model_mediated_tool_call", "session failed for a reason other than unavailable authentication", "inspect the bounded session diagnostic")

        print(f"source_sha={source_sha}")
        print(f"claude_version={claude_version}")
        print(f"installed_cache_version={cache_version}")
        print(f"model_mediated_tool_call={model_status}")
        print("verification=pass")


def write_fake_tools(root: Path) -> tuple[Path, Path]:
    fake_bin = root / "fake-bin"
    fake_bin.mkdir()
    claude = fake_bin / "claude"
    server = fake_bin / "assay-mcp-server"
    python = sys.executable
    claude.write_text(
        f"""#!{python}
import json, os, pathlib, shutil, subprocess, sys, time
args = sys.argv[1:]
config = pathlib.Path(os.environ["CLAUDE_CONFIG_DIR"])
source_file = config / "marketplace-source"
fail_phase = os.environ.get("FAKE_FAIL_PHASE")

def phase_name(values):
    mapping = {{
        ("plugin", "validate"): "plugin_validate",
        ("plugin", "marketplace", "add"): "marketplace_add",
        ("plugin", "install"): "plugin_install",
        ("plugin", "marketplace", "update"): "marketplace_update",
        ("plugin", "update"): "plugin_update",
        ("plugin", "list"): "installed_cache",
        ("mcp", "list"): "mcp_list_connected",
    }}
    for prefix, name in mapping.items():
        if tuple(values[:len(prefix)]) == prefix:
            return name
    return "actual_session"

phase = phase_name(args)
if fail_phase == phase:
    print(f"synthetic failure in {{phase}}", file=sys.stderr)
    raise SystemExit(9)
if os.environ.get("FAKE_OVERSIZE_PHASE") == phase:
    print("x" * 1_048_577)
    raise SystemExit(0)
if os.environ.get("FAKE_HANG_PHASE") == phase:
    subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    time.sleep(60)
if os.environ.get("FAKE_ORPHAN_PIPE_PHASE") == phase:
    subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    raise SystemExit(0)

if args == ["--version"]:
    print("2.1.32 (Claude Code)")
elif args[:2] == ["plugin", "validate"]:
    assert pathlib.Path(args[2]).is_file()
    print("valid")
elif args[:3] == ["plugin", "marketplace", "add"]:
    config.mkdir(parents=True, exist_ok=True)
    source_file.write_text(str(pathlib.Path(args[3]).resolve()))
    print("added")
elif args[:2] == ["plugin", "install"]:
    source = pathlib.Path(source_file.read_text())
    cache = config / "plugins/cache/assay/assay/fake-v1"
    cache.parent.mkdir(parents=True, exist_ok=True)
    if cache.exists(): shutil.rmtree(cache)
    shutil.copytree(source / "packaging/claude-plugin", cache)
    (cache / ".mcp.json").write_text('{{"stale":true}}')
    print("installed")
elif args[:3] == ["plugin", "marketplace", "update"]:
    print("marketplace updated")
elif args[:2] == ["plugin", "update"]:
    source = pathlib.Path(source_file.read_text())
    cache = config / "plugins/cache/assay/assay/fake-v1"
    if not os.environ.get("FAKE_NO_UPDATE"):
        if cache.exists(): shutil.rmtree(cache)
        shutil.copytree(source / "packaging/claude-plugin", cache)
    print("plugin updated")
elif args[:3] == ["plugin", "list", "--json"]:
    source = pathlib.Path(source_file.read_text())
    cache = source / "packaging/claude-plugin" if os.environ.get("FAKE_SOURCE_ONLY") else config / "plugins/cache/assay/assay/fake-v1"
    print(json.dumps([{{"id":"assay@assay","version":"fake-v1","installPath":str(cache)}}]))
elif args[:2] == ["mcp", "list"]:
    print("assay: Connected")
else:
    debug = pathlib.Path(args[args.index("--debug-file") + 1])
    debug.write_text('Successfully connected to assay\\nSkill prompt: showing "assay:assay-golden-path"\\n')
    print("Invalid API key", file=sys.stderr)
    raise SystemExit(1)
""",
        encoding="utf-8",
    )
    server.write_text(
        f"""#!{python}
import json, os, pathlib, sys
tool_names = {EXPECTED_TOOLS!r}
if os.environ.get("FAKE_TOOL_DRIFT"):
    tool_names[-1] = "assay_policy_changed"
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request: continue
    method = request.get("method")
    if method == "initialize":
        result = {{"protocolVersion":"2024-11-05","capabilities":{{}},"serverInfo":{{"name":"fake","version":"1"}}}}
    elif method == "tools/list":
        result = {{"tools":[{{"name":name,"description":"test","inputSchema":{{"type":"object"}}}} for name in tool_names]}}
    elif method == "tools/call":
        policy_name = request["params"]["arguments"]["policy"]
        policy = pathlib.Path.cwd() / policy_name
        if not policy.is_file():
            payload = {{"error":{{"code":"E_POLICY_NOT_FOUND","message":f"Policy not found: {{policy_name}}"}}}}
        else:
            blocked = "install_surface_probe" in policy.read_text()
            payload = {{"allowed": not blocked}}
        result = {{"content":[{{"type":"text","text":json.dumps(payload)}}],"isError":False}}
    else:
        result = {{}}
    print(json.dumps({{"jsonrpc":"2.0","id":request["id"],"result":result}}), flush=True)
""",
        encoding="utf-8",
    )
    claude.chmod(0o755)
    server.chmod(0o755)
    return claude, server


def self_test() -> None:
    script = Path(os.environ["ASSAY_CLAUDE_WORKFLOW_SCRIPT"]).resolve()
    source_root = script.parents[2]
    with tempfile.TemporaryDirectory(prefix="assay-claude-plugin-selftest-") as temporary:
        temp = Path(temporary)
        claude, server = write_fake_tools(temp)
        base_env = clean_env(
            {
                "ASSAY_CLAUDE_BIN": str(claude),
                "ASSAY_MCP_SERVER_BIN": str(server),
                "ASSAY_SOURCE_ROOT": str(source_root),
                "ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS": "2",
                "PATH": os.pathsep.join([str(claude.parent), os.environ.get("PATH", "")]),
            }
        )

        passing = run_bounded("self_test", [str(script), "--verify"], cwd=source_root, env=base_env)
        text = passing.stdout.decode("utf-8", "replace")
        required = [
            "marketplace_update=pass",
            "plugin_update=pass",
            "installed_cache=pass",
            "mcp_list_connected=pass",
            "initialize=pass",
            "tools_list=pass",
            "policy_root_resolved_to_consumer=pass",
            "missing_policy_refused=pass",
            "actual_session_mcp_connected=pass",
            "skill_discovery=pass",
            "model_mediated_tool_call=unavailable",
            "verification=pass",
        ]
        missing = [line for line in required if line not in text]
        if missing:
            fail("self_test", f"passing workflow omitted phases: {missing}", "restore the explicit phase output contract")

        mutations = [
            ("FAKE_FAIL_PHASE", "marketplace_update", "phase=marketplace_update"),
            ("FAKE_SOURCE_ONLY", "1", "phase=installed_cache"),
            ("FAKE_TOOL_DRIFT", "1", "phase=tools_list"),
            ("FAKE_NO_UPDATE", "1", "phase=installed_cache"),
            ("FAKE_OVERSIZE_PHASE", "plugin_validate", "stdout exceeded 1048576-byte ceiling"),
            ("FAKE_HANG_PHASE", "plugin_validate", "process tree exceeded 2s deadline"),
            ("FAKE_ORPHAN_PIPE_PHASE", "plugin_validate", "process tree exceeded 2s deadline"),
        ]
        for key, value, expected in mutations:
            mutated_env = dict(base_env)
            mutated_env[key] = value
            result = run_bounded(
                "self_test_mutation",
                [str(script), "--verify"],
                cwd=source_root,
                env=mutated_env,
                allowed_codes=range(0, 256),
            )
            output = (result.stdout + result.stderr).decode("utf-8", "replace")
            if result.returncode == 0 or expected not in output:
                fail("self_test", f"mutation {key} did not bite its named guard: {output[-600:]}", "repair the workflow discriminator")

    print("claude_plugin_install_self_test=pass")


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--verify"
    try:
        if mode == "--self-test":
            self_test()
        elif mode == "--verify":
            verify_workflow()
        else:
            fail("arguments", f"unknown argument: {mode}", "use --verify or --self-test")
    except WorkflowError as error:
        print(f"phase={error.phase} status=fail reason={error.reason}", file=sys.stderr)
        print(f"next_step={error.next_step}", file=sys.stderr)
        return 1
    return 0


raise SystemExit(main())
