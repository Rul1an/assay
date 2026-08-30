"""Drive and self-test the disposable Claude plugin installation contract."""

from __future__ import annotations

import json
import math
import os
import re
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, NoReturn

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
DRIVER = Path(__file__).resolve()
SOURCE_ROOT = DRIVER.parents[2]
WORKFLOW_SCRIPT = DRIVER.with_name("test-claude-plugin-install.sh")


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


def fail(phase: str, reason: str, next_step: str) -> NoReturn:
    raise WorkflowError(phase, reason, next_step)


def workflow_timeout_seconds() -> float:
    raw = os.environ.get(
        "ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS", str(DEFAULT_TIMEOUT_SECONDS)
    )
    try:
        timeout = float(raw)
    except ValueError:
        fail(
            "arguments",
            f"ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS must be a positive number, got {raw!r}",
            "set ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS to a finite value greater than zero",
        )
    if not math.isfinite(timeout) or timeout <= 0:
        fail(
            "arguments",
            f"ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS must be positive and finite, got {raw!r}",
            "set ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS to a finite value greater than zero",
        )
    return timeout


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
    timeout = workflow_timeout_seconds()
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

            # Descendants can inherit a pipe after the direct child exits. The
            # same absolute deadline continues to govern that drain.
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
    with path.open("rb") as source:
        payload = source.read(MAX_BYTES + 1)
    if len(payload) > MAX_BYTES:
        fail(phase, f"file grew beyond {MAX_BYTES}-byte ceiling: {path}", "inspect the package before installing it")
    return payload


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


def compare_installed_package(source_package: Path, installed: Path) -> None:
    source_files = descendants_are_regular(source_package, "installed_cache")
    installed_files = descendants_are_regular(installed, "installed_cache")
    source_relatives = {path.relative_to(source_package) for path in source_files}
    installed_relatives = {path.relative_to(installed) for path in installed_files}
    if source_relatives != installed_relatives:
        missing = sorted(str(path) for path in source_relatives - installed_relatives)
        extra = sorted(str(path) for path in installed_relatives - source_relatives)
        fail(
            "installed_cache",
            f"installed file set drifted: missing={missing}, extra={extra}",
            "remove the stale install, update the marketplace, and reinstall the plugin",
        )
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


def response_result(
    responses: dict[int, dict[str, object]], response_id: int, phase: str
) -> dict[str, object]:
    response = responses.get(response_id)
    if not isinstance(response, dict):
        fail(phase, f"missing JSON-RPC response id {response_id}", "inspect the server stdout protocol stream")
    result = response.get("result")
    if not isinstance(result, dict):
        fail(phase, f"JSON-RPC response id {response_id} has no object result", "inspect the server response shape")
    return result


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
    if command != "assay-mcp-server":
        fail("binary_spawn", f"installed MCP command drifted: {command!r}", "restore the release binary name in the plugin manifest")
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

    policy = consumer / PROBE_POLICY
    if (consumer / "policies").exists():
        fail(
            "policy_root_resolved_to_consumer",
            "consumer probe unexpectedly contains the server's default policies directory",
            "remove the default-path fixture so the probe discriminates `.` from `policies`",
        )
    policy.write_text(probe_policy_body(), encoding="utf-8")
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
    result = run_bounded(
        "initialize",
        [str(expected_server), "--policy-root", "."],
        cwd=consumer,
        env=env,
        stdin=stdin,
    )
    responses = parse_protocol(result.stdout)
    initialize = response_result(responses, 1, "initialize")
    if initialize.get("protocolVersion") != "2024-11-05":
        fail("initialize", "server did not negotiate protocol 2024-11-05", "inspect the installed server version and manifest argv")
    print("initialize=pass")

    tools = response_result(responses, 2, "tools_list").get("tools")
    if not isinstance(tools, list) or any(
        not isinstance(tool, dict) or not isinstance(tool.get("name"), str)
        for tool in tools
    ):
        fail("tools_list", "tools/list returned an untyped tool entry", "inspect the release server tool schema")
    names = sorted(tool["name"] for tool in tools)
    if names != EXPECTED_TOOLS:
        fail("tools_list", f"release tool names drifted: {names}", "install the release server built from the same source SHA")
    print("tools_list=pass")

    policy_result = response_result(responses, 3, "policy_root_resolved_to_consumer")
    if policy_result.get("isError") is not True:
        fail(
            "policy_root_resolved_to_consumer",
            "consumer policy denial did not set MCP isError",
            "preserve the release server's denied-result contract before interpreting its payload",
        )
    content = policy_result.get("content", [])
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

    missing_content = response_result(responses, 4, "missing_policy_refused").get("content", [])
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


# One transcript validator. Fixture replay and the live session both call it;
# a second parser is how the two would drift while each kept its own tests green.
# The byte ceiling is MAX_BYTES, the same one `run_bounded` and `read_bounded`
# already apply, because a second larger ceiling here would be unreachable.
# Released Claude hosts expose decide under exactly these two names. Classification
# is exact equality through classify_decide_tool_route; a prefix/suffix/contains
# stand-in would accept near-misses the host never listed.
DECIDE_TOOL_PROJECT = "mcp__assay__assay_policy_decide"
DECIDE_TOOL_PLUGIN = "mcp__plugin_assay_assay__assay_policy_decide"
DECIDE_TOOL_ROUTES = {
    DECIDE_TOOL_PROJECT: "project",
    DECIDE_TOOL_PLUGIN: "plugin",
}


def classify_decide_tool_route(name: str) -> str | None:
    """Map one tool name to observed_route. The one classifier."""
    return DECIDE_TOOL_ROUTES.get(name)


# The pinned probe. The live prompt asks for exactly this decision and the
# validator requires exactly it back, so the two cannot drift into asking for
# one probe and accepting another.
# Two probes, deliberately separate. The denied one proves the policy root
# resolves to the consumer; `assay-mcp-server` maps that denial to MCP
# `isError: true`, so it can never produce an accepted transcript. The allowed
# one is the model-mediated record path and returns a clean result.
PROBE_TOOL = "install_surface_probe"
TRANSCRIPT_PROBE_TOOL = "install_surface_allowed_probe"
# The field the allowed decision actually carries. `policy_decide` returns
# `{"allowed": true, "reason": ...}` for an allow and `matches` only for a deny,
# so the prompt must ask for the field the probe can produce.
TRANSCRIPT_RESULT_FIELD = "reason"


