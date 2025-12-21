# Implementation Plan - PR9: Judge Metrics (LLM-as-a-Judge)

Based on [ADR-004 v2](docs/ADR-004-Judge-Metrics.md).

## User Review Required
> [!IMPORTANT]
> **Exit Code Policy**: We are strictly adhering to "No Exit Code 3". All "unstable" or "disagreement" states will result in `WARN` status (Exit 0) by default, or `FAIL` (Exit 1) under `--strict`.
> **Cache Strategy**: implementation uses a **separate** `judge_cache` table. VCR cache is strictly for completion replay. Judge cache is for metric result replay.

## Proposed Changes

### 1. CLI & Config (`verdict-cli`, `verdict-core`)
Implement new flags and environment variable precedence.

#### [MODIFY] [main.rs](file:///crates/verdict-cli/src/main.rs)
- Add `JudgeArgs` flattened struct to `RunArgs` and `CiArgs`.
- Flags:
    - `--judge` (`none`|`openai`|`fake`)
    - `--judge-model`
    - `--judge-samples` (default 3)
    - `--judge-temperature` (default 0.0)
    - `--judge-max-tokens` (default 800)
    - `--judge-refresh`
    - `--judge-api-key` (advanced/hidden)
- implement `load_judge_config` with precedence (Flag > Env > Default).

#### [MODIFY] [model.rs](file:///crates/verdict-core/src/model.rs)
- Update `Expected` enum to support `faithfulness`, `relevance`.
- `Settings`: Add `JudgeConfig` but ONLY for suite-level overrides (rubric, etc). Runtime settings (provider, keys) stay in CLI/Env.

### 2. Core Engine & Providers
Implement the "Enrichment Pattern" with separate Judge Cache.

#### [NEW] [judge/mod.rs](file:///crates/verdict-core/src/judge/mod.rs)
- `JudgeService`: Orchestrates calls.
- `VoteAggregator`: Implements majority vote + agreement calc.
- `enrich_judge(tc, resp)`: Logic to check trace -> cache -> live.

#### [NEW] [storage/judge_cache.rs](file:///crates/verdict-core/src/storage/judge_cache.rs)
- Separate SQLite table `judge_cache`.
- Schema: `key` (PK), `provider`, `model`, `rubric_id`, `rubric_version`, `created_at`, `payload_json`.
- `get(key)`, `put(key, payload)`.

#### [MODIFY] [runner.rs](file:///crates/verdict-core/src/engine/runner.rs)
- Inject `JudgeService` into `Runner`.
- **Enrichment Logic**:
    1.  **Trace Check**: If `resp.meta.verdict.judge.<rubric>` exists, use it (Source: "trace").
    2.  **Judge Disabled**: If metadata missing AND `--judge none` -> **Exit 2** (Config Error) with actionable hint.
    3.  **Cache Check**: Generate key (incl. temp/tokens/samples). If hit -> Use it (Source: "cache").
    4.  **Live Call**: If enabled -> Call provider -> Cache -> Use it (Source: "live").

#### [MODIFY] [redaction.rs](file:///crates/verdict-core/src/redaction.rs)
- Update redaction logic to redact `rationale` fields in judge metadata if `--redact-prompts` is active. (Samples/scores are safe).

### 3. Caching (Key Structure)
cache key generator in `judge/mod.rs` must include:
- `provider`, `model`
- `rubric_id`, `rubric_version`
- `temperature`, `max_tokens`
- `samples` (k)
- `input_hash` (prompt + response + normalized context)
- `template_hash` (prompt template)

### 4. Metrics
#### [MODIFY] [judge.rs](file:///crates/verdict-metrics/src/judge.rs)
- Implement `FaithfulnessMetric`, `RelevanceMetric`.
- Logic: Read from `resp.meta.verdict.judge`.
- Use `score + epsilon >= min_score` for pass check.

## Reference: Copy/Paste Ready Text

