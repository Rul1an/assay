# CI — wat de assessment nog vraagt

Overzicht van **open punten** uit de assessment-docs (REVIEWER-PACK, PINNED-ACTIONS, PERFORMANCE-ASSESSMENT, BRANCH-PROTECTION) die nog actie vragen voor CI/workflows.

**Al gedaan:** Branch protection (main), CODEOWNERS, required status checks (CI, Smoke Install, assay-action-contract-tests, MCP Security), workflow permissions (read default, job-level contents: read), environment: release/crates/pypi in release.yml, fork-guards op self-hosted, OIDC voor crates.io en PyPI.

---

## 1. Security & supply chain (REVIEWER-PACK, PINNED-ACTIONS)

| Item | Bron | Actie |
|------|------|--------|
| **Actions pinnen op SHA** | REVIEWER-PACK checklist; PINNED-ACTIONS.md | Third-party actions staan nu op @v1/@v2/@v4/@v5 of @main. Aanbevolen: high-risk eerst (Bencher, gh-release, upload-sarif, test-reporter, actions/checkout, dtolnay/rust-toolchain) op commit-SHA zetten; daarna in Settings → Actions “Require SHA pinning” aanzetten. Zie [PINNED-ACTIONS.md](PINNED-ACTIONS.md). |
| **Allowed actions beperken** | REVIEWER-PACK sectie 2 | In **Settings → Actions → General**: “Allow [org] and verified creators” of allowlist i.p.v. “Allow all actions”. |
| **Fork PR policy vastleggen** | REVIEWER-PACK sectie 2 | In **Settings → Actions → General**: (1) Draaien fork-PR workflows? (2) Read-only of write token? (3) Secrets geblokkeerd? Documenteer keuze (screenshot of één regel). |
| **GHAS** | REVIEWER-PACK sectie 2 | Beslissen: Code scanning (CodeQL), Secret scanning (push protection), Dependency review aan/uit? |

---

## 2. Environments (REVIEWER-PACK, BRANCH-PROTECTION-SETUP)

| Item | Bron | Actie |
|------|------|--------|
| **Environment reviewers** | BRANCH-PROTECTION-SETUP checklist; REVIEWER-PACK sectie 3 | In **Settings → Environments**: voor `release`, `crates` en `pypi` **Required reviewers** toevoegen (bv. 1–2 maintainers). release.yml gebruikt deze environments al op de juiste jobs; de approval gate werkt pas als de reviewers in de UI staan. |

---

## 3. Optioneel (branch protection / repo)

| Item | Bron | Actie |
|------|------|--------|
| **Signed commits** | REVIEWER-PACK sectie 2 | Optioneel: “Require signed commits” op main aanzetten. |
| **Linear history** | REVIEWER-PACK sectie 2 | Optioneel: “Require linear history” op main aanzetten. |

---

## 4. Performance / observability (PERFORMANCE-ASSESSMENT)

| Item | Bron | Actie |
|------|------|--------|
| **Cache-hit in CI job summary** | PERFORMANCE-ASSESSMENT § “Bewijs van cache-hit”, “Huidige stand” | In de **Criterion-perf-job** (ci.yml): stap toevoegen die **cache-hit** (van rust-cache of assay-perf cache) in de **job summary** logt (`echo "cache-hit=..."`), zodat in de UI zichtbaar is of het een cache-hit was. baseline-gate-demo doet dit al. |
| **Fase-timings / SQLite-counters** | PERFORMANCE-ASSESSMENT P0.3 | Voor echte P0.3-validatie: fase-timings en SQLite-contention (bv. sqlite_busy_count) first-class in summary.json of bench-output; zie doc voor minimale set. |
| ✅ **Bencher policy** | PERFORMANCE-ASSESSMENT § Bencher policy | **Gedaan:** stdin/pipe-modus, korte IDs (sw/sr), threshold-flags (t_test, upper_boundary 0.99), exacte commands in doc, perf_pr = warn. **Later:** perf_pr_gate.yml met --err + label perf-gate voor hard-fail. |

---

## 5. Al geïmplementeerd (geen actie)

- Workflow permissions: read-only default; job-level `contents: read` waar nodig.
- Geen `pull_request_target`; self-hosted jobs alleen bij non-fork PR (fork-guard).
- Caches: hashFiles/vaste prefix; concurrency op ebpf-smoke en kernel-matrix.
- OIDC voor crates.io en PyPI; Bencher static token met same-repo guard.
- **Bencher CI baseline + PR compare:** perf_main.yml (main baseline, nightly), perf_pr.yml (PR compare); benchmarks sw/50x400b, sw/12xlarge, sr/wc; stdin/pipe-modus; thresholds (t_test, upper_boundary 0.99); reports in Bencher UI met Δ% en thresholds.

---

**Korte prioriteit:** (1) Environment reviewers instellen (release/crates/pypi) → direct human-in-the-loop op publish. (2) SHA-pinning voor high-risk actions + allowed actions beperken. (3) Fork PR policy documenteren. (4) Cache-hit in perf-job summary; daarna optioneel GHAS, signed commits, performance-counters.