def transcript_prompt() -> str:
    """The one live prompt. Built here so the fake can hold it to its contract."""
    return (
        "Use the assay golden-path skill. Call the assay_policy_decide tool "
        f"once for tool {TRANSCRIPT_PROBE_TOOL} with policy {PROBE_POLICY}, "
        f"then report its result field {TRANSCRIPT_RESULT_FIELD} verbatim."
    )


def transcript_prompt_contract() -> str:
    """The substring the session must carry, so fixture bytes cannot mask drift."""
    return f"result field {TRANSCRIPT_RESULT_FIELD} verbatim"


PROBE_POLICY = "install-surface-policy.yaml"


def probe_policy_body() -> str:
    """The one policy body the probe writes and every guard reads."""
    return f"blocklist:\n  - {PROBE_TOOL}\n"


EXPECTED_DECIDE_INPUT = {"tool": TRANSCRIPT_PROBE_TOOL, "policy": PROBE_POLICY}
# A dependency token shorter than this matches too much ordinary prose to
# demonstrate that a later turn consumed the result.
MIN_DEPENDENCY_TOKEN = 8


def _stream_envelopes(stream: bytes) -> list[dict[str, object]]:
    """Decode bounded stream-json. Any malformed line refuses the whole stream."""
    if len(stream) > MAX_BYTES:
        raise ValueError(f"transcript exceeds {MAX_BYTES}-byte ceiling")
    try:
        text = stream.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("transcript is not UTF-8") from error
    envelopes: list[dict[str, object]] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except (ValueError, RecursionError) as error:
            raise ValueError(f"line {number} is not JSON: {error}") from error
        if not isinstance(value, dict):
            raise ValueError(f"line {number} is not a JSON object")
        envelopes.append(value)
    return envelopes


def _content_blocks(envelope: dict[str, object]) -> list[dict[str, object]]:
    message = envelope.get("message")
    if not isinstance(message, dict):
        return []
    content = message.get("content")
    if not isinstance(content, list):
        return []
    return [block for block in content if isinstance(block, dict)]


def _result_text(block: dict[str, object]) -> str:
    content = block.get("content")
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    return "".join(
        item["text"]
        for item in content
        if isinstance(item, dict) and isinstance(item.get("text"), str)
    )


def _decide_payload_tokens(payload: object) -> tuple[list[str], str]:
    """Type the assay_policy_decide result and return its quotable values.

    The contract is the server's, not this script's: a decision carries
    ``allowed: bool``, a denial carries a non-empty ``matches`` list whose every
    member is a non-empty string, and an allow carries a non-empty ``reason``.
    A non-string member is refused rather than filtered away, because silently
    dropping it would accept a payload the server never emits.
    """
    if not isinstance(payload, dict):
        return [], "result payload is not a JSON object"
    allowed = payload.get("allowed")
    if not isinstance(allowed, bool):
        return [], "result payload has no boolean 'allowed' field"
    if allowed:
        reason = payload.get("reason")
        if not isinstance(reason, str) or not reason.strip():
            return [], "allow decision carries no 'reason' string"
        return [reason], ""
    matches = payload.get("matches")
    if not isinstance(matches, list) or not matches:
        return [], "deny decision carries no non-empty 'matches' list"
    for position, item in enumerate(matches):
        if not isinstance(item, str) or not item.strip():
            return [], f"deny decision 'matches[{position}]' is not a non-empty string"
    return list(matches), ""


def _is_error_flag_ok(block: dict[str, object]) -> bool:
    """One rule for every `is_error` in the stream contract.

    The field may be absent. When present it must be the literal boolean
    `False`; identity is used rather than equality so that `0`, which compares
    equal to `False` in Python, is refused along with `"false"` and every other
    stand-in. A value the CLI never emits is not evidence of a clean result.
    """
    if "is_error" not in block:
        return True
    return block["is_error"] is False


def _envelope_role(envelope: dict[str, object]) -> str | None:
    message = envelope.get("message")
    if not isinstance(message, dict):
        return None
    role = message.get("role")
    return role if isinstance(role, str) else None


