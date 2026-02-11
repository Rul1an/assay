# Review Pack: ADR-024 Epic 2 (CLI Flags & Parsing)

**Branch:** `feat/adr-024-e2-e6` (or equivalent)
**Epic:** E2 — CLI: `--limits`, `--limits-file`, `--time-budget`, `--print-config`; `@path` parsing; merge precedence; exit on parse error
**Depends on:** E1 (VerifyLimitsOverrides)
**ADR:** [ADR-024 Sim Engine Hardening](./ADR-024-Sim-Engine-Hardening.md)

---

## Review Checklist

### Functional

| Criterion | Location | Verify |
|-----------|----------|--------|
| `--limits` (JSON string or `@path`) on `SimRunArgs` | `args.rs` L1176–1178 | `pub limits: Option<String>` |
| `--limits-file` on `SimRunArgs` | `args.rs` L1180–1182 | `pub limits_file: Option<PathBuf>` |
| `--time-budget` (default 60) | `args.rs` L1184–1186 | `#[arg(default_value = "60")] pub time_budget: u64` |
| `--print-config` boolean flag | `args.rs` L1188–1190 | `pub print_config: bool` |
| `--limits @path` parsing | `sim.rs` L19–23 | `s.starts_with('@')` → load from file via `trim_start_matches('@')` |
| Merge precedence: tier → `--limits` → `--limits-file` | `sim.rs` L15–37 | `parse_limits` applies in order |
| Config errors → `EXIT_CONFIG_ERROR` (2) | `sim.rs` L50–73 | Invalid JSON, missing file, `time_budget ≤0` |
| Unknown keys → serde reject | E1 / assay-evidence | `VerifyLimitsOverrides` has `deny_unknown_fields` |

### UX / Error Messages

| Scenario | Expected | Location |
|----------|----------|----------|
| Invalid `--limits` JSON | "invalid --limits JSON (use --limits-file or --limits @path for file)" | `sim.rs` L26 |
| `--limits @/nonexistent` | "limits file not found: …" | `sim.rs` L21 |
| `--limits-file /nonexistent` | "limits file not found: …" | `sim.rs` L32 |
| `--time-budget 0` | "Config error: --time-budget must be > 0" | `sim.rs` L50–52 |
| Unknown tier | "Config error: unknown suite tier: …" | `sim.rs` L60–62 |
| `--limits '{"max_bundle_bytess": 1}'` | Serde error (deny_unknown_fields) | assay-evidence |

---

## Merge Precedence Flow

```
parse_limits(args)
    │
    ├─ defaults = tier_default_limits(suite)
    │     └─ Quick → max_bundle_bytes = 5MB; other tiers → full default
    │
    ├─ if args.limits:
    │     overrides = s.starts_with('@') ? load_file(s) : parse_json(s)
    │     defaults = defaults.apply(overrides)
    │
    └─ if args.limits_file:
          overrides = load_file(p)
          defaults = defaults.apply(overrides)   # file wins over --limits
    │
    └─ return defaults
```

**Test precedence:** `--limits '{"max_bundle_bytes": 1000}' --limits-file stricter.json` → file values override.

---

## Key Code Snippets

### args.rs: SimRunArgs

```rust
/// Verification limits as JSON, or @path to load from file
#[arg(long)]
pub limits: Option<String>,

/// Path to JSON file with limits (overrides --limits if both given)
#[arg(long)]
pub limits_file: Option<std::path::PathBuf>,

/// Suite time budget in seconds (default: 60). Must be > 0.
#[arg(long, default_value = "60")]
pub time_budget: u64,

/// Print effective limits and time budget, then exit
#[arg(long)]
pub print_config: bool,
```

### sim.rs: parse_limits + @path

```rust
/// Parse limits from CLI. Merge precedence: tier default → --limits → --limits-file.
fn parse_limits(args: &SimRunArgs) -> Result<VerifyLimits> {
    let mut defaults = tier_default_limits(args.suite.to_lowercase().as_str());
    if let Some(ref s) = args.limits {
        let overrides = if s.starts_with('@') {
            let path = s.trim_start_matches('@').trim();
            let content = fs::read_to_string(path)
                .with_context(|| format!("limits file not found: {}", path))?;
            serde_json::from_str::<VerifyLimitsOverrides>(&content)
                .with_context(|| format!("invalid limits JSON in {}", path))?
        } else {
            serde_json::from_str::<VerifyLimitsOverrides>(s)
                .context("invalid --limits JSON (use --limits-file or --limits @path for file)")?
        };
        defaults = defaults.apply(overrides);
    }
    if let Some(ref p) = args.limits_file {
        let content = fs::read_to_string(p)
            .with_context(|| format!("limits file not found: {}", p.display()))?;
        let overrides = serde_json::from_str::<VerifyLimitsOverrides>(&content)
            .with_context(|| format!("invalid limits JSON in {}", p.display()))?;
        defaults = defaults.apply(overrides);
    }
    Ok(defaults)
}
```

