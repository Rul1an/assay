# ADR-025: Evidence-as-a-Product — Reliability Surfaces, Completeness/Closure, and Portable Verifiability

## Status
Proposed (Feb 2026)

## Context
The agent engineering market (2026) has commoditized "eval CI" and "observability". Differentiation now comes from:

1.  **Multi-run reliability** as a first-order property (pass^k, stress/fault surfaces).
2.  **Auditability** as a sociotechnical system (tamper-evident + governance context, not just logs).
3.  **Security** via enforcement points (policy enforcement + evidence emission).
4.  **Standard-first interoperability** (OTEL GenAI + MCP semconv).
5.  **Attestation/transparency stacks** as evidence substrate (in-toto/DSSE/SCITT/Sigstore/SLSA).
6.  **Compliance hooks** (EU AI Act Art 12/19, OWASP Agentic Top 10).
7.  **CI gates** as a commodity integration surface (Actions/Evals), not a differentiator.

Assay’s wedge is **portable, verifiable “evidence primitives”** + **policy packs** + **stability assurance** (pass^k) + **closure/confidence**.

## Decision

### Track 1 — Reliability Surface (pass^k + faults) as Evidence
We introduce **Soak/Surface** as the primary simulation product:
- `assay sim soak` executes N runs (seeded), collecting policy outcomes, infra errors, and summary metrics.
- **Pass^k Semantics**: `pass_all` (AND over k runs) is the strict assurance bar. We also report `pass_rate` and `pass_probability_estimate` (beta posterior or CI95) for statistical confidence.
- **Decision Policy**: User defines strictness via `decision_policy`: `{ stop_on_violation: bool, max_failures: u32, min_runs: u32 }`.

**Normative**: `pass^k` and drift are first-class outputs. Soak reports must include a `decision_policy` used to reach the verdict.

### Track 2/3 — Evidence Completeness + Closure Score (audit + replay readiness)
We distinguish between **Completeness** (Pack-Relative) and **Closure** (Replay-Relative):

1.  **Completeness** ("Did we capture what the Pack needs?"):
    -   Defined relative to a *specific pack*.
    -   Signals are defined in a canonical registry (e.g., `policy_decisions`, `tool_calls`).
    -   State: `captured` (present), `redacted` (removed but committed with hash/metadata), `unknown` (missing/undetectable).

2.  **Closure** ("Can we reconstruct the run?"):
    -   Defined relative to *replayability*.
    -   Score is deterministic (0.0-1.0) based on presence of replay-critical signals (inputs, model ID, tool outputs, RNG seeds).
    -   **Normative**: Score must be calculated from the bundle contents alone (no heuristics).

### Track 4 — Pack "Required Signals" Registry
To prevent schema drift, we introduce a namespaced, additive field for packs:
-   **Field**: `x-assay.requires_signals` (v0).
-   **Registry**: Packs must select from a canonical list of signal types (e.g., `policy_decisions`, `tool_io_bodies`, `model_identity`, `prompt_lineage`, `human_approvals`).
-   This avoids free-text requirements and enables automated completeness checks.

### Track 5 — Standard-first export (OTEL GenAI + MCP)
We support a dual-format approach with a strict versioning policy:
1.  **Target**: GenAI semconv (stable opt-in) + MCP semconv.
2.  **Policy**: Best-effort translation.
3.  **Transparency**: Reports must include a `mapping_loss` section detailing dropped attributes and unknown events.

### Track 6 — Attestation Envelope (Portable Verifiability)
We evolve the Evidence Bundle towards an "attestation bundle" using **DSSE envelopes**.
-   **Payload v1**:
    -   Digests of bundle artifacts (manifest, events).
    -   Pack versions + mapping references.
    -   Closure report digest.
-   **Verification**: OSS capability for offline verification (providing public key).
-   **Threat Model**: Protects against tampering (integrity) and repudiation (provenance). Does *not* guarantee confidentiality (payload content).

