# Epic E5 & E8 Verification Report (Step 3 — DX Implementation Plan)

**Date:** 2026-02-02
**Scope:** Epic E5 (Observability & privacy defaults), Epic E8 (OTel GenAI Observability)
**Source:** [DX-IMPLEMENTATION-PLAN.md](../maintainers/DX-IMPLEMENTATION-PLAN.md)
**Test outcomes:** [E5-E8-TEST-REPORT.md](E5-E8-TEST-REPORT.md) §E5, §E8

---

## Summary

| Epic | Story | Status | Evidence |
|------|-------|--------|----------|
| **E5** | E5.1 Privacy default | ✅ Implemented | Default `capture_mode: Off`; no prompt/response in OTel, SARIF, summary |
| **E5** | E5.2 Golden tests | ✅ Implemented | `otel_contract.rs`: Off, BlobRef, RedactedInline invariants |
| **E8** | E8.1 Semconv version gating | ✅ Implemented | `genai_semconv_version`, `SemConvStability`, `GenAiSemConv` trait, V1_28_0 |
| **E8** | E8.2 Low-cardinality + reject dynamic labels | ✅ Implemented | `MetricRegistry` + `check_labels`; cardinality test added |
| **E8** | E8.3 Composable redaction | ✅ Implemented | `RedactionConfig.policies`; golden test `test_invariant_redacted_inline` |

---

## E5: Observability & privacy defaults

### E5.1 — Privacy default: do-not-store-prompts default on

**Requirement:** Default config → no prompt/response body in OTel events, replay bundles, SARIF, job summary; only hashes/digests or truncated safe snippets opt-in.

**Evidence:**

1. **Config default** — `crates/assay-core/src/config/otel.rs`:
   - `PromptCaptureMode::Off` is `#[default]`.
   - `capture_acknowledged: false` by default; capture requires explicit opt-in and `capture_acknowledged: true`.

2. **OTel spans** — `crates/assay-core/src/providers/llm/tracing.rs`:
   - `PromptCaptureMode::Off`: no `gen_ai.prompt` or payload in span.
   - `BlobRef`: only `assay.blob.ref` (hmac256:...) in span; no inline prompt.
   - `RedactedInline`: regex/structured redaction applied before storing in span.

3. **SARIF** — `crates/assay-core/src/report/sarif.rs`:
   - `write_sarif` uses only `r.test_id` and `r.message`; does **not** serialize `r.details` (where prompt could live). So SARIF output never contains prompt/response body.

4. **summary.json** — `crates/assay-core/src/report/summary.rs`:
   - `Summary` has `results` (counts), `provenance` (digests), no prompt/response fields. No prompt in summary output.

5. **Replay / VCR** — `crates/assay-core/src/vcr/mod.rs`:
   - `ScrubConfig` with `request_body_paths` / `response_body_paths` for redaction; `default_secure()` scrubs auth headers. Cassettes can be scrubbed; no default inline prompt in bundle schema.

**Verification:** Default config → OTel/SARIF/summary do not contain prompt/response body. ✅

---

### E5.2 — Golden tests on exports (default → no prompt)

**Requirement:** Golden tests: default config → no prompt/response body in OTel, replay, SARIF, summary.

**Evidence:**

1. **OTel golden tests** — `crates/assay-core/tests/otel_contract.rs`:
   - `test_invariant_capture_off`: Asserts `!output.contains("\"gen_ai.prompt\"")` and no sensitive secret in output.
   - `test_invariant_blob_ref`: Asserts no `gen_ai.prompt` in output; asserts `assay.blob.ref` and `hmac256:` present.
   - `test_invariant_redacted_inline`: Asserts redacted prompt (e.g. `sk-[REDACTED]`) present and raw secret absent.

2. **Config guardrails test** — `crates/assay-core/src/config/otel.rs` (inline `#[cfg(test)]`):
   - `test_guardrails_validation`: Allowlist, TLS, localhost, suffix/prefix attack tests.

**Run:**

```bash
cargo test -p assay-core --test otel_contract
cargo test -p assay-core config::otel::tests
```

**Result:** All tests pass. ✅

**Note:** SARIF and summary do not include prompt by design (schema has no such field). No separate golden file for “SARIF/summary default no prompt” — code path guarantees it.

---

## E8: P1.2 OTel GenAI (Observability)

### E8.1 — Semconv version gating: config + manifest; versioned span attributes

**Requirement:** Config + manifest; versioned span attributes (GenAI semconv).

**Evidence:**

1. **Config** — `crates/assay-core/src/config/otel.rs`:
   - `genai_semconv_version: String` (default `"1.28.0"`).
   - `semconv_stability: SemConvStability` (StableOnly / ExperimentalOptIn).