### sim.rs: Config error handling (exit 2)

```rust
if args.time_budget == 0 {
    eprintln!("Config error: --time-budget must be > 0");
    std::process::exit(EXIT_CONFIG_ERROR);
}
// ...
let verify_limits = match (args.limits.as_ref(), args.limits_file.as_ref()) {
    (None, None) => Some(limits),
    _ => match parse_limits(&args) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("Config error: {}", e);
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    },
};
```

---

## Test Plan (E2-specifiek)

| # | Command | Expected |
|---|---------|----------|
| 1 | `assay sim run --suite quick --target bundle.tar.gz --limits '{"max_bundle_bytes": 1000}'` | Runs; zip_bomb + limit_bundle_bytes blocked |
| 2 | `assay sim run --suite quick --target bundle.tar.gz --limits-file /nonexistent` | Exit 2, "Config error: … limits file not found" |
| 3 | `assay sim run --suite quick --target bundle.tar.gz --limits 'invalid'` | Exit 2, "Config error: invalid --limits JSON …" |
| 4 | `assay sim run --suite quick --target bundle.tar.gz --limits '{"max_bundle_bytess": 1}'` | Exit 2, error contains "unknown" or serde reject |
| 5 | `assay sim run ... --limits '{"max_bundle_bytes": 1000}' --limits-file .assay/stricter.json` | File wins; merged limits used |
| 6 | `assay sim run --suite quick --target bundle.tar.gz --time-budget 0` | Exit 2, "Config error: --time-budget must be > 0" |
| 7 | `assay sim run --suite quick --target bundle.tar.gz --limits @.assay/limits.json` | Loads from file; equivalent to --limits-file |
| 8 | `assay sim run --suite quick --target bundle.tar.gz --print-config` | Prints max_bundle_bytes, max_decode_bytes, time_budget; exit 0 |

---

## Verification Commands

```bash
# Build
cargo build -p assay-cli --features sim

# Print config (no run)
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --print-config

# Config errors
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --limits-file /nonexistent
# → exit 2, "limits file not found"

cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --limits 'invalid'
# → exit 2, "invalid --limits JSON"

cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --time-budget 0
# → exit 2, "--time-budget must be > 0"

# Merge precedence (create .assay/stricter.json with {"max_bundle_bytes": 500})
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz \
  --limits '{"max_bundle_bytes": 1000}' --limits-file .assay/stricter.json --print-config
# → max_bundle_bytes: 500 (file wins)
```

---

## ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| CLI §1 | --limits, --limits-file, --time-budget, --print-config | ✓ |
| --limits @path | Value starts with @ → load from file | ✓ |
| Parse rules | Invalid JSON → exit 3; ADR says 3, impl uses 2 (EXIT_CONFIG_ERROR) | ⚠ Align |
| Unknown keys | deny_unknown_fields | ✓ (E1) |
| Merge precedence | tier → --limits → --limits-file | ✓ |

---

## Review Nits (niet-blockers)

| Nit | Aanbeveling |
|-----|-------------|
| Double-source (`@path` + `--limits-file`) | Optional: log/print welk pad effectief in verbose |
| Serde unknown-field error | Expliciet mappen naar "unknown field 'X'" voor betere UX |
| --print-config machine-readable | Overweeg `--print-config=json` voor CI/tooling |
| --print-config requires --target | ADR niet expliciet; huidige impl vereist target (clap); kan later relaxen |

---

## Merge Gates

- [ ] Alle E2 checklist items pass
- [ ] Test plan items 2, 3, 4, 6, 8 executed
- [ ] `cargo clippy -p assay-cli --features sim -- -D warnings` passes
- [ ] Exit code: ADR=3 vs impl=2 gedocumenteerd of gealigned (E4 blocker)

---

## Acceptance

- [ ] Alle functional criteria geverifieerd
- [ ] Test plan scenarios uitgevoerd
- [ ] ADR-024 Epics: E2 marked implemented