def classify_model_mediated_call(stream: bytes) -> tuple[str, str]:
    """Classify one Claude stream-json transcript.

    Returns ``pass`` only when the transcript shows exactly one accepted
    decide tool_use across the project and plugin names, exactly one matching
    non-error tool_result carried by a user envelope after it, a payload typed
    against the tool's own contract, a later assistant message quoting a value
    the model could not have taken from its own request, and exactly one
    terminal result envelope after that turn reporting success. The pass detail
    reports ``observed_route=project`` or ``observed_route=plugin``. Every one of
    those envelopes must share one non-empty session id. Absence of an
    accepted decide name, including a wrong Assay tool that is not decide,
    is ``not_exercised`` and keeps the invoked name in the detail; every
    other shape is ``unavailable``. An incomplete observation never becomes
    clean.

    Envelope types this rule does not name are ignored, so a future CLI may add
    them without turning an otherwise valid transcript into a refusal.
    """
    try:
        envelopes = _stream_envelopes(stream)
    except ValueError as error:
        return "unavailable", str(error)

    uses: list[tuple[int, str, str, str, object]] = []
    other_uses: list[str] = []
    results: list[tuple[int, str, dict[str, object]]] = []
    texts: list[tuple[int, str]] = []
    terminals: list[tuple[int, dict[str, object]]] = []
    for index, envelope in enumerate(envelopes):
        kind = envelope.get("type")
        role = _envelope_role(envelope)
        if kind == "result":
            terminals.append((index, envelope))
            continue
        for block in _content_blocks(envelope):
            block_type = block.get("type")
            if kind == "assistant" and role == "assistant" and block_type == "tool_use":
                name, identifier = block.get("name"), block.get("id")
                route = classify_decide_tool_route(name) if isinstance(name, str) else None
                if route is not None:
                    if not isinstance(identifier, str) or not identifier:
                        return "unavailable", f"{name} tool_use has no id"
                    uses.append((index, identifier, name, route, block.get("input")))
                elif isinstance(name, str):
                    other_uses.append(name)
            elif kind == "assistant" and role == "assistant" and block_type == "text":
                value = block.get("text")
                if isinstance(value, str):
                    texts.append((index, value))
            elif kind == "user" and role == "user" and block_type == "tool_result":
                identifier = block.get("tool_use_id")
                if isinstance(identifier, str) and identifier:
                    results.append((index, identifier, block))

    if not uses:
        if other_uses:
            invoked = ", ".join(sorted(other_uses))
            return (
                "not_exercised",
                f"no accepted assay_policy_decide tool_use in transcript; invoked {invoked}",
            )
        return "not_exercised", "no accepted assay_policy_decide tool_use in transcript"
    if len(uses) > 1:
        names = ", ".join(sorted(name for _i, _d, name, _r, _in in uses))
        return "unavailable", f"expected exactly one Assay decide tool_use across routes, found {len(uses)}: {names}"

    use_index, use_id, use_name, route, use_input = uses[0]
    # Exact object, not a superset: this is a pinned probe, so a transcript that
    # decided some other tool or policy did not run the probe the prompt asked
    # for, and neither did one that carried extra arguments we never requested.
    if use_input != EXPECTED_DECIDE_INPUT:
        return "unavailable", f"{use_name} input is not the pinned probe {EXPECTED_DECIDE_INPUT}"

    matching = [entry for entry in results if entry[1] == use_id]
    if not matching:
        return "unavailable", f"no user tool_result matches tool_use_id {use_id!r} for {use_name}"
    if len(matching) > 1:
        return "unavailable", f"expected exactly one tool_result for {use_id!r}, found {len(matching)}"
    result_index, _identifier, result_block = matching[0]
    if result_index <= use_index:
        return "unavailable", f"tool_result for {use_id!r} precedes its tool_use"
    if not _is_error_flag_ok(result_block):
        return "unavailable", f"tool_result for {use_name} reports or malforms is_error"

    try:
        payload = json.loads(_result_text(result_block))
    except (ValueError, RecursionError) as error:
        return "unavailable", f"tool_result payload for {use_name} is not JSON: {error}"
    tokens, problem = _decide_payload_tokens(payload)
    if problem:
        return "unavailable", f"{use_name}: {problem}"

    # A value the model already held in its own tool_use input proves nothing
    # about consuming the result. Only server-generated text can carry that.
    request = json.dumps(use_input, sort_keys=True) if use_input is not None else ""
    derived = [
        token
        for token in tokens
        if len(token) >= MIN_DEPENDENCY_TOKEN and token not in request
    ]
    if not derived:
        return "unavailable", f"{use_name} result carries no value absent from its own request"
    dependent_index = None
    for index, value in texts:
        if index > result_index and any(token in value for token in derived):
            dependent_index = index
            break
    if dependent_index is None:
        return "unavailable", f"no later assistant message quotes the {use_name} result"

    # A transcript that stops before its own terminal envelope is an incomplete
    # observation, and an incomplete observation is never a clean result.
    if len(terminals) != 1:
        return "unavailable", f"expected exactly one terminal result envelope, found {len(terminals)}"
    terminal_index, terminal = terminals[0]
    if terminal_index <= dependent_index:
        return "unavailable", "terminal result envelope precedes the dependent assistant turn"
    if terminal.get("subtype") != "success":
        return "unavailable", f"terminal result subtype is {terminal.get('subtype')!r}, not 'success'"
    # `subtype: success` and `is_error: true` co-occur in this CLI, so both are
    # checked; neither alone is the completion signal.
    if not _is_error_flag_ok(terminal):
        return "unavailable", "terminal result envelope reports or malforms is_error"

    sessions = {
        envelopes[index].get("session_id")
        for index in (use_index, result_index, dependent_index, terminal_index)
    }
    if len(sessions) != 1:
        return "unavailable", "transcript envelopes do not share one session id"
    session = sessions.pop()
    if not isinstance(session, str) or not session:
        return "unavailable", "transcript envelopes carry no non-empty session id"

    return (
        "pass",
        f"observed_route={route} {use_name} invoked once in session {session} and its result quoted before a successful close",
    )


def verify_workflow() -> None:
    script = WORKFLOW_SCRIPT
    source_root = SOURCE_ROOT
    source_package = source_root / "packaging/claude-plugin"
    marketplace = source_root / ".claude-plugin/marketplace.json"
    claude_lookup = shutil.which("claude")
    server_lookup = shutil.which("assay-mcp-server")
    if claude_lookup is None:
        fail("prerequisite", "Claude Code executable not found", "install Claude Code and put `claude` on PATH")
    if server_lookup is None:
        fail("prerequisite", "assay-mcp-server executable not found", "install the exact-SHA `assay-mcp-server` on PATH")
    claude = Path(claude_lookup).resolve()
    server = Path(server_lookup).resolve()
    read_bounded(marketplace, "plugin_validate")

    git = run_bounded("source_sha", ["git", "rev-parse", "HEAD"], cwd=source_root, env=clean_env())
    source_sha = git.stdout.decode().strip()
    if len(source_sha) != 40 or any(char not in "0123456789abcdef" for char in source_sha):
        fail("source_sha", f"invalid git SHA: {source_sha!r}", "run from a committed git worktree")
    expected_cache_version = source_sha[:12]

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
            cache_version = str(record["version"])
            reported_install_path = str(record["installPath"])
        except (KeyError, TypeError, json.JSONDecodeError) as error:
            fail("installed_cache", f"plugin list JSON is invalid: {error}", "run `claude plugin list --json` and inspect assay@assay")
        installed = config / "plugins/cache/assay/assay" / expected_cache_version
        if cache_version != expected_cache_version:
            fail(
                "installed_cache",
                f"installed cache version {cache_version!r} does not match source {expected_cache_version!r}",
                "run marketplace update and plugin update from the committed source head",
            )
        if reported_install_path != str(installed):
            fail(
                "installed_cache",
                f"plugin list reported an unexpected install path: {reported_install_path}",
                "remove the stale install and reinstall with a fresh CLAUDE_CONFIG_DIR",
            )
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
                transcript_prompt(),
                "--debug-file",
                str(debug),
                "--output-format",
                "stream-json",
                # Claude Code refuses stream-json under --print without this.
                # `json` reports only the final result, which cannot distinguish
                # a real tool invocation from a no-op.
                "--verbose",
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
        auth_unavailable = session.returncode != 0 and any(
            token in combined for token in ("api key", "auth", "login", "credential")
        )
        if session.returncode != 0 and not auth_unavailable:
            fail("model_mediated_tool_call", "session failed for a reason other than unavailable authentication", "inspect the bounded session diagnostic")
        if auth_unavailable:
            model_status = "unavailable"
            model_detail = "authentication unavailable for the disposable profile"
        else:
            # The same validator fixture replay uses. Process exit is never the
            # evidence: a zero exit with no Assay tool_use is `not_exercised`.
            model_status, model_detail = classify_model_mediated_call(session.stdout)
        print(f"model_mediated_detail={model_detail}")

        print(f"source_sha={source_sha}")
        print(f"claude_version={claude_version}")
        print(f"installed_cache_version={cache_version}")
        print(f"model_mediated_tool_call={model_status}")
        print("verification=pass")


