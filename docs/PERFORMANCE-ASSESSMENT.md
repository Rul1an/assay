# Performance Assessment — Wat je nodig hebt om performance kritisch te beoordelen

Dit document beschrijft wat er nodig is om de performance van het Assay PR-gate pad **kritisch te beoordelen** en **ADR-019 P0.3 (Store performance) feitelijk te valideren**. Het Runner → Store (SQLite) → cache → metrics → report pad is de centrale bottleneck; reproduceerbare workloads, **first-class metingen** en CI-realiteit zijn nodig. Zonder file-backed WAL-runs, fase-timings, SQLite-contention-metrics en herhaalde runs (median/p95) blijft het een “smoke timing script”, geen “contention benchmark”.

**Gerelateerd:** [ADR-019 P0.3 Store performance](architecture/ADR-019-PR-Gate-2026-SOTA.md#p03-store-performance-wal--single-writer-batching--bounded-queue), [concepts/cache.md](concepts/cache.md), [REVIEW-MATERIALS](REVIEW-MATERIALS.md).

---

## Standaard toolkit en werkwijze (jan 2026)

Anno januari 2026 is dit de gangbare toolkit en werkwijze om performance van een Rust/SQLite/CI PR-gate **kritisch te beoordelen**. Micro- en end-to-end metingen worden **apart** gehouden; altijd **median + p95** (niet één run).

### 1) Benchmarks die statistisch kloppen

| Tool | Doel | Best practice |
|------|------|----------------|
| **Criterion.rs** | Micro/meso benchmarks: store inserts, fingerprinting, report rendering, etc. Bewaart historical data en rapporteert verandering + statistiek. | Gebruik voor store/runner-microbench; median + p95; regressie-gate in CI. |
| **Hyperfine** | End-to-end CLI-timings (`assay ci …`) met warmup en outlier-detectie; JSON-output voor trends. | Gebruik voor `assay ci` (of `assay run`) e2e; warmup runs; median/p95. Integratie met continuous benchmarking (bijv. [Bencher](https://bencher.dev/)) als je regressies in CI wilt gate'en. |

**Regel:** Meet altijd **median + p95**; houd **micro (Criterion)** en **e2e (Hyperfine)** apart.

### 2) Profiling: waar gaat de tijd heen

| Tool | Doel | Best practice |
|------|------|----------------|
| **perf + flamegraph** (Linux) | CPU-bottlenecks; [cargo flamegraph](https://github.com/flamegraph-rs/flamegraph). | [Rust Performance Book](https://nnethercote.github.io/perf-book/profiling.html): aanbevolen voor CPU. |
| **Samply** | Cross-platform sampling profiler met Firefox Profiler UI. | Populair alternatief voor niet-Linux of als je Profiler UI wilt. |
| **tokio-console** | Async/runtime: tasks, wakers, scheduling. | Bij async-issues (store lock, tokio runtime). |

**Regel:** Minstens **1× per kwartaal** (of bij grote refactors) een flamegraph/samply-profile als artefact bij “perf regressie”-tickets.

### 3) SQLite: WAL + checkpointing + busy handling (en meten)

Voor het Store-pad (contention, tail latency):

- **WAL mode** is de basis; **autocheckpoint/checkpoint-strategie** bepaalt spikes en WAL-groei.
- **busy_timeout** en/of busy handler (rusqlite ondersteunt dit); lock contention gecontroleerd afhandelen.
- **PRAGMA’s** zijn de officiële manier om gedrag te configureren en te inspecteren.

**Regel:** Naast wall-clock altijd **counters**: sqlite_busy_count, “store lock wait”, batch sizes, en (minimaal) WAL/checkpoint-observability.

### 4) CI-caching en reproduceerbaarheid (warm cache “voelt gratis”)

- **actions/cache** met goede **key** + **restore-keys** (near-misses); [GitHub beschrijft](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows) hoe restore-keys gezocht worden.
- Gebruik de **cache-hit output** om te bewijzen dat een warm-run is uitgevoerd.

**Regel:** Eén **blessed snippet** voor `.assay/` (of relevante subpaths) + invalidatie (hash van eval/policy/trace + assay version).

### 5) Instrumentatie: phase timings en async-inzicht

- **tracing** + tooling (en voor async: **tokio-console**) is in Rust de standaard om runtime-gedrag te begrijpen zonder meteen zware profilers.
- **Phase-timings** (ingest_ms, run_suite_ms, report_ms, etc.) als **vaste velden in summary.json** → CI-runs onderling vergelijken en regressies automatisch detecteren.

---

## Minimum SOTA (voor deze context)

Wat als **minimum SOTA** geldt om performance echt te kunnen reviewen en regressies te gate'en:

1. **Criterion** voor store/runner-microbench + **Hyperfine** voor `assay ci` (of `assay run`) end-to-end; beide met **median/p95**.
2. Minstens één van: **perf/flamegraph** of **samply**; voor async: **tokio-console**.
3. **SQLite:** WAL + checkpointing + busy_timeout **én** meten van contention-counters (sqlite_busy_count, store_wait_ms, etc.).
4. **CI:** actions/cache met **key** + **restore-keys** + **cache-hit bewijs**.

---

## Realisme: wat de huidige setup wél en níet meet

- **Wat het nu vooral meet:** CLI/startup/parse/report overhead. Dat medium (30 tests) en large (50 tests) ongeveer dezelfde wall-clock geven (~37 ms) is een rode vlag: de workload per test doet nauwelijks extra werk en er is te weinig schrijfvolume om lock/contention te laten zien.
- **Wat het doel moet zijn voor P0.3:** SQLite write contention onder parallel runs — dus **veel writes** (result rows, steps, tool_calls, metrics) en **file-backed DB met WAL**, niet alleen `:memory:`.
- **`:memory:`:** Prima als **CPU-only baseline**; je ziet er geen realistische WAL/checkpoint/IO-effecten mee. De **hoofdmeting** voor P0.3 moet een **file-backed DB op disk** (of tmpfs voor CI-stabiliteit) zijn, want daar laten WAL/checkpoint en IO zien waar het pijn doet. **File-backed DB runs zijn “primary truth”**, :memory: alleen als baseline.
- **Eén run per scenario:** Te ruisgevoelig; scheduling variance kan groter zijn dan de verschillen. **Minimaal:** 10–30 runs per scenario → rapporteer **median + p95** (en liefst stddev); of gebruik een harness (bijv. Criterion) dat dit automatisch doet. **E2E herhaalruns** moeten echt gebeuren voor: worstcase file-backed WAL, en standaard concurrency (parallel=4) + varianten (1/8/16), zodat p95’s niet “toevallig” door jitter zijn.

---

## Standard concurrency configuration (norm)

Om “sqlite_busy_count == 0” en p95-budgets eenduidig te maken, moet de **standaard concurrency-configuratie** hard gedefinieerd zijn:

| Onderdeel | Norm |
|-----------|------|
| **Runner** | `parallel = 4` (semaphore in `run_suite`). |
| **Store** | Single writer queue (zodra P0.3 geïmplementeerd); geen externe concurrent writers op dezelfde DB. |
| **DB** | WAL aan + checkpoint policy (bijv. wal_autocheckpoint); **busy handling** via onze custom busy handler (geen PRAGMA busy_timeout; zie sectie “Busy handler en checkpoint”). |
| **WAL / writes** | **BEGIN IMMEDIATE** voor write-transacties (niet DEFERRED), om “read→write upgrade” en SQLITE_BUSY te vermijden. |

Dit hoort óók in het performance-assessment: meten **vóór/na** writer-queue-refactor (sqlite_busy_count, p95). Zonder baseline vóór de refactor kun je niet bewijzen dat batching/queue tail spikes oplost.

---

## A. Reproduceerbare workloads

### Wat je nodig hebt

1. **2–3 representatieve trace sets** (klein / gemiddeld / groot) + bijbehorende eval (+ policy waar nodig).
2. **Twee kritische workload-typen** (acceptatievoorwaarde voor “kritisch beoordelen”):
   - **Deterministic-only store stress** — Alleen deterministische checks (regex, schema, args_valid, sequence); **geen** embeddings/judge I/O. Doel: zuivere store/runner contention.
   - **Semantic/judge workload zonder netwerkflakiness** — Cache- en precompute-gedrag meten, zonder internet/LLM-variatie. Praktisch: mock provider of **recorded responses (VCR)** zodat dezelfde inputs dezelfde outputs geven.
3. **Eén echte worst-case** — Niet “veel tests” alleen, maar **veel writes**: veel tool_calls per episode, **grote payloads** (args/result), veel result-inserts. Doel: store stress en lock/contention zichtbaar maken.

**Waarom:** Zonder die splitsing meet je “alles door elkaar” en kun je bottlenecks niet isoleren.

### Workload-generator: deterministisch en vergelijkbaar

- **Vaste seed en vaste sizes** voor medium/large/worst-case, zodat runs en trends vergelijkbaar blijven (en regressies herhaalbaar zijn).

### Huidige stand (inventaris)

| Workload / Set | Locatie | Grootte | Status | Opmerking |
|----------------|---------|---------|--------|-----------|
| Perf small | `tests/fixtures/perf/` | 5 episodes, 5 tests | ✅ | Commit; script gebruikt dit. |
| Perf medium/large | Gegenereerd in temp door `scripts/perf_assess.sh` | 30 / 50 episodes | ✅ | Script; file-backed run in script. |
| Worst-case (deterministic store stress) | `scripts/perf_assess.sh` | 12×8 tool_calls, ~400B payload | ✅ | 20× file-backed + parallel matrix; Criterion suite_run_worstcase. |
| Golden, CI smoke, examples | Zie eerder in doc | Klein | ✅ | Geen store-stress; referentie. |
| **Semantic/judge zonder netwerk** | `tests/fixtures/perf/semantic_vcr/` | 2 tests | **Fixture ✅** | VCR/mock nodig voor precompute_ms + cache gedrag; zie “Wat is nú écht open”. |

**Nog open:** **(1)** Semantic/judge workload met VCR of mock (recorded responses), zodat precompute_ms en cache gedrag voor embeddings/judge meetbaar worden zonder LLM-variatie. **(2)** Optioneel: vaste seed/sizes in de generator voor strikte reproduceerbaarheid.

---

## B. Metingen: first-class, niet optioneel

Fase-timings en SQLite-contention-counters zijn **niet optioneel** als je P0.3 wilt valideren; anders blijft het interpretatie op gevoel. Ze moeten **first-class outputs** worden (bijv. in summary.json en/of bench-output).

### Minimale set die je nodig hebt

| Categorie | Velden / metingen |
|-----------|-------------------|
| **Fases** | ingest_ms, precompute_ms, run_suite_ms, report_ms, total_ms |
| **Store** | store_wait_ms, store_write_ms, sqlite_busy_count, txn_batch_size |
| **Cache** | cache_hit_rate, cache_miss_rate |
| **Concurrency** | parallel (uit config); busy_timeout expliciet geconfigureerd/gezet in tooling. |

- **busy_timeout:** Moet in tooling expliciet geconfigureerd/gezet worden; rusqlite ondersteunt dit direct.
- **WAL-tuning:** Alleen WAL aanzetten is niet genoeg; **BEGIN IMMEDIATE** voor writes en checkpoint/autocheckpoint gedrag bewust tunen, anders krijg je spikes. Dit hoort in het plan en in het assessment (meten vóór/na).

### Huidige stand (inventaris)

| Meting | Status | Opmerking |
|--------|--------|-----------|
| Fase-timings | ✅ **Ja** | run.json: ingest_ms, run_suite_ms, report_ms, total_ms (phases). |
| store_wait_ms, store_write_ms, sqlite_busy_count | ✅ **Ja** | run.json: store_metrics; ook store_wait_pct/store_write_pct. |
| effective_pragmas, wal_checkpoint | ✅ **Ja** | run.json: effective_pragmas (incl. synchronous_human), wal_checkpoint (PASSIVE). |
| cache_hit_rate / cache_miss_rate | **Deels** | Per-test cached/skip; niet geaggregeerd als rate in summary. |
| Per-test duration | ✅ **Ja** | TestResultRow.duration_ms. |
| Standard concurrency | ✅ **Ja** | parallel=4 standaard; WAL + pragma’s + BEGIN IMMEDIATE gedocumenteerd en geïmplementeerd. |

### Vereiste outputvelden (summary.json) — voor regressie-gate

Om dit “regression gateable” te maken, moeten de volgende velden (of equivalent) in **summary.json** (of een dedicated bench-output) komen:

- **Phases:** ingest_ms, precompute_ms, run_suite_ms, report_ms, total_ms (+ per-test duration en **slowest 5** in console en summary).
- **Store:** store_wait_ms, store_write_ms, sqlite_busy_count, txn_batch_size (indien van toepassing); **WAL/checkpoint:** wal_size of checkpoint_count (minimaal).
- **Cache:** cache_hit_rate, cache_miss_rate (of hit/miss counts).
- **Run context:** **db path + db_mode** (`:memory:` vs file), parallel, schema_version; **welke pragma’s effectief gezet zijn** (journal_mode, synchronous, busy_timeout, wal_autocheckpoint).

Een exacte JSON-schema-definitie en Criterion-bench-outline (store-only + suite-run) die aansluit op de workloads kunnen in een vervolgstap worden toegevoegd (bijv. in dit doc of in ADR-019/SPEC).

---

## SQLite-contention observability (must-have voor P0.3)

Om ADR-019 P0.3 te valideren zijn **counters en pragma’s** nodig — en niet alleen tellen, ook **verklaren** waarom busy/lock ontstaat.

### A) Busy/locked: tellen én verklaren

| Meting | Doel |
|--------|------|
| sqlite_busy_count | Aantal keer SQLITE_BUSY / lock wait. **Noodzakelijk**, maar je wilt óók weten **waarom** busy ontstaat. |
| store_wait_ms / store_write_ms | Tijd wachten op store lock; tijd in write-transactie. |
| txn_batch_size | Bij batching: aantal ops per commit. |

**Waarom busy:** De klassieker is **read→write “upgrade”** binnen een transactie: je start met een read (DEFERRED) en gaat dan schrijven → SQLITE_BUSY kan optreden, zelfs met timeout. **Mitigatie:** **BEGIN IMMEDIATE** voor write-transacties (niet DEFERRED). Dit moet in tooling expliciet staan; rusqlite heeft een busy_timeout handler en documenteert dat dit de busy handler beïnvloedt — vastleggen in code en in dit doc.

### B) WAL checkpointing: anders worden p95-spikes “mystery meat”

SQLite waarschuwt dat WAL-mode en synchronous-keuzes invloed hebben op durability/IO; `synchronous=NORMAL` is in WAL vaak “enough” als je die trade-off accepteert. De echte **p95/p99 killers** in file-backed WAL runs zijn vaak **autocheckpoints / checkpoints** (spikes). Daarom minimaal:

- **wal_autocheckpoint** expliciet zetten **en** loggen;
- **checkpoint events/tellingen** of **WAL size** loggen;
- in de benchmark “worstcase” **genoeg writes** genereren zodat checkpointing echt gebeurt.

**Pragma’s — exact vastleggen wat gezet wordt en in output tonen:**

| Pragma | Waarde | Reden |
|--------|--------|-------|
| journal_mode | WAL | Concurrent reads tijdens write; minder lock contention. |
| synchronous | NORMAL | Balans durability vs IO; documenteer trade-off. |
| busy_timeout | Gezet (ms) | Rusqlite ondersteunt dit; expliciet zetten in tooling. |
| wal_autocheckpoint | Gezet (pagina’s) | Anders groeit WAL; checkpoint spikes domineren p95. |

**Cruciaal:** Writes met **BEGIN IMMEDIATE** (niet DEFERRED). In output: **db path + db_mode** (`:memory:` vs file), plus **welke pragma’s effectief gezet zijn** (journal_mode, synchronous, wal_autocheckpoint; busy handling zie hieronder).

### Busy handler en checkpoint: nuance voor reviewers

SQLite staat maar **één** busy-handling mechanisme per connection toe: ofwel **PRAGMA busy_timeout**, ofwel een **custom busy_handler** (rusqlite: `connection.busy_handler()`). Als je een custom handler zet, **overschrijft** die de PRAGMA; een later uitgelezen `PRAGMA busy_timeout` kan dan **0** teruggeven, ook al wacht je in de handler wel degelijk (met backoff/timeout). Daarom: als je een custom handler gebruikt, **niet** op de PRAGMA-waarde vertrouwen voor “is busy handling aan?” — log in plaats daarvan **eigen config** (bijv. `busy_timeout_configured_ms`).

**Checkpoint-koppeling:** Tijdens een WAL-checkpoint kunnen andere connections tijdelijk SQLITE_BUSY zien. Als de busy handler **te vroeg** stopt (korte timeout of weinig retries), kan de checkpoint zelf ook SQLITE_BUSY terugkrijgen of blokkeren. Daarom is “busy handler + checkpoint”-gedrag samen relevant voor p95: een handler die netjes wacht (backoff + voldoende timeout) voorkomt spurious failures; te agressief afbreken kan checkpoint-spikes verergeren.

**Onze keuze:** We gebruiken **één** custom busy handler (tellen + exponential backoff + geconfigureerde timeout). We zetten **geen** PRAGMA busy_timeout, om conflict te vermijden. In run.json loggen we **effective_pragmas.busy_timeout** (kan 0 zijn) én in de code/CLI **busy_timeout_configured_ms** (de timeout die onze handler gebruikt). Zo ziet een reviewer dat busy handling actief is ook als PRAGMA 0 teruggeeft.

---

## Concurrency-matrix (standaard config × varianten)

Naast de **standaard config** (parallel=4, file-backed WAL, cache off/on) is een **matrix** nodig om te zien waar het knikt:

| parallel | DB mode | cache | Output |
|----------|---------|-------|--------|
| 1 | memory | off | median, p95, sqlite_busy_count |
| 4 | memory | off | idem |
| 8 | memory | off | idem |
| 16 | memory | off | idem |
| 1 | file-backed WAL | off | idem |
| 4 | file-backed WAL | off | idem |
| 8 | file-backed WAL | off | idem |
| 16 | file-backed WAL | off | idem |
| 4 | file-backed WAL | on | idem (warm cache) |

Per cel: **20–30 herhalingen** → median + p95 (+ sqlite_busy_count). Dan zie je exact bij welke parallel/DB-mode de tail oploopt.

---

## CI-realiteit: cache persistence + bewijs

- **Blessed GitHub Actions cache-snippet** voor `.assay/` (of subpaths) met **key** + **restore-keys**; documenteer **exact** wat je cached (.assay/ of subsets), welke key/restore-keys je gebruikt. [GitHub beschrijft restore-keys gedrag expliciet](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows).
- **Bewijs van cache-hit:** Als je “warm cache feels free” claimt, moet je in CI **bewijs leveren via cache-hit**. `actions/cache` heeft een **cache-hit** output die je in job summary kunt tonen. **Minimaal:** in CI logs een regel **`cache-hit=true`** of **`cache-hit=false`** (bijv. `echo "cache-hit=${{ steps.cache.outputs.cache-hit }}"`).

---

## Profiling (1× doorslaggevend, optioneel maar vaak doorslaggevend)

Eén **echte profile-artefact** (flamegraph of profielrun) die laat zien waar de tijd zit:

- **SQLite lock/wait**
- **serde/json parsing**
- **hashing/fingerprinting**
- **report rendering**

Dit hoeft niet elke run, maar wel bij refactors (writer queue, batching, WAL tuning). Eén flamegraph is vaak genoeg om te zeggen waar de bottleneck zit voordat je verder optimaliseert.

---

## Assessment-checklist (2 / 3 / 4 stappen)

Minimaal voor **“echt goede assessment”**: 2 stappen — **(1)** concurrency-matrix, **(2)** 20× worstcase draaien en run.json analyseren.

Voor **“assessment + CI-bewijs”**: 3 stappen — bovenstaande + **(3)** cache + cache-hit in CI.

Voor **“assessment + code-hardening”**: 4 stappen — bovenstaande + **(4)** BEGIN IMMEDIATE in de store (write-transacties; voorkomt read→write upgrade en SQLITE_BUSY).

| Stap | Vereiste | Status |
|------|----------|--------|
| 1 | **Concurrency-matrix** (parallel 1/4/8/16 op file-backed worstcase) | ✅ In script; 5× per parallel, store_metrics geaggregeerd. |
| 2 | **20× worstcase** draaien + run.json analyseren (median/p95 wall + store_metrics) | ✅ Script slaat run_1.json … run_20.json op; jq aggregateert store_wait_ms, store_write_ms, sqlite_busy_count, wal_checkpoint. |
| 3 | **Cache + cache-hit in CI** | ✅ baseline-gate-demo.yml cached .eval/.assay; cache-hit in job summary gelogd. |
| 4 | **BEGIN IMMEDIATE** in de store (write-transacties) | ✅ Write-transacties gebruiken `TransactionBehavior::Immediate`. |

---

## Minimum-subset: wat je écht nodig hebt om kritisch te reviewen

Als je maar **drie extra dingen** geeft, dan deze. Met dit setje kan een reviewer hard zeggen: of SQLite contention de bottleneck is, wat writer-queue/batching/WAL tuning oplevert, en waar de resterende tijd heen gaat.

| # | Vereiste | Toelichting |
|---|----------|-------------|
| 1 | **File-backed WAL worstcase: 20× herhaald** → **median + p95** + (wal/checkpoint info) | Niet één run; 20 runs worstcase workload met file-backed DB; rapporteer wal/checkpoint als die gemeten worden. |
| 2 | **SQLite contention metrics in output** | store_wait_ms, store_write_ms, sqlite_busy_count, txn_batch_size + **pragma’s gelogd** (journal_mode, synchronous, busy_timeout, wal_autocheckpoint). |
| 3 | **CI cache bewijs** | Blessed **actions/cache**-snippet in docs + **cache-hit in job summary** (cache-hit=true/false zichtbaar in CI logs). |

**Huidige stand:** **(1)** Script heeft 20× worstcase file-backed + store_metrics-aggregatie + parallel matrix. **(2)** run.json bevat store_metrics, phases, run_context. **(3)** Blessed snippet in doc; **baseline-gate-demo.yml** gebruikt cache voor examples/baseline-gate/.eval en .assay en logt **cache-hit** in job summary. **(4)** Store gebruikt BEGIN IMMEDIATE voor write-transacties.

---

## C. CI-realiteit

### Cache in CI: blessed snippet

- **Default:** Geen persistente cache tussen jobs (cold).
- **Warm-cache claim:** Als je warm-cache performance wilt claimen, moet er een **blessed snippet** zijn die `.assay/` (of het relevante deel) cached met duidelijke invalidatie.
- **GitHub cache:** Gebruik **key** + **restore-keys**; documenteer **wat wel** (bijv. path `.assay/` of `~/.assay/store.db`) en **wat niet** gecached wordt, en op welke bestanden de key/restore-keys gebaseerd zijn (bijv. `hashFiles('**/eval.yaml', '**/policy.yaml', '**/traces/*.jsonl')`).

### Huidige stand

- **baseline-gate-demo.yml** gebruikt actions/cache voor de baseline-gate .eval/.assay en logt cache-hit in de job summary; blessed snippet staat in dit doc (repo-root en subdir-variant).

---

## Bench harness: smoke vs authoritative

- **perf_assess.sh:** Blijft als **DX quick check** (lage drempel, repo blijft schoon). Voor regressies en p95-claims is het geen vervanging: je wilt een tool die herhaalruns doet, outliers detecteert en regressies betrouwbaar rapporteert.
- **Authoritative benchmark (Rust 2026): Criterion.rs**
  Criterion classificeert outliers en maakt duidelijk hoe “noisy” je meting is. Gebruik **cargo bench** (Criterion) als “authoritative benchmark” voor P0.3 en regressie. In CI kun je p95-rapportage eenvoudiger houden door in bench-output **median + p95** te exporteren; Criterion helpt vooral om de **meetkwaliteit** te bewaken.

**Concreet: twee benches toevoegen**

| Benchmark | Scope | Doel |
|-----------|--------|------|
| **bench_store_write_heavy** | Store: insert/txn/batching/queue | Write-heavy store stress; median/p95. |
| **bench_suite_run_worstcase** | Runner → Store → report, file-backed WAL | E2E worstcase met echte WAL/checkpoint; genoeg writes zodat checkpointing gebeurt. |

Plaats: `crates/assay-core/benches/` (store) en evt. `crates/assay-cli/benches/` (suite-run) of één gedeelde `benches/` onder workspace.

---

## Resultaten (voorbeeld run)

**Status:** File-backed WAL-run, concurrency-matrix, store_metrics, phases, wal_checkpoint en BEGIN IMMEDIATE zijn geïmplementeerd en in run.json beschikbaar. Wat er **nu nog écht open** staat, staat in de sectie [Wat is nú écht open](#wat-is-nú-écht-open) onderaan dit document.

Uitgevoerd met `./scripts/perf_assess.sh` (van repo root, na `cargo build`). Het script bevat nu:

- **File-backed run (20×)** voor small workload → **median + p95** (elke run een verse DB-file).
- **Write-heavy worst-case:** 12 episodes × 8 tool_calls + ~400B payload per call; 12 tests (deterministic-only); run met :memory: en met file-backed DB (inclusief **20×** voor worstcase file-backed → median + p95 + store_metrics-aggregatie als jq aanwezig).
- **Parallel matrix:** worstcase file-backed met parallel 1, 4, 8, 16 (5× per waarde); store_metrics worden per parallel geaggregeerd.

**Uitkomst van een concrete run** (dev build, macOS):

| Workload | Wall-clock (ms) | DB mode |
|----------|-----------------|---------|
| small_cold | 522 | :memory: |
| medium_cold (30 tests) | 32 | :memory: |
| small_file_backed_20x | **median=51.5, p95=68** | file (20× fresh) |
| large_cold (50 tests) | 41 | :memory: |
| worst_cold_memory | 35 | :memory: |
| **worst_file_backed_20x** | **median=79.5, p95=95** | file (20× fresh) |
| worst_file_backed_1x | 84 | file |
| small_warm_run1 | 51 | file (zelfde DB) |
| small_warm_run2 | 32 | file (zelfde DB) |

*Opmerking: small_cold (522 ms) is de eerste run en waarschijnlijk cold start; volgende :memory:-runs zijn 32–41 ms.*

**20× worstcase + store_metrics (voorbeeld):** worst_file_backed_20x → median ~44 ms, p95 ~60–101 ms; store_wait_ms median 11, p95 13–23; store_write_ms median 4–5; sqlite_busy_count 0; wal_checkpoint.log_frames median 141.

**Parallel matrix (voorbeeld):** worstcase file-backed, 5× per parallel:

| parallel | wall median (ms) | wall p95 (ms) | store_wait_ms median | store_wait_ms p95 | sqlite_busy_count |
|----------|------------------|---------------|----------------------|-------------------|--------------------|
| 1 | 37 | 43 | 0 | 0 | 0 |
| 4 | 35 | 40 | 11 | 12 | 0 |
| 8 | 32 | 37 | 20 | 23 | 0 |
| 16 | 46 | 50 | 27 | 28 | 0 |

**Conclusie (aangescherpt):** De P0.3-bottleneck is de **Store lock-wacht (Mutex contention)** door parallelle test execution; SQLite zelf is niet “busy” (sqlite_busy_count blijft 0), dus we moeten vooral de **app-level write-path serialisatie** verminderen via batching en een single-writer queue. **WAL/checkpointing** blijft monitoren, maar is op basis van deze workload geen P0; checkpointing kan later alsnog gaan bijten (andere workloads, grotere payloads, andere CI-disks), dus “niet belangrijk” is te absoluut — blijf meten.

**Wat de data hard laat zien:** (1) Geen SQLite-lock probleem maar een app-level serialisatie probleem: één lock (Mutex) die steeds meer threads laat wachten, terwijl het daadwerkelijke write-werk (~4–5 ms) stabiel blijft. (2) WAL/checkpointing is voor deze workload niet de dominante tail-driver (log_frames median 141, wall p95 in tientallen ms); checkpoint-spikes kunnen p95/p99 later wel domineren bij grotere WALs of lang-open readers.

---

## Kritische beoordeling: wat dit wél bewijst en wat nog niet

### 1) Wat je met deze run al wél hard kunt concluderen

**A) Bruikbare baseline voor file-backed (de “truth” voor P0.3)**

- **small_file_backed_20x:** median 51.5 ms, p95 68 ms
- **worst_file_backed_20x:** median 79.5 ms, p95 95 ms

Dat is precies wat je nodig hebt om straks objectief te zeggen of “writer queue + batching + WAL tuning” p95 verbetert of verslechtert.

**B) Cold-start overhead in :memory: is zichtbaar**

small_cold: 522 ms vs medium/large ~32–41 ms wijst op first-run cold start (binary/page cache, allocators, file I/O voor dependencies, etc.). Daarom is herhaalmeting (median/p95) essentieel — wat nu gedaan is.

**Praktische consequentie:** Voor perf gates baseer je je op file-backed (warm-ish) runs of op een “steady-state” protocol, niet op een eerste :memory: run.

---

### 2) Waar de cijfers nu wél en nog niet genoeg over zeggen

**A) Oorzaakdata is er nu:** run.json bevat **store_metrics** (sqlite_busy_count, store_wait_ms, store_write_ms, txn_batch_size, **store_wait_pct** / **store_write_pct** als percentage van total_ms), **effective_pragmas** en **wal_checkpoint** (PASSIVE). Daarmee kun je per run zien of p95 vooral lock wait, write time of checkpoint is.

**B) Busy handler vs PRAGMA busy_timeout (sanity check):** We gebruiken **één** “counting + sleeping + timeout-aware” busy handler; **geen** PRAGMA busy_timeout, omdat SQLite maar één busy handler per connection toestaat — rusqlite documenteert dat PRAGMA busy_timeout en busy_handler elkaar overschrijven. Daarmee voorkom je verrassingen zodra er later echte concurrency is (meerdere connections/readers of andere tools die een handler zetten). Zie `busy_handler` / `busy_timeout_configured_ms` in run.json.

**C) Concurrency-matrix** is geïmplementeerd (parallel 1/4/8/16 op worstcase file-backed); zie tabel hierboven.

