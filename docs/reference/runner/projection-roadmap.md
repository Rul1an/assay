# Assay-Runner Projection Roadmap

> **Status:** planning reference, not a shipped contract.
> This page turns the runner-vs-OTel and cross-runtime drift experiments
> into a product-facing projection roadmap. It does not change the Runner
> archive v0 contracts, does not add a public CLI surface, and does not
> promote projection output to primary evidence.

Assay-Runner now has enough measured-run data to show a recurring
product lesson:

```text
Raw evidence is the source of truth. Projection reports make it readable.
```

The next useful work is not to claim more semantic equivalence. It is to
make the boundaries between raw observation, classification, projection,
and policy interpretation explicit enough that reviewers can tell which
claim is being made.

## SOTA Context

The relevant 2026 direction is layered observability:

| Layer | Role | Assay implication |
|---|---|---|
| OTel / OpenInference GenAI traces | Reported control flow, provider and tool attributes such as `gen_ai.provider.name` and `gen_ai.tool.call.id` | Good join vocabulary when present, but not the runtime evidence carrier |
| Runner archives | Linux/eBPF + cgroup measured runtime evidence with health gates | Source of bounded runtime observations |
| Tetragon / Falco / Tracee-style eBPF tooling | Generic syscall, file, process, and network observability | Confirms the domain vocabulary, but usually lacks agent/run/tool context |
| SLSA / in-toto provenance | Content-addressed links between artifacts and claims | Model for digest-bound evidence references, not inline trace payloads |

Assay's product niche is the projection layer above measured runtime
evidence: keep raw observed effects intact, then add narrow, auditable
classification rows that say how a reviewer may compare them.

Reference anchors:

- OpenTelemetry GenAI semantic conventions:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/>
- OpenInference semantic conventions:
  <https://arize-ai.github.io/openinference/spec/semantic_conventions.html>
- Tetragon runtime observability:
  <https://tetragon.io/docs>
- Falco syscall field vocabulary:
  <https://falco.org/docs/reference/rules/supported-fields/>
- SLSA provenance model:
  <https://slsa.dev/spec/v1.1/provenance>

## Core Rule

Every projection MUST preserve four things:

1. the raw observed value;
2. the projected value, if any;
3. the rule that produced the projection;
4. the confidence and non-claim attached to that rule.

Projection code MUST NOT silently rewrite raw evidence, infer semantic
workload equivalence, or collapse unknowns into a convenient bucket.

## Claim Levels

Projection output should use explicit claim levels instead of prose-only
qualifiers:

| Claim level | Meaning | Example |
|---|---|---|
| `raw_observed` | The runner observed this value directly inside a clean capture boundary | `open_read:/tmp/run-a/work/fixture-input.txt` |
| `projected_equivalent` | Raw values differ, but a declared projection rule maps both to the same logical role | `workdir/input` on both sides |
| `classified_drift` | Raw or projected values differ and the report can name the class of drift | `provider_api` endpoint differs |
| `inconclusive` | The available evidence is not strong enough for the dimension | Missing SDK events, unknown endpoint, older archive without open metadata |

Reports may include more detailed enums, but they should preserve this
four-level ladder so readers can distinguish fact from projection.

## Phase 1: Path Projection v0

Path projection is the highest-leverage next step. Current drift reports
show too much absolute run-directory noise. The fix is not to remove raw
paths; it is to add a logical path layer above them.

### Shape

```json
{
  "raw_path": "/opt/actions-runner/_work/assay/.../workdir/fixture-input.txt",
  "projected_path": "workdir/input",
  "path_class": "workload_fixture",
  "relation": "inside_run_workdir",
  "rule": "workload_contract_input_path",
  "confidence": "declared",
  "claim_level": "projected_equivalent"
}
```

### Initial path classes

| Class | Meaning |
|---|---|
| `workload_fixture` | Declared input fixture, output fixture, or workload-owned scratch value |
| `runtime_package` | Runtime package files such as `node_modules` |
| `provider_sdk` | Provider SDK files that are not workload-owned |
| `loader` | Dynamic loader, libc, locale, timezone, or interpreter bootstrap behavior |
| `experiment_harness` | Runner, workflow, compare, or test harness plumbing |
| `cache` | Package manager, runtime, or tool cache paths |
| `unknown` | Observed path did not match any declared rule |

### Rules

- Workload-contract paths have the highest confidence: `declared`.
- Run workdir prefix stripping is allowed only when the prefix is
  declared by the workflow or artifact metadata.
- Prefix or substring heuristics must emit `confidence=heuristic`.
- Unknown paths remain `unknown`; they are not failures by default.

