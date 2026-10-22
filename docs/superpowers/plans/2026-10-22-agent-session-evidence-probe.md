# Single-session agent evidence PR1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Decide from one bounded real Linux session whether original vendor records plus existing Assay observations justify a source adapter and subsequent handoff work.

**Architecture:** Codex owns the branch and experiment helpers. Claude advises on host operation; Muse supplies public, read-only experimental critique; a reserved non-builder reviews the final head. Reuse released Assay capture and existing verification; PR1 does not add a production source format or join predicate.

**Tech Stack:** Python standard library for a bounded recorder and controlled stdio MCP fixture; Claude Code CLI; released Assay CLI and an independently pinned eBPF object; Linux cgroup v2.

**Spec:** [Approved goal and proof contract](../specs/2026-10-22-agent-session-evidence-design.md), design commit `e21699bd5c930ff8763578b91d788aadb81cb06c`.

## Global constraints

- Existing incident input occurrences: 64. Outer source bytes: 256 MiB. Declared occurrence bytes: 240 MiB. All nested limits remain effective.
- Maximum capture window: 30 minutes. Preserve each failed attempt; do not drop records to fit.
- Original hook stdin bytes, one occurrence per invocation. No JSON rewriting, timestamp backfill, ID injection or deduplication to manufacture uniqueness.
- No production ingestion changes, predicate reinterpretation, new telemetry framework, issue closure, push or merge in this execution stage.
- No credential copying, API-billing substitution, permission bypass, persistent capability grants, or running the model as root.
- Public evidence contains only synthetic task data. Authentication/profile directories stay outside the retained pack.
- At most 4 MiB is proposed for checked-in retained evidence. Larger original evidence stays privately retained and the publication decision stops; it is not silently rewritten.
- Named source fields and eligibility are decisions after capture, not invented format guarantees.

## Review focus

1. Recorder failure must invalidate the experiment without changing the agent's policy decision; test disk/output failure and oversized stdin.
2. Pre/post IDs and simultaneous identical calls must remain distinct; test concurrent recorder invocations and duplicate payloads.
3. Agent permissions and actual wire delivery are different; retain vendor rejection, proxy denial and fixture receipt separately.
4. A coherent source replacement can pass internal integrity; require a receiver-selected external expected digest for the substitution control.
5. Missing native timestamps and unexercised controls must stop strong-binding/value conclusions, not become success through recorder metadata or synthetic substitutes.

## Task 1 — pin and preflight the actual Linux host

**Files:** Create `examples/agent-session-evidence-probe/README.md` and `examples/agent-session-evidence-probe/protocol.json` during execution. Private raw preflight receipt lives outside the repo; only a sanitized status belongs in the public example.

**Consumes:** approved spec; installed host/tool metadata; existing runner and MCP wrapper contracts.
**Produces:** a machine-readable preregistration and a pass/unavailable disposition for each infrastructure prerequisite.

- [ ] Pin a dedicated Claude installation to official release `2.1.308` (observed official release on 2026-10-22), Assay `v6.15.0`, and an eBPF artifact built from the exact Assay release tag source SHA and bound by digest. Put selected versions in the protocol file once; install/invocation read that file. Recheck actual CLI help and binary hashes; do not update the existing macOS CLI.
- [ ] Provision an owner-only task/profile directory on Linux, outside the main checkout. Use `CLAUDE_CONFIG_DIR` with no copied profile; authenticate through the user's normal subscription login directly on Linux. The user handles login secrets/codes. Retain only selected auth-status fields, never the auth output wholesale.
- [ ] Prove isolated configuration: one task canary server is present, known maintainer servers are absent, and only the explicit settings/MCP config is used. Do not assume `--bare` supports the selected hook/auth route.
- [ ] Inspect existing eBPF provenance before building anything. If missing, stop for a separately scoped compatible build; do not relabel a source-built object as a release asset.
- [ ] Verify privilege separation with a harmless UID/cgroup witness before any model call. Existing Runner inherits its UID. Use a scoped standard `runuser` transition to the ordinary task user as its child, if that route preserves the cgroup and clears capabilities. Launch the ordinary child with `runuser -u <task-user> -- env -i` and an explicit minimal environment (task HOME/PATH/config only). Record effective UID, capabilities, preserved cgroup membership and `authMethod=claude.ai`; verify API/token environment keys are absent without printing values. Stop on mismatch. No `setcap` or general sudo configuration changes.
- [ ] Freeze exact paths, digest inventory, 30-minute deadline and model/turn ceiling before capture. Use at most eight turns for the initial attempt; retain any budget exhaustion. No paid API fallback. Measure whether the subscription CLI supports a monetary estimate flag instead of treating it as a hard spending cap.

## Task 2 — minimal byte recorder and controlled fixture, tests first

**Files:** Create `examples/agent-session-evidence-probe/record_hook.py`, `fixture_server.py`, `test_probe.py`, and the task-owned policy/settings templates. No Cargo source changes.

**Interfaces:** `record_hook.capture(stream, output_dir, max_bytes) -> receipt_path` copies one invocation to a fresh file; `fixture_server` implements only pinned initialize, tools/list and tools/call handling. No retained command is executed on replay. Protocol configuration supplies limits and fixed task paths; payload values never select filesystem paths or executable commands.

