# ADR-025 E2: Manifest x-assay Extensions

**Status:** Proposed (Feb 2026)
**Epic:** E2 — Producer/consumer split in manifest.json
**Refs:** [ADR-025 Epics](./ADR-025-Epics.md), [ADR-025 Evidence-as-a-Product](./ADR-025-Evidence-as-a-Product.md)

**Revision (2026-02):** Merge-blockers addressed: (1) schema_version aligned with EVIDENCE-CONTRACT-v1, (2) digest canonicalization normatief (RFC 8785 + vendored pack bytes), (3) sidecar path/naming convention, (4) subject[] attestation-shaped, (5) bundle_finalized + authoritative anchor. Nits: PII/secrets, pack digest semantics, serde flatten voor roundtrip.

---

## Goal

Provenance in manifest.json with clear **producer vs consumer** split. Producer metadata is immutable; consumer evaluations live as **sidecar attestations** to preserve tamper-evidence (DSSE/SCITT ready).

| Layer | Field | Content |
|-------|-------|---------|
| **Producer** | `x-assay.bundle_provenance` | toolchain, model identity, run_id |
| **Consumer** | Sidecar `evaluation.json` / `--emit-eval` | packs_applied, decision_policy, results_digest, bundle_digest |

### Tamper-Evidence Constraint

**Producer manifest MUST remain immutable** once bundle is final. Mutating manifest.json for consumer evaluations would change bundle bytes → digest/root/signature changes → breaks DSSE/SCITT.

**Consumer evaluations** therefore land as:
- **Optie A (aanbevolen):** Sidecar `evaluation.json` with `bundle_digest` + `results_digest`; verifiable overlay.
- **Optie B:** Future: DSSE-signed attestation envelope.

---

## manifest.json — x-assay extension v1 (normatief)

### Top-level contract

- **manifest.schema_version:** Remains as defined in [EVIDENCE-CONTRACT-v1](../spec/EVIDENCE-CONTRACT-v1.md): integer `1` only. Do NOT change.
- **x-assay.schema_version:** MUST be present when x-assay is present; always string, e.g. `"x-assay-ext-v1"`.
- **Merge gate:** manifest uses integer; x-assay uses string. Never mix; tooling MUST validate accordingly.
- `x-assay` is optional and additive.
- `x-assay.extensions` holds forward-compat unknown keys; consumers MUST preserve on roundtrip.
- Future x-assay fields use stable names; experimental/iterative fields go in `extensions` or `x-assay.experimental` to avoid key collision.

### Authoritative digest anchor + finalization

- **bundle_digest**: Logical digest sha256(JCS(run_root, algorithms, files)). Not the tar.gz bytes; avoids circularity. `files` MUST include at least `events.ndjson`; manifest is excluded. This is the content-identity anchor for integrity and provenance.
- `x-assay.bundle_provenance.evidence.bundle_digest` MUST equal this value.
- **bundle_finalized:** When present and `true`, indicates the bundle is immutable (exported/created). Tooling MAY treat absence as unfinalized; producers MUST set `bundle_finalized: true` on export.
- When a bundle is finalized, it is immutable; no further manifest mutations.

```json
{
  "schema_version": 1,
  "bundle_id": "...",
  "producer": { ... },
  "run_id": "...",
  "event_count": 42,
  "run_root": "...",
  "algorithms": { ... },
  "files": { ... },
  "x-assay": {
    "schema_version": "x-assay-ext-v1",
    "bundle_finalized": true,
    "bundle_provenance": { },
    "extensions": {}
  }
}
```

**Note:** `evaluations[]` is NOT in manifest (tamper-evidence). Evaluations live in sidecar.

---

## x-assay.bundle_provenance (producer-side)

### JSON shape v1

```json
{
  "created_at": "2026-02-11T10:31:42Z",
  "producer": {
    "name": "assay-cli",
    "version": "2.11.0",
    "build": {
      "git_commit": "9b8f0c1",
      "dirty": false
    }
  },
  "evidence": {
    "schema_version": "evidence-bundle-v1",
    "format": "tar.gz",
    "bundle_digest": "sha256:..."
  },
  "run": {
    "run_id": "a2b3c4d5-...",
    "assayrunid": "AR_01H..."
  },
  "model": {
    "provider": "openai",
    "model_id": "gpt-5.2",
    "config_digest": "sha256:...",
    "parameters": { "temperature": 0.0 }
  },
  "source": {
    "repo_url": "https://github.com/acme/agent",
    "git_ref": "refs/heads/main",
    "commit": "9b8f0c1"
  },
  "environment": {
    "ci_provider": "github-actions",
    "runner_os": "linux",
    "runner_arch": "x86_64"
  }
}
```