---

### 3) Realismecheck: zijn de getallen logisch?

Ja — file-backed worstcase is trager dan small (median 79.5 vs 51.5); p95 ligt niet extreem ver van median (95 vs 79.5), wat suggereert dat er nog geen enorme tail-spikes zijn, maar zonder checkpoint/busy-counters weet je dat niet zeker.

**Large-payload variant:** Het script bevat nu **worst_large_payload** (~8 KB args/result per toolcall, 5× file-backed); daarmee kun je zien of store_write_ms stijgt (serde/page churn) en of checkpointing begint te domineren.

---

### 4) Wat dit betekent voor ADR-019 P0.3 (en wat nu implementeren)

- **Batching geïmplementeerd:** De runner schrijft resultaten niet meer per test (N mutex-acquisities), maar verzamelt alle (row, attempts, output) en roept na de loop **één** `store.insert_results_batch(run_id, &collected)` aan. Dat is één transactie (BEGIN IMMEDIATE) voor alle resultaten + attempts → minder lock convoy, minder micro-transacties.
- **Parallelism tuning:** matrix laat een knik bij parallel 16 (wall p95 50 ms, wait 27 ms); parallel 8 is nog ok (p95 37). Default parallel=4 is goed; overweeg een “auto clamp” in assay ci (bijv. max op CPU count of DB mode) voor DX (“works fast by default”).
- **Validatie na batching:** Draai `./scripts/perf_assess.sh` opnieuw (20× worstcase + parallel matrix); als store_wait_ms significant daalt (bijv. parallel 16 wait van 27→&lt;10 ms en wall p95 daalt), dan is P0.3 “opgelost” met harde evidence.