- [ ] Write failing tests: exact byte parity including whitespace; two equal payloads produce two occurrences; concurrent calls cannot overwrite; oversized input is not accepted as complete; failed output never truncates an old object; stdout stays empty; missing recorder executable, killed hook and missing lifecycle stream make the experiment incomplete even when no failure marker exists.
- [ ] Add the minimum recorder to pass those tests. Use bounded reads and exclusive creation, no envelope added to original bytes. Invoke an absolute Python executable with `-I`. Failure writes a separate static failure marker where possible; a supervisor reconciles vendor `--include-hook-events` starts/responses, tool-use entries and retained hook occurrences. A missing recorder, incomplete stream or unmatched event makes the experiment non-success even without a marker. This is a producer-reported consistency check, not independent proof of total activity. Do not use hook exit 2 or decision JSON to change tool policy.
- [ ] Write failing fixture tests for initialize/version recording, allowed call, fixed denial target, deterministic retry signal, identical concurrent inputs, and malformed/oversized requests. No arbitrary shell execution or network destination comes from arguments. The oracle records only frames reaching the server; it cannot prove the exact client request for a proxy-denied call.
- [ ] Implement only three controlled MCP tools: a fixed local observation operation, a policy-denied operation, and a vendor-denied operation. The observation operation touches a task file, executes fixed `/usr/bin/printf`, and connects to its own loopback listener. The oracle records original received frames separately from A/B/C evidence.
- [ ] Confirm tests on Linux with `python3 -m unittest discover -s examples/agent-session-evidence-probe -p test_probe.py`; run the same small suite locally. Mutate byte rewriting, exclusive creation and failure reporting individually and show the relevant tests fail. Restore each mutation.
- [ ] Commit the complete probe helpers and protocol before measuring. Export a clean run kit from that SHA and hash it; do not execute a moving worktree.

## Task 3 — bounded live capture and decision record

**Files:** Complete the example README; retain a compact allowlisted `record/` directory only after privacy/size review. Create `examples/agent-session-evidence-probe/RESULTS.md` with exact evidence references and non-claims.

**Consumes:** Task 1 pass disposition and Task 2 frozen run kit.
**Produces:** original bytes, provenance, A/B/C answers and explicit go/stop decision; not an incident-v1 vendor importer.

- [ ] Configure `PreToolUse`, `PostToolUse` and `PostToolUseFailure` recorders for measured lifecycle shapes. Preserve supported structured CLI output as vendor comparison context; no new production admission is implied. Apply narrow allow rules only for the synthetic task and deny the designated vendor-rejection tool. No all-tools allow or bypass mode.
- [ ] Wrap the one fixture server with existing `assay mcp wrap --policy ... --event-source ... --decision-log ... --session-context-out ... -- ...`. Run the bounded unprivileged host under `assay runner-spike run --kernel-capture --ebpf ... --redaction-key ephemeral ...`. Read exact flags from the pinned binary. Keep Runner's default redaction and label its output accurately. Use fresh exclusive output paths and record wrapper starts; an unexpected server restart stops this initial probe as incomplete, rather than reusing a context path or merging decision logs. Add a restart control to prove that stop. Verify the pinned decision-log open mode before capture.
- [ ] Exercise allowed MCP call, proxy denial, vendor rejection, built-in tool, parallel identical calls, retry/reuse and positive host file/process/network controls. Record controls actually observed. Separate capture-infrastructure operations (recorder, runuser, authentication/telemetry) from controlled task operations without deleting either from originals. Select synthetic paths in advance; auth-profile paths in kernel records remain a publication-review boundary. Do not force tool-call IDs into request arguments. If the model does not exercise a control, retain the attempt as incomplete; no synthetic patch-up.
- [ ] Compare nested A/B/C views of the same run using the spec's six questions. Native vendor inspection remains available in A. Keep oracle frames out of all recipient views. Native timestamp, ID propagation, uniqueness, lifecycle and window eligibility must be recorded separately; unresolved means no strong-binding claim.
- [ ] Exercise offline comparison controls by removing a source, duplicating an occurrence, swapping a valid source with/without an external digest expectation and corrupting bytes. These are research controls, not claims that the existing incident verifier admits vendor records. Reuse the existing incident fixture/replay harness for existing-format verification; do not manufacture Inspector bytes.
- [ ] Write a per-question evidence table, missing controls, friction and a bounded decision. No correct incremental receiver decision means stop or adapter-only. No native timing/identity means stop strong joins. Neither condition authorizes v2 automatically.
- [ ] Run `git diff --check`, applicable normal hooks, link/privacy/size checks and the focused Linux tests. Freeze final SHA and retained hashes, then obtain independent review. Hosted checks and a PR are later delivery steps, not local success claims.

## Current preflight observations (not execution acceptance)

On 2026-10-22 the existing Linux VM answered `Linux 6.8.0-147-generic aarch64`.
The runner snapshot classified it healthy/idle with 6.3 GiB free. `/opt/rust/bin/assay`
reported 6.15.0 and exposes `runner-spike`. `claude` was not on the guest's default
PATH or at `/home/ubuntu/.local/bin/claude`; no Claude profile directory was found
at the probed default location. No isolated login was attempted. eBPF artifact
compatibility and privilege-drop behavior have not been measured. These observations
do not establish absence from all guest paths or all users.

The local macOS PATH Claude is 2.1.32; it is not the proposed Linux host or the
Desktop-bundled co-designer. The official CLI release lookup returned v2.1.308.
No model run, helper implementation or package-format change has started.

Primary operating references: [CLI flags](https://code.claude.com/docs/en/cli-reference),
[authentication](https://code.claude.com/docs/en/authentication),
[hook interface](https://code.claude.com/docs/en/hooks), and
[settings precedence](https://code.claude.com/docs/en/settings).

Claude's source review identified inherited Runner privileges, absent bundled eBPF,
and uncertain hook/rejection ordering as preflight gates. Muse's advisory identified
identity conflation, duplicate erasure, truncation and invalid vendor baselines;
Muse read the design, not the pinned incident implementation, so that advice is not
source verification or quorum.