2. **Trait + versioned impl** — `crates/assay-core/src/otel/semconv.rs`:
   - `GenAiSemConv` trait with `version()`, `system()`, `request_model()`, `usage_input_tokens`, `prompt_content()`, etc.
   - `V1_28_0` impl with fixed attribute names (e.g. `gen_ai.prompt`, `gen_ai.completion`).

3. **Span attributes** — `crates/assay-core/src/providers/llm/tracing.rs`:
   - Spans record `assay.semconv.genai` = config version.
   - Attributes use semconv keys from `GenAiSpanBuilder` (e.g. `gen_ai.system`, `gen_ai.request.model`).

**Verification:** Semconv version in config; versioned span attributes via trait. ✅

---

### E8.2 — Low-cardinality enforcement + cardinality budget tests + “reject dynamic labels” guard

**Requirement:** Spans + metrics (GenAI semconv); low-cardinality enforcement; cardinality budget tests; “reject dynamic labels” guard in code.

**Evidence:**

1. **Guard in code** — `crates/assay-core/src/otel/metrics.rs`:
   - `MetricRegistry` with `filter_labels()`.
   - `FORBIDDEN_LABELS`: `trace_id`, `span_id`, `user_id`, `prompt_hash`, `file_path`.
   - Registration paths use `filter_labels`; forbidden labels → `Err` + log, no registration.

2. **Public check API** — `check_labels(&self, labels: &[&str]) -> Result<(), String>` for validation/tests.

3. **Cardinality test** — `crates/assay-core/src/otel/metrics.rs` (`#[cfg(test)] mod tests`):
   - `test_cardinality_forbidden_labels_rejected`: Asserts `check_labels(&["model", "operation"]).is_ok()`; `check_labels(&["user_id"])`, `&["trace_id"]`, `&["prompt_hash"]`, `&["file_path"]`, `&["model", "user_id"]` all `is_err()`.

**Run:**

```bash
cargo test -p assay-core otel::metrics::tests::test_cardinality_forbidden_labels_rejected
```

**Result:** Pass. ✅

---

### E8.3 — Composable redaction policies; golden tests default vs full

**Requirement:** Composable redaction policies; golden tests default vs full.

**Evidence:**

1. **Redaction config** — `crates/assay-core/src/config/otel.rs`:
   - `RedactionConfig { policies: Vec<String> }` (regex/pattern list).
   - Used by `RedactionService` in tracing.

2. **Modes** — `PromptCaptureMode`: Off, RedactedInline, BlobRef.
   - `RedactedInline`: `redact_inline()` applies policies (e.g. `sk-` → `sk-[REDACTED]`).
   - `BlobRef`: only digest in span; no inline content.

3. **Golden tests** — Same as E5.2: `test_invariant_capture_off`, `test_invariant_blob_ref`, `test_invariant_redacted_inline` cover default (Off) vs BlobRef vs RedactedInline with policies.

**Verification:** Composable policies; golden tests for default (Off) and full (BlobRef/RedactedInline). ✅

---

## Tests run (evidence)

```bash
# E5/E8 OTel contract + config guardrails
cargo test -p assay-core --test otel_contract
cargo test -p assay-core config::otel::tests

# E8.2 cardinality
cargo test -p assay-core otel::metrics::tests
```

All relevant tests pass. ✅

---

## Definition of Done (DoD) — §8.3.5

| DoD Item | Status |
|----------|--------|
| Semconv abstraction: `GenAiSemConv` with v1.28.0 impl | ✅ |
| Stability config: `semconv_stability` gate | ✅ |
| Privacy modes: off / blob_ref / redacted_inline | ✅ |
| Guardrails: TLS/Allowlist when capture ON | ✅ |
| Golden tests: normalized snapshot verification (Off, BlobRef, RedactedInline) | ✅ |
| Low-cardinality: forbidden labels rejected + test | ✅ |

---

## OpenClaw hardenings — status

Bron: ADR-008 §4, DX-IMPLEMENTATION-PLAN §8.3.4, ROADMAP "OpenClaw Hardening Kit", `otel-collector-openclaw-check.yaml`.

### ✅ Uitgevoerd (in code / templates)