### Normatieve regels v1

- `created_at` MUST be RFC3339 UTC.
- `producer.name` / `producer.version` MUST be present.
- `evidence.bundle_digest` MUST be deterministic. v1 uses **logical digest**: sha256(JCS(run_root, algorithms, files)). No circular reference; byte-for-byte reproducible. `files` MUST include at least `events.ndjson`; manifest is excluded to avoid circularity. **bundle_digest** thus binds to the events stream + integrity root.
- `created_at` MUST be RFC3339 UTC. For determinism: caller supplies or writer uses first event time (not Utc::now()).
- `run.run_id` or `run.assayrunid` MUST be present (minstens één).
- `model.config_digest` MUST be digest over canonical model config (**no secrets/PII**).
- `source`, `environment` optional but aanbevolen.
- **Secrets/PII (normatief):** `producer.build.dirty` OK. `command.args` MUST be scrubbed (no tokens, headers, file paths with secrets). `environment` only coarse-grained (ci_provider, runner_os, runner_arch); no hostname, user, or PII.

---

## Consumer evaluation (sidecar)

### Sidecar location + naming (normatief)

**Default path:** `./assay-evaluations/<bundle_digest_hex>/<evaluation_id>.json`

- `bundle_digest_hex`: SHA-256 hex (without `sha256:` prefix) of full bundle.
- `evaluation_id`: UUID from evaluation.

**CLI:**
- `--emit-eval <path>`: write to file; `-` = stdout.
- `--emit-eval-dir <dir>`: write into dir with default filename `<evaluation_id>.json`.
- Stderr line after lint/soak: `Wrote evaluation sidecar: <path> (bundle sha256:...)`

### UX

```bash
assay evidence lint bundle.tar.gz --emit-eval eval.json
assay evidence lint bundle.tar.gz --emit-eval -           # stdout
assay evidence lint bundle.tar.gz --emit-eval-dir ./evals # default naming
assay sim soak --target bundle.tar.gz --emit-eval soak-eval.json
```

### JSON shape v1 (evaluation.json)

```json
{
  "schema_version": "evaluation-v1",
  "evaluation_id": "b7ff8d0f-3b6a-45c0-9a54-4e7c3c3a1a2d",
  "created_at": "2026-02-11T10:35:10Z",
  "subject": [
    { "name": "bundle.tar.gz", "digest": { "sha256": "<hex>" } },
    { "name": "manifest.json", "digest": { "sha256": "<hex>" } }
  ],
  "actor": {
    "type": "ci",
    "id": "github-actions:acme/agent:workflow=lint.yml:run=12345",
    "display": "CI lint gate"
  },
  "command": {
    "name": "assay evidence lint",
    "version": "2.11.0",
    "args": ["bundle.tar.gz", "--pack", "cicd-starter"]
  },
  "inputs": {
    "bundle_digest": "sha256:...",
    "manifest_digest": "sha256:...",
    "packs_applied": [
      {
        "name": "cicd-starter",
        "version": "1.0.0",
        "kind": "quality",
        "digest": "sha256:...",
        "source": "builtin"
      }
    ],
    "decision_policy": { "pass_on_severity_at_or_above": "warning" },
    "limits": { "digest": "sha256:...", "effective": { ... } },
    "seed": 42,
    "time_budget_secs": 60
  },
  "outputs": {
    "status": "pass",
    "summary": { "errors": 0, "warnings": 2, "info": 1 },
    "results_digest": "sha256:...",
    "report": {
      "schema_version": "lint-report-v1",
      "path": "reports/lint-20260211T103510Z.json",
      "digest": "sha256:..."
    }
  }
}
```

### Normatieve regels v1

- `subject` MUST be present (attestation-shaped; DSSE migration ready).
- `subject` entries: at least `bundle.tar.gz` and `manifest.json` digests.
- `evaluation_id` MUST be UUID.
- `created_at` MUST be RFC3339 UTC.
- `inputs.bundle_digest` MUST be present; MUST match evaluated bundle.
- `inputs.packs_applied[].digest` MUST be present.
- `outputs.results_digest` MUST be present (digest over RFC 8785 JCS canonical report JSON).
- `outputs.status` ∈ `pass` | `fail` | `infra_error`.
- `outputs.report.path` optional (often useless when report is CI artifact); future: consider `uri` or `artifact_ref` for CI run URLs.
- `limits.effective`: only fields that deviate from default + digest of full config; avoids large payloads.

### Digest canonicalization (normatief v1)

**Merge gate:** Implementations MUST follow this; tooling/interops break otherwise.