### Acceptance Criteria

- Raw path sets remain present in the report.
- Every projected path carries `rule` and `confidence`.
- Projection is idempotent.
- A report can show "raw differs, projected matches" without calling the
  workloads semantically equivalent.

## Phase 2: Runtime / Noise Taxonomy v0

The taxonomy should be designed in parallel with path projection because
path and network projection both need the same classification language.

Implementation note: the cross-runtime drift comparator now emits this
taxonomy as vocabulary-only metadata. It validates declared projection
classes and preserves `unknown`, but it does not yet infer taxonomy
classes from paths or endpoints.

Initial taxonomy:

| Category | Applies to | Notes |
|---|---|---|
| `workload_fixture` | Paths, operations | Declared experiment input/output/scratch |
| `runtime_package` | Paths | Language runtime and package tree behavior |
| `provider_sdk` | Paths, endpoints, SDK events | Provider client implementation behavior |
| `loader` | Paths, process events | Dynamic loader and interpreter setup |
| `experiment_harness` | Paths, process events, metadata | Runner/workflow/comparator plumbing |
| `provider_api` | Network | Model provider API traffic |
| `dns` | Network | DNS lookup traffic when visible |
| `telemetry` | Network | SDK/runtime telemetry not needed for task result |
| `package_fetch` | Network | Package registry or dependency download traffic |
| `unknown` | All | Explicitly unclassified |

Taxonomy strings are reviewer vocabulary, not policy verdicts. A
`provider_sdk` difference may be expected, suspicious, or irrelevant
depending on the policy layer that consumes the report.

## Phase 3: Report Provenance Metadata

Report-level provenance should land early because it improves every
later experiment at low implementation cost.

Implementation note: the cross-runtime drift comparator now emits a
`provenance` block in JSON reports. Archive manifest digests, schema
versions, observation-health gates, and correlation status are derived
from the two input archives. Workflow URL, runner label, kernel tuple,
Assay version/commit, and eBPF object digest are caller-supplied anchors
and stay `null` when not provided.

Minimum metadata block:

```json
{
  "assay_version": "3.x.y",
  "assay_commit": "sha",
  "runner_schema_versions": {
    "archive": "assay.runner.archive.v0",
    "kernel_event": "assay.runner.kernel_event.v0"
  },
  "kernel": {
    "os": "linux",
    "release": "6.8.0-117-generic",
    "arch": "aarch64"
  },
  "ebpf_object_digest": "sha256:...",
  "workflow": {
    "url": "https://github.com/Rul1an/assay/actions/runs/...",
    "runner_label": "assay-bpf-runner"
  },
  "input_archives": [
    {
      "run_id": "run_arm_a_...",
      "manifest_digest": "sha256:..."
    }
  ]
}
```

The metadata block is for anchoring and reproducibility. It does not
upgrade a projection into primary evidence.

## Phase 4: Network Projection v0

Network projection should follow path projection. Keep the taxonomy
small and allow `unknown`.

Implementation note: the cross-runtime drift comparator now supports
declared exact endpoint aliases and CIDR aliases. Raw endpoints remain
unchanged in the report; projection is additive and unmatched endpoints
stay `unknown`.

### Shape

```json
{
  "projection": {
    "schema": "assay.runner.network_projection.v0",
    "status": "applied",
    "dimension": "network_endpoints",
    "claim_level": "projected_equivalent",
    "in_both": ["provider_api"],
    "rules": ["declared_network_cidr_alias"],
    "mappings": [
      {
        "side": "a",
        "raw_value": "34.120.10.20:443",
        "projected_value": "provider_api",
        "network_class": "provider_api",
        "relation": "declared_cidr",
        "rule": "declared_network_cidr_alias",
        "confidence": "declared",
        "claim_level": "projected_equivalent"
      }
    ]
  }
}
```

### Initial endpoint classes

| Class | Meaning |
|---|---|
| `provider_api` | Model/provider API endpoint |
| `dns` | DNS lookup endpoint |
| `telemetry` | Observability or SDK telemetry endpoint |
| `package_fetch` | Dependency/package retrieval endpoint |
| `unknown` | Endpoint could not be safely classified |

IP-only endpoints should remain `unknown` unless the report can name the
classification rule. DNS, SNI, SDK metadata, or an explicit allowlist may
raise confidence, but the report must say which source was used.

## Phase 5: Runtime Drift Projection Artifact

Path projection, network projection, taxonomy, and metadata are now
stable enough to freeze the first projection artifact shape.

Implemented schema:

