# Replay Bundle Specification v1

**Status:** Draft (Aligned for E9c planning)
**Version:** 1.0.1-draft
**Date:** 2026-02
**ADR:** [ADR-019: PR Gate 2026 SOTA §5 Replay Bundle](./ADR-019-PR-Gate-2026-SOTA.md#5-replay-bundle-lightweight-differentiator)

---

## 1. Overview

This specification defines the **Replay Bundle**: a lightweight artifact that captures enough context to reproduce a run locally for support and DX. It is not a full provenance platform (SLSA/in-toto); it is a stepping stone so that "send bundle" → reproduce exactly, and PR summary can offer "Reproduce locally" without SaaS.

### Design Principles

- **Minimal** — Only what is needed to rerun and compare: config/policy/baseline digests, trace reference or digest, outputs, env.
- **Best-effort deterministic** — Replay may not be byte-identical (e.g. judge variance); the goal is "same inputs → same conclusions" where deterministic; for judge, record/replay of outputs is optional.
- **Support/DX first** — Primary use: support ("stuur bundle") and PR summary ("Reproduce locally"). No attestation or signing required for v1.

### Alignment Note (2026-02)

This spec is aligned to current E9 implementation direction:
- Producer emits a **single canonical archive**.
- Default output path is under `.assay/bundles/` using run_id-based naming.
- Offline replay is hermetic by default; missing dependencies are explicit errors.

### Out of Scope (v1)

- Full supply-chain attestations (SLSA/in-toto) for every run.
- Cryptographic signing of the bundle (future version MAY add).
- Mandate/evidence bundle format (see evidence contract; replay bundle is run/eval-focused).

---

## 2. Bundle Location and Name

- **Default producer path:** `.assay/bundles/<run_id>.tar.gz`.
- **Replay input:** `assay replay --bundle <path>` accepts explicit path.
- **Compatibility:** Implementations MAY support legacy paths (e.g. `.assay/replay.bundle`) for reading, but SHOULD NOT use them as default write target.

---

## 3. Bundle Format

**Normative producer format (v1):**
- Single archive: **`.tar.gz`**
- Canonical layout:
  - `manifest.json`
  - `files/`
  - `outputs/`
  - `cassettes/`

**Normative:** The bundle MUST contain `manifest.json` at archive root.
Consumers MAY support additional legacy/dev formats, but producer output is canonicalized to `.tar.gz`.

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
| `source_run_path`   | string | No       | Path used to select source run when creating bundle (audit). |
| `selection_method`  | string | No       | How source was selected (e.g. `"run-id"` or `"mtime-latest"`). |
| `outputs`           | object | No       | Paths or digests of outputs; see §3.2. |
| `toolchain`         | object | No       | Captured toolchain/runner metadata. |
| `seeds`             | object | No       | order/judge seed values from original run. |
| `replay_coverage`   | object | No       | complete/incomplete tests + reason map. |
| `scrub_policy`      | object | No       | Bundle scrub policy used at creation time. |
| `files`             | object | No       | File manifest map: path -> sha256/size/(mode/content_type). |
| `env`               | object | No       | Environment metadata (legacy/free-form). |

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
- Canonical layout prefixes for data files: `files/`, `outputs/`, `cassettes/`.

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
- **Where:** Default path `.assay/bundles/<run_id>.tar.gz`; MAY be overridden.
- **What:** Copy or include config (or digest), policy digest, baseline digest, trace file (or digest + path), outputs, and scrubbed cassettes; write manifest.json with assay_version and required metadata.

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
- **Re-run:** Execute the same logical run (e.g. assay run with same config and trace) in deterministic replay mode by default.
- **Compare (optional):** Implementation MAY compare new run outputs (e.g. summary) to bundled outputs and report differences.
- **Exit:** Exit code SHOULD reflect success of replay (0 = replay completed; non-zero = replay failed or diff detected).

### 5.3 Offline Hermetic Contract

- **Default mode:** Offline hermetic. Replay MUST NOT perform outbound network.
- **Missing dependencies:** If replay requires missing inputs (e.g. uncached judge response), result MUST be explicit error (`E_REPLAY_MISSING_DEPENDENCY`, exit code 2 in CLI mapping).
- **Live mode:** `--live` MAY allow outbound provider calls; replay provenance SHOULD record live/offline mode and seed overrides.

### 5.4 "Reproduce Locally" in PR Summary

The GitHub Action or CI step MAY write a Replay Bundle (e.g. as an artifact) and include in the Check Run Summary a link or instruction such as: "Reproduce locally: download the replay bundle artifact and run `assay replay --bundle ./replay.bundle`." This is the primary DX use case.

---

## 6. Conformance

- **Producers:** Any code that writes a replay bundle MUST follow §3 (canonical `.tar.gz`, manifest schema, required contents).
- **Consumers:** `assay replay --bundle` MUST accept bundles with schema_version 1 and MUST use manifest.json to resolve config, trace, and outputs. Unknown manifest fields MUST be ignored.
- **Contract tests:** Implementations SHOULD include a test that creates a bundle (after a run) and runs `assay replay --bundle` and verifies that replay completes (and optionally that deterministic results match).

### 6.1 Security/Trust Profile (Recommended)

- Verify manifest-vs-file hashes for all manifest entries.
- Secret scan policy:
  - hard fail on `files/` and `cassettes/`
  - warn on `outputs/`
- Optional (recommended) attestation/signature verification profile for higher-assurance environments.

---

## 7. Version History

| schema_version | Date    | Changes |
|----------------|---------|---------|
| 1              | 2026-01 | Initial: manifest schema, bundle format, assay replay --bundle semantics. |
| 1              | 2026-02 | Alignment update: canonical `.tar.gz` producer format, `.assay/bundles/<run_id>.tar.gz` default, offline hermetic contract. |

---

## 8. References

- [ADR-019 PR Gate 2026 SOTA §5 Replay Bundle](./ADR-019-PR-Gate-2026-SOTA.md#5-replay-bundle-lightweight-differentiator)
- [SPEC-PR-Gate-Outputs-v1](./SPEC-PR-Gate-Outputs-v1.md) (summary.json and outputs)