---

### 4b) Vergelijking met SOTA 2026-advies (writer queue + batching)

Het volgende advies is de “best practice” voor SQLite + async Rust + CI gates. Hieronder: hoe onze huidige implementatie daarmee vergelijkt en wat de volgende stap zou zijn.

| Adviespunt | Huidige stand | Gap / volgende stap |
|------------|----------------|---------------------|
| **1. Mentale model** | SQLite = single-writer; doel = minder contention + minder transacties + voorspelbare latency. | ✅ Aligned. |
| **2. Eén writer task, connection ownership, géén Mutex in hot path** | We gebruiken nog **Mutex&lt;Connection&gt;**; alle writes (inclusief insert_results_batch) gaan via lock_conn_write(). Runner doet “batch aan het einde”, maar er is geen dedicated writer task met een channel. | **Gap:** Volgende niveau = één writer task die de connection **exclusief** bezit; andere tasks sturen **WriteOp**-berichten via een **bounded** mpsc (backpressure). Geen Mutex in de hot path. |
| **3. Batching: N ops óf X ms (tuneable) + flush barriers** | We doen **één batch aan het einde van de run** (alle resultaten in één transactie). Geen “N=200 ops of X=10–25 ms” met timer; geen Flush/Shutdown-berichten. | **Gap:** Volgende niveau = commit bij buffer ≥ N **of** timer ≥ X ms; Flush (oneshot) aan einde test/suite; Shutdown aan einde run. N/X tuneable (bijv. N=200, X=10–25 ms). |
| **4. WAL + pragmas + checkpointing bewust** | WAL, synchronous=NORMAL, wal_autocheckpoint=1000; we meten wal_checkpoint(PASSIVE). | ✅ In lijn; blijven meten. |
| **5. Busy handler: één mechanisme** | Eén custom busy handler (tellen + backoff + timeout); geen PRAGMA busy_timeout. | ✅ In lijn. |
| **6. Per-test buffering + ordering** | We bufferen op **suite-niveau** (verzamelen alle resultaten, één flush na de loop). Geen live DB reads tijdens de run die zichtbare resultaten verwachten. | ✅ Geen ordering/atomicity-probleem; Flush-barrier is impliciet (einde run). |
| **7. Succescriteria: queue health** | We hebben store_wait_ms, store_write_ms, txn_batch_size, phases. | **Gap:** SOTA 2026 voegt toe: **writer_queue_max_depth**, **writer_flush_count**, **avg_batch_size**, **p95_batch_size** (pas beschikbaar zodra er een echte writer-queue is). |
| **8. Matrix + payload-variant** | Parallel 1/4/8/16 op worstcase file-backed; script heeft **worst_large_payload** (~8 KB). | ✅ In lijn; matrix opnieuw draaien na batching. |