def write_fake_tools(root: Path, cache_version: str) -> tuple[Path, Path]:
    fixtures_dir = str(STREAM_FIXTURES)
    contract = transcript_prompt_contract()
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
    cache = config / "plugins/cache/assay/assay/{cache_version}"
    cache.parent.mkdir(parents=True, exist_ok=True)
    if cache.exists(): shutil.rmtree(cache)
    shutil.copytree(source / "packaging/claude-plugin", cache)
    (cache / ".mcp.json").write_text('{{"stale":true}}')
    print("installed")
elif args[:3] == ["plugin", "marketplace", "update"]:
    print("marketplace updated")
elif args[:2] == ["plugin", "update"]:
    source = pathlib.Path(source_file.read_text())
    cache = config / "plugins/cache/assay/assay/{cache_version}"
    if not os.environ.get("FAKE_NO_UPDATE"):
        if cache.exists(): shutil.rmtree(cache)
        shutil.copytree(source / "packaging/claude-plugin", cache)
        if os.environ.get("FAKE_EXTRA_CACHE_FILE"):
            (cache / "unexpected.txt").write_text("unexpected")
    print("plugin updated")
elif args[:3] == ["plugin", "list", "--json"]:
    source = pathlib.Path(source_file.read_text())
    cache = source / "packaging/claude-plugin" if os.environ.get("FAKE_SOURCE_ONLY") else config / "plugins/cache/assay/assay/{cache_version}"
    version = "forged-v2" if os.environ.get("FAKE_CACHE_VERSION_DRIFT") else "{cache_version}"
    print(json.dumps([{{"id":"assay@assay","version":version,"installPath":str(cache)}}]))
elif args[:2] == ["mcp", "list"]:
    print("assay: Connected")
else:
    debug = pathlib.Path(args[args.index("--debug-file") + 1])
    debug.write_text('Successfully connected to assay\\nSkill prompt: showing "assay:assay-golden-path"\\n')
    # The fake is the witness for the invocation contract. Without this, the
    # workflow could revert to `--output-format json` and every test would stay
    # green while the live session stopped emitting a transcript to validate.
    if "--output-format" not in args or args[args.index("--output-format") + 1] != "stream-json":
        print("session must request --output-format stream-json", file=sys.stderr)
        raise SystemExit(64)
    if "--verbose" not in args:
        print("stream-json under --print requires --verbose", file=sys.stderr)
        raise SystemExit(64)
    # Injecting fixture bytes without reading the prompt is exactly how a live
    # prompt asking for a field the probe cannot return stayed green. The fake
    # holds the session to its own output contract before it replays anything.
    if "-p" not in args or {contract!r} not in args[args.index("-p") + 1]:
        print("session prompt must request the typed result field verbatim", file=sys.stderr)
        raise SystemExit(64)
    fixture = os.environ.get("FAKE_STREAM_FIXTURE")
    if fixture:
        sys.stdout.write(pathlib.Path({fixtures_dir!r}, fixture).read_text(encoding="utf-8"))
        raise SystemExit(0)
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
        if os.environ.get("FAKE_NAMELESS_TOOL"):
            result["tools"].append({{"description":"nameless"}})
    elif method == "tools/call":
        policy_name = request["params"]["arguments"]["policy"]
        policy = pathlib.Path.cwd() / policy_name
        if not policy.is_file():
            payload = {{"error":{{"code":"E_POLICY_NOT_FOUND","message":f"Policy not found: {{policy_name}}"}}}}
        else:
            blocked = "install_surface_probe" in policy.read_text()
            payload = {{"allowed": not blocked}}
        result = {{"content":[{{"type":"text","text":json.dumps(payload)}}],"isError":True}}
    else:
        result = {{}}
    if os.environ.get("FAKE_NULL_RESULT_METHOD") == method:
        result = None
    if os.environ.get("FAKE_POLICY_IS_NOT_ERROR") and method == "tools/call" and policy_name == "install-surface-policy.yaml":
        result["isError"] = False
    print(json.dumps({{"jsonrpc":"2.0","id":request["id"],"result":result}}), flush=True)
