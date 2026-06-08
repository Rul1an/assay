# Runner Artifact v0 Contracts

> Internal Phase 2A reference. These contracts describe the normalized
> runner-spike artifacts proven by the delegated Linux/eBPF Phase 1 acceptance
> run. They are not a public Assay-Runner release contract.

The Phase 1 delegated proof writes a deterministic archive containing three
load-bearing JSON artifacts:

- `observation-health.json`
- `capability-surface.json`
- `correlation-report.json`

The determinism claim is over these normalized artifacts and the normalized
layer streams in the archive. It is not a claim that raw kernel telemetry, raw
ring-buffer delivery, dynamic loader behavior, or the eBPF object are
byte-identical across runs.

Machine-readable golden-shape examples are listed in
[`golden/index.md`](golden/index.md). They are not substitutes for delegated
acceptance runs; they freeze the v0 field set and serialization shape that
docs and tests should preserve. Example values are illustrative unless this
contract explicitly defines a field's allowed value vocabulary.

## Contract Principles

1. **Monitor observes broadly.** Kernel, policy, and SDK monitors may observe
   more than the runner-spike bundle is willing to claim as evidence.
2. **Normalizer claims narrowly.** The runner-spike normalizer keeps only
   events that support the measured-run attribution claim.
3. **Passing health stays strict.** A passing Linux/eBPF delegated run requires
   complete kernel capture, zero ring-buffer drops, and clean cgroup
   correlation.
4. **v0 artifacts are deterministic.** Set-like fields serialize in stable
   order; three-run delegated determinism compares the normalized JSON
   artifacts byte-for-byte.
5. **Unsupported expansion is a contract change.** New artifact fields,
   schema strings, layer status values, or evidence categories require an
   explicit Phase 2 contract review.

## Phase 1 Proof-Pack Manifest

The `assay.runner.phase1_proof_pack.v0` manifest is a hand-curated retention
record for the Phase 1 delegated acceptance run. It is not emitted by the
runner and is not part of the runner archive schema. Its purpose is to retain
workflow metadata, logs, excerpts, and digest links for the historical
delegated proof after GitHub Actions log retention changes.

## Telemetry Versus Evidence

The monitor layer may observe telemetry that is useful for debugging but not
for the runner-spike capability claim. The normalizer excludes this telemetry
from `capability-surface.json` and `layers/kernel.ndjson`.

| Filter level | Examples | Reason |
|---|---|---|
| Event type | `EVENT_INODE_RESOLVED` | Kernel-internal resolution telemetry; it does not add an agent capability claim |
| Dynamic loader paths | `/etc/ld.so.cache`, `/lib/*`, `/lib32/*`, `/lib64/*`, `/usr/lib/*` | Runtime loader behavior rather than agent-selected behavior |
| Locale/runtime paths | `/usr/share/locale/*`, `/etc/localtime`, Python bootstrap probes | libc/interpreter initialization noise |
| Toolchain/RPATH paths | `.rustup/toolchains/*/*.so*`, `target/*/{build,debug,release}/*.so*` | Cargo/Rust loader behavior |
| Runtime dependency trees | `node_modules/*` | SDK/runtime implementation detail, not a runner capability claim |
| Kernel and device introspection | `/proc/*`, `/sys/*`, `/dev/*` | Runtime plumbing and host introspection |

Policy-denied paths remain evidence. For example, a `file_blocked` policy
event for `/lib/.../libc.so.6` is preserved because the policy decision is the
claim, even though ordinary loader `openat` telemetry for that path is
filtered.

## `layers/kernel.ndjson`

Schema string:

```text
assay.runner.kernel_event.v0
```

Machine-readable line schema:

[`schema/kernel-event-v0.schema.json`](schema/kernel-event-v0.schema.json)

`layers/kernel.ndjson` is an NDJSON stream: each non-empty line validates
independently against the line schema. The schema covers the enriched
open metadata shape now emitted by Runner while preserving compatibility
with older v0 archives where those open metadata fields are absent.