| Hardening | Bron | Bewijs |
|-----------|------|--------|
| **capture_acknowledged** verplicht bij capture on | ADR-008, config | `config/otel.rs` validate: `OpenClaw: 'otel.capture_acknowledged' must be true` |
| **TLS** verplicht voor remote OTLP (https:// of localhost) | ADR-008 §4.1 | validate: `OTEL_EXPORTER_OTLP_ENDPOINT` moet https:// of http://localhost |
| **Explicit allowlist** verplicht bij capture on | ADR-008 §4.2 | validate: `exporter.allowlist` verplicht; wildcard `*.trusted.org` + suffix/prefix checks |
| **Localhost export** standaard geweigerd | ADR-008 §4.3 | `exporter.allow_localhost` default false; validate blokkeert localhost tenzij expliciet true |
| **Collector bind 127.0.0.1** (geen 0.0.0.0) | DX §8.3.4, template | `resources/otel-collector-openclaw-check.yaml`: grpc `127.0.0.1:4317`, http `127.0.0.1:4318` |
| **Collector downstream redaction** | ADR-008 §6 | Zelfde YAML: redaction processor, allowed_keys o.a. `assay.blob.ref`, blocked_values `sk-*`, `Bearer *` |
| **Collector resource normalization** | OpenClaw defense | Zelfde YAML: delete `process.command_line`, `process.executable.path` |
| **Exporter TLS** in template | OpenClaw defense | Zelfde YAML: `tls: insecure: false` |
| **BlobRef** (assay.blob.ref, assay.blob.kind) | ADR-008 §3 | `providers/llm/tracing.rs`: BlobRef mode schrijft alleen ref + kind, geen prompt inline |
| **Low-cardinality / reject dynamic labels** | ADR-008 §5, E8.2 | `otel/metrics.rs` FORBIDDEN_LABELS + test |

### ❌ Nog niet uitgevoerd (roadmap / harness-ready)

| Hardening | Bron | Opmerking |
|-----------|------|-----------|
| **DNS anti-bypass** (geen private→public jump bij resolution) | ADR-008 §4.1 | Niet in `config/otel.rs`; optioneel P2 in roadmap (Q2/Q3 Advanced Hardening). |
| **assay.blob.redaction** attribute | ADR-008 §3 | Alleen `assay.blob.ref` en `assay.blob.kind` geïmplementeerd. |
| **BlobRef BYOS metadata** (assay.blob.alg, scope, retention_class) | DX "OpenClaw harness-ready" | Voor enterprise/WORM; gepland als harness-upgrade. |
| **assay-openclaw docs** (secure-by-default deployment profiles) | ROADMAP Q2 | Nog geen aparte assay-openclaw deploy-docs. |
| **Baseline pack** "No Prompt Leakage" + "TLS/allowlist required" | ROADMAP Q2 | Pack engine bestaat; deze specifieke pack nog niet. |
| **openclaw-supplychain-baseline pack** (ClawHub skill linting) | ROADMAP Q3 | Toekomst. |
| **Governance Proxy** (reverse proxy vóór Gateway) | ROADMAP Q3/Q4 P2 | Toekomst. |

**Conclusie OpenClaw:** Alle **telemetry/OTel-gerichte** OpenClaw hardenings uit ADR-008 en §8.3.4 (capture_acknowledged, TLS, allowlist, localhost-denial, collector template 127.0.0.1 + redaction + TLS) zijn uitgevoerd. Nog niet gedaan: DNS anti-bypass, extra BlobRef-metadata, assay-openclaw docs, baseline pack, Supply Chain pack en Governance Proxy (roadmap Q2–Q4).

---

## Hoe enable je capture veilig (Sign-off snippet)

Om payload capture (BlobRef of RedactedInline) veilig in te schakelen:

1. **Acknowledgment** — Zet `otel.capture_acknowledged: true` (two-person rule; geen capture per ongeluk).
2. **Allowlist** — Vul `exporter.allowlist` met expliciete host(s), bijv. `["*.mycorp.com"]` of `["otel.mycorp.com"]`. Geen substring: `evilmycorp.com` wordt niet toegestaan door `*.mycorp.com`.
3. **TLS** — Gebruik `https://` voor `OTEL_EXPORTER_OTLP_ENDPOINT`; voor remote endpoints is TLS verplicht.
4. **Localhost** — Standaard geblokkeerd. Alleen toegestaan met `exporter.allow_localhost = true` **én** een allowlist-match (bijv. `127.0.0.1` in allowlist). Remote endpoints: altijd `https://` (TLS verplicht).
5. **BlobRef** — Bij `capture_mode: BlobRef` moet `ASSAY_ORG_SECRET` gezet zijn (geen `ephemeral-key`); anders faalt validatie.

Config-voorbeeld (eval.yaml of otel-config):

```yaml
otel:
  capture_mode: BlobRef
  capture_acknowledged: true
  exporter:
    allowlist: ["*.mycorp.com"]
    allow_localhost: false
```

Environment: `OTEL_EXPORTER_OTLP_ENDPOINT=https://otel.mycorp.com`, `ASSAY_ORG_SECRET=<org-secret>`.

---

## Conclusion

Epic E5 and Epic E8 (Step 3) are **implemented and verified**. Evidence: config defaults, OTel/SARIF/summary code paths, `MetricRegistry` + cardinality test, redaction config and golden tests. All listed tests pass.

OpenClaw **telemetry surface** hardenings (config guardrails + collector template) are **implemented**; roadmap items (docs, packs, proxy) and harness-ready upgrades (BlobRef metadata, DNS anti-bypass) are **not yet** implemented.

---

## Sign-off beslissing

**SIGN-OFF: Epic E5 & E8 (Step 3) — audit-ready baseline**

Op basis van de sign-off bundle is voldaan aan de kern-invariants:

- Privacy-by-default is afdwingbaar en bewezen (Off → geen prompt keys, geen secrets).
- BlobRef is privacy-hard (HMAC-format + secret required) en bewezen.
- RedactedInline is expliciet opt-in (ack) én scrubbed (geen raw secret).
- Sampling gate voorkomt “ghost work” wanneer spans niet recorded worden.
- Guardrails (TLS + allowlist + localhost policy) zijn hard en getest tegen wildcard/prefix/suffix valkuilen.
- VCR default_secure scrubbing is aanwezig en contract-tested.
- SARIF/summary regressietest voorkomt secret leakage via reporting.

Dit is genoeg om het “SOTA 2026 privacy-by-default + observability” verhaal serieus hard te claimen, zonder marketing-overreach.

---

### Waarom dit nu wél “bewijsbaar” is

1. **“gen_ai.prompt fysiek absent” is nu echt bewezen**
   Het verschil tussen “key bestaat maar null” (slechte audit story) en “key bestaat niet” (sterke audit story) is afgedekt met: (a) code: `gen_ai.prompt` alleen declareren in RedactedInline-branch; (b) test: `parse_span_field_keys()` assert dat de key niet in keys voorkomt (Off/BlobRef). Dit was één van de grootste blockers voor harde claims.

2. **Sampling gate is nu contract-tested**
   De `EnvFilter("warn")`-aanpak bewijst dat bij disabled info-spans geen `assay.blob.ref` en geen `gen_ai.prompt` in de output verschijnen. Story: “we doen geen expensive/gevoelige capture-work als er toch niets geëxporteerd wordt”.

3. **BlobRef secret policy is nu fail-closed**
   BlobRef mag niet werken zonder echte `ASSAY_ORG_SECRET`; “ephemeral-key” is expliciet verboden in validatie. BlobRef is daarmee in lijn met “privacy-hard reference”.

---

### Opmerkingen / polish (niet-blocking)

- **A) parse_field_value()** — Pakt nu alleen `as_str()`. Voor token-velden (numeriek) later eventueel uitbreiden naar `to_string()` voor numbers/bools. Niet nodig voor sign-off.
- **B) OTLP exporter test** — Gestructureerde asserts op fmt-json zijn voldoende voor Step 3. Voor “harness for OpenClaw” productisatie: in Q2/Q3 een in-memory OTEL SDK exporter / OTLP mock collector test toevoegen (bewijs over de hele pipeline). Roadmap-waardig.
- **C) Localhost policy** — Doc-snippet is aangescherpt: remote endpoints altijd https; localhost alleen met `allow_localhost = true` én allowlist-match, in lijn met `validate()`.

---

### OpenClaw “harnas” implicatie

Met deze sign-off bundle is er een sterke kern voor “OpenClaw Hardening Kit (Option B)” op de telemetry/privacy-as: prompt/response lekt niet per ongeluk via OTel; allowlist/TLS/localhost deny maken exfiltratie moeilijker; regressietests blokkeren secret leakage in reports. Wat nog niet in scope is (en niet hoeft voor Step 3): tool execution governance vóór OpenClaw actions, supply-chain linting voor skills/plugins, gateway/proxy enforcement tegen UI token exfil flows. Dit staat in de roadmap.

---

### Definitieve sign-off tekst (copy-paste)

**SIGN-OFF (E5/E8 Step 3):**
De implementatie voldoet aan privacy-by-default en observability-baselines voor audit-grade omgevingen. Contracttests bewijzen (1) fysieke afwezigheid van gen_ai.prompt in Off/BlobRef, (2) BlobRef met HMAC-format en verplicht org secret, (3) RedactedInline met policy-scrubbing, (4) sampling gate die capture-work voorkomt bij non-recorded spans, (5) guardrails voor TLS/allowlist/localhost, en (6) geen prompt/secret leakage via SARIF/summary en VCR defaults. **Resultaat: PASS, audit-ready baseline.**