""",
        encoding="utf-8",
    )
    claude.chmod(0o755)
    server.chmod(0o755)
    return claude, server


def self_test() -> None:
    # First, because every later failure caused by a re-coupled probe would
    # surface as an unexplained fixture mismatch instead of the design error.
    assert_transcript_probe_is_not_denied()
    assert_transcript_prompt_contract()
    assert_changelog_history_self_test()
    assert_changelog_contract()
    assert_hook_files_include_changelog()
    assert_stream_fixture_table()
    assert_wrong_assay_tool_keeps_invoked_name()
    assert_companion_non_decide_does_not_invalidate()
    script = WORKFLOW_SCRIPT
    source_root = SOURCE_ROOT
    git = run_bounded("self_test", ["git", "rev-parse", "HEAD"], cwd=source_root, env=clean_env())
    source_sha = git.stdout.decode().strip()
    if len(source_sha) != 40 or any(char not in "0123456789abcdef" for char in source_sha):
        fail("self_test", f"invalid git SHA: {source_sha!r}", "run the self-test from a committed git worktree")
    with tempfile.TemporaryDirectory(prefix="assay-claude-plugin-selftest-") as temporary:
        temp = Path(temporary)
        claude, _server = write_fake_tools(temp, source_sha[:12])
        base_env = clean_env(
            {
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
            ("FAKE_CACHE_VERSION_DRIFT", "1", "phase=installed_cache"),
            ("FAKE_EXTRA_CACHE_FILE", "1", "phase=installed_cache"),
            ("FAKE_TOOL_DRIFT", "1", "phase=tools_list"),
            ("FAKE_NAMELESS_TOOL", "1", "phase=tools_list"),
            ("FAKE_NULL_RESULT_METHOD", "initialize", "phase=initialize"),
            ("FAKE_POLICY_IS_NOT_ERROR", "1", "phase=policy_root_resolved_to_consumer"),
            ("FAKE_NO_UPDATE", "1", "phase=installed_cache"),
            ("FAKE_OVERSIZE_PHASE", "plugin_validate", "stdout exceeded 1048576-byte ceiling"),
            ("FAKE_HANG_PHASE", "plugin_validate", "process tree exceeded 2s deadline"),
            ("FAKE_ORPHAN_PIPE_PHASE", "plugin_validate", "process tree exceeded 2s deadline"),
            ("ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS", "invalid", "phase=arguments"),
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

        # Live/fixture parity. The workflow is driven end to end with the fake
        # session replaying each checked-in transcript, and must report exactly
        # what `classify_model_mediated_call` returns for the same bytes. This is
        # what makes "one validator" checkable rather than asserted.
        replayed = 0
        for name, expected, route in STREAM_FIXTURE_EXPECTATIONS:
            replayed += 1
            live_env = dict(base_env)
            live_env["FAKE_STREAM_FIXTURE"] = name
            live = run_bounded(
                "self_test_live_parity",
                [str(script), "--verify"],
                cwd=source_root,
                env=live_env,
                allowed_codes=range(0, 256),
            )
            live_text = (live.stdout + live.stderr).decode("utf-8", "replace")
            if f"model_mediated_tool_call={expected}" not in live_text:
                fail(
                    "self_test",
                    f"live replay of {name} did not report {expected}: {live_text[-400:]}",
                    "route the live session through classify_model_mediated_call",
                )
            if expected == "pass":
                if f"observed_route={route}" not in live_text:
                    fail(
                        "self_test",
                        f"live replay of {name} omitted observed_route={route}: {live_text[-400:]}",
                        "report observed_route from classify_decide_tool_route",
                    )
            elif "observed_route=" in live_text:
                fail(
                    "self_test",
                    f"live replay of {name} reported a route on {expected}: {live_text[-400:]}",
                    "only a pass transcript reports observed_route",
                )
        if replayed != len(STREAM_FIXTURE_EXPECTATIONS):
            fail(
                "self_test",
                f"live parity covered {replayed} fixtures, expected {len(STREAM_FIXTURE_EXPECTATIONS)}",
                "iterate STREAM_FIXTURE_EXPECTATIONS, not a hardcoded subset",
            )

    replay_stream_fixtures()
    assert_plugin_prefix_parity()

    print("claude_plugin_install_self_test=pass")


STREAM_FIXTURES = DRIVER.parent / "fixtures" / "claude-stream"
STREAM_FIXTURE_EXPECTATIONS = (
    ("valid-allow.jsonl", "pass", "project"),
    ("valid-allow-plugin.jsonl", "pass", "plugin"),
    ("valid-allow-companion-non-decide.jsonl", "pass", "project"),
    ("deny-probe-arrives-as-error.jsonl", "unavailable", None),
    ("no-call.jsonl", "not_exercised", None),
    ("assistant-envelope-user-role.jsonl", "not_exercised", None),
    ("duplicate-call.jsonl", "unavailable", None),
    ("mismatched-id.jsonl", "unavailable", None),
    ("error-result.jsonl", "unavailable", None),
    ("malformed.jsonl", "unavailable", None),
    ("independent-final.jsonl", "unavailable", None),
    ("dependency-echoes-input.jsonl", "unavailable", None),
    ("result-before-use.jsonl", "unavailable", None),
    ("duplicate-result.jsonl", "unavailable", None),
    ("untyped-payload.jsonl", "unavailable", None),
    ("allow-without-reason.jsonl", "unavailable", None),
    ("deny-without-matches.jsonl", "unavailable", None),
    ("matches-mixed-types.jsonl", "unavailable", None),
    ("wrong-assay-tool.jsonl", "not_exercised", None),
    ("dual-route.jsonl", "unavailable", None),
    ("near-miss-project-prefix.jsonl", "not_exercised", None),
    ("malformed-plugin-prefix.jsonl", "not_exercised", None),
    ("missing-terminal-result.jsonl", "unavailable", None),
    ("terminal-result-error.jsonl", "unavailable", None),
    ("terminal-result-not-success.jsonl", "unavailable", None),
    ("duplicate-terminal-result.jsonl", "unavailable", None),
    ("terminal-result-wrong-session.jsonl", "unavailable", None),
    ("terminal-result-before-dependency.jsonl", "unavailable", None),
    ("tool-result-in-assistant-envelope.jsonl", "unavailable", None),
    ("quote-in-user-role-envelope.jsonl", "unavailable", None),
    ("tool-result-is-error-not-bool.jsonl", "unavailable", None),
    ("tool-result-is-error-zero.jsonl", "unavailable", None),
    ("terminal-result-is-error-not-bool.jsonl", "unavailable", None),
    ("tool-use-null-input.jsonl", "unavailable", None),
    ("tool-use-wrong-probe-input.jsonl", "unavailable", None),
    ("tool-use-surplus-input-key.jsonl", "unavailable", None),
)


CHANGELOG_CLAIM_MARKERS = (
    DECIDE_TOOL_PROJECT,
    DECIDE_TOOL_PLUGIN,
    "observed_route",
    "wrong Assay tool",
)
RELEASE_HEADING = re.compile(
    r"## \[(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\]"
    r" - [0-9]{4}-[0-9]{2}-[0-9]{2}"
)


def _changelog_claim_section(text: str) -> str:
    """Return the one active or released section carrying every Claude claim."""
    headings = list(re.finditer(r"^## .+$", text, re.MULTILINE))
    unreleased = [match for match in headings if match.group(0) == "## [Unreleased]"]
    if len(unreleased) != 1:
        fail("changelog", "CHANGELOG.md must have one Unreleased section", "restore the Unreleased heading")
    unreleased_index = headings.index(unreleased[0])
    if unreleased_index + 1 >= len(headings) or RELEASE_HEADING.fullmatch(
        headings[unreleased_index + 1].group(0)
    ) is None:
        fail(
            "changelog",
            "CHANGELOG Unreleased is not followed by a dated semver release",
            "restore numbered release history immediately after Unreleased",
        )

    owners: list[int] = []
    for marker in CHANGELOG_CLAIM_MARKERS:
        position = text.find(marker)
        if position < 0:
            fail(
                "changelog",
                f"CHANGELOG history does not name {marker!r}",
                "record the two exact decide names, observed_route, and wrong-tool class",
            )
        owner = next(
            (index for index in range(len(headings) - 1, -1, -1) if headings[index].start() < position),
            -1,
        )
        owners.append(owner)
    if len(set(owners)) != 1 or owners[0] < 0:
        fail(
            "changelog",
            "CHANGELOG Claude host claims are split across sections",
            "keep the complete claim set in one active or released section",
        )
    owner = owners[0]
    heading = headings[owner].group(0)
    if heading != "## [Unreleased]" and RELEASE_HEADING.fullmatch(heading) is None:
        fail(
            "changelog",
            "CHANGELOG Claude host claims are outside active or released history",
            "move the complete claim set under Unreleased or a dated semver release",
        )
    end = headings[owner + 1].start() if owner + 1 < len(headings) else len(text)
    return text[headings[owner].start() : end]


def assert_changelog_contract() -> None:
    """Active or released text must match the two-name union and wrong-tool class.

    The classifier accepts exactly the project and plugin decide names and
    treats a wrong Assay tool as ``not_exercised``. The changelog is the
    public contract; a one-name or wrong-tool-unavailable sentence is a
    contradiction, not a comment.
    """
    section = _changelog_claim_section((SOURCE_ROOT / "CHANGELOG.md").read_text(encoding="utf-8"))
    for needle in (DECIDE_TOOL_PROJECT, DECIDE_TOOL_PLUGIN, "observed_route"):
        if needle not in section:
            fail(
                "changelog",
                f"CHANGELOG claim section does not name {needle!r}",
                "record the two exact decide names and observed_route",
            )
    if "wrong-tool," in section and "stay `unavailable`" in section:
        fail(
            "changelog",
            "CHANGELOG Unreleased still lists wrong-tool among unavailable shapes",
            "record the not_exercised reclassification for a wrong Assay tool",
        )
    marker = "wrong Assay tool"
    idx = section.find(marker)
    if idx < 0:
        fail(
            "changelog",
            "CHANGELOG claim section does not name the wrong Assay tool reclassification",
            "say a wrong Assay tool stays not_exercised",
        )
    if "not_exercised" not in section[idx : idx + 240]:
        fail(
            "changelog",
            "CHANGELOG Unreleased does not classify a wrong Assay tool as not_exercised",
            "land the reclassification next to the wrong Assay tool wording",
        )
    print("changelog_contract=pass")


def assert_changelog_history_self_test() -> None:
    """Pin active, released, split, preamble, and non-version section bounds."""
    active = (SOURCE_ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    _changelog_claim_section(active)
    headings = list(re.finditer(r"^## .+$", active, re.MULTILINE))
    unreleased = next(match for match in headings if match.group(0) == "## [Unreleased]")
    first_release = headings[headings.index(unreleased) + 1]
    body_start = active.find("\n", unreleased.start()) + 1
    body = active[body_start : first_release.start()]
    released = (
        active[:body_start]
        + "\n## [99.99.99] - 2099-12-31\n"
        + body
        + active[first_release.start() :]
    )
    _changelog_claim_section(released)

    for marker in CHANGELOG_CLAIM_MARKERS:
        split = released.replace(marker, "", 1).replace(
            "## [Unreleased]\n", f"## [Unreleased]\n\n{marker}\n", 1
        )
        try:
            _changelog_claim_section(split)
        except WorkflowError:
            pass
        else:
            fail("changelog_self_test", f"split marker stayed green: {marker}", "restore section closure")

    preamble = released
    for marker in CHANGELOG_CLAIM_MARKERS:
        preamble = preamble.replace(marker, "", 1)
    preamble = "\n".join(CHANGELOG_CLAIM_MARKERS) + "\n" + preamble
    try:
        _changelog_claim_section(preamble)
    except WorkflowError:
        pass
    else:
        fail("changelog_self_test", "preamble claims stayed green", "restore section lower bound")

    nonversion = active.replace(
        "## [Unreleased]\n", "## [Unreleased]\n\n## [Migration Notes]\n", 1
    )
    try:
        _changelog_claim_section(nonversion)
    except WorkflowError:
        pass
    else:
        fail("changelog_self_test", "non-version heading stayed green", "require dated semver history")
    print("changelog_history_self_test=pass")


CLAUDE_PLUGIN_HOOK_ID = "claude-plugin-install-workflow-self-test"


def _claude_plugin_hook_block(config: str) -> str:
    marker = f"id: {CLAUDE_PLUGIN_HOOK_ID}"
    start = config.find(marker)
    if start < 0:
        fail(
            "hook_files",
            "pre-commit config lost claude-plugin-install-workflow-self-test",
            "restore the Claude plugin install self-test hook",
        )
    nxt = config.find("\n      - id:", start + len(marker))
    return config[start:] if nxt < 0 else config[start:nxt]


def _hook_files_selector(block: str) -> str:
    """The hook's files: regex. Comments are not the selector."""
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("#") or not stripped.startswith("files:"):
            continue
        value = stripped[len("files:") :].strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
            value = value[1:-1]
        if not value:
            fail(
                "hook_files",
                "claude-plugin-install-workflow-self-test files selector is empty",
                "restore the files regex that includes CHANGELOG.md",
            )
        return value
    fail(
        "hook_files",
        "claude-plugin-install-workflow-self-test has no files: selector",
        "restore the files regex that includes CHANGELOG.md",
    )