Each line is one normalized kernel event. Common fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.kernel_event.v0` |
| `run_id` | string | yes | Run identifier shared by all archive artifacts |
| `seq` | integer | yes | Runner-assigned sequence in normalized kernel layer order |
| `pid` | integer | yes | Kernel-observed thread-group id for the event |
| `event_type` | integer | yes | Internal monitor event id (`1` for `openat`, `2` for `connect`, etc.) |
| `kind` | string | yes | Normalized event kind: `openat`, `connect`, `exec`, `file_blocked`, `connect_blocked`, or reserved `event_<id>` for unrecognized monitor ids |
| `value` | string or null | yes | Normalized path or endpoint when available |

Known v0 `event_type` / `kind` pairs:

| `event_type` | `kind` |
|---:|---|
| `1` | `openat` |
| `2` | `connect` |
| `4` | `exec` |
| `10` | `file_blocked` |
| `20` | `connect_blocked` |

Other internal monitor ids remain valid only as `kind=event_<id>`.
`value=null` means the monitor event was preserved but the normalizer
could not decode a path or endpoint from it.

Open events may additionally carry syscall metadata:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `flags` | integer | no | Linux open flags captured at syscall entry |
| `mode` | integer | no | Linux create mode argument, when provided |
| `resolve` | integer | no | `openat2` resolve flags, when non-zero |
| `return_value` | integer | no | Syscall return value from `sys_exit_openat*` |
| `access_mode` | enum | no | `read`, `write`, `read_write`, or `unknown`, derived from `flags & O_ACCMODE` |
| `operation_flags` | array[string] | no | Derived operation hints such as `create`, `truncate`, `append`, `exclusive` |
| `status` | enum | no | `success` when return value is non-negative, otherwise `error` |

These fields are optional so older v0 archives remain readable. Consumers that
need read/write/create/remove distinctions must require the optional open
metadata and treat older archives as inconclusive for that dimension.

Open metadata example:

```json
{
  "schema": "assay.runner.kernel_event.v0",
  "run_id": "run_001",
  "seq": 0,
  "pid": 1234,
  "event_type": 1,
  "kind": "openat",
  "value": "/tmp/work/fixture-output.txt",
  "flags": 577,
  "mode": 420,
  "return_value": 4,
  "access_mode": "write",
  "operation_flags": ["create", "truncate"],
  "status": "success"
}
```

Undecoded event example:

```json
{
  "schema": "assay.runner.kernel_event.v0",
  "run_id": "run_001",
  "seq": 1,
  "pid": 1234,
  "event_type": 999,
  "kind": "event_999",
  "value": null
}
```

## `observation-health.json`

Schema string:

```text
assay.runner.observation_health.v0
```

Fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.observation_health.v0` |
| `run_id` | string | yes | Non-empty run identifier shared by all archive artifacts |
| `platform` | string | yes | Platform name used by the runner-spike path; Phase 1 accepted `linux` |
| `kernel_layer` | enum | yes | `complete`, `partial_ringbuf_drops`, or `absent` |
| `ringbuf_drops` | integer | yes | Total ring-buffer drops for the measured kernel capture window |
| `policy_layer` | enum | yes | `present` or `absent` |
| `sdk_layer` | enum | yes | `present`, `self_reported`, or `absent` |
| `cgroup_correlation` | enum | yes | `clean`, `partial`, or `failed` |
| `network_protocol_coverage` | enum | yes | Network protocol coverage scope for this run's kernel evidence |
| `network_endpoint_claim_scope` | enum | yes | Claim boundary for interpreting `network_endpoints` |
| `notes` | array[string] | yes | Stable code-prefixed capture notes used by delegated determinism |

`sdk_layer=self_reported` is the accepted v0 value for the OpenAI Agents SDK
fixture because SDK events are emitted by the fixture/runtime path itself.
`sdk_layer=present` remains in the v0 enum vocabulary for a future directly
corroborated SDK layer, but the accepted S5 fixture does not use it.