```text
assay.runner.runtime_drift.v0
```

This is a projection artifact, not a primary evidence artifact. It reads
Runner archives and emits a comparison report. The contract is documented
in [`runtime-drift-v0.md`](runtime-drift-v0.md), with a machine-readable
schema at
[`schema/runtime-drift-v0.schema.json`](schema/runtime-drift-v0.schema.json).

Required design properties:

- input archive manifest digests are recorded;
- raw and projected views are both present;
- every projection row carries rule and confidence;
- health gates are copied from the source archives;
- `non_claims` are machine-readable;
- policy acceptability is out of scope.

Example row:

```json
{
  "dimension": "kernel_file_operations",
  "classification": "runtime-induced",
  "claim_level": "projected_equivalent",
  "reason": "raw absolute paths differ; projected fixture operations match",
  "raw": {
    "base_only": ["open_read:/tmp/a/workdir/fixture-input.txt"],
    "head_only": ["open_read:/tmp/b/workdir/fixture-input.txt"]
  },
  "projected": {
    "unchanged": ["open_read:workdir/input"]
  },
  "rules": ["workload_contract_input_path"],
  "non_claims": [
    "projection_no_semantic_workload_equivalence",
    "projection_no_policy_acceptability_verdict"
  ]
}
```

## Phase 6: Kernel Event Metadata Schema

Kernel expansion is valuable, but it should come after the projection
layer is readable. That condition is now met by
[`runtime-drift-v0.md`](runtime-drift-v0.md). The first kernel follow-up
is deliberately small: freeze the current enriched `kernel_event.v0`
line shape in
[`schema/kernel-event-v0.schema.json`](schema/kernel-event-v0.schema.json)
so consumers can validate whether an archive supports operation-aware
open projections.

The open metadata added to kernel event v0 already supports
operation-aware projections for `openat` and `openat2` calls. Successful
calls can project access and operation hints:

- `read`;
- `write`;
- `read_write`;
- `create`;
- `truncate`;
- `append`.

Failed calls are represented separately by the kernel event `status`
field (`status=error`) derived from the syscall return value. `error` is
not an operation class alongside `read` or `write`.

Future kernel-event expansion should consider:

| Candidate | Why | Caution |
|---|---|---|
| `unlinkat` | Remove/delete semantics | Path resolution and policy interpretation need care |
| `renameat` / `renameat2` | Move/replace semantics | Needs old/new path modeling |
| `mkdirat` / `rmdir` | Directory effects | May add loader/cache noise |
| fd-level `read` / `write` byte counts | Stronger "content read/written" claims | High volume, fd state, aggregation, and privacy risk |

Do not add fd-level byte semantics until the report can keep them
separate from open-intent semantics. `open_read` is not the same claim as
"bytes were read from this file."

## Non-Claims

Projection reports MUST carry explicit non-claims. Initial codes:

| Code | Meaning |
|---|---|
| `projection_no_raw_evidence_rewrite` | Raw observed evidence is preserved and remains source of truth |
| `projection_no_semantic_workload_equivalence` | Matching projected values do not prove the workloads are semantically identical |
| `projection_no_policy_acceptability_verdict` | The report does not decide whether drift is acceptable |
| `projection_unknowns_preserved` | Unknown or unclassified values are not collapsed into a class |
| `projection_confidence_is_not_truth` | Confidence describes the rule source, not absolute correctness |

## Suggested Slice Sequence

| Slice | Deliverable | Gate |
|---|---|---|
| 1 | Path projection helper over existing cross-runtime drift fixtures | Unit tests with raw/projected dual view |
| 2 | Runtime/noise taxonomy constants and docs | Unknown-preserving tests |
| 3 | Report provenance metadata block | Existing live run can be re-rendered with metadata |
| 4 | Network projection helper | Exact endpoint, CIDR, and unknown fallback tests |
| 5 | Freeze `assay.runner.runtime_drift.v0` projection schema | Synthetic fixture covers all claim levels + schema sidecar matches comparator output |
| 6 | Kernel event metadata schema sidecar | Enriched open metadata line shape is machine-readable; no new syscall claims |

## Relationship To Existing Contracts

- [`artifacts-v0.md`](artifacts-v0.md) remains the archive evidence
  contract. Projection output reads those artifacts; it does not replace
  them.
- [`cross-runtime-diff-v0.md`](cross-runtime-diff-v0.md) remains the
  narrow Phase 2C capability-surface projection with work-dir prefix
  canonicalization only.
- The cross-runtime drift experiment is an exploratory implementation
  proving why richer projection layers are useful.