def _hook_files_selects_changelog(pattern: str) -> bool:
    try:
        compiled = re.compile(pattern)
    except re.error as error:
        fail(
            "hook_files",
            f"claude-plugin-install-workflow-self-test files selector is not a regex: {error}",
            "keep a valid files regex that matches CHANGELOG.md",
        )
    return compiled.search("CHANGELOG.md") is not None


def _changelog_comment_decoy(config: str) -> str:
    """Drop CHANGELOG.md from the files regex and leave a CHANGELOG comment."""
    block = _claude_plugin_hook_block(config)
    start = config.find(block)
    decoy_lines: list[str] = []
    replaced = False
    for line in block.splitlines():
        stripped = line.strip()
        if stripped.startswith("files:") and not stripped.startswith("#"):
            dropped = (
                line.replace("CHANGELOG\\.md|", "")
                .replace("|CHANGELOG\\.md", "")
                .replace("CHANGELOG\\.md", "")
            )
            if dropped == line:
                fail(
                    "hook_files",
                    "could not drop CHANGELOG.md from the files selector for the decoy",
                    "keep CHANGELOG.md in the files regex so the decoy can remove it",
                )
            decoy_lines.append(dropped)
            decoy_lines.append("        # CHANGELOG remains in comments only")
            replaced = True
        else:
            decoy_lines.append(line)
    if not replaced:
        fail(
            "hook_files",
            "could not build a CHANGELOG comment decoy",
            "keep a files: selector on the Claude plugin install hook",
        )
    return config[:start] + "\n".join(decoy_lines) + config[start + len(block) :]