`notes` are strings in v0, but they are not arbitrary prose. The token before
the first colon is the machine-readable note code, for example
`s2_kernel_capture`, `s4_policy_capture`, or `s5_sdk_capture`. Text after the
colon is a stable deterministic message for reviewers and diffs. A future v1
may split these into `{code, message}` objects, but v0 consumers should parse
the code prefix when they need machine-readable note identity.

Validation rules:

- `run_id` must not be empty.
- `ringbuf_drops > 0` requires `kernel_layer=partial_ringbuf_drops`.
- non-`linux` platforms require `kernel_layer=absent`.
- `cgroup_correlation=failed` is not valid for a passing Phase 1 run.

Interpretation rules:

- `kernel_layer=complete` means capture health was clean for the attached hooks;
  it does not, by itself, prove protocol-complete network coverage.
- `network_protocol_coverage=absent` means Runner observed no network protocol
  events in the capture window. It is not a positive network coverage claim.
- `network_protocol_coverage=unknown` means network protocol coverage cannot be
  interpreted, for example because relevant network hook events may have been
  dropped.
- `network_protocol_coverage=connect_only` means Runner observed
  `connect()`-level network evidence only.
- `network_protocol_coverage=datagram_peer_observed` means Runner observed
  datagram peer evidence from `sendto` or `sendmsg`, without a matching
  `connect()` event in the same capture window.
- `network_protocol_coverage=connect_and_datagram_peer_observed` means Runner
  observed both `connect()` and datagram peer evidence in the same capture
  window.
- `network_endpoint_claim_scope=diagnostic_only` means `network_endpoints`
  are useful for coarse/diagnostic review, not for exact peer-set claims on
  datagram protocols such as QUIC.
- `network_endpoint_claim_scope=not_applicable` means no network endpoint claim
  is available for this run.
- `network_endpoint_claim_scope=unknown` means no bounded endpoint claim should
  be inferred because network coverage is not interpretable.
- Datagram peer evidence is stronger than `connect_only` for QUIC-style
  capture, but v0 still keeps `network_endpoint_claim_scope=diagnostic_only`
  unless a future layer can make a bounded exact peer-set claim.

Passing Linux/eBPF example:

```json
{
  "schema": "assay.runner.observation_health.v0",
  "run_id": "run_openai_agents_kernel_policy_determinism",
  "platform": "linux",
  "kernel_layer": "complete",
  "ringbuf_drops": 0,
  "policy_layer": "present",
  "sdk_layer": "self_reported",
  "cgroup_correlation": "clean",
  "network_protocol_coverage": "connect_only",
  "network_endpoint_claim_scope": "diagnostic_only",
  "notes": [
    "s2_kernel_capture: monitor_events=4 ringbuf_drops=0 network_protocol_coverage=connect_only network_endpoint_claim_scope=diagnostic_only",
    "s4_policy_capture: policy_events=1",
    "s5_sdk_capture: sdk_events=3 sdk_tool_calls=1"
  ]
}
```

## `capability-surface.json`

Schema string:

```text
assay.runner.capability_surface.v0
```

Fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.capability_surface.v0` |
| `run_id` | string | yes | Non-empty run identifier shared by all archive artifacts |
| `filesystem_paths` | array[string] | yes | Stable sorted set of observed filesystem evidence values |
| `network_endpoints` | array[string] | yes | Stable sorted set of observed network endpoint values under the coverage and claim-scope limits declared in `observation_health` |
| `process_execs` | array[string] | yes | Stable sorted set of observed process execution values |
| `mcp_tools` | array[string] | yes | Stable sorted set of observed MCP/tool names |
| `policy_decisions` | array[string] | yes | Stable sorted set of policy decision summaries |

`filesystem_paths` stores full observed v0 path values, not directory-prefix
projections. During Phase 2A consolidation this field was renamed from the
earlier internal `filesystem_prefixes` label because the old name implied a
projection the artifact never provided. Prefix projection remains a later
capability-diff transformation, not part of this artifact contract.

For current Linux kernel capture, `network_endpoints` is populated from
`EVENT_CONNECT` / `EVENT_CONNECT_BLOCKED` sockaddr values. For datagram
protocols, especially QUIC, this is a connect-attempt surface, not a proven
actual peer-set surface.

Example:

```json
{
  "schema": "assay.runner.capability_surface.v0",
  "run_id": "run_kernel_policy_determinism",
  "filesystem_paths": [
    "/tmp/assay-runner-kernel-policy/work/input.txt",
    "/tmp/assay-runner-kernel-policy/work/output.txt"
  ],
  "network_endpoints": [],
  "process_execs": [
    "/usr/bin/cat"
  ],
  "mcp_tools": [
    "read_file"
  ],
  "policy_decisions": [
    "allow:read_file"
  ]
}
```

## `correlation-report.json`

Schema string:

```text
assay.runner.correlation_report.v0
```

Fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `schema` | string | yes | Must equal `assay.runner.correlation_report.v0` |
| `run_id` | string | yes | Non-empty run identifier shared by all archive artifacts |
| `status` | enum | yes | `clean`, `partial`, or `failed` |
| `bindings` | array[object] | yes | Stable correlation bindings from SDK/policy windows to kernel evidence |
| `ambiguities` | array[string] | yes | Stable list of unresolved correlation issues |

Binding fields:

| Field | Type | Required | Semantics |
|---|---|---:|---|
| `tool_call_id` | string | yes | Tool-call id used to bind SDK and policy layers |
| `policy_decision` | string or null | yes | Matched coarse policy outcome, when present (`allow` or `deny` in v0 accepted fixtures) |
| `kernel_event_count` | integer | yes | Count of normalized kernel events in the binding window |
| `window.start` | string | yes | Inclusive window start marker |
| `window.end` | string | yes | Inclusive window end marker |

Correlation windows use runner-defined phase markers from one canonical runner
clock. SDK-provided timestamps are informational only and MUST NOT be used as
primary join keys for v0 correlation. They also MUST NOT be used as an ordering
fallback to disambiguate call-id-less tool bindings.

Clean v0 correlation requires a stable `tool_call_id` for every tool-call
binding. The first Phase 2B capability-diff contract inherits this rule: it may
diff clean bindings by `tool_call_id`, but it must not introduce deterministic
order-fallback for call-id-less agent runtimes. If a runtime cannot provide a
stable tool-call id, the runner must report a partial or failed correlation
state until a separate call-id-less fixture and contract explicitly define the
fallback semantics.

Example:

```json
{
  "schema": "assay.runner.correlation_report.v0",
  "run_id": "run_openai_agents_kernel_policy_determinism",
  "status": "clean",
  "bindings": [
    {
      "tool_call_id": "tc_runner_policy_001",
      "policy_decision": "allow",
      "kernel_event_count": 2,
      "window": {
        "start": "run_started",
        "end": "run_finished"
      }
    }
  ],
  "ambiguities": []
}
```

## Archive Relationship

The archive manifest is deterministic and uses:

```text
assay.runner.archive_manifest.v0
```

The manifest records each archive path, byte length, and `sha256:` digest.
The three v0 JSON artifacts must all share the archive `run_id`.

## Dependency Upgrade Flow

The OpenAI Agents acceptance fixture pins and asserts the SDK version used by
the delegated gate. For dependency bumps:

1. Update the fixture dependency.
2. Run the SDK fixture locally enough to verify package metadata loading.
3. Update the expected SDK version in the acceptance wrapper only in the same
   change that updates the dependency.
4. Dispatch `Runner Spike Delegated` with
   `gates=openai-agents-kernel-policy` and `build_ebpf=true`.
5. Merge only after the delegated gate proves SDK, policy, and kernel
   correlation remains byte-stable over three runs.

Version bumps must not silently relax schema validation or determinism
assertions.
