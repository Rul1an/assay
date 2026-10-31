# Experimental MCP session-stop observation

`assay mcp wrap --session-stop-out PATH` writes one separate
`assay.mcp.session.stop.v1` CloudEvent after the proxy returns and performs bounded
child cleanup. It requires `--event-source`. The file must be new and separate
from other outputs; Unix mode is 0600. If `--decision-log` is supplied, also pass
`--decision-log-new`: accounting must not be confused with a historical append log.
Without the new flag, existing protocol output and exit-code behavior is unchanged.
A run that saw only `server/discover` creates no stop file and writes no stop event; see
[the wrap reference](../reference/cli/mcp-server.md).

```sh
assay mcp wrap --policy policy.yaml --event-source assay://example/session \
  --decision-log decisions.ndjson --decision-log-new \
  --session-context-out session-context.json \
  --session-stop-out session-stop.json -- python3 server.py
```

## Payload contract

The event envelope uses the existing CloudEvents fields. `id` is
`evt_stop_<session_id>`; the optional context link is the unchanged
`evt_context_<session_id>` identifier. It is not a global or authenticated identity.

| Field | Meaning |
| --- | --- |
| `server_id`, `server_pid` | Configured label and spawned local PID; not executable identity |
| `session_context_id` | Existing context event ID when requested, otherwise null |
| `session_end` | `client_closed` after both I/O workers and child wait finish, or `proxy_error` for a returned I/O/join error |
| `termination` | `exited`, `signaled`, or `unknown`, from the observed child status |
| `exit_code`, `signal`, `core_dumped` | Observed OS status; unavailable fields are null; signals are Unix numeric values |
| `assay_kill_requested` | Assay attempted a kill after try_wait returned no status. Does not claim kill causality or success |
| `decision_log` | `complete`, `incomplete`, `temporary`, `absent`, or `unknown` |
| `decision_events_written` | Count of complete successful write_all calls in this invocation |
| `decision_events_dropped` | Count of the failed emission and subsequent refused emissions |
| `decision_prefix_sha256` | Lowercase SHA-256 of exactly the successfully written complete lines, including newlines; excludes any partial failed tail |
| `assertion_basis` | `producer_observation` |
| `non_claims` | Explicit exclusions: execution proof, authentication, global coverage, durable-storage proof |

Accounting fields are null when no emitter was initialized. `incomplete` takes
precedence when the writer stopped or the proxy returned an error. `temporary`
means the coverage-only log is not retained; `unknown` means a requested writer
was not initialized. `complete` describes this writer's observed emissions, not
all activity in the session or the behavior of the server. A nonzero child exit
can have a complete decision log. A poisoned writer is incomplete even when the
number of unobserved emissions is unknown; dropped is not an estimate of them.

Cleanup preserves #3419: try_wait first, then a kill attempt and at most two seconds
of polling. This is not a deadline for ordinary live tool calls. The process may
exit concurrently with a kill attempt. Stop output is best effort on returned
errors; process-wide panic, SIGKILL, abrupt termination, startup failure, or output
failure can leave the created file empty or partial, or leave no file. Missing or malformed closure
is **unknown**, never evidence of a complete capture.

Assay installs no signal handler for this flag. A signal terminating the wrapper can leave
its stop file empty, partial or absent; closure is then unknown. `signal` describes the wrapped
server, not the proxy. `client_closed` records the stdin-EOF path, not why the host closed
stdin. Shipped dev/release profiles abort on panic; unwind-only tests do not prove that a
shipped binary will clean up or write a record after panic.

A requested stop write failure fails the command and reports the observed child
status. Ordinary child exit codes remain unchanged when the write succeeds. No
fsync/power-loss, host reboot, atomic publication, or owner-tamper guarantee is made.
The target is exclusively new, so a failed write cannot truncate earlier evidence.

## Bounded receiver and capture

The existing `examples/agent-session-evidence-probe/wrap_entry.py` requests fresh
decisions, request receipts, context, and stop output and retains proxy stderr.
After the outer supervisor has stopped writers and retained its cleanup receipt:

```sh
python3 examples/agent-session-evidence-probe/session_stop.py /path/to/capture
```

The reader needs only `session_stop.py`, `record_hook.py`, `protocol.json`, `session-context.json`,
`session-stop.json`, and `decisions.ndjson`. It does not read the oracle, hooks,
credentials, or vendor transcript. Inputs are regular no-symlink files, bounded
before JSON materialization (64 KiB per companion, 8 MiB log, 10,000 decisions,
32 JSON levels). Duplicate keys and nonfinite numbers are refused. The required stable context fields (ADR-006), context
pair, event count, IDs, complete-line bytes and digest must agree.

`closed` means a recorded ending and matching retained decision accounting, even
for exit 7 or a signal. It does not mean successful execution. `incomplete` retains
known loss separately; absent, inconsistent or unretained evidence is `unknown`.
The CLI reader returns zero only for `closed`. Keep the existing receipt linkage
and outer supervisor checks: this companion replaces neither. Copy raw bytes and
record source/binary/helper hashes and the manifest before handing off a package;
file/digest agreement does not authenticate its producer.

Full A/B/C remains on hold. This experimental event is not admitted as an incident
source and does not alter the privileged-action profile or existing identifiers.