def assert_hook_files_include_changelog() -> None:
    """The pre-push hook must run when CHANGELOG.md changes.

    The files: regex is the selector. A comment that mentions CHANGELOG is
    not, so a token search over the hook block is not the contract.
    """
    config = (SOURCE_ROOT / ".pre-commit-config.yaml").read_text(encoding="utf-8")
    pattern = _hook_files_selector(_claude_plugin_hook_block(config))
    if not _hook_files_selects_changelog(pattern):
        fail(
            "hook_files",
            "claude-plugin-install-workflow-self-test files regex does not match CHANGELOG.md",
            "add CHANGELOG.md to the hook files contract",
        )
    decoy = _changelog_comment_decoy(config)
    decoy_pattern = _hook_files_selector(_claude_plugin_hook_block(decoy))
    if _hook_files_selects_changelog(decoy_pattern):
        fail(
            "hook_files",
            "CHANGELOG comment decoy still matched the files selector",
            "inspect the files: regex, not a CHANGELOG token in the hook block",
        )
    print("hook_files_include_changelog=pass")


def assert_stream_fixture_table() -> None:
    """One table owns status and route. A side map can be emptied and skipped."""
    on_disk = {path.name for path in STREAM_FIXTURES.glob("*.jsonl")}
    named: set[str] = set()
    if not STREAM_FIXTURE_EXPECTATIONS:
        fail(
            "stream_fixture",
            "STREAM_FIXTURE_EXPECTATIONS is empty",
            "keep one row per checked-in stream fixture",
        )
    for index, row in enumerate(STREAM_FIXTURE_EXPECTATIONS):
        if not isinstance(row, tuple) or len(row) != 3:
            fail(
                "stream_fixture",
                f"expectation {index} must be (name, status, route), got {row!r}",
                "fold observed_route into STREAM_FIXTURE_EXPECTATIONS",
            )
        name, expected, route = row
        named.add(name)
        if expected == "pass":
            if route not in ("project", "plugin"):
                fail(
                    "stream_fixture",
                    f"{name}: pass row must name observed_route project or plugin, got {route!r}",
                    "put the route on the same row as the status",
                )
        elif route is not None:
            fail(
                "stream_fixture",
                f"{name}: non-pass row must not claim a route, got {route!r}",
                "use None for fixtures that cannot report observed_route",
            )
    if on_disk != named:
        fail(
            "stream_fixture",
            f"fixture table drifted from disk: extra={sorted(named - on_disk)} missing={sorted(on_disk - named)}",
            "keep one STREAM_FIXTURE_EXPECTATIONS row per claude-stream jsonl",
        )
    named_project = next((row for row in STREAM_FIXTURE_EXPECTATIONS if row[0] == "valid-allow.jsonl"), None)
    named_plugin = next((row for row in STREAM_FIXTURE_EXPECTATIONS if row[0] == "valid-allow-plugin.jsonl"), None)
    if named_project != ("valid-allow.jsonl", "pass", "project"):
        fail(
            "stream_fixture",
            "named project-route PASS fixture valid-allow.jsonl is required",
            "keep valid-allow.jsonl as pass/project; any other project mention is not that pin",
        )
    if named_plugin != ("valid-allow-plugin.jsonl", "pass", "plugin"):
        fail(
            "stream_fixture",
            "named plugin-route PASS fixture valid-allow-plugin.jsonl is required",
            "keep valid-allow-plugin.jsonl as pass/plugin; any other plugin mention is not that pin",
        )
    print("stream_fixture_table=pass")


def assert_wrong_assay_tool_keeps_invoked_name() -> None:
    """A wrong Assay tool is not_exercised and still names what was invoked."""
    status, detail = classify_model_mediated_call(
        read_bounded(STREAM_FIXTURES / "wrong-assay-tool.jsonl", "stream_fixture")
    )
    if status != "not_exercised":
        fail(
            "stream_fixture",
            f"wrong-assay-tool.jsonl: expected not_exercised, got {status} ({detail})",
            "keep an unaccepted Assay name as not_exercised",
        )
    if "mcp__assay__assay_check_args" not in detail:
        fail(
            "stream_fixture",
            f"wrong-assay-tool.jsonl lost the invoked-name diagnostic: {detail!r}",
            "keep the invoked tool name in the not_exercised detail",
        )
    print("wrong_assay_tool_keeps_invoked_name=pass")


def assert_companion_non_decide_does_not_invalidate() -> None:
    """An unrelated non-decide tool_use does not break exactly one accepted decide."""
    status, detail = classify_model_mediated_call(
        read_bounded(STREAM_FIXTURES / "valid-allow-companion-non-decide.jsonl", "stream_fixture")
    )
    if status != "pass" or "observed_route=project" not in detail:
        fail(
            "stream_fixture",
            f"companion non-decide must stay pass/project, got {status} ({detail})",
            "count only accepted decide names toward the exactly-one rule",
        )
    print("companion_non_decide_does_not_invalidate=pass")


