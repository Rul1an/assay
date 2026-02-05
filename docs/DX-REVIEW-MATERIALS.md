# DX Review Materials — PR Gate Developer Experience

Dit document ondersteunt een review van de developer experience rond de PR gate: first 15 minutes, PR feedback UX, en ergonomie/debuggability.

**Implementatie:** Concrete fixes en per-file patchlist staan in [DX-IMPLEMENTATION-PLAN.md](DX-IMPLEMENTATION-PLAN.md) (P0/P1 backlog, test cases).

---

## A. "First 15 minutes" ervaring

### A.1 assay init — output en gegenereerde bestanden

**Commando's:**

```bash
# Alleen policy + config (geen CI)
assay init

# Met CI-scaffolding (eval, traces, workflow)
assay init --ci
# of
assay init --ci github
```

**init (zonder --ci):**

- Scant op MCP-config (claude_desktop_config.json, mcp.json), Node (package.json), Python (pyproject.toml/requirements.txt).
- Output: "Scanning project...", "Generating Assay Policy & Config...".
- Gegenereerde bestanden:
  - `policy.yaml` — uit pack (default pack: veilige baseline; `--pack` kiest pack).
  - `assay.yaml` (of `--config`) — [templates.rs ASSAY_CONFIG_DEFAULT_YAML](https://github.com/Rul1an/assay/blob/main/crates/assay-cli/src/templates.rs): `version: 2`, `policy: "policy.yaml"`, `baseline: ".assay/baseline.json"`.
  - Optioneel `.gitignore` (`.assay`, `*.db`, etc.) bij `--gitignore`.

**init --ci:**

- Bovenstaande plus:
  - `ci-eval.yaml` — smoke suite (regex, json_schema, semantic_similarity).
  - `schemas/ci_answer.schema.json` — JSON Schema voor ci_smoke_schema.
  - `traces/ci.jsonl` — voorbeeldtrace voor replay.
  - `.github/workflows/assay.yml` — **let op:** template gebruikt nog `Rul1an/assay-action@v1` en v1.4.0; aanbevolen is `Rul1an/assay/assay-action@v2` (zie [guides/github-action](getting-started/ci-integration.md)).

**Defaults (veiligheid en bruikbaarheid):**

- Policy pack: blocking defaults (Exec/Shell/Python geblokkeerd in typische baseline).
- Config: verwijst naar policy + baseline path; geen onveilige defaults in gegenereerde YAML.
- **Reviewpunten:** Template workflow versie (v1 vs v2), of `assay init --ci` een "blessed" quickstart is t.o.v. `assay init-ci`.

**Relevante code:**

- [crates/assay-cli/src/cli/commands/init.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-cli/src/cli/commands/init.rs) — init flow, pack selection, write_file_if_missing.
- [crates/assay-cli/src/templates.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-cli/src/templates.rs) — ASSAY_CONFIG_DEFAULT_YAML, CI_WORKFLOW_YML, CI_EVAL_YAML, CI_TRACES_JSONL.
- [docs/reference/cli/init.md](reference/cli/init.md) — init documentatie.

### A.2 Minimale voorbeeldrepo: 0 → CI gate

**Bestaande quickstart (geen aparte Node/Python repo in tree):**

- **examples/baseline-gate/** — eval + baseline + trace; geen volledige repo met CI.
- **assay init --ci** — genereert workflow + ci-eval + traces; lokaal "0 → run" mogelijk, maar geen kant-en-klare Node/Python-voorbeeldrepo in de repo.

**Reproduceerbare "0 → CI gate" (binnen deze repo):**

```bash
cd /tmp && mkdir assay-dx-demo && cd assay-dx-demo
git init
assay init --ci github
# Aanpassen: .github/workflows/assay.yml → uses: Rul1an/assay/assay-action@v2, en config/trace_file pad controleren
assay run --config ci-eval.yaml --trace-file traces/ci.jsonl --output junit --output sarif
# Lokaal: junit.xml + SARIF in output-dir; in CI: zelfde command + upload SARIF/JUnit.
```

**Gap voor reviewer:** Er is geen aparte minimale Node- of Python-voorbeeldrepo (bijv. `examples/dx-demo-node` / `examples/dx-demo-python`) met alleen `assay init --ci` + één test die 0 → PR gate demonstreert. De reviewer kan de bovenstaande stappen volgen; voor "one-click" gevoel zou zo'n voorbeeldrepo toegevoegd kunnen worden.

---

## B. PR feedback UX

### B.1 JUnit — test failures "native" in CI

- **CLI:** `assay run --config <config> --trace-file <trace> --output junit` schrijft JUnit XML (default `junit.xml`; `--junit <path>` overschrijft).
- **Formaat:** [crates/assay-core/src/report/junit.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-core/src/report/junit.rs) — suite name, testcase per test, `<failure>` bij Fail/Error, `<system-out>` voor Warn/Flaky details (ADR-003).
- **CI:** In eigen workflows kan JUnit geüpload worden (bijv. `actions/junit-report` of `reporter: java-junit` in summary). De **assay-action** zelf schrijft JUnit niet automatisch; de stap die `assay` aanroept moet `--output junit` (en eventueel `--junit`) gebruiken en daarna de JUnit-artifact uploaden/rapporteren.
- **Voorbeeld (bestaand):** [.github/workflows/smoke-install.yml](https://github.com/Rul1an/assay/blob/main/.github/workflows/smoke-install.yml) — "Run Assay suite (strict + JUnit)", "Upload JUnit report artifact", "Report (JUnit)" met `reporter: java-junit`.

**Review:** Run lokaal met `--output junit`, open `junit.xml` en controleer of failed tests als `<failure>` en Warn/Flaky in `<system-out>` staan; in een branch met deze workflow zie je test-annotations in de GitHub UI.

### B.2 SARIF — findings in GitHub Security tab

- **CLI:** `assay run --output sarif` (of evidence lint) produceert SARIF; Action uploadt SARIF via `github/codeql-action/upload-sarif`.
- **Action:** [assay-action/action.yml](https://github.com/Rul1an/assay/blob/main/assay-action/action.yml) — step `upload-sarif`, alleen bij same-repo PR/push (geen upload op fork PR).
- **Review:** PR met findings → Security tab toont code scanning alerts; geen findings → geen alerts. Vergelijk met [REVIEW-MATERIALS](REVIEW-MATERIALS.md) Set A/B: run met failing trace en controleer of de bijbehorende finding in SARIF en in de Security tab verschijnt.

### B.3 PR comment — alleen bij findings

- **Action:** `comment_diff: true` (default kan per versie verschillen); step "Post or update PR comment" draait alleen als `findings_error != '0' || findings_warn != '0'` (zie action.yml rond regel 612–614).
- **Body:** Bevat `<!-- assay-report -->`; comment wordt gemaakt/bijgewerkt door `peter-evans/create-or-update-comment`.
- **Review:** PR met 0 findings → geen comment (of alleen bij eerdere run); PR met 1+ finding → één comment met rapport. Fork PR: geen SARIF upload, geen comment (permissions).

### B.4 Exit codes en strict semantics

- **Betekenis (run/ci):**
  - **0** — Alles geslaagd (Pass).
  - **1** — Een of meer tests failed (Fail/Error; onder `--strict` ook Warn/Flaky).
  - **2** — Config/user error (ontbrekende config, ontbrekende trace, invalid YAML, etc.).
  - **3** — Trace file not found (in run.md); in ADR-019 wordt 3 ook "infra/judge unavailable" (nog niet overal geïmplementeerd).
- **Strict mode:** `--strict` → Warn en Flaky tellen als fail (exit 1); zonder `--strict` → exit 0 bij alleen Warn/Flaky (zie [ADR-003](architecture/ADR-003-Gate-Semantics.md)).
- **Action:** Exit codes worden doorgegeven; step faalt bij non-zero zodat de job faalt.
- **Documentatie:** [reference/cli/run.md](reference/cli/run.md) (Exit Codes), [guides/troubleshooting.md](guides/troubleshooting.md) (Configuration Errors = 2, Test Failures = 1).

**Review:** Bewust een run met (1) missing config, (2) missing trace, (3) één failing test, (4) alleen Warn — en controleer exit code en of de foutmelding duidelijk "wat ging fout" en "wat te doen" geeft.

---

## C. Ergonomie en debuggability

### C.1 Kwaliteit van errors — "wat ging fout + wat moet ik doen?"

- **Plekken:** Config- en validatiefouten (o.a. in doctor), run failures (console summary + message per test), evidence verify/lint.
- **Documentatie:** [guides/troubleshooting.md](guides/troubleshooting.md) — Configuration Errors (Exit 2), Test Failures (Exit 1), met voorbeelden en fix-stappen.
- **Doctor:** Parsed errors (bijv. unknown field) krijgen waar mogelijk een similarity-hint ("Replace `x` with `y`") in [crates/assay-core/src/doctor/mod.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-core/src/doctor/mod.rs).

**Review:** Voer bewust foute config, ontbrekende trace, en een failing test uit; beoordeel of het bericht en (indien aanwezig) doctor/explain een duidelijke volgende stap geven.

### C.2 assay doctor — outputkwaliteit en actionability

- **Doel:** Valideert config, trace(s), baseline, DB; rapporteert diagnostiek en suggesties.
- **Output:** DoctorReport met o.a. config summary, trace summary (entries, schema_version, coverage: embeddings, judge_faithfulness, judge_relevance), baseline summary, DB stats, cache summary; diagnostics met codes (bijv. E_CFG_PARSE) en fix-stappen waar mogelijk.
- **Code:** [crates/assay-core/src/doctor/mod.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-core/src/doctor/mod.rs), [crates/assay-cli/src/cli/commands/doctor.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-cli/src/cli/commands/doctor.rs).
- **Review:** Run `assay doctor --config <eval.yaml> --trace-file <trace.jsonl>` op een geldige setup en op een setup met ontbrekende/ongeldige bestanden; beoordeel of de output direct bruikbare acties geeft (bestand/pad, verwachte waarde, suggestie).

### C.3 assay explain — violations en koppeling naar policy/trace step

- **Doel:** Legt stap-voor-stap uit hoe een trace tegen een policy wordt geëvalueerd (tool-calls, verdicts, regels).
- **CLI:** `assay explain --trace <file> --policy <policy.yaml>`; format: terminal, markdown, html, json. Optioneel `--blocked-only`, `--verbose`.
- **Structuur (core):** [crates/assay-core/src/explain.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-core/src/explain.rs) — ExplainedStep (index, tool, verdict, rules_evaluated), RuleEvaluation (rule_id, rule_type, passed, explanation, context), TraceExplanation (blocking_rules, first_block_index).
- **Koppeling:** rule_id en step index maken koppeling naar policy-regel en trace-step mogelijk; output toont per step welke regels werden geëvalueerd en of ze passed/blocked.
- **Review:** Run explain op een trace met één geblokkeerde call; controleer of (1) de geblokkeerde step en (2) de verantwoordelijke rule_id/regel duidelijk zijn en of je in policy en trace de juiste plek kunt vinden.

### C.4 Performance-DX — progress, timings, slowest tests, cache hit rate

- **Console summary:** [crates/assay-core/src/report/console.rs](https://github.com/Rul1an/assay/blob/main/crates/assay-core/src/report/console.rs) — per test duration (`(X.Xs)`), status (Pass/Fail/Warn/Flaky/Skipped), bij skip: reason, fingerprint, "To rerun: assay run --refresh-cache".
- **Progress:** "Running N tests..." aan het begin; tijdens de run "Running test X/N..." (throttled: max ~10 updates + altijd final N/N). Bij total ≤ 1 geen N/M-regels. Geen progress bar.
- **Timings:** duration_ms per TestResultRow, afgedrukt in console summary.
- **Slowest tests:** Niet expliciet gesorteerd of geaggregeerd in één regel ("slowest 5"); wel per-test duration zichtbaar.
- **Cache hit rate:** Niet als aparte KPI in console; wel skip-reason en fingerprint bij cached/skipped tests. Cache/logic in runner en store.
- **Review:** Run een suite met meerdere tests (bijv. examples of tests/fixtures); beoordeel of je snel ziet welke tests traag zijn en of cache (skip) herkenbaar is. Eventueel: wens voor "slowest N" of "cache hit rate" in summary (backlog/ADR-019).

---

## Snelle checklist voor de reviewer

| Onderdeel | Wat te doen | Waar te kijken |
|-----------|-------------|----------------|
| A.1 init output | `assay init` en `assay init --ci` in lege dir | Gegenereerde bestanden, template-versie (v1 vs v2 action) |
| A.2 0→CI | Volg "0 → CI gate" stappen hierboven of gebruik examples/baseline-gate | Of er een minimale Node/Python-voorbeeldrepo ontbreekt |
| B.1 JUnit | Run met `--output junit`, open junit.xml; in CI: smoke-install workflow | Failure vs system-out, annotations in GitHub |
| B.2 SARIF | Run met failing trace, upload SARIF, open Security tab | Findings komen overeen met failures |
| B.3 PR comment | PR met findings vs zonder findings | Comment alleen bij findings |
| B.4 Exit codes | Misconfig, missing trace, fail, warn | Exit 2/1/0 en duidelijke boodschap |
| C.1 Errors | Bewust foute config + failing test | troubleshooting.md + terminal output |
| C.2 doctor | `assay doctor` op goede en kapotte setup | Actionable diagnostics en fix-stappen |
| C.3 explain | `assay explain` op trace met blocked step | rule_id + step index → policy + trace |
| C.4 Performance | Run suite, kijk naar summary | Per-test timing, geen "slowest"/cache rate (eventueel wens) |

---

## Referenties

- [REVIEW-MATERIALS.md](REVIEW-MATERIALS.md) — trace sets, evidence bundles, MCP/trust, quickstart.
- [ADR-003 Gate Semantics](architecture/ADR-003-Gate-Semantics.md) — Pass/Fail/Warn/Flaky, strict mode.
- [ADR-019 PR Gate 2026 SOTA](architecture/ADR-019-PR-Gate-2026-SOTA.md) — blessed flow, exit codes, DX-doelen.
- [getting-started/ci-integration.md](getting-started/ci-integration.md) — CI-integratie en Action-gebruik.
- [reference/cli/run.md](reference/cli/run.md) — run, output formats, exit codes.
- [guides/troubleshooting.md](guides/troubleshooting.md) — veelvoorkomende fouten en fixes.
