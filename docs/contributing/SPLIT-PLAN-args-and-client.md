# Split Plan: args.rs & client.rs (Feb 2026)

Prioritized refactor of the two largest handwritten files. Best practices: module-per-domain, <800 LOC/file, explicit re-exports, minimal import churn.

---

## 1. assay-cli `args.rs` (1262 LOC) — **High ROI**

### Current structure

- Single file with `Cli`, `Command` enum, and 40+ arg structs
- Imports: `crate::cli::args::*` from main.rs, dispatch, and 20+ command modules
- Feature gate: `#[cfg(feature = "sim")]` for SimArgs, SoakArgs, etc.

### Target layout

```
crates/assay-cli/src/cli/
  args/
    mod.rs        # Cli, Command enum; re-exports all (thin facade, NO glob use super::*)
    common.rs    # OutputFormat, ValidateOutputFormat, JudgeArgs
    run.rs       # RunArgs, CiArgs
    baseline.rs  # BaselineArgs, BaselineSub, ...
    bundle.rs    # BundleArgs, BundleSub, BundleCreateArgs, BundleVerifyArgs, ReplayArgs
    trace.rs     # TraceArgs, TraceSub
    init.rs      # InitArgs, InitCiArgs
    quarantine.rs
    validate.rs  # ValidateArgs
    policy.rs    # PolicyArgs, PolicyCommand, ...
    evidence.rs  # EvidenceArgs
    mcp.rs       # McpArgs, McpSub, ConfigPathArgs, McpWrapArgs
    setup.rs     # SetupArgs
    tool.rs      # ToolArgs
    sim.rs       # #[cfg(feature = "sim")] — entire module
    devtools.rs  # DoctorArgs, SandboxArgs, WatchArgs, DemoArgs, DiscoverArgs
    migration.rs # MigrateArgs, ImportArgs, FixArgs, CoverageArgs, CalibrateArgs, MaxRisk
```

**Facade hygiene:** Declare `pub mod` first, then `pub use`, then `Cli`/`Command`. No `use super::*`.

### Migration strategy

1. **Phase A:** Create `args/` dir, move Cli+Command to `mod.rs`, add contract test, extract `common.rs`
2. **Phase B:** Extract always-on domains first (run, baseline, bundle, policy), then rest, sim last
3. **Phase C:** Update imports in dispatch.rs

### Contract test (add with Phase A)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }
}
```

---

## 2. assay-registry `client.rs` (1273 LOC) — **Medium ROI**

### Target layout

```
crates/assay-registry/src/
  client/
    mod.rs       # RegistryClient, URL building, auth, payload parsing (no status logic)
    http.rs      # FetchBytes, fetch_bytes_with_retry — ONLY place for status codes
    helpers.rs   # parse_pack_url, parse_revocation_body
```

**Leak-free contract:** `client/mod.rs` never interprets status codes. All mapping in `http.rs`.

### client/http.rs boundary

- `FetchBytes` enum: `Ok { body, etag }`, `NotModified`, `NotFound`
- `fetch_bytes_with_retry<F>(client, make_request, retry)` — closure-based, no Request::try_clone
- RegistryError: `is_retryable()`, `with_attempts(n)`

### Migration strategy

- **Phase 0:** Extract integration tests to `tests/registry_client.rs` (highest ROI)
- **Phase 1-4:** Split client, http, helpers; add RegistryError methods

---

## 3. Implementation order

| Step | Task |
|------|------|
| 0 | registry: extract integration tests |
| 1 | args: create args/ dir, contract test, common.rs |
| 2 | args: extract run, baseline, bundle, policy |
| 3 | args: extract remaining domains |
| 4 | args: extract sim last |
| 5 | registry: split client |
| 6 | registry: RegistryError is_retryable/with_attempts |

---

## 4. References

- CONTRIBUTING.md § File size guideline
- CLAUDE.md: CLI entry points, assay-cli structure
