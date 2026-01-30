# Replay Bundle Specification v1

**Status:** Draft
**Version:** 1.0.0
**Date:** 2026-01
**ADR:** [ADR-019: PR Gate 2026 SOTA §5 Replay Bundle](./ADR-019-PR-Gate-2026-SOTA.md#5-replay-bundle-lightweight-differentiator)

---

## 1. Overview

This specification defines the **Replay Bundle**: a lightweight artifact that captures enough context to reproduce a run locally for support and DX. It is not a full provenance platform (SLSA/in-toto); it is a stepping stone so that "send bundle" → reproduce exactly, and PR summary can offer "Reproduce locally" without SaaS.

### Design Principles

- **Minimal** — Only what is needed to rerun and compare: config/policy/baseline digests, trace reference or digest, outputs, env.
- **Best-effort deterministic** — Replay may not be byte-identical (e.g. judge variance); the goal is "same inputs → same conclusions" where deterministic; for judge, record/replay of outputs is optional.
- **Support/DX first** — Primary use: support ("stuur bundle") and PR summary ("Reproduce locally"). No attestation or signing required for v1.

### Out of Scope (v1)

- Full supply-chain attestations (SLSA/in-toto) for every run.
- Cryptographic signing of the bundle (future version MAY add).
- Mandate/evidence bundle format (see evidence contract; replay bundle is run/eval-focused).

---

## 2. Bundle Location and Name

- **Default path:** `.assay/replay.bundle` (directory or archive; see §3).
- **Alternative:** Implementations MAY support a custom path via `assay replay --bundle <path>`.

---

## 3. Bundle Format

The bundle MAY be either:

- **Option A — Directory:** A directory named `replay.bundle` (or the path given to `--bundle`) containing a manifest file and referenced files.
- **Option B — Archive:** A single file (e.g. `.tar.gz` or `.zip`) that expands to the same structure as Option A.

**Normative:** The bundle MUST contain a **manifest** file that describes contents and digests. The manifest MUST be named `manifest.json` and MUST be at the root of the bundle (directory root or archive root after extraction).

### 3.1 Manifest Schema (manifest.json)

| Field               | Type   | Required | Description |
|---------------------|--------|----------|-------------|
| `schema_version`    | integer| **Yes**  | Version of this manifest schema. MUST be `1` for this spec. |
| `created_at`        | string | No       | ISO 8601 UTC timestamp when the bundle was created. |
| `assay_version`     | string | **Yes**  | Assay CLI version that produced the run (e.g. `"2.12.0"`). |
| `config_digest`     | string | No       | Digest of config file used (e.g. `sha256:...`). |
| `policy_digest`     | string | No       | Digest of policy/pack used (e.g. `sha256:...`). |
| `baseline_digest`   | string | No       | Digest of baseline used, if applicable. |
| `trace_digest`      | string | No       | Digest of trace input (or primary trace), if included or referenced. |
| `trace_path`        | string | No       | Relative path inside bundle to trace file(s), or pointer (e.g. `traces/run.jsonl`). |
| `outputs`           | object | No       | Paths or digests of outputs; see §3.2. |
| `env`               | object | No       | Environment metadata (e.g. `runner`, `os`); free-form. |

### 3.2 outputs Object

| Field        | Type   | Required | Description |
|--------------|--------|----------|-------------|
| `junit`     | string | No       | Relative path inside bundle to junit.xml (e.g. `reports/junit.xml`). |
| `sarif`     | string | No       | Relative path inside bundle to sarif.json. |
| `summary`   | string | No       | Relative path inside bundle to summary.json. |

Paths are relative to the bundle root. If outputs are inlined by reference only (e.g. digest), the manifest MAY omit paths and only include digests; implementations SHOULD include at least summary path for DX.

### 3.3 Required Contents (Minimum)

- **manifest.json** (with schema_version, assay_version).
- At least one of: config snapshot or config_digest; trace file (at trace_path) or trace_digest; summary.json (at outputs.summary).

**Normative:** A valid v1 bundle MUST have manifest.json with schema_version 1 and assay_version set. It SHOULD include summary.json so that "reproduce locally" can show the original outcome.

### 3.4 Example manifest.json

```json
{
  "schema_version": 1,
  "created_at": "2026-01-28T14:00:00Z",
  "assay_version": "2.12.0",
  "config_digest": "sha256:abc123...",
  "policy_digest": "sha256:def456...",
  "baseline_digest": "sha256:789...",
  "trace_digest": "sha256:012...",
  "trace_path": "traces/run.jsonl",
  "outputs": {
    "junit": "reports/junit.xml",
    "sarif": "reports/sarif.json",
    "summary": "reports/summary.json"
  },
  "env": {
    "runner": "ubuntu-latest",
    "os": "linux"
  }
}
```

---

## 4. Bundle Creation

- **When:** The implementation MAY create a replay bundle after each `assay ci` or `assay run` (e.g. when a non-zero exit occurs, or always, or when requested via a flag such as `--write-replay-bundle`).
- **Where:** Default path `.assay/replay.bundle` (directory or archive); MAY be overridden.
- **What:** Copy or link config (or store digest), policy digest, baseline digest, trace file (or digest + path), and outputs (junit.xml, sarif.json, summary.json) into the bundle; write manifest.json with assay_version and digests.

**Normative:** If bundle creation is implemented, it MUST produce a valid manifest (§3) and MUST include assay_version and at least one of config_digest, trace_path/trace_digest, or outputs.summary.

---

## 5. assay replay --bundle Semantics

### 5.1 Command

```
assay replay --bundle <path>
```

- **path:** Path to the bundle (directory or archive). Default MAY be `.assay/replay.bundle`.

### 5.2 Behaviour

- **Parse manifest:** Read manifest.json; validate schema_version (MUST support 1); load assay_version, config_digest, policy_digest, baseline_digest, trace_path, outputs.
- **Resolve inputs:** Resolve config (from bundle or from digest check); resolve trace from trace_path inside bundle (or fail with clear message if missing).
- **Re-run:** Execute the same logical run (e.g. assay run with same config and trace) in a best-effort deterministic way. For deterministic metrics, results SHOULD match; for judge-based metrics, record/replay of judge outputs is OPTIONAL (implementations MAY replay cached judge results if stored in the bundle).
- **Compare (optional):** Implementation MAY compare new run outputs (e.g. summary) to bundled outputs and report differences.
- **Exit:** Exit code SHOULD reflect success of replay (0 = replay completed; non-zero = replay failed or diff detected, per implementation).

### 5.3 Best-Effort Deterministic

- **Deterministic metrics:** Same config + same trace → same metric results (e.g. regex, json_schema, sequence_valid). Replay SHOULD produce the same outcome for these.
- **Judge metrics:** Judge calls may not be replayed identically (provider variance). Implementations MAY:
  - Skip judge and use cached results from the bundle (if stored), or
  - Re-run judge and accept possible variance, or
  - Mark replay as "partial" when judge was re-run and results differ.

**Normative:** Replay MUST NOT require network or judge calls for deterministic-only replay when cached/bundled results are available. When judge is re-run, implementations SHOULD document that results may differ.

### 5.4 "Reproduce Locally" in PR Summary

The GitHub Action or CI step MAY write a Replay Bundle (e.g. as an artifact) and include in the Check Run Summary a link or instruction such as: "Reproduce locally: download the replay bundle artifact and run `assay replay --bundle ./replay.bundle`." This is the primary DX use case.

---

## 6. Conformance

- **Producers:** Any code that writes a replay bundle MUST follow §3 (manifest schema and required contents). Bundle MUST be readable by the consumer (directory or archive format documented).
- **Consumers:** `assay replay --bundle` MUST accept bundles with schema_version 1 and MUST use manifest.json to resolve config, trace, and outputs. Unknown manifest fields MUST be ignored.
- **Contract tests:** Implementations SHOULD include a test that creates a bundle (after a run) and runs `assay replay --bundle` and verifies that replay completes (and optionally that deterministic results match).

---

## 7. Version History

| schema_version | Date    | Changes |
|----------------|---------|---------|
| 1              | 2026-01 | Initial: manifest schema, bundle format, assay replay --bundle semantics. |

---

## 8. References

- [ADR-019 PR Gate 2026 SOTA §5 Replay Bundle](./ADR-019-PR-Gate-2026-SOTA.md#5-replay-bundle-lightweight-differentiator)
- [SPEC-PR-Gate-Outputs-v1](./SPEC-PR-Gate-Outputs-v1.md) (summary.json and outputs)
