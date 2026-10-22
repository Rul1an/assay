# Bounded agent-session evidence probe

Experimental helpers for the [approved PR1 plan](../../docs/superpowers/plans/2026-10-22-agent-session-evidence-probe.md).
This directory is not a new incident-package source format or an agent safety check.

`protocol.json` is the single version/limit pin used by installation and execution.
Use a fresh task directory with separate `hooks/` and `oracle/` directories. Never
point these helpers at an existing profile or evidence directory. The oracle is
server-received wire data and must stay outside all recipient comparison views.

## Helpers and tests

- `record_hook.py OUTPUT_DIR` copies each stdin invocation verbatim to a reserved
  `.raw` slot, then writes a separate length/SHA256 completion receipt. A receipt
  means byte capture completed, not that the payload is valid evidence. Incomplete
  raw bytes survive without a receipt. The recorder's stdout is empty and failures
  do not change tool policy; outer lifecycle reconciliation is mandatory.
- `record_hook.reconcile(...)` checks bounded retained bytes against measured
  `(hook_event_name, tool_use_id)` tuples from starts, responses and independently
  enumerated expected lifecycle events. It is a consistency check, not completeness
  or authenticity. A live vendor-stream parser is deliberately not invented before
  the actual fields are captured. If the source lacks the needed fields, report
  unavailable; never substitute timestamps or a guessed ordering.
- `fixture_server.py ORACLE_DIR` is a deliberately small stdio MCP fixture. It
  retains incoming frames before parsing, accepts only fixed local operations,
  and emits no model-selected commands or destinations. Its wire loop is serial;
  the thread test exercises handler safety, not live agent concurrency.
- `wrap_entry.py ATTEMPT_DIR ASSAY_BINARY` starts the real proxy once. A reconnect
  leaves a restart marker and fails. The real decision emitter appends, so attempt
  paths must be fresh; the session-context writer refuses an existing path.

Run the complete helper suite without a model, login, eBPF or network service:

```sh
python3 -B -m unittest discover -s examples/agent-session-evidence-probe -p test_probe.py
```

Tests use only a loopback listener, fixed `/usr/bin/printf`, temporary directories
and subprocesses of these helpers. They do not inspect auth or run arbitrary argv.

## Capture gates

Install from the official vendor package into a task-owned prefix using the version
in `protocol.json`. Keep credentials in a separate profile authenticated directly
on Linux by the operator. Never copy an existing profile or set an API-key fallback.
Use the release CLI and a separately built eBPF object from the same source tag;
record both hashes and the source/lock/toolchain provenance.

Before any model call, prove non-root UID, zero effective capabilities, retained
cgroup membership, a minimal environment and the expected subscription auth method.
Use only synthetic task paths and narrow tool permissions. An unexpected restart,
recorder/lifecycle mismatch, exceeded bound or unexercised control makes the attempt
incomplete. Retain failure evidence. Do not patch the capture with synthetic events.

Expected controls and the A/B/C questions are in the design. Vendor rejection and
proxy denial are different: the server oracle sees only requests reaching it.
Recorder and authentication operations may appear in kernel data; they must not be
presented as task activity. Absence of kernel events is not an activity-absence proof.

## Current status

Helpers are preparation for the real session, not its result. No live A/B/C outcome,
native strong binding, cold receiver, market value or production admission is
established by their unit tests. The dedicated Linux installation and harmless
privilege preflight are separate operational evidence. Authentication and the
actual host lifecycle capture remain prerequisites of a live session.
