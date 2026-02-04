# E5/E8 Sign-off Test Report

**Document:** E5-E8-TEST-REPORT
**Scope:** Epic E5 (Observability & privacy defaults), Epic E8 (OTel GenAI) — audit-grade sign-off bundle
**Related:** [E5-E8-VERIFICATION.md](E5-E8-VERIFICATION.md), [DX-IMPLEMENTATION-PLAN.md](../maintainers/DX-IMPLEMENTATION-PLAN.md)

---

## 1. Executive summary

| Metric | Value |
|--------|--------|
| **Report date** | 2026-02-02 |
| **Crate** | assay-core |
| **Total tests (sign-off scope)** | 12 |
| **Passed** | 12 |
| **Failed** | 0 |
| **Skipped** | 0 |
| **Result** | **PASS** |

Alle tests binnen het sign-off-scope (OTel capture contract, SARIF/summary privacy, allowlist guardrails, BlobRef secret policy, VCR scrub defaults) zijn geslaagd. De uitkomst ondersteunt **sign-off** op E5/E8 als audit-ready baseline.

---

## 2. Test environment

| Item | Value |
|------|--------|
| **Command** | `cargo test -p assay-core` (filtered by sign-off suites) |
| **Rust** | `cargo` default (stable) |
| **Platform** | Ontwikkel-/CI-omgeving waar `cargo test` draait |
| **Reproduce** | Zie sectie 6 |

---

## 3. Results by suite

### 3.1 OTel capture contract (`--test otel_contract`)

Bewijs dat capture-modes correct worden geëxporteerd en dat `gen_ai.prompt` fysiek ontbreekt in Off/BlobRef.

| Test | Status | Doel |
|------|--------|------|
| `test_invariant_capture_off` | Pass | Off: geen `gen_ai.prompt` in export; geen sensitive secret; gestructureerde assert op ontbrekende key |
| `test_invariant_blob_ref` | Pass | BlobRef: geen `gen_ai.prompt`; wel `assay.blob.ref` met prefix `hmac256:`; parsed JSON asserts |
| `test_invariant_redacted_inline` | Pass | RedactedInline: `gen_ai.prompt` aanwezig en geredacteerd; geen raw secret in export |
| `test_capture_requires_sampled_span_no_work_when_disabled` | Pass | Bij “sampling drop” (filter=warn): geen blob-ref en geen prompt in output |

**Sign-off items:** 1 (E2E structured export), 2 (gen_ai.prompt fysiek absent), 3 (sampling gate).

---

### 3.2 SARIF & summary privacy (`--test contract_sarif`)

Regressie: prompt/secret lekt niet naar SARIF of summary.json.

| Test | Status | Doel |
|------|--------|------|
| `test_sarif_and_summary_never_contain_prompt_secret` | Pass | Results met `details.prompt = "sk-123..."` → SARIF en summary.json bevatten de string niet |
| `test_invariant_sarif_always_has_locations` | Pass | Elke SARIF-result heeft ten minste één location (synthetic fallback) |

**Sign-off item:** 7 (SARIF/summary regression).

---

### 3.3 OTel config guardrails (`config::otel::tests`)

Allowlist, TLS, localhost en BlobRef-secret policy.

| Test | Status | Doel |
|------|--------|------|
| `test_guardrails_validation` | Pass | Allowlist verplicht; TLS voor remote; suffix/prefix attack (evilexample.com, example.com.attacker.tld) geblokkeerd; wildcard *.trusted.org toegestaan |
| `test_allowlist_wildcard_mycorp_allowed_evil_denied` | Pass | `*.mycorp.com` staat `https://otel.mycorp.com` toe; weigert `https://evilmycorp.com` |
| `test_allowlist_port_and_trailing_dot` | Pass | Host met port (`https://otel.mycorp.com:443`) matcht op host-only rule |
| `test_allow_localhost_default_deny_explicit_true_allowed` | Pass | Default: localhost geblokkeerd; met `allow_localhost = true` + allowlist toegestaan |
| `test_blob_ref_requires_assay_org_secret` | Pass | BlobRef: ontbrekende of `ephemeral-key` ASSAY_ORG_SECRET → validatiefout; geldige secret → OK |

**Sign-off items:** 5 (allowlist parsing), 6 (ASSAY_ORG_SECRET required).

---

### 3.4 VCR scrub config (`vcr::tests`)

Default scrub-paths voor cassettes.

| Test | Status | Doel |
|------|--------|------|
| `test_default_secure_scrub_paths` | Pass | `ScrubConfig::default_secure()` scrubt o.a. Authorization, x-api-key, api-key, set-cookie; geen body-paths in default |

**Sign-off item:** 4 (VCR/default geen bodies loggen).

---

## 4. Traceability (sign-off bundle → tests)

| # | Sign-off item | Bewijs |
|---|----------------|--------|
| 1 | E2E export met gestructureerde asserts (Off/BlobRef/RedactedInline) | otel_contract: parsed JSON + key presence/absence |
| 2 | gen_ai.prompt fysiek absent (geen null/empty) in Off/BlobRef | otel_contract + code: veld alleen in RedactedInline-span |
| 3 | Sampling: capture_requires_sampled_span | otel_contract: `test_capture_requires_sampled_span_no_work_when_disabled` |
| 4 | VCR/default geen prompt/response bodies | vcr: `test_default_secure_scrub_paths` |
| 5 | Allowlist parsing (wildcard, evilmycorp, port, allow_localhost) | config::otel::tests (4 tests) |
| 6 | BlobRef: ASSAY_ORG_SECRET verplicht | config::otel::tests: `test_blob_ref_requires_assay_org_secret` |
| 7 | SARIF/summary bevatten geen prompt-secret | contract_sarif: `test_sarif_and_summary_never_contain_prompt_secret` |
| 8 | Doc: “Hoe enable je capture veilig” | E5-E8-VERIFICATION.md sectie “Hoe enable je capture veilig” |

---

## 5. Conclusion

- Alle 12 tests in scope zijn **geslaagd**.
- Geen failures of unexpected skips.
- Build van assay-core (incl. deze tests) **zonder compilerwarnings**.

De testuitkomsten ondersteunen **sign-off** op E5/E8 als audit-ready baseline en als basis voor een OpenClaw hardening kit.

**Sign-off beslissing:** Zie [E5-E8-VERIFICATION.md](E5-E8-VERIFICATION.md) § Sign-off beslissing voor de volledige review, kern-invariants en definitieve sign-off tekst.

---

## 6. Reproducibility

Voer lokaal dezelfde tests uit:

```bash
# Alle sign-off tests (compact)
cargo test -p assay-core --test otel_contract --test contract_sarif
cargo test -p assay-core config::otel::tests
cargo test -p assay-core vcr::tests::test_default_secure_scrub_paths

# Of volledige assay-core test suite
cargo test -p assay-core
```

Verwacht: alle bovenstaande tests **ok**, geen failures.

---

## 7. Definitieve sign-off tekst (copy-paste)

**SIGN-OFF (E5/E8 Step 3):**
De implementatie voldoet aan privacy-by-default en observability-baselines voor audit-grade omgevingen. Contracttests bewijzen (1) fysieke afwezigheid van gen_ai.prompt in Off/BlobRef, (2) BlobRef met HMAC-format en verplicht org secret, (3) RedactedInline met policy-scrubbing, (4) sampling gate die capture-work voorkomt bij non-recorded spans, (5) guardrails voor TLS/allowlist/localhost, en (6) geen prompt/secret leakage via SARIF/summary en VCR defaults. **Resultaat: PASS, audit-ready baseline.**