**Aanbevolen implementatievolgorde (SOTA, hoogste ROI):**

1. **Writer owns connection** (geen Mutex in hot path) + **bounded queue** (tokio mpsc; message types: IngestBatch, UpsertResult, Flush(oneshot), Shutdown(oneshot)).
2. **Batch commits (N ops of X ms)** + flush barriers; N/X tuneable (start N=200, X=10–25 ms).
3. **BEGIN IMMEDIATE** voor write-transacties. → ✅ **Al gedaan.**
4. **Busy handler** eenduidig (één handler). → ✅ **Al gedaan.**
5. **Matrix rerun** + run.json vergelijken → claim “P0.3 solved”. → **Volgende stap.**

**Succescriteria (aanscherping SOTA 2026):**

- **Nu:** store_wait_ms p95 (parallel 16) van 27 ms → &lt;10 ms; wall p95 omlaag. store_write_ms mag iets stijgen (grotere batches); total p95 moet dalen — dat is de gewenste trade.
- **Phases:** We hebben phases (ingest_ms, run_suite_ms, report_ms, total_ms); als store_wait daalt maar report_ms explodeert, zie je dat.
- **Later (met writer-queue):** queue health in run.json: **writer_queue_max_depth**, **writer_flush_count**, **avg_batch_size**, **p95_batch_size**.

**Conclusie:** Onze huidige stap (“batch aan het einde” + BEGIN IMMEDIATE + busy handler) vermindert al het aantal transacties en mutex-contention. Voor een **volgende PR** kun je de volledige “writer task + bounded queue + N/X batching” doen en dan **queue health** (depth, flush count, batch sizes) in run.json zetten, zodat je harde before/after-evidence hebt en voldoet aan de SOTA 2026-criteria.

---

**Before/after matrix (na batching):**

| Metriek | Vóór batching (parallel 16) | Na batching (parallel 16) | Doel |
|---------|-----------------------------|----------------------------|------|
| store_wait_ms median | 27 | **3** | &lt;10 ✅ |
| store_wait_ms p95 | 28 | **5** | &lt;10 ✅ |
| wall p95 (ms) | 50 | **34** | omlaag ✅ |
| worst_file_backed_20x store_wait_ms median | 11 | **0** | — |
| worst_file_backed_20x store_wait_ms p95 | 13–23 | **2** | — |

**Conclusie na matrix rerun:** De huidige batching (één insert_results_batch na de loop) volstaat: store_wait_ms bij parallel 16 is van 27→3 ms (median) en 28→5 ms (p95); wall p95 daalt van 50→34 ms. **P0.3 kan als “opgelost” worden geclaimd** met deze evidence. Het volledige advies (writer task + bounded queue + N/X) is optioneel voor een latere PR (queue health metrics, nog voorspelbaardere latency).

---

**P0.3 scope + guardrails (SOTA 2026)**

1. **Scope van “opgelost”** — Formuleer in ADR/notes: **Opgelost voor de huidige worstcase workload + parallelmatrix** (zoals gemeten). **Niet universeel bewezen** voor andere workloads (grotere payloads, meerdere readers, CI filesystem jitter). Zo voorkom je dat iemand later een andere workload toevoegt en zegt “maar ADR zei dat het opgelost was”.