def assert_transcript_prompt_contract() -> None:
    """The prompt must ask for a field the allowed probe actually returns.

    `matches` exists only on a denial, and the denied probe is deliberately not
    the transcript probe. A prompt asking for a match string therefore requests
    data the live session can never produce, and fixture replay cannot see it
    because the fake injects bytes without reading the prompt.
    """
    prompt = transcript_prompt()
    if transcript_prompt_contract() not in prompt:
        fail(
            "transcript_prompt",
            f"live prompt does not request {TRANSCRIPT_RESULT_FIELD!r} verbatim: {prompt!r}",
            "ask for the field the allowed decision carries",
        )
    if "match" in prompt:
        fail(
            "transcript_prompt",
            "live prompt asks for a match string, which only a denial carries",
            "the transcript probe is allowed, so its result has no matches list",
        )
    print("transcript_prompt_contract=pass")


def assert_transcript_probe_is_not_denied() -> None:
    """The transcript probe must be a decision the real server can return cleanly.

    `assay-mcp-server` maps a policy denial to MCP `isError: true`
    (`classify_tool_result`: `is_error = has_error || explicit_allowed == Some(false)`),
    and the validator refuses any errored tool_result. So a probe on a blocklisted
    tool can never produce a transcript that passes, and a fixture claiming
    otherwise would assert a shape the server does not emit.
    """
    blocklist = probe_policy_body()
    requested = EXPECTED_DECIDE_INPUT["tool"]
    if f"- {requested}\n" in blocklist:
        fail(
            "transcript_probe",
            f"transcript probe {requested!r} is blocked by the probe policy; a denial "
            "arrives as isError and can never yield an accepted transcript",
            "give the transcript path its own allowed probe and keep the blocked tool "
            "for policy-root verification",
        )
    if f"- {PROBE_TOOL}\n" not in blocklist:
        fail(
            "transcript_probe",
            f"policy-root probe {PROBE_TOOL!r} is no longer blocked by the probe policy",
            "restore the denied probe used by the policy-root phase",
        )
    print("transcript_probe_is_not_denied=pass")


def assert_plugin_prefix_parity() -> None:
    """Every STREAM_FIXTURE_EXPECTATIONS entry that carries the project prefix.

    Fixtures whose bytes contain the exact project namespace are rewritten to
    the plugin namespace and must keep the same status. A passing mirror must
    also report observed_route=plugin. Fixtures without that exact prefix are
    skipped, so near-miss and already-plugin transcripts do not invent a second
    allowlist.
    """
    short = "assay_policy_decide"
    project_prefix = DECIDE_TOOL_PROJECT.removesuffix(short).encode()
    plugin_prefix = DECIDE_TOOL_PLUGIN.removesuffix(short).encode()
    if not project_prefix or project_prefix == plugin_prefix:
        fail(
            "plugin_prefix_parity",
            "could not derive distinct project/plugin prefixes from the accepted names",
            "keep classify_decide_tool_route on two exact names",
        )
    eligible = [
        name
        for name, _expected, _route in STREAM_FIXTURE_EXPECTATIONS
        if project_prefix in read_bounded(STREAM_FIXTURES / name, "stream_fixture")
    ]
    mirrored = 0
    for name, expected, _route in STREAM_FIXTURE_EXPECTATIONS:
        original = read_bounded(STREAM_FIXTURES / name, "stream_fixture")
        if project_prefix not in original:
            continue
        mirrored += 1
        status, detail = classify_model_mediated_call(
            original.replace(project_prefix, plugin_prefix)
        )
        if status != expected:
            fail(
                "plugin_prefix_parity",
                f"{name} plugin mirror: expected {expected}, got {status} ({detail})",
                "classify both exact decide names through classify_decide_tool_route",
            )
        if expected == "pass" and "observed_route=plugin" not in detail:
            fail(
                "plugin_prefix_parity",
                f"{name} plugin mirror omitted observed_route=plugin: {detail!r}",
                "report observed_route from classify_decide_tool_route",
            )
    if not eligible:
        fail(
            "plugin_prefix_parity",
            "no STREAM_FIXTURE_EXPECTATIONS entry carried the project prefix",
            "keep at least one project-route fixture in the expectation table",
        )
    if mirrored != len(eligible):
        fail(
            "plugin_prefix_parity",
            f"plugin-prefix parity covered {mirrored} fixtures, expected {len(eligible)}",
            "iterate STREAM_FIXTURE_EXPECTATIONS, not a hardcoded subset",
        )
    print("plugin_prefix_parity=pass")


def replay_stream_fixtures() -> None:
    """Replay checked-in transcripts through the one production validator.

    Fixture replay and the live workflow call `classify_model_mediated_call`;
    a second parser here would let the two drift while both stayed green.
    """
    replayed = 0
    for name, expected, route in STREAM_FIXTURE_EXPECTATIONS:
        replayed += 1
        path = STREAM_FIXTURES / name
        status, detail = classify_model_mediated_call(read_bounded(path, "stream_fixture"))
        if status != expected:
            fail(
                "stream_fixture",
                f"{name}: expected {expected}, got {status} ({detail})",
                "repair the transcript validator or the fixture it is measured against",
            )
        if expected == "pass":
            if f"observed_route={route}" not in detail:
                fail(
                    "stream_fixture",
                    f"{name}: expected observed_route={route} in {detail!r}",
                    "report observed_route from classify_decide_tool_route",
                )
        elif "observed_route=" in detail:
            fail(
                "stream_fixture",
                f"{name}: {expected} detail must not report a route: {detail!r}",
                "only a pass transcript reports observed_route",
            )
    if replayed != len(STREAM_FIXTURE_EXPECTATIONS):
        fail(
            "stream_fixture",
            f"fixture replay covered {replayed} fixtures, expected {len(STREAM_FIXTURE_EXPECTATIONS)}",
            "iterate STREAM_FIXTURE_EXPECTATIONS, not a hardcoded subset",
        )
    # A literal, not a multiple of the ceiling under test: a fixture sized from
    # MAX_BYTES would grow with a raised ceiling and never observe it.
    oversize = b'{"type":"system"}\n' * 200_000
    if len(oversize) <= MAX_BYTES:
        fail(
            "stream_fixture",
            f"oversize fixture ({len(oversize)} bytes) must exceed the {MAX_BYTES}-byte ceiling",
            "grow the oversize fixture past the shipped ceiling",
        )
    status, _detail = classify_model_mediated_call(oversize)
    if status != "unavailable":
        fail(
            "stream_fixture",
            f"oversized stream must be unavailable, got {status}",
            "restore the transcript byte ceiling",
        )
    print("stream_fixture_replay=pass")


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
