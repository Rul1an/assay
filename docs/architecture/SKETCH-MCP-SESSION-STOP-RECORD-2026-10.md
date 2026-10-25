# SKETCH: MCP session stop record

Status: Design sketch (no code)
Date: 2026-10-25
Related: #3387 (session context), #3412 (`--decision-log-new`), #3414 (decision-log write failures)

## Problem

`assay mcp wrap` records how a session starts (`--session-context-out`, one
`assay.mcp.session.context.v1` event) but nothing about how it ends. Three facts are lost today:

1. **How the wrapped server ended.** `McpProxy::run` returns `status.code().unwrap_or(1)`, so a
   server killed by a signal and a server that exited with status 1 are the same number. The
   signal is not recorded anywhere. Worse, the endings a stop record matters most for never reach
   a status at all: if the client-side thread panics or `wait` fails, `run_inner` returns an error
   and `Drop for McpProxy` then calls `kill()` on a child that was never reaped, so its real
   termination is replaced by one Assay caused.
2. **Whether the decision log is complete.** Since #3414 a failed write ends the log and prints
   one stderr line, but no artifact says so. A log that stopped cleanly between events and a short
   session are the same bytes; a write that failed partway leaves a malformed last line.
3. **Which process this was.** Host lifecycle capture is listed as unfinished in
   `examples/agent-session-evidence-probe/README.md`; an experiment that needs the server's pid
   and termination has to record them outside Assay today.

## Proposal

An opt-in `--session-stop-out PATH`, reserved through the same `reserve` as the other new-file
outputs (new file, mode `0600` on Unix, alias check against the outputs listed there, before the
child starts).

**Prerequisite: reap on every path.** Once a child was spawned, every exit from the session must
`wait` for it before anything kills it: a client-thread panic, a failed join, an I/O error.
`Drop` keeps `kill()` only as the last resort for a child that is still running when `wait`
itself failed, and records that it did. The proxy returns the reaped `ExitStatus` (plus how the
session ended) instead of an `i32`, through a new entry point; `run` keeps its signature.

After the reap, Assay writes one CloudEvent:

```json
{
  "specversion": "1.0",
  "type": "assay.mcp.session.stop.v1",
  "id": "evt_stop_<session_id>",
  "source": "<event source>",
  "time": "<RFC 3339, wrapper clock>",
  "data": {
    "server_id": "<label>",
    "session_context_id": "evt_context_<session_id> | null",
    "session_end": "client_closed | proxy_error | proxy_panic",
    "termination": "exited | signaled | killed_by_assay | unknown",
    "exit_code": 0,
    "signal": null,
    "core_dumped": false,
    "server_pid": 12345,
    "decision_log": "complete | incomplete | temporary | absent",
    "decision_events_written": 17,
    "decision_events_dropped": 0
  }
}
```

- `termination`, `exit_code`, `signal`, `core_dumped` come from the reaped `ExitStatus`
  (`ExitStatusExt::signal` / `core_dumped` on Unix; on Windows `termination` is `exited` unless
  Assay killed it). `signal` is the number only; names are platform-defined.
  `killed_by_assay` means Assay terminated a child it could not reap; `unknown` means no status
  was obtained. `session_end` says which side ended the session.
- `server_pid` is the pid Assay spawned. It identifies a process on this host for this boot only;
  it is not an identity claim.
- `decision_log` is `incomplete` when `FileDecisionEmitter` stopped (#3414), `temporary` when
  `--coverage-out` used an internal log without `--decision-log`, `absent` when no log was
  written, otherwise `complete`. `decision_events_written` and `decision_events_dropped` count
  events the emitter wrote and refused after stopping, so a reader can check the first against
  the log and see the loss from the second.
- `session_context_id` links the pair when both outputs are requested; the stop record must not
  require a context record.

The wrapper's own exit code is unchanged when the stop record is written. If a requested stop
record cannot be written, `assay mcp wrap` fails even when the child exited non-zero, and prints
the child's termination on stderr: a requested record that silently does not exist is the gap
this sketch closes. (`--coverage-out` lets the child's code win instead; the stop record is
deliberately stricter.)

## Compatibility

- New CLI flag and a new event type: additive. `assay-cli` has no library target.
- `assay-core` needs to expose the reaped status. That must not add a field to `ProxyConfig` or
  change `run`'s return type (semver-major). Candidate: a new
  `McpProxy::run_with_reserved_outputs`-style entry point returning a small struct with private
  fields and accessors, or a `#[non_exhaustive]` struct.
- `FileDecisionEmitter` needs a way to report "stopped" and the written count to its owner without
  changing the public `DecisionEmitter` trait, for example an `Arc` handle created alongside it.
- The schema is registered like `assay.mcp.session.context.v1` in `docs/spec/EVIDENCE-CONTRACT-v1.md`.

## Non-claims

- Not proof that tool calls executed, that the server behaved, or that the log is untampered: the
  record is written by the same process that wrote the log, with the same owner.
- No cgroup in v1. Reading `/proc/<pid>/cgroup` at spawn races with the child moving itself, and
  the Runner already owns cgroup placement; a cgroup field belongs to a Runner-backed variant.
- No duration or resource accounting in v1.

## Decisions taken in review

A read-only Grok consult on the first draft (2026-10-25) corrected three statements about current
behaviour and moved the open questions to decisions: numeric signal only; count written and
dropped events; a requested stop record that cannot be written always fails the command; keep
context and stop as two files, because the context file is defined as exactly one event written
at start. Its main finding, now the prerequisite above, was that the first draft only covered a
clean reap.

## Order of work

1. Reap on every post-spawn path and stop killing before `wait`, with tests for a client-thread
   panic and a failed join. This is a correctness fix on its own and lands first.
2. Expose the emitter's stopped state and counts through a handle, without changing
   `DecisionEmitter`.
3. Add `--session-stop-out` and register `assay.mcp.session.stop.v1`.