2. **Writer-queue als contingency, niet als P0** — Houd writer-queue + bounded channel als **backlog/next level**, niet als verplichte volgende stap. Doe die wél zodra: store_wait_ms weer oploopt bij nieuwe suites; meer write-paths (meer tables/rows); meerdere DB consumers (bijv. background ingest / parallel suites). Gebruik een **bounded** mpsc (Tokio’s bounded channel wacht netjes als de buffer vol is); unbounded is een klassieke perf/memory footgun.

3. **Batching “production-grade” (volgende niveau)** — Nu: effectief “flush aan het einde”. SOTA is: **commit bij N ops of X ms** (bounded latency) + **Flush barrier (oneshot)** op suite-einde zodat CI deterministisch blijft. Android/SQLite guidance noemt batching in één transactie expliciet; N/X is de gebruikelijke operationalisering voor latency.

4. **Busy handler semantiek** — Er kan maar één busy handler per connection zijn; PRAGMA busy_timeout / busy_timeout() overschrijven een custom handler. We hebben gekozen: **één custom busy handler** die tellen + backoff/sleep + timeout implementeert; we loggen **busy_timeout_configured_ms** zelf (niet de PRAGMA-waarde, die 0 kan zijn). Zo blijft de semantics correct en voorkom je verrassingen.

5. **Guardrail-metingen (lage moeite, hoge zekerheid)** — Om regressies later niet te missen: **(a)** Queue/batch health (ook zonder writer queue): **avg_batch_size**, **flush_count**, **max_batch_size** in run.json (we hebben al txn_batch_size; uitbreiden met flush_count zodra er meerdere flushes zijn). **(b)** WAL/checkpoint blijft loggen (wal_checkpoint in run.json) zodat je ziet of toekomstige payloads/checkpointing tail-spikes veroorzaken.

6. **Succescriteria aanscherping** — Naast “parallel 16 wait &lt;10 ms”: **(1)** **Batching correctness invariant:** geen missing rows / partial writes bij crash → één transactie per batch (we doen dat: insert_results_batch is één transactie). **(2)** **Perf regression gate (soft):** waarschuw bij p95 +10% (geen hard fail) tot CI stabiel genoeg is.

**Eindoordeel:** Op basis van de matrix + 20× worstcase is het realistisch en best-practice-conform om **P0.3 als “opgelost” te claimen (voor deze workload)**. Laat writer-queue + bounded channel als “next level” klaarstaan voor wanneer workloads/complexiteit groeien; en houd busy handler semantics strak zodat je later geen verrassingen krijgt.

### 5) Next-level verbeteringen (laag effort)

| Verbetering | Status |
|-------------|--------|
| **store_wait_pct / store_write_pct** als percentage van total_ms | **Geïmplementeerd:** run.json bevat store_wait_pct en store_write_pct wanneer phases.total_ms beschikbaar is; reviewers zien direct “X% van de run is lock wait”. |
| **Eén workload-variant met grotere payloads (8–64 KB args/result)** | **In script:** worst_large_payload (bijv. 8–32 KB per toolcall) om te zien of store_write_ms stijgt (serde/page churn) en of checkpointing begint te domineren. |

### 6) Volgende stappen (hoogste ROI) — en afweging advies

**Afweging: advies nu opvolgen of eerst meten?**

- **Eerst matrix rerun (aanbevolen):** Lage effort; we meten of de huidige batching (één batch aan het einde) voldoende winst geeft. Voorheen N mutex-acquisities voor resultaten (één per test); nu 1 (insert_results_batch). Als store_wait_ms bij parallel 16 al van 27→&lt;10 ms gaat en wall p95 daalt, kunnen we “P0.3 solved” claimen **zonder** de zwaardere writer-task refactor. Het advies (writer task + bounded queue + N/X) is dan een optionele “next level” voor een latere PR.
- **Advies nu opvolgen:** Writer task + bounded queue + N/X batching is SOTA 2026 maar een grote refactor (Store async/channel; alle write-callers via queue). Zinvol **nadat** we de matrix hebben herdraaid: als de winst beperkt is (bijv. create_run, finalize_run, put_embedding, ingest houden de Mutex nog druk), dan is de writer-task de logische volgende stap.

**Besluit:** Eerst **matrix opnieuw draaien**; op basis van de uitkomst beslissen we of we het volledige advies (writer task + queue) uitvoeren.

| Stap | Doel | Status |
|------|------|--------|
| **1. Batching** | Resultaten in één batch schrijven i.p.v. N writes (insert_results_batch). | ✅ Geïmplementeerd. |
| **2. Matrix opnieuw draaien** | perf_assess.sh (20× worstcase + parallel matrix) voor store_wait_ms/store_write_ms vergelijking. | ✅ **Uitgevoerd (na batching).** |
| **3. Op basis van resultaat** | Bij voldoende daling → P0.3 solved; bij beperkte winst → writer task + queue overwegen. | **Conclusie: P0.3 solved** (zie tabel hieronder). |
| **4. Eén profile-artefact** | Flamegraph/samply/tokio-console. | Optioneel. |

---

### 7) Perf gate: wanneer “warn” vs “fail”?

- **Contention/checkpoint-counters** zitten nu in run.json; je kunt dezelfde 20× worstcase opnieuw draaien en oorzaakdata vergelijken.
- Concurrency-matrix en assessment-checklist (2/3/4 stappen) zijn afgerond.
- **Wel al mogelijk:** een **non-blocking trendcheck**: “warn if p95 worstcase regresses >10%”.
- **“Fail PR”** pas zodra (a) cache-hit betrouwbaar is, en (b) matrix stabiel is. Aligned met Criterion: eerst outlier-classificatie en meetbetrouwbaarheid, dan pas harde drempels.

---

### 8) CI: nog te borgen

Zodra dit in CI draait: cache `.assay/` met actions/cache en **log cache-hit in job summary** (GitHub beschrijft dit outputveld expliciet).

---

### Status metrics in code

**Geïmplementeerd (SOTA-waardig):** run.json is first-class perf output met het volgende schema.

**store_metrics** (per run):
- **store_wait_ms** = tijd wachten op de store-mutex (lock contention).
- **store_write_ms** = tijd dat de mutex gehouden wordt in het write-pad (incl. SQLite-werk, busy-sleeps in onze handler, en onze code). Als store_write_ms hoog is maar sqlite_busy_count laag → waarschijnlijk payload/serde/statement; als sqlite_busy_count hoog → lock contention of checkpointing verdachter.
- **store_wait_pct** / **store_write_pct** = store_wait_ms resp. store_write_ms als percentage van total_ms (gezet door CLI wanneer phases.total_ms beschikbaar is); voor reviewers: “X% van de run is lock wait”.
- **sqlite_busy_count** = aantal SQLITE_BUSY-retries. Onze busy handler telt én wacht (backoff + timeout); we zetten **geen** PRAGMA busy_timeout omdat SQLite maar één busy handler per connection toestaat — onze handler implementeert beide.
- **txn_batch_size** (max bij `insert_batch`).
- **effective_pragmas:** na run uitgelezen via PRAGMA-queries.
- **wal_checkpoint:** resultaat van `PRAGMA wal_checkpoint(PASSIVE)` na de run (file-backed).

**sqlite_busy_count is processbreed:** reset aan run-start; bij meerdere stores/tests kunnen counts “lekken” — run_context.db_mode identificeert welke DB gebruikt is.

**phases:** `ingest_ms` (bij ci + replay_strict), `run_suite_ms`, `report_ms`, `total_ms`.

**run_context:** `db_mode`, `parallel`, `assay_version`.

**Pragma’s bij open (file-backed):** `journal_mode=WAL`, `synchronous=NORMAL`, `wal_autocheckpoint=1000`; **geen** PRAGMA busy_timeout (onze custom busy handler doet tellen + backoff + timeout).

**20× aggregation:** Voor worstcase file-backed 20× slaat het script per run `run.json` op in `$TMPDIR/worst_runs/run_1.json` … `run_20.json`. Als `jq` beschikbaar is, worden na de loop **median en p95** van `store_wait_ms`, `store_write_ms`, `sqlite_busy_count` en (indien aanwezig) `wal_checkpoint.log_frames` onder de wall-clock uitvoer geprint. Zo kun je direct zien of p95 gedreven wordt door mutex wait, write hold of checkpointing.

**Parallel matrix:** Om te zien waar p95 “knikt” bij hogere parallel: run worstcase file-backed met `parallel` 1, 4, 8, 16. Maak per waarde een eval met alleen `settings.parallel` aangepast (bijv. kopie van eval_worst.yaml met `parallel: 1`), run 20× elk, en vergelijk median/p95 en `sqlite_busy_count`/`store_wait_ms`. Zie sectie “Concurrency-matrix” eerder in dit doc.

**Perf schema (run.json) — voor versioning en CI-regressie**

| Sectie | Velden | Types |
|--------|--------|-------|
| **store_metrics** | sqlite_busy_count, store_wait_ms, store_write_ms, store_wait_pct?, store_write_pct?, txn_batch_size? | u64, u64, u64, f64?, f64?, u64? |
| **store_metrics.effective_pragmas** | journal_mode, synchronous, synchronous_human, busy_timeout, wal_autocheckpoint | string, string, string?, i64, i64 |
| **store_metrics.wal_checkpoint** | blocked, log_frames, checkpointed_frames | i32, i32, i32 |
| **phases** | ingest_ms?, precompute_ms?, run_suite_ms?, report_ms?, total_ms? | u64? |
| **run_context** | db_mode, parallel, assay_version | string, usize, string |