### CLI Help Text (clap attributes)
```rust
// --judge
help = "Enable or disable LLM-as-judge evaluation.
- none: judge calls disabled (replay/trace-only)
- openai: live judge calls via OpenAI
- fake: deterministic fake judge (tests/dev)"

// --judge-model
help = "Judge model identifier (provider-specific). Example: gpt-4o-mini"

// --judge-samples
help = "Number of judge samples per test (majority vote). Default: 3. Tip: for critical production gates consider: --judge-samples 5"

// --judge-refresh
help = "Ignore judge cache and re-run judge calls (live mode only)."

// --judge-temperature
help = "Temperature used for judge calls (affects cache key). Default: 0.0"

// --judge-max-tokens
help = "Max tokens for judge response (affects cache key). Default: 800"
```

### Error Messages
| Scenario | Exit | Message | Hint |
| :--- | :--- | :--- | :--- |
| **Missing API Key** | 2 | `config error: judge 'openai' requires OPENAI_API_KEY.` | `set OPENAI_API_KEY in your environment` |
| **Unknown Provider** | 2 | `config error: unknown judge provider '<VALUE>'.` | `valid values: none \| openai \| fake` |
| **Judge Disabled** | 2 | `config error: test '<TEST_ID>' requires judge results ... but judge is disabled.` | `options: 1) run live ... 2) run replay/CI offline ...` |
| **Timeout** | 1 | `error: judge call timed out after <SECONDS>s for test '<TEST_ID>'.` | `increase timeout via settings.timeout_seconds` |
| **Invalid Schema** | 2 | `config error: judge response for ... is invalid` | `try upgrading Verdict or re-running with --judge-refresh` |

### Warnings / Notes
*   **Disagreement**: `warning: judge samples disagreed for test '<TEST_ID>' (<PASS_COUNT>/<K> passed). marking as unstable (status=Warn).`
*   **Cache Hit**: `note: judge(faithfulness:v1) cache hit (test='<TEST_ID>')`

### Trace Schema (Meta)
```json
"meta": {
  "verdict": {
    "judge": {
      "faithfulness": {
        "rubric_version": "v1",
        "passed": true,
        "score": 0.92,
        "source": "cache", // or "live" or "trace"
        "samples": [true, true, false],
        "agreement": 0.67,
        "citations": [],
        "rationale": "Supported by chunk:2.",
        "cached_at": "2025-12-20T10:00:00Z"
      }
    }
  }
}
```

## Verification Plan

### Automated Tests
- `judge_cache_key_sensitivity`: Ensure changing `temperature` or `k` changes the key.
- `voting_logic`: Verify majority vote (e.g. `[true, true, false]` -> `pass`, agreement `0.67`).

### Failure Mode Verification (Manual/Integration)
1.  **Trace Offline (Happy)**: Trace contains judge meta → Run with `--judge none` → PASS (Source: "trace").
2.  **Trace Offline (Missing)**: Trace missing judge meta → Run with `--judge none` → **EXIT 2** (Config Error).
3.  **Cache Hit**: Run live once. Run again (no refresh) → Source: "cache".
4.  **Disagreement**: Mock a [true, false, true] response → Status **Warn** (or Fail if `--strict`). Result `pass=1`.

## Future: PR11 Baseline Reference (Copy/Paste Ready)

### CLI Help Text (Baseline)
```rust
// --baseline
help = "Load a baseline.json and compare this run against it (relative thresholds). If a baseline entry is missing for a test, Verdict emits a warning by default."

// --export-baseline
help = "Export a baseline.json from this run (for main branch baseline publishing)."

// --require-baseline
help = "Fail (config error) if any test is missing a baseline entry. Note: --strict will also treat baseline-missing warnings as blocking."
```

### Baseline Messages
| Scenario | Message | Hint |
| :--- | :--- | :--- |
| **Missing Entry** | `warning: no baseline entry found for test '<TEST_ID>' (metric='<METRIC>'). warning: regression check skipped for this test.` | `hint: to create a baseline run: verdict ci --export-baseline baseline.json` |
| **Schema Mismatch** | `config error: unsupported baseline schema_version <X> (supported: <Y>).` | `hint: regenerate the baseline with: verdict ci --export-baseline baseline.json` |
