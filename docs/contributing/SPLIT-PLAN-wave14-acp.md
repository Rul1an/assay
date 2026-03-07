# Wave14 Plan - `assay-adapter-acp` Split

## Intent

Split `crates/assay-adapter-acp/src/lib.rs` into bounded internal modules while
preserving behavior and public adapter contract.

## Scope

- Step1 freeze: docs + gate script only
- Step2 mechanical: move-only split under `crates/assay-adapter-acp/src/**`
- Step3 closure: docs + gate script only
- Step4 promote: single final promote PR (`main <- step3`)

## Public contract freeze

- no behavior drift in strict/lenient convert modes
- no event type string drift
- no lossiness/measurement classification drift
- public adapter entrypoint remains stable (`AcpAdapter` + `ProtocolAdapter` impl)

## Mechanical target layout (Step2)

- `crates/assay-adapter-acp/src/lib.rs` (thin facade + public surface + wrapper)
- `crates/assay-adapter-acp/src/adapter_impl/mod.rs` (entry)
- `crates/assay-adapter-acp/src/adapter_impl/convert.rs` (`convert_impl`)
- `crates/assay-adapter-acp/src/adapter_impl/lossiness.rs`
- `crates/assay-adapter-acp/src/adapter_impl/normalize.rs` (pure canonicalization helpers)
- `crates/assay-adapter-acp/src/adapter_impl/raw_payload.rs` (raw payload hash/ref)
- `crates/assay-adapter-acp/src/adapter_impl/mapping/*.rs` (event/payload mapping)
- `crates/assay-adapter-acp/src/tests/mod.rs` (moved tests)

## Boundary guard

- helper modules remain deterministic/pure by default
- no hidden IO in helper modules unless explicitly planned
- tests/fixtures keep existing contract behavior and deterministic assertions

## Step1 targeted checks (locked)

- `cargo fmt --check`
- `cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings`
- `cargo test -p assay-adapter-acp`

## Step2 invariants to enforce

- `lib.rs` has exactly one call-site to `convert_impl(...)`
- `lib.rs` contains no mapping logic
- deterministic canonical digest behavior remains key-order independent
- lossiness semantics and strict/lenient error kind boundaries remain unchanged

## Promote discipline

1. PR1: Step1 `main <- step1`
2. PR2: Step2 `step1 <- step2`
3. PR3: Step3 `step2 <- step3`
4. Before final promote: merge `origin/main` into step3 branch and rerun Step3 gate
5. Final promote PR: `main <- step3`

Only enable auto-merge when `mergeStateStatus=CLEAN`.
For flaky infra failures: rerun failed checks only.