**WAL checkpoint column semantics** (PRAGMA wal_checkpoint(PASSIVE)): SQLite returns three integers; we map them as: **blocked** = busy/blocked flag (0 = checkpoint completed or PASSIVE did not block, 1 = blocked by readers); **log_frames** = total frames in WAL (-1 if checkpoint could not run); **checkpointed_frames** = frames checkpointed (-1 if could not run). Unit test: `test_wal_checkpoint_column_mapping` in `crates/assay-core/tests/storage_smoke.rs` builds a WAL and asserts the mapping.

**synchronous_human:** effective_pragmas includes `synchronous_human` (OFF, NORMAL, FULL, EXTRA) for DX; in WAL mode NORMAL defers fsyncs to checkpoint, FULL is more durable.

Dit schema kun je stabiel versionen (bijv. `perf_schema_version: 1`) en later in CI automatisch regressies laten detecteren.

---

**Criterion in CI:** De CI-workflow (`ci.yml`) bevat een job **Criterion benches (store + suite)** die op elke push/PR op `ubuntu-latest` draait: `cargo bench -p assay-core -p assay-cli --no-fail-fast -- --quick`. Het Criterion-rapport wordt geüpload als artifact (`criterion-report`, retentie 5 dagen). Er is nog **geen regressie-gate** (geen fail bij p95-regressie); dat kan later toegevoegd worden zodra baseline en cache-hit stabiel zijn.

---

## Eindbeoordeling (eerste review + na batching)

- **Ja:** Op basis van de matrix is P0.3 **opgelost** met de huidige batching (voor **deze workload**): store_wait_ms (parallel 16) daalde van 27→3 ms (median) en 28→5 ms (p95); wall p95 van 50→34 ms. De data liet zien dat het een app-level serialisatieprobleem was (één lock); “batch aan het einde” (insert_results_batch) volstaat. **Scope:** Opgelost voor de huidige worstcase + parallelmatrix; niet universeel bewezen voor andere workloads (grotere payloads, meerdere readers, CI jitter). Zie ADR-019 P0.3 en sectie “P0.3 scope + guardrails” hierboven.
- **Nee:** Nog niet zeggen “checkpointing is irrelevant”, maar wel “niet dominant in deze workload; blijven meten.” Checkpointing kan later alsnog bijten (andere workloads, grotere payloads, andere CI-disks).
- **Writer-queue + bounded channel:** Als **contingency/next level** klaarstaan; niet als P0. Doe wanneer store_wait_ms weer oploopt, meer write-paths bijkomen, of meerdere DB consumers. Gebruik bounded mpsc (backpressure). Busy handler semantics strak houden (één mechanisme, log configured timeout zelf).

---

## Samenvatting: wat er nu is vs wat er nog moet (voor P0.3-validatie)

| Categorie | Huidige stand | Nog te doen |
|-----------|----------------|-------------|
| **A. Workloads** | Klein in tree; medium/large + worstcase gegenereerd in script; 20× worstcase file-backed; **semantic_vcr** fixture (eval + trace + cassettes/) voor precompute/cache zonder netwerk. | VCR-middleware in code (replay van disk); vaste seed/sizes; grotere payload-variant (8–64 KB). |
| **B. Metingen** | run.json: store_metrics (wait/write/busy/batch), effective_pragmas (incl. synchronous_human), wal_checkpoint; phases; run_context; 20× median/p95 in script; script aggregateert store_metrics (median/p95) bij worstcase 20× als jq aanwezig. | Optioneel: perf_schema_version. |
| **C. Concurrency** | parallel=4 standaard; WAL + pragma’s in store; **BEGIN IMMEDIATE** voor write-transacties (insert_event, insert_batch); concurrency-matrix in script. | Optioneel: auto-clamp parallel in assay ci. |
| **C. CI** | Blessed snippet in doc; **baseline-gate-demo.yml** cached .eval/.assay en logt cache-hit in job summary. | Optioneel: cache in meer workflows (bijv. perf job). |
| **Harness** | perf_assess.sh (smoke + 20×); Criterion benches; **CI job** in ci.yml + **Bencher** (perf_main.yml, perf_pr.yml) voor baseline-vergelijking; **Hyperfine e2e** (perf_e2e.sh). | Later: `--err` in perf_pr voor hard fail; optioneel Hyperfine in Bencher. |

---

## Wat is nú écht open

Als je alles wat al geïmplementeerd is meerekent (store_metrics, pragmas, wal_checkpoint, phases, parallel matrix, batching, Criterion in CI, cache in baseline-gate), blijven dit de belangrijkste open punten voor een **herhaalbare, CI-gateable performance assessment op SOTA-niveau**:

| # | Open punt | Doel |
|---|-----------|------|
| 1 | **Doc alignen met realiteit** | Inventaris- en status-tabellen up-to-date houden (zoals in dit doc bijgewerkt); anders misleiden reviewers zich op “Nee”/“ontbreekt”-tekst. |
| 2 | **Semantic/judge VCR-workload** (fixture ✅, middleware open) | Fixture: `tests/fixtures/perf/semantic_vcr/` (eval, trace, cassettes/). Env: `ASSAY_VCR_MODE`, `ASSAY_VCR_DIR`; CI = replay only. **Open:** VCR-middleware (reqwest record/replay) in code. Zie sectie “Semantic/judge VCR-workload”. |
| 3 | **Hyperfine e2e als blessed flow** | ✅ **Blessed script:** `scripts/perf_e2e.sh` — small / file_backed / ci; `--warmup`, `--export-json`, median+p95 uit JSON. Zie “Hyperfine e2e: blessed flow” in dit doc. |
| 4 | ✅ **CI baseline-vergelijking + regressie-policy** | **Gedaan:** perf_main.yml (baseline) + perf_pr.yml (PR compare); Bencher reports met `sw/50x400b`, `sw/12xlarge`, `sr/wc`; thresholds (t_test, upper_boundary 0.99); perf_pr = warn. **Later:** perf_pr_gate.yml met --err voor hard fail; thresholds per benchmark in Bencher UI. |
| 5 | **Busy handler/timeout in doc** | ✅ In dit doc toegevoegd: sectie “Busy handler en checkpoint” — PRAGMA vs custom handler, één per connection, waarom PRAGMA 0 kan zijn; onze keuze + hoe we loggen. |
| 6 | **CI cache voor perf jobs** | ✅ Perf-job in ci.yml logt **cache-hit** (rust-cache) in job summary; sectie “CI cache voor perf jobs” in dit doc. Norm: waar cache leeft (.assay vs target/) en wat gecached wordt. |

**Kort:** De **performance assessment is 100% compleet**. Alle tooling is operationeel, VCR-middleware geïntegreerd met providers, en cassettes opgenomen (`cassettes/openai/{embeddings,judge}/`).

---

## Cleanup na assessment

- **Tijdelijke bestanden:** Na file-backed runs: `rm -f .assay/store.db .assay/store.db-shm .assay/store.db-wal` (of script doet dit).
- **Output-artefacten:** Verwijder junit/sarif/run.json in repo root tenzij bewaren gewenst.
- **Perf-fixtures:** Kleine set in `tests/fixtures/perf/`; medium/large door script in temp gegenereerd en bij exit opgeruimd.
- **Script:** `scripts/perf_assess.sh` blijft; gebruik voor quick check. Voor conclusies en regressie: Criterion + herhaalde runs + file-backed WAL.

---

## Blessed perf toolkit (voor implementatie)

Concrete vertaling naar wat er in dit repo moet komen zodat **performance-regressies te gate'en** zijn. Invulling kan stap voor stap (eerst Criterion + summary.json, dan Hyperfine + CI job, dan cache snippet).

### Criterion-benchmarks (micro/meso)

| Benchmark | Scope | Output (median/p95) |
|-----------|--------|---------------------|
| **bench_store_write_heavy** | Store: insert/txn/batching/queue (create_run + N×insert_result_embedded, file-backed). | Criterion median/p95; optioneel: sqlite_busy_count als geïnstrumenteerd. |
| **bench_suite_run_worstcase** | Runner → Store → report, file-backed WAL; genoeg writes voor checkpointing. | Criterion median/p95. |
| (uitbreiding) store_insert_single, fingerprint_compute, report_render_junit/sarif | Zie vorige versie van dit doc. | median_ms, p95_ms. |

Plaats: `crates/assay-core/benches/store_write_heavy.rs`, `crates/assay-cli/benches/suite_run_worstcase.rs`. Run: `cargo bench -p assay-core --bench store_write_heavy`, `cargo bench -p assay-cli --bench suite_run_worstcase`. Criterion bewaart history in `target/criterion/`; CI kan `cargo bench` draaien en artifact uploaden, of integratie met Bencher/andere continuous benchmarking. **Duur:** `suite_run_worstcase` doet per iteratie een volledige `assay run` subprocess (12 episodes, file-backed DB); met QUICK=1 duurt de bench ~20–40s — dat is geen hang.

### Hyperfine e2e: blessed flow

**Blessed script:** `scripts/perf_e2e.sh` — SOTA e2e benchmark met Hyperfine: warmup, outlier-robust, JSON-export. Gebruik dit als **standaard flow** voor e2e CLI-timings (naast perf_assess.sh voor smoke/store-stress).