- **JSON digests** (`results_digest`, `limits.digest`, `manifest_digest`): RFC 8785 (JCS) canonicalization.
- **Pack digests:** SHA-256 over **exact vendored pack bytes** — the pack.yaml file as read from `packs/<name>/pack.yaml` (before any include expansion). No canonical-YAML; use raw file bytes for stability. One meaning only; document and stick to it.

---

## Implementation Phases

### Phase 1: Schema + preservation

- Add `XAssayExtension` struct: `schema_version`, `bundle_finalized`, `bundle_provenance`, `extensions` (forward-compat).
- Add to assay-evidence Manifest (optional `#[serde(rename = "x-assay")]`).
- Reader/writer accept and preserve x-assay.
- **Unknown keys:** x-assay MUST use `#[serde(flatten)]` with `BTreeMap<String, Value>` for unknown top-level fields under x-assay, so future fields are roundtrip-preserved. (`extensions` alone only preserves keys inside extensions, not future top-level x-assay fields.)
- **Roundtrip test:** bundle with x-assay + `extensions.future_field` → read → write → deep equal.

### Phase 2: Producer population

- evidence export / bundle create: populate `bundle_provenance` from ProducerMeta + toolchain.
- **Producer scope:** Only where bundles are actually created (writer usage). NOT in lint/soak (those consume bundles).

### Phase 3: Consumer sidecar (niet manifest-mutation)

- `--emit-eval <path|-|dir>` on lint + soak; `--emit-eval-dir` for CI.
- Evaluation contains `subject[]`, `bundle_digest`, `manifest_digest`, `results_digest`, packs_applied.
- `assay evidence verify --eval eval.json` (MVP): check bundle_digest match, results_digest match, pack digests if bytes available. Enables sidecars in CI with one command.

---

## Merge Gates Phase 2 (producer bundle_provenance)

- [ ] No secrets/PII: `model.config_digest` = digest over scrubbed canonical config; no tokens/headers in `command.args`.
- [ ] Digest anchor unambiguous: `bundle_digest` = sha256 over full `.tar.gz` bytes.
- [ ] Timestamps: `created_at` RFC3339 UTC, parseable.
- [ ] Stability: existing fields may not be renamed without schema bump.

## Merge Gates Phase 3 (consumer sidecar --emit-eval)

- [ ] **No bundle mutation:** CI test: lint/soak with `--emit-eval` MUST NOT change bundle hash.
- [ ] Canonical digest: `results_digest` via RFC 8785/JCS; same report → same digest (test it).
- [ ] Verification: `assay evidence verify --eval eval.json --bundle bundle.tar.gz` checks: bundle_digest match, pack digests (when bytes available), results_digest match.

---

## Merge Gates Phase 1

- [ ] **Reserved keys:** `extra` MUST NOT contain reserved keys (schema_version, bundle_finalized, bundle_provenance, extensions); validate before serialize.
- [ ] **extensions vs extra:** Use extensions for future keys; extra is preservation-only (doc + unit test).
- [ ] **schema_version:** manifest.schema_version integer 1 (per EVIDENCE-CONTRACT-v1); x-assay.schema_version string `"x-assay-ext-v1"`; never mixed.
- [ ] **Digest canonicalization:** RFC 8785 JCS for JSON; pack digest = sha256 over exact vendored pack.yaml bytes (pre-include-expansion).
- [ ] **Sidecar path/naming:** default `./assay-evaluations/<bundle_digest_hex>/<evaluation_id>.json`; CLI flags `--emit-eval`, `--emit-eval-dir` documented.
- [ ] **subject[]:** Evaluation MUST include subject[] with bundle.tar.gz + manifest.json digests (attestation-shaped; DSSE-ready).
- [ ] **Authoritative anchor + finalization:** bundle_digest of full .tar.gz is anchor; `bundle_finalized: true` set on export.
- [ ] Schema in place; roundtrip preserves x-assay.
- [ ] Roundtrip preserves unknown keys (serde flatten + BTreeMap for x-assay top-level).
- [ ] Verify/lint/soak accept bundles with x-assay.
- [ ] No breaking changes to existing bundles.
- [ ] Design gate: Phase 3 does NOT mutate producer artifact.

---

## Acceptance Criteria (epic-level)

- [ ] manifest.json supports `x-assay` (optional, additive)
- [ ] `x-assay.bundle_provenance` when bundle created (Phase 2)
- [ ] Consumer evaluations via sidecar `--emit-eval` (Phase 3), NOT manifest mutation
- [ ] Digest format: `sha256:hex`
- [ ] Backward compatible: bundles without x-assay remain valid