## Data Contracts (Normative)

### 1) Soak Report v1 (Normative)
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://assay.dev/schemas/soak-report-v1.schema.json",
  "title": "Assay Soak Report v1",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schema_version",
    "mode",
    "iterations",
    "seed",
    "time_budget_secs",
    "limits",
    "packs",
    "results"
  ],
  "properties": {
    "schema_version": {
      "type": "string",
      "const": "soak-report-v1"
    },
    "mode": {
      "type": "string",
      "const": "soak"
    },
    "generated_at": {
      "type": "string",
      "format": "date-time"
    },
    "assay_version": {
      "type": "string",
      "minLength": 1
    },
    "suite": {
      "type": "string",
      "minLength": 1,
      "description": "Optional tier/name, e.g. quick/nightly or a named soak profile."
    },
    "iterations": {
      "type": "integer",
      "minimum": 1
    },
    "seed": {
      "type": "integer",
      "minimum": 0
    },
    "time_budget_secs": {
      "type": "integer",
      "minimum": 1
    },
    "limits": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "max_bundle_bytes",
        "max_decode_bytes",
        "max_manifest_bytes",
        "max_events_bytes",
        "max_events",
        "max_line_bytes",
        "max_path_len",
        "max_json_depth"
      ],
      "properties": {
        "max_bundle_bytes": { "type": "integer", "minimum": 1 },
        "max_decode_bytes": { "type": "integer", "minimum": 1 },
        "max_manifest_bytes": { "type": "integer", "minimum": 1 },
        "max_events_bytes": { "type": "integer", "minimum": 1 },
        "max_events": { "type": "integer", "minimum": 1 },
        "max_line_bytes": { "type": "integer", "minimum": 1 },
        "max_path_len": { "type": "integer", "minimum": 1 },
        "max_json_depth": { "type": "integer", "minimum": 1 }
      }
    },
    "packs": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/pack_ref" }
    },
    "decision_policy": {
      "type": "object",
      "additionalProperties": false,
      "required": ["pass_on_severity_at_or_above"],
      "properties": {
        "pass_on_severity_at_or_above": {
          "type": "string",
          "enum": ["info", "warning", "error"],
          "description": "Defines what counts as a failing rule severity threshold."
        },
        "stop_on_first_failure": {
          "type": "boolean",
          "default": false
        },
        "max_failures": {
          "type": "integer",
          "minimum": 1,
          "description": "Optional early-stop threshold."
        }
      }
    },
    "results": {
      "type": "object",
      "additionalProperties": false,
      "required": [
        "runs",
        "passes",
        "failures",
        "infra_errors",
        "pass_rate",
        "pass_all"
      ],
      "properties": {
        "runs": { "type": "integer", "minimum": 1 },
        "passes": { "type": "integer", "minimum": 0 },
        "failures": { "type": "integer", "minimum": 0 },
        "infra_errors": { "type": "integer", "minimum": 0 },
        "pass_rate": {
          "type": "number",
          "minimum": 0,
          "maximum": 1
        },
        "pass_all": {
          "type": "boolean",
          "description": "True iff all runs passed under the decision policy."
        },
        "first_failure_at": {
          "type": ["integer", "null"],
          "minimum": 1,
          "description": "1-based index of first failing run, or null if none."
        },
        "violations_by_rule": {
          "type": "object",
          "additionalProperties": {
            "type": "integer",
            "minimum": 1
          },
          "description": "Map from canonical rule id (pack@ver:rule) to count of runs where it violated."
        },
        "infra_errors_by_kind": {
          "type": "object",
          "additionalProperties": {
            "type": "integer",
            "minimum": 1
          },
          "description": "Optional breakdown, e.g. time_budget_exceeded, subprocess_failed, io_error."
        },
        "pass_rate_ci95": {
          "type": "array",
          "minItems": 2,
          "maxItems": 2,
          "items": { "type": "number", "minimum": 0, "maximum": 1 },
          "description": "Optional 95% CI for pass_rate; implement as Wilson or Beta posterior interval."
        }
      }
    },
    "runs": {
      "type": "array",
      "items": { "$ref": "#/$defs/run_result" },
      "description": "Optional per-run detail; can be omitted for compact reports."
    }
  },
  "$defs": {
    "pack_ref": {
      "type": "object",
      "additionalProperties": false,
      "required": ["name", "version"],
      "properties": {
        "name": { "type": "string", "minLength": 1 },
        "version": { "type": "string", "minLength": 1 },
        "kind": { "type": "string", "minLength": 1 },
        "digest": { "type": "string", "minLength": 1 },
        "source": {
          "type": "string",
          "description": "Optional URI/path for provenance (built-in, local, url)."
        }
      }
    },
    "run_result": {
      "type": "object",
      "additionalProperties": false,
      "required": ["index", "status", "duration_ms"],
      "properties": {
        "index": { "type": "integer", "minimum": 1 },
        "status": {
          "type": "string",
          "enum": ["pass", "fail", "infra_error"]
        },
        "duration_ms": { "type": "integer", "minimum": 0 },
        "violated_rules": {
          "type": "array",
          "items": { "type": "string", "minLength": 1 },
          "description": "Canonical rule ids (pack@ver:rule) that violated in this run."
        },
        "infra_error_kind": {
          "type": "string",
          "minLength": 1
        },
        "infra_error_message": {
          "type": "string"
        }
      }
    }
  }
}
```

### 2) Completeness + Closure v1 (Normative)
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://assay.dev/schemas/closure-v1.schema.json",
  "title": "Assay Completeness + Closure v1",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "completeness", "closure"],
  "properties": {
    "schema_version": {
      "type": "string",
      "const": "closure-v1"
    },
    "generated_at": {
      "type": "string",
      "format": "date-time"
    },
    "bundle_digest": {
      "type": "string",
      "minLength": 1,
      "description": "Optional sha256 (or similar) digest of the evidence bundle for linking."
    },
    "pack_context": {
      "type": "array",
      "items": { "$ref": "#/$defs/pack_ref" },
      "description": "Optional: the packs used to compute required signals."
    },
    "completeness": { "$ref": "#/$defs/completeness" },
    "closure": { "$ref": "#/$defs/closure" }
  },
  "$defs": {
    "pack_ref": {
      "type": "object",
      "additionalProperties": false,
      "required": ["name", "version"],
      "properties": {
        "name": { "type": "string", "minLength": 1 },
        "version": { "type": "string", "minLength": 1 },
        "kind": { "type": "string", "minLength": 1 },
        "digest": { "type": "string", "minLength": 1 }
      }
    },
    "signal": {
      "type": "string",
      "pattern": "^[a-z0-9][a-z0-9_\\\\.-]*[a-z0-9]$",
      "description": "Canonical signal key. Prefer a registry to avoid drift."
    },
    "completeness": {
      "type": "object",
      "additionalProperties": false,
      "required": ["required", "captured", "redacted", "unknown"],
      "properties": {
        "required": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "captured": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "redacted": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "unknown": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "by_signal": {
          "type": "object",
          "additionalProperties": { "$ref": "#/$defs/signal_detail" },
          "description": "Optional per-signal detail (why missing, where expected)."
        }
      }
    },
    "signal_detail": {
      "type": "object",
      "additionalProperties": false,
      "required": ["status"],
      "properties": {
        "status": {
          "type": "string",
          "enum": ["captured", "redacted", "missing", "unknown"]
        },
        "reason": { "type": "string" },
        "evidence_paths": {
          "type": "array",
          "items": { "type": "string", "minLength": 1 },
          "description": "JSON pointer(s) or path hints where the signal should be found."
        },
        "commitment": {
          "type": "object",
          "additionalProperties": false,
          "required": ["alg", "digest"],
          "properties": {
            "alg": { "type": "string", "minLength": 1 },
            "digest": { "type": "string", "minLength": 1 },
            "size_bytes": { "type": "integer", "minimum": 0 }
          },
          "description": "For redacted signals: a verifiable commitment (hash/size) without revealing content."
        }
      }
    },
    "closure": {
      "type": "object",
      "additionalProperties": false,
      "required": ["score", "confidence", "captured", "missing"],
      "properties": {
        "score": {
          "type": "number",
          "minimum": 0,
          "maximum": 1
        },
        "confidence": {
          "type": "string",
          "enum": ["low", "medium", "high"]
        },
        "captured": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "missing": {
          "type": "array",
          "items": { "$ref": "#/$defs/signal" }
        },
        "uncontrolled_dependencies": {
          "type": "array",
          "items": { "type": "string", "minLength": 1 },
          "description": "Optional: known nondeterministic inputs (network, live tools) that prevent hermetic replay."
        },
        "scoring": {
          "type": "object",
          "additionalProperties": false,
          "required": ["method", "weights"],
          "properties": {
            "method": {
              "type": "string",
              "enum": ["weighted_ratio_v1"]
            },
            "weights": {
              "type": "object",
              "additionalProperties": {
                "type": "number",
                "minimum": 0
              },
              "description": "Optional: per-signal weights used to compute score."
            }
          },
          "description": "Optional scoring transparency for audits/CI."
        }
      }
    }
  }
}


### 3) Manifest additions
`manifest.json` attributes:
- `x-assay.packs_applied[]`: `{name, version, digest, kind, source_url?}`
- `x-assay.mappings[]`: `{rule, framework, ref}`

### UX/DX Requirements (Feb 2026)
- **Unified Happy Path:**
    - `assay evidence lint <bundle>` (default: lint with `cicd-starter`).
    - `assay sim soak --iterations N --pack <pack> --target <bundle> --report out.json`.
    - **Normative**: Soak must use the *same* pack loader/resolution as Lint.
- **Explainability:**
    - `assay evidence lint --explain closure.score`.
    - `assay evidence lint --explain <pack>:<rule>` (shows missing signals if applicable).
- **Machine-Readable Reports:** ALL commands supporting `--report` must output JSON with `schema_version`. Stdout remains human-readable summary.

## Rollout Plan

### Iteration 1 (MVP): Audit Kit Baseline & Soak MVP
- `assay sim soak` + report v1 (with `decision_policy`, `pass_rate`).
- `manifest.json` `x-assay.*` metadata.
- Pack-provided `x-assay.requires_signals` (minimal registry).

### Iteration 2: Closure Score & Explainability
- Completeness Matrix (Pack-relative) + Closure Score (Replay-relative).
- `redacted` vs `unknown` definitions in lint reporting.
- Advanced `--explain` (closure gaps).

### Iteration 3: Attestation & OTEL
- DSSE envelope generation (opt-in) + offline verify command.
- OTEL export + `mapping_loss` report.

## Open-core Boundary

- **OSS:** Soak MVP, Closure/Completeness v1, Open Packs, OTEL bridge (opt-in), Offline Verification.
- **Pro:** Signing/Attestation Key Mgmt, Enforcement Gateway, Private/Advanced Packs.

## Acceptance Criteria (Gates)

### I1:
- `assay sim soak` with `report.json` containing `schema_version` and valid `pass_rate`/`pass_all`.
- Manifest `x-assay` fields populated correctly.
- Packs in OSS repo define `requires_signals` from v0 registry; parser validates this.

### I2:
- Completeness matrix calculation is deterministic.
- Closure score logic documented and tested with fixed fixtures.
- `--explain` renders closure gaps.

### I3:
- DSSE envelope generation (feature-flagged).
- OTEL export produces valid SemConv + `mapping_loss` section in report.