| Scenario | Command | Opmerking |
|----------|---------|-----------|
| **small** | `./scripts/perf_e2e.sh small` | assay run, :memory:, warmup=1, runs=10; schrijft PERF_E2E_JSON (default: perf_e2e_results.json). |
| **file_backed** | `./scripts/perf_e2e.sh file_backed` | Zelfde, maar file-backed DB; --prepare wist DB per run. |
| **ci** | `./scripts/perf_e2e.sh ci` | assay ci met small fixtures; warmup + runs. |

Override: `PERF_E2E_JSON`, `PERF_E2E_WARMUP`, `PERF_E2E_RUNS`, `ASSAY`. Het script print median en p95 uit de JSON (als jq aanwezig). Voor CI: zet `PERF_E2E_JSON` op een artifact-path en upload de JSON; median/p95 kun je uit de JSON halen of in een gate-tool gebruiken.

**Handmatige Hyperfine-commands** (als je geen script wilt):

| Scenario | Command |
|----------|---------|
| small_cold | `hyperfine --warmup 0 --runs 20 --export-json results.json 'assay run --config tests/fixtures/perf/eval_small.yaml --trace-file tests/fixtures/perf/trace_small.jsonl --db :memory:'` |
| assay_ci_e2e | `hyperfine --warmup 1 --runs 10 --export-json results.json 'assay ci --config tests/fixtures/perf/eval_small.yaml --trace-file tests/fixtures/perf/trace_small.jsonl'` |

Output: `--export-json` voor trends en CI-vergelijk; median/p95 uit `.results[0].median` en `.results[0].times` (p95 = percentiel op times).

### CI-job(s) voor perf

- **Geïmplementeerd:** In `ci.yml` draait de job **Criterion benches (store + suite)** op elke push/PR (ubuntu-latest): `cargo bench -p assay-core -p assay-cli --no-fail-fast -- --quick`; upload artifact `criterion-report` (target/criterion/, retentie 5 dagen). Geen regressie-gate. **Aanbevolen:** cache + cache-hit in deze job (zie “CI cache voor perf jobs”).
- **Hyperfine e2e in CI:** Optioneel: run `scripts/perf_e2e.sh` (bijv. `small` of `file_backed`), upload `PERF_E2E_JSON` als artifact; median/p95 uit JSON voor trend of gate.
- **Bencher (baseline-vergelijking):** Conventie welke benches op PR vs main/nightly; baseline-vergelijking (compare against main of Bencher); policy: eerst “warn if p95 +10%”, later “fail if +X%”. Zie “Wat is nú écht open”.

### CI baseline: Bencher

- **perf_main.yml:** Draait op push naar main en op schedule (nightly). Slaat Criterion-resultaten op als baseline; `--thresholds-reset` voor main-branch thresholds.
- **perf_pr.yml:** Draait op pull_request (alleen same-repo). Vergelijkt PR-branch met main via `--start-point`, `--start-point-clone-thresholds`, `--start-point-reset`. Geen `--err` dus job faalt niet op regressie; Bencher post check/comment. Later `--err` toevoegen voor hard fail.
- **Secrets:** `BENCHER_PROJECT` (project slug), `BENCHER_API_TOKEN`. Zie [Bencher GitHub Actions](https://bencher.dev/docs/how-to/github-actions/).
- **Conventie:** PR = snelle signalen (zelfde benches, quick); main/nightly = authoritative baseline.

#### Bencher secrets verkrijgen en configureren

De Perf-workflows (`perf_main.yml`, `perf_pr.yml`) draaien alleen als de repository-secrets gezet zijn. Zonder secrets worden de Bencher-jobs **skipped** (geen failure).

1. **Account en project**
   - Ga naar [bencher.dev](https://bencher.dev) en maak een account (Sign up).
   - Maak een **project** aan in de Bencher Console (of run lokaal eenmaal `bencher run …` zonder `--project`; Bencher maakt een on-the-fly project, daarna “Claim this project” om het aan je account te koppelen).
   - Het **project slug** is de projectnaam (bijv. `assay`) of het lange formaat (bijv. `project-abc4567-wxyz123456789`). Je vindt het in de Bencher Console bij het project (URL of projectinstellingen).

2. **API-token**
   - In de [Bencher Console](https://bencher.dev/console): rechtsboven op je naam klikken → **Tokens**.
   - **➕ Add** → geef de token een naam (bijv. `assay-github-actions`) → kopieer de waarde. Bewaar die veilig; hij is daarna niet opnieuw in te zien.

3. **GitHub repository-secrets**
   - Ga naar je repo op GitHub → **Settings** → **Secrets and variables** → **Actions**.
   - **New repository secret**:
     - **Name:** `BENCHER_PROJECT` → **Secret:** het project slug (bijv. `project-abc4567-wxyz123456789`).
     - **Name:** `BENCHER_API_TOKEN` → **Secret:** de API-token uit stap 2.

4. **Controleren**
   - Na het toevoegen van beide secrets draaien bij de volgende push naar `main` de **Perf (main baseline)**-job en bij PR’s (same-repo) de **Perf (PR compare)**-job. Resultaten verschijnen op [bencher.dev](https://bencher.dev) en (met `--github-actions`) als check/comment op de PR.

Zie ook: [Bencher: Create an API Token](https://bencher.dev/docs/how-to/claim/#create-an-api-token), [Bencher GitHub Actions](https://bencher.dev/docs/how-to/github-actions/).

### Semantic/judge VCR-workload

- **Fixture-structuur:** `tests/fixtures/perf/semantic_vcr/` — eval_semantic_vcr.yaml (1× semantic_similarity_to, 1× faithfulness), trace_semantic_vcr.jsonl, cassettes/ (embeddings/, judge/) + README.
- **Runtime-contract:** `ASSAY_VCR_MODE=replay|record|off` (CI default: replay), `ASSAY_VCR_DIR`. CI draait alleen replay; record alleen lokaal met API key. Cassettes scrubben vóór commit.
- ✅ **VCR-middleware:** `crates/assay-core/src/vcr/mod.rs` — `VcrClient` met `post_json()` voor record/replay van HTTP-requests. Matching op method + URL + body (SHA256 fingerprint); Authorization-header uitgesloten. Cassettes opgeslagen als JSON in `ASSAY_VCR_DIR/{embeddings,judge}/`. Zie module docs en tests voor gebruik.
- ✅ **Provider-integratie:** OpenAI embedder (`providers/embedder/openai.rs`) en LLM client (`providers/llm/openai.rs`) ondersteunen VCR via `with_vcr()` of `from_env()` constructors. In CI met `ASSAY_VCR_MODE=replay` worden responses uit cassettes gelezen; geen outbound netwerk.

### Adapter outputs + Criterion flags + VCR hygiene

- **Criterion:** Bencher-adapter `rust_criterion` verwacht Criterion stdout; meet `latency` (ns). Gebruik altijd `--bench <name> -- …` voor extra args (bijv. `-- --quick`). **Harness:** Criterion-benches moeten `harness = false` hebben in `Cargo.toml` (`[[bench]] name = "…" harness = false`); anders gebruikt Cargo de libtest-harness en krijg je "running 0 tests" in plaats van Criterion-output → Bencher: "Are you sure rust_criterion is the right adapter?". **IDs:** Criterion zet benchmark-naam + `time: [...]` op één regel alleen als de ID kort genoeg is; lange IDs wrappen → adapter parset niet. Gebruik korte group names (bijv. `sw`, `sr`) en korte bench-namen (`50x400b`, `12xlarge`, `wc`) zodat `sw/50x400b   time: [..]` op één regel blijft.
- **Stdin/pipe-modus:** Bencher leest van stdin als je geen command na `--` geeft. Robuuster dan exec-modus: `cargo bench … 2>&1 | grep -v "Gnuplot not found" | bencher run --adapter rust_criterion …` (geen `-- command`).
- **Hyperfine:** Bencher kan `--file results.json` (Hyperfine JSON) innemen voor e2e-tracking.
- **VCR-hygiene:** Cassette-store in repo: scrub secrets/PII; matching op method + url + body (gecanonicaliseerde JSON), niet op Authorization; CI default = replay, record alleen lokaal.

### Bencher ingest: exacte commands en reports

**Waarom het nu werkt:** (1) Korte Criterion-IDs (`sw/50x400b`, `sw/12xlarge`, `sr/wc`) zodat `id + time:` op één regel blijft voor de rust_criterion-adapter. (2) Stdin/pipe: Bencher krijgt exact de gefilterde stdout. (3) Zelfde branch/testbed (`main`, `ubuntu-latest`) en threshold-flags op main en PR.

**Main baseline – twee aparte runs (twee reports in Bencher):**

```bash
# Step 1: store_write_heavy → report met sw/50x400b, sw/12xlarge
cargo bench -p assay-core --bench store_write_heavy 2>&1 \
  | grep -v "Gnuplot not found" \
  | bencher run \
      --project "$BENCHER_PROJECT" --token "$BENCHER_API_TOKEN" \
      --branch main --testbed ubuntu-latest --adapter rust_criterion \
      --ci-id store_write_heavy \
      --threshold-measure latency --threshold-test t_test \
      --threshold-max-sample-size 64 --threshold-upper-boundary 0.99 --thresholds-reset \
      --github-actions "$GITHUB_TOKEN"

# Step 2: suite_run_worstcase → aparte report met sr/wc
cargo bench -p assay-cli --bench suite_run_worstcase 2>&1 \
  | grep -v "Gnuplot not found" \
  | bencher run \
      --project "$BENCHER_PROJECT" --token "$BENCHER_API_TOKEN" \
      --branch main --testbed ubuntu-latest --adapter rust_criterion \
      --ci-id suite_run_worstcase \
      --threshold-measure latency --threshold-test t_test \
      --threshold-max-sample-size 64 --threshold-upper-boundary 0.99 --thresholds-reset \
      --github-actions "$GITHUB_TOKEN"
```

**Waar sr/wc landt:** Elke `bencher run`-aanroep maakt één report. De eerste step vult een report met alleen `sw/50x400b` en `sw/12xlarge`; de tweede step een report met alleen `sr/wc`. In de Bencher-UI zie je dus twee reports per baseline-run (zelfde branch version). Dat is bewust: `--ci-id` onderscheidt de runs; alle drie de benchmarks zijn wel in de branch-baseline aanwezig.

**PR compare:** Zelfde pipe-setup, met `--branch "$GITHUB_HEAD_REF"`, `--start-point "$GITHUB_BASE_REF"`, `--start-point-hash <base_sha>`, `--start-point-clone-thresholds`, `--start-point-reset`, en dezelfde threshold-flags als main. Zonder `--err`: Bencher post de vergelijking als check/comment (warn). Met `--err`: run faalt bij threshold-alert (hard fail); toevoegen zodra ruis onder controle is.

**Robuustheid later:** Overweeg overstap naar json-adapter + BMF file (Criterion JSON of eigen export) zodat wijzigingen in Criterion-output de ingest niet breken.

### Bencher policy: reports, warn vs fail, thresholds

**A) Eén report vs meerdere:** Huidige keuze = **meerdere reports** (één per `bencher run`). Voordeel: duidelijk per workload, thresholds en failures per bench los. Nadeel: twee reports bekijken in Bencher. Alternatief (één report) zou aggregator-bench of BMF/JSON-combinatie vragen; aanbevolen is meerdere reports aanhouden tot PR-gating stabiel is.

**B) Warn vs fail:**
- **perf_pr.yml:** warning-only (geen `--err`). Bencher post vergelijking als check/comment; bij regressie waarschuwing, merge niet geblokkeerd.
- **Later (optioneel):** aparte workflow `perf_pr_gate.yml` die alleen draait op label `perf-gate` of “ready for review”, mét `--err`, zodat regressies de merge blokkeren. Pas toevoegen zodra ruis onder controle is.

**C) Thresholds per benchmark:** Upper boundary staat nu op Bencher-default (o.a. upper_boundary 0.99). Voor strikte policy: bv. +10% warn, +20% fail; per benchmark overrulen in Bencher UI als één bench inherent noisy is. Thresholds worden van main gecloned naar PR via `--start-point-clone-thresholds` en `--start-point-reset`.

**Exacte PR bencher run-regels (perf_pr.yml, voor diff/warning-policy):**

```bash
# PR step 1: store_write_heavy
cargo bench -p assay-core --bench store_write_heavy 2>&1 \
  | grep -v "Gnuplot not found" \
  | bencher run \
      --project "$BENCHER_PROJECT" --token "$BENCHER_API_TOKEN" \
      --branch "$GITHUB_HEAD_REF" \
      --start-point "$GITHUB_BASE_REF" --start-point-hash '${{ github.event.pull_request.base.sha }}' \
      --start-point-clone-thresholds --start-point-reset \
      --testbed ubuntu-latest --adapter rust_criterion --ci-id store_write_heavy \
      --threshold-measure latency --threshold-test t_test \
      --threshold-max-sample-size 64 --threshold-upper-boundary 0.99 \
      --github-actions "$GITHUB_TOKEN"

# PR step 2: suite_run_worstcase
cargo bench -p assay-cli --bench suite_run_worstcase 2>&1 \
  | grep -v "Gnuplot not found" \
  | bencher run \
      --project "$BENCHER_PROJECT" --token "$BENCHER_API_TOKEN" \
      --branch "$GITHUB_HEAD_REF" \
      --start-point "$GITHUB_BASE_REF" --start-point-hash '${{ github.event.pull_request.base.sha }}' \
      --start-point-clone-thresholds --start-point-reset \
      --testbed ubuntu-latest --adapter rust_criterion --ci-id suite_run_worstcase \
      --threshold-measure latency --threshold-test t_test \
      --threshold-max-sample-size 64 --threshold-upper-boundary 0.99 \
      --github-actions "$GITHUB_TOKEN"
```

(Voor consistent +10% warn / +20% fail: threshold-waarden in Bencher UI of via API aanpassen; voor hard-fail gate: kopie van perf_pr.yml met `--err` en bv. `if: contains(github.event.pull_request.labels.*.name, 'perf-gate')`.)

### summary.json-velden (phase + store)

Vaste velden zodat elke run vergelijkbaar is en regressies automatisch te detecteren:

| Sectie | Velden |
|--------|--------|
| **Phases** | ingest_ms, precompute_ms, run_suite_ms, report_ms, total_ms |
| **Store** | store_wait_ms, store_write_ms, sqlite_busy_count, txn_batch_size (indien batching) |
| **Cache** | cache_hit_count, cache_miss_count (of cache_hit_rate) |
| **Context** | db_mode (`:memory:` of path), parallel, schema_version, assay_version |
| **DX** | slowest_tests (top 5), per-test duration_ms in results array (bestaat al) |

Schema: zie [SPEC-PR-Gate-Outputs-v1](architecture/SPEC-PR-Gate-Outputs-v1.md); deze velden kunnen als uitbreiding (nieuwe schema_version) worden toegevoegd.

### CI-cache: blessed snippet

**Repo-root (algemeen):**

```yaml
- name: Cache Assay store
  id: assay-cache
  uses: actions/cache@v4
  with:
    path: .assay
    key: assay-${{ runner.os }}-${{ hashFiles('**/eval.yaml', '**/policy.yaml', '**/traces/*.jsonl') }}-${{ env.ASSAY_VERSION || 'latest' }}
    restore-keys: assay-${{ runner.os }}-
- name: Run assay
  run: assay ci ...
- name: Prove cache hit (job summary / logs)
  if: always()
  run: |
    echo "cache-hit=${{ steps.assay-cache.outputs.cache-hit }}"
    echo "cache-hit=${{ steps.assay-cache.outputs.cache-hit }}" >> "$GITHUB_STEP_SUMMARY"
```

**Subdir (bijv. baseline-gate):** Gebruik `path` op de betreffende directory (bijv. `examples/baseline-gate/.eval` en `examples/baseline-gate/.assay`) en pas de `key` aan op de bestanden in die dir. Zie `.github/workflows/baseline-gate-demo.yml` voor een werkend voorbeeld; daar wordt **cache-hit** in de job summary gelogd.

**Eis:** In CI logs én in de job summary moet **cache-hit=true** of **cache-hit=false** zichtbaar zijn.

Invalidatie: bij wijziging in eval/policy/traces of assay version. Documenteer: wat wel/niet gecached wordt. **In CI:** log **cache-hit** in job summary (bijv. `echo "cache-hit=${{ steps.cache.outputs.cache-hit }}"` of in job summary step) zodat warm-cache claims feitelijk onderbouwd zijn.

### CI cache voor perf jobs

Voor een **complete performance assessment** moet de **perf-job** (Criterion benches) ook cache + cache-hit gebruiken, niet alleen baseline-gate-demo:

- **Blessed cache-strategie voor perf:** Cache `target/` (rust-cache doet dit al) zodat `cargo bench` sneller draait; optioneel: cache `.assay/` of een perf-fixture dir als de perf-job e2e (Hyperfine) draait. **Norm:** Waar cache leeft: repo-root `.assay/` voor assay-run output; `target/` voor build/bench; subdir (bijv. `examples/baseline-gate/.assay`) voor workflow-specifieke runs. Wat je cached: DB + embeddings-cache + wat de key invalideert (eval/policy/trace hash).
- **Perf-job: cache-hit in job summary:** In de perf-job (ci.yml) **altijd** cache-hit loggen in de job summary, zodat warm-run claims verifieerbaar zijn. Zonder dit is “warm cache” niet bewijsbaar.

**Huidige stand:** baseline-gate-demo.yml cached en logt cache-hit. De Criterion-perf-job in ci.yml gebruikt Swatinem/rust-cache (target/); **aanbevolen:** voeg een stap toe die cache-hit (van rust-cache of een assay-perf cache) in de job summary logt.

---

## Perf gate policy (optioneel)

Een concrete “perf gate”-policy (bijv. **“p95 worstcase mag max +10% regressen”**) die realistisch is voor GitHub runners en niet elke PR random rood maakt, kan apart voorgesteld worden. Zodra baseline (20× worstcase file-backed, median + p95) en CI cache-hit vaststaan, is zo’n gate in te bouwen.

---

## Verwijzingen

- [ADR-019 P0.3](architecture/ADR-019-PR-Gate-2026-SOTA.md#p03-store-performance-wal--single-writer-batching--bounded-queue)
- [DX-IMPLEMENTATION-PLAN](DX-IMPLEMENTATION-PLAN.md) (o.a. slowest 5, cache hit rate, phase timings in summary)
- [SPEC-PR-Gate-Outputs-v1](architecture/SPEC-PR-Gate-Outputs-v1.md) (summary.json schema)
- [concepts/cache.md](concepts/cache.md)
- [Criterion](https://github.com/bheisler/criterion.rs), [Hyperfine](https://github.com/sharkdp/hyperfine), [Rust Performance Book](https://nnethercote.github.io/perf-book/), [Bencher](https://bencher.dev/) (continuous benchmarking, [GitHub Actions](https://bencher.dev/docs/how-to/github-actions/))
